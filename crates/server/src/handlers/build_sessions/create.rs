use std::sync::Arc;

use aide::{transform::TransformOperation, OperationIo};
use axum::{extract::State, http::StatusCode, Extension, Json};
use axum_derive_error::ErrorResponse;
use db::{
    build_session, build_session_token, source_code, user, ActiveValue, DatabaseConnection, DbErr,
    EntityTrait, QuerySelect, SelectExt, TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};
use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use validator::{Validate, ValidationError};

use crate::{auth::AuthenticatedUserId, schema::example_error, validation::ValidatedJson};

/// Errors that may occur during the build session creation process.
#[derive(ErrorResponse, Display, From, Error, OperationIo)]
#[aide(output)]
pub(super) enum BuildSessionCreateError {
    /// Database-related error.
    DatabaseError(DbErr),

    /// User was already deleted at the time the request was being executed.
    #[status(StatusCode::FORBIDDEN)]
    #[display(fmt = "non-existent user")]
    NonExistentUser,

    /// Provided source code identifier does not exist.
    #[status(StatusCode::NOT_FOUND)]
    #[display(fmt = "source code not found")]
    SourceCodeNotFound,
}

/// JSON request body.
#[derive(Deserialize, Validate, JsonSchema)]
pub(super) struct BuildSessionCreateRequest {
    /// Source code identifier.
    #[schemars(example = "crate::schema::example_database_identifier")]
    source_code_id: i64,

    /// `cargo-contract` tooling version.
    #[validate(length(max = 32), custom = "validate_cargo_contract_version")]
    #[schemars(example = "crate::schema::example_cargo_contract_version")]
    cargo_contract_version: String,

    /// Relative project directory, that can be used to build multi-contract projects.
    ///
    /// If empty, the source code root will be used.
    #[validate(length(max = 64), custom = "validate_project_directory")]
    #[schemars(example = "crate::schema::example_folder")]
    project_directory: Option<String>,
}

/// Validate the provided cargo-contract version to be a valid Semver string.
fn validate_cargo_contract_version(cargo_contract_version: &str) -> Result<(), ValidationError> {
    Version::parse(cargo_contract_version)
        .map(|_| ())
        .map_err(|_| ValidationError::new("invalid cargo-contract version"))
}

/// Validate the provided project directory to be an alphanumeric-based path.
fn validate_project_directory(project_directory: &str) -> Result<(), ValidationError> {
    if project_directory.chars().all(|ch| {
        matches!(ch, '.' | '/' | '_' | '-')
            || ch.is_ascii_alphanumeric()
            || ch.is_ascii_whitespace()
    }) {
        Ok(())
    } else {
        Err(ValidationError::new("expected alphanumeric-based path"))
    }
}

/// JSON response body.
#[derive(Serialize, JsonSchema)]
pub(super) struct BuildSessionCreateResponse {
    /// Build session identifier.
    #[schemars(example = "crate::schema::example_database_identifier")]
    id: i64,
}

/// Generate OAPI documentation for the [`create`] handler.
pub(super) fn docs(op: TransformOperation) -> TransformOperation {
    op.summary("Create new build session.")
        .response::<200, Json<BuildSessionCreateResponse>>()
        .response_with::<404, Json<Value>, _>(|op| {
            op.description("Provided source code identifier is incorrect.")
                .example(example_error(BuildSessionCreateError::SourceCodeNotFound))
        })
}

