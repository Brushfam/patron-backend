use std::sync::Arc;

use aide::{transform::TransformOperation, OperationIo};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use axum_derive_error::ErrorResponse;
use db::{code, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QuerySelect};
use derive_more::{Display, Error, From};
use serde_json::Value;

use crate::{hex_hash::HexHash, schema::example_error};

/// Errors that may occur during the WASM blob request handling.
#[derive(ErrorResponse, Display, From, Error, OperationIo)]
#[aide(output)]
pub(super) enum BuildSessionWasmError {
    /// Database-related error.
    DatabaseError(DbErr),

    /// The provided code hash doesn't have any WASM blobs saved in the database.
    #[status(StatusCode::NOT_FOUND)]
    #[display(fmt = "build session not found")]
    BuildSessionNotFound,
}

/// Generate OAPI documentation for the [`wasm`] handler.
pub(super) fn docs(op: TransformOperation) -> TransformOperation {
    op.summary("Get WASM blob of the latest build session.")
        .response::<200, Vec<u8>>()
        .response_with::<404, Json<Value>, _>(|op| {
            op.description("No build sessions with the provided code hash were found.")
                .example(example_error(BuildSessionWasmError::BuildSessionNotFound))
        })
}

/// WASM blob request handler.
pub(super) async fn wasm(
    Path(code_hash): Path<HexHash>,
    State(db): State<Arc<DatabaseConnection>>,
) -> Result<Vec<u8>, BuildSessionWasmError> {
    let wasm = code::Entity::find()
        .select_only()
        .column(code::Column::Code)
        .filter(code::Column::Hash.eq(&code_hash.0[..]))
        .into_tuple::<Vec<u8>>()
        .one(&*db)
        .await?
        .ok_or(BuildSessionWasmError::BuildSessionNotFound)?;

    Ok(wasm)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::testing::{create_database, ResponseBodyExt};

    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use common::config::Config;
    use db::{code, ActiveValue, DatabaseConnection, EntityTrait};
    use tower::ServiceExt;

    async fn create_test_code(db: &DatabaseConnection) {
        code::Entity::insert(code::ActiveModel {
            hash: ActiveValue::Set(vec![0; 32]),
            code: ActiveValue::Set(vec![1, 2, 3]),
        })
        .exec_without_returning(db)
        .await
        .expect("unable to insert code");
    }

    #[tokio::test]
    async fn successful() {
        let db = create_database().await;

        create_test_code(&db).await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()))
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/buildSessions/wasm/{}", hex::encode([0; 32])))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.bytes().await, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn unknown() {
        let db: DatabaseConnection = create_database().await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()))
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/buildSessions/wasm/{}", hex::encode([0; 32])))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND)
    }
}
