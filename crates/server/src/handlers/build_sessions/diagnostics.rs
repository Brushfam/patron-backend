use std::sync::Arc;

use aide::{transform::TransformOperation, OperationIo};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use axum_derive_error::ErrorResponse;
use db::{
    build_session, diagnostic, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter,
    QuerySelect, SelectExt, TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};
use futures_util::TryStreamExt;
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;

use crate::schema::example_error;

/// Errors that may occur during the diagnostics request handling.
#[derive(ErrorResponse, Display, From, Error, OperationIo)]
#[aide(output)]
pub(super) enum BuildSessionDiagnosticError {
    /// Database-related error.
    DatabaseError(DbErr),

    /// Requested build session was not found.
    #[status(StatusCode::NOT_FOUND)]
    #[display(fmt = "build session not found")]
    BuildSessionNotFound,
}

/// JSON response body.
#[derive(Serialize, JsonSchema)]
pub(super) struct BuildSessionDiagnosticResponse {
    /// Diagnostic level(Error, Warning)
    #[schemars(example = "crate::schema::example_diagnostic_level")]
    level: diagnostic::Level,

    /// Start cursor position of the diagnostic.
    #[schemars(example = "crate::schema::example_diagnostic_start")]
    start: i64,

    /// End cursor position of the diagnostic.
    #[schemars(example = "crate::schema::example_diagnostic_end")]
    end: i64,

    /// Diagnostic message.
    #[schemars(example = "crate::schema::example_diagnostic_message")]
    message: String,
}

/// Generate OAPI documentation for the [`diagnostics`] handler.
pub(super) fn docs(op: TransformOperation) -> TransformOperation {
    op.summary("Get all diagnostics for file.")
        .response::<200, Json<Vec<BuildSessionDiagnosticResponse>>>()
        .response_with::<404, Json<Value>, _>(|op| {
            op.description("No build sessions with the provided identifier were found.")
                .example(example_error(
                    BuildSessionDiagnosticError::BuildSessionNotFound,
                ))
        })
}

/// Diagnostics request handler.
///
/// This route is used in the CLI to get all diagnostics for a file.
pub(super) async fn diagnostics(
    Path(id): Path<i64>,
    State(db): State<Arc<DatabaseConnection>>,
) -> Result<Json<Vec<BuildSessionDiagnosticResponse>>, BuildSessionDiagnosticError> {
    db.transaction(|txn| {
        Box::pin(async move {
            let build_session_exists = build_session::Entity::find()
                .select_only()
                .filter(build_session::Column::Id.eq(id))
                .exists(txn)
                .await?;

            if !build_session_exists {
                return Err(BuildSessionDiagnosticError::BuildSessionNotFound);
            }

            diagnostic::Entity::find()
                .select_only()
                .columns([
                    diagnostic::Column::Level,
                    diagnostic::Column::Start,
                    diagnostic::Column::End,
                    diagnostic::Column::Message,
                ])
                .filter(diagnostic::Column::BuildSessionId.eq(id))
                .into_tuple::<(diagnostic::Level, i64, i64, String)>()
                .stream(txn)
                .await?
                .err_into()
                .and_then(|(level, start, end, message)| async move {
                    Ok(BuildSessionDiagnosticResponse {
                        level,
                        start,
                        end,
                        message,
                    })
                })
                .try_collect()
                .await
                .map(Json)
        })
    })
    .await
    .into_raw_result()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::testing::{create_database, ResponseBodyExt};

    use assert_json::assert_json;
    use axum::{body::Body, http::Request};
    use common::config::Config;
    use db::{
        build_session, diagnostic, file, public_key, source_code, token, user, ActiveValue,
        DatabaseConnection, EntityTrait,
    };
    use tower::ServiceExt;

    async fn create_test_env(db: &DatabaseConnection) {
        let user = user::Entity::insert(user::ActiveModel::default())
            .exec_with_returning(db)
            .await
            .expect("unable to create user");

        let (model, _token) = token::generate_token(user.id);

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
            archive_hash: ActiveValue::Set(vec![0; 32]),
            ..Default::default()
        })
        .exec_with_returning(db)
        .await
        .expect("unable to create source code")
        .id;

        let build_session = build_session::Entity::insert(build_session::ActiveModel {
            user_id: ActiveValue::Set(Some(user.id)),
            source_code_id: ActiveValue::Set(source_code_id),
            status: ActiveValue::Set(build_session::Status::Completed),
            cargo_contract_version: ActiveValue::Set(String::from("3.0.0")),
            code_hash: ActiveValue::Set(Some(vec![0; 32])),
            ..Default::default()
        })
        .exec_with_returning(db)
        .await
        .expect("unable to insert build session");

        let file = file::Entity::insert(file::ActiveModel {
            source_code_id: ActiveValue::Set(source_code_id),
            name: ActiveValue::Set(String::from("test.rs")),
            text: ActiveValue::Set(String::from("fn main() {}")),
            ..Default::default()
        })
        .exec_with_returning(db)
        .await
        .expect("unable to insert file");

        diagnostic::Entity::insert(diagnostic::ActiveModel {
            build_session_id: ActiveValue::Set(build_session.id),
            file_id: ActiveValue::Set(file.id),
            level: ActiveValue::Set(diagnostic::Level::Error),
            start: ActiveValue::Set(0),
            end: ActiveValue::Set(1),
            message: ActiveValue::Set(String::from("test")),
            ..Default::default()
        })
        .exec_with_returning(db)
        .await
        .expect("unable to insert diagnostic");

        diagnostic::Entity::insert(diagnostic::ActiveModel {
            build_session_id: ActiveValue::Set(build_session.id),
            file_id: ActiveValue::Set(file.id),
            level: ActiveValue::Set(diagnostic::Level::Warning),
            start: ActiveValue::Set(2),
            end: ActiveValue::Set(3),
            message: ActiveValue::Set(String::from("test2")),
            ..Default::default()
        })
        .exec_with_returning(db)
        .await
        .expect("unable to insert diagnostic");
    }

    #[tokio::test]
    async fn successful() {
        let db = create_database().await;

        create_test_env(&db).await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()))
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/buildSessions/diagnostics/1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_json!(response.json().await,
            [
                {
                    "level": "error",
                    "end": 1,
                    "start": 0,
                    "message": "test"
                },
                {
                    "level": "warning",
                    "end": 3,
                    "start": 2,
                    "message": "test2"
                }
            ]
        );
    }

    #[tokio::test]
    async fn unknown() {
        let db = create_database().await;

        create_test_env(&db).await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()))
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/buildSessions/diagnostics/2",))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;

        assert_eq!(404, response.unwrap().status());
    }
}