/// Build session creation handler.
pub(super) async fn create(
    Extension(current_user): Extension<AuthenticatedUserId>,
    State(db): State<Arc<DatabaseConnection>>,
    ValidatedJson(request): ValidatedJson<BuildSessionCreateRequest>,
) -> Result<Json<BuildSessionCreateResponse>, BuildSessionCreateError> {
    db.transaction(|txn| {
        Box::pin(async move {
            let user_exists = user::Entity::find_by_id(current_user.id())
                .select_only()
                .exists(txn)
                .await?;

            if !user_exists {
                return Err(BuildSessionCreateError::NonExistentUser);
            }

            let source_code_exists = source_code::Entity::find_by_id(request.source_code_id)
                .select_only()
                .exists(txn)
                .await?;

            if source_code_exists {
                let model = build_session::Entity::insert(build_session::ActiveModel {
                    user_id: ActiveValue::Set(Some(current_user.id())),
                    source_code_id: ActiveValue::Set(request.source_code_id),
                    cargo_contract_version: ActiveValue::Set(request.cargo_contract_version),
                    project_directory: ActiveValue::Set(request.project_directory),
                    ..Default::default()
                })
                .exec_with_returning(txn)
                .await?;

                build_session_token::Entity::insert(build_session_token::ActiveModel {
                    token: ActiveValue::Set(build_session_token::generate_token()),
                    source_code_id: ActiveValue::Set(request.source_code_id),
                    build_session_id: ActiveValue::Set(model.id),
                })
                .exec_without_returning(txn)
                .await?;

                Ok(Json(BuildSessionCreateResponse { id: model.id }))
            } else {
                Err(BuildSessionCreateError::SourceCodeNotFound)
            }
        })
    })
    .await
    .into_raw_result()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::testing::{create_database, RequestBodyExt, ResponseBodyExt};

    use assert_json::{assert_json, validators};
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use common::config::Config;
    use db::{public_key, source_code, token, user, ActiveValue, DatabaseConnection, EntityTrait};
    use serde_json::json;
    use tower::{Service, ServiceExt};

    async fn create_test_env(db: &DatabaseConnection) -> (String, i64) {
        let user = user::Entity::insert(user::ActiveModel::default())
            .exec_with_returning(db)
            .await
            .expect("unable to create user");

        let (model, token) = token::generate_token(user.id);

        token::Entity::insert(model)
            .exec_without_returning(db)
            .await
            .expect("unable to insert token");

        public_key::Entity::insert(public_key::ActiveModel {
            user_id: ActiveValue::Set(user.id),
            address: ActiveValue::Set(Vec::new()),
            ..Default::default()
        })
        .exec_without_returning(db)
        .await
        .expect("unable to create public key");

        let source_code_id = source_code::Entity::insert(source_code::ActiveModel {
            user_id: ActiveValue::Set(Some(user.id)),
            archive_hash: ActiveValue::Set(Vec::new()),
            ..Default::default()
        })
        .exec_with_returning(db)
        .await
        .expect("unable to create source code")
        .id;

        (token, source_code_id)
    }

    #[tokio::test]
    async fn create() {
        let db = create_database().await;

        let (token, source_code_id) = create_test_env(&db).await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/buildSessions")
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from_json(json!({
                        "source_code_id": source_code_id,
                        "cargo_contract_version": "3.0.0",
                        "project_directory": "./contracts/test/../another_contract"
                    })))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_json!(response.json().await, {
            "id": validators::i64(|_| Ok(()))
        });
    }

    #[tokio::test]
    async fn invalid_version() {
        let db = create_database().await;

        let (token, source_code_id) = create_test_env(&db).await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/buildSessions")
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from_json(json!({
                        "source_code_id": source_code_id,
                        "cargo_contract_version": "abc-1.2.3",
                    })))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn invalid_source_code_id() {
        let db = create_database().await;

        let (token, _) = create_test_env(&db).await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/buildSessions")
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from_json(json!({
                        "source_code_id": 123,
                        "cargo_contract_version": "3.0.0",
                    })))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn invalid_project_directory() {
        let db = create_database().await;

        let (token, _) = create_test_env(&db).await;

        let mut service = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()));

        let response = service
            .call(
                Request::builder()
                    .method("POST")
                    .uri("/buildSessions")
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from_json(json!({
                        "source_code_id": 123,
                        "cargo_contract_version": "3.0.0",
                        "project_directory": "��",
                    })))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let response = service
            .call(
                Request::builder()
                    .method("POST")
                    .uri("/buildSessions")
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from_json(json!({
                        "source_code_id": 123,
                        "cargo_contract_version": "3.0.0",
                        "project_directory": "\\",
                    })))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
