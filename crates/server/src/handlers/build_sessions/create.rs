use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Extension, Json};
use axum_derive_error::ErrorResponse;
use db::{
    build_session, build_session_token, source_code, user, ActiveValue, DatabaseConnection, DbErr,
    EntityTrait, QuerySelect, SelectExt, TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::{auth::AuthenticatedUserId, validation::ValidatedJson};

/// Regular expression to match stable versions of Rust toolchain and `cargo-contract`.
///
/// Currently, this regex does not support any nightly or unstable versions of the previously mentioned tooling.
static VERSION_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$"#).expect("invalid regex string")
});

/// Errors that may occur during the build session creation process.
#[derive(ErrorResponse, Display, From, Error)]
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
#[derive(Deserialize, Validate)]
pub(super) struct BuildSessionCreateRequest {
    /// Source code identifier.
    source_code_id: i64,

    /// `cargo-contract` tooling version.
    #[validate(regex = "VERSION_REGEX")]
    cargo_contract_version: String,

    /// Rust tooling version.
    #[validate(regex = "VERSION_REGEX")]
    rustc_version: String,
}

/// JSON response body.
#[derive(Serialize)]
pub(super) struct BuildSessionCreateResponse {
    /// Build session identifier.
    id: i64,
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
                    rustc_version: ActiveValue::Set(request.rustc_version),
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
    use tower::ServiceExt;

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
                        "rustc_version": "1.69.0"
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
                        "rustc_version": "1.69.0"
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
                        "rustc_version": "1.69.0"
                    })))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
