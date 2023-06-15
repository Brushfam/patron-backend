use std::sync::Arc;

use aide::{transform::TransformOperation, OperationIo};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use axum_derive_error::ErrorResponse;
use db::{
    build_session, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use derive_more::{Display, Error, From};
use serde_json::Value;

use crate::{hex_hash::HexHash, schema::example_error};

/// Errors that may occur during the contract metadata request.
#[derive(ErrorResponse, Display, From, Error, OperationIo)]
#[aide(output)]
pub(super) enum BuildSessionMetadataError {
    /// Database-related error.
    DatabaseError(DbErr),

    /// Unable to parse the metadata stored inside of a database as a JSON value.
    #[display(fmt = "invalid metadata")]
    InvalidMetadata,

    /// Unable to find the requested build session.
    #[status(StatusCode::NOT_FOUND)]
    #[display(fmt = "build session not found")]
    BuildSessionNotFound,
}

/// Generate OAPI documentation for the [`metadata`] handler.
pub(super) fn docs(op: TransformOperation) -> TransformOperation {
    op.summary("Get JSON metadata of the latest build session.")
        .response_with::<200, Json<Value>, _>(|op| {
            op.description("JSON metadata response.")
                .example(Value::Object(Default::default()))
        })
        .response_with::<404, Json<Value>, _>(|op| {
            op.description("No build sessions with the provided code hash were found.")
                .example(example_error(
                    BuildSessionMetadataError::BuildSessionNotFound,
                ))
        })
}

/// Contract metadata request handler.
pub(super) async fn metadata(
    Path(code_hash): Path<HexHash>,
    State(db): State<Arc<DatabaseConnection>>,
) -> Result<Json<serde_json::Value>, BuildSessionMetadataError> {
    let model = build_session::Entity::find()
        .select_only()
        .column(build_session::Column::Metadata)
        .filter(build_session::Column::CodeHash.eq(&code_hash.0[..]))
        .filter(build_session::Column::Metadata.is_not_null())
        .order_by_desc(build_session::Column::CreatedAt)
        .into_tuple::<Vec<u8>>()
        .one(&*db)
        .await?
        .ok_or(BuildSessionMetadataError::BuildSessionNotFound)?;

    let json =
        serde_json::from_slice(&model).map_err(|_| BuildSessionMetadataError::InvalidMetadata)?;

    Ok(Json(json))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::testing::{create_database, ResponseBodyExt};

    use assert_json::assert_json;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use common::config::Config;
    use db::{build_session, source_code, user, ActiveValue, DatabaseConnection, EntityTrait};
    use serde_json::json;
    use tower::ServiceExt;

    async fn create_test_env(db: &DatabaseConnection) {
        let user = user::Entity::insert(user::ActiveModel::default())
            .exec_with_returning(db)
            .await
            .expect("unable to create user");

        let source_code_id = source_code::Entity::insert(source_code::ActiveModel {
            user_id: ActiveValue::Set(Some(user.id)),
            archive_hash: ActiveValue::Set(vec![0; 32]),
            ..Default::default()
        })
        .exec_with_returning(db)
        .await
        .expect("unable to create source code")
        .id;

        build_session::Entity::insert(build_session::ActiveModel {
            user_id: ActiveValue::Set(Some(user.id)),
            source_code_id: ActiveValue::Set(source_code_id),
            status: ActiveValue::Set(build_session::Status::Completed),
            cargo_contract_version: ActiveValue::Set(String::from("3.0.0")),
            rustc_version: ActiveValue::Set(String::from("1.69.0")),
            code_hash: ActiveValue::Set(Some(vec![0; 32])),
            metadata: ActiveValue::Set(Some(
                serde_json::to_vec(&json! ({
                    "val": 123
                }))
                .unwrap(),
            )),
            ..Default::default()
        })
        .exec_without_returning(db)
        .await
        .expect("unable to insert build session");
    }

    #[tokio::test]
    async fn successful() {
        let db = create_database().await;

        create_test_env(&db).await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()))
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/buildSessions/metadata/{}", hex::encode([0; 32])))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_json!(response.json().await, {
            "val": 123
        });
    }

    #[tokio::test]
    async fn unknown() {
        let db = create_database().await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()))
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/buildSessions/metadata/{}", hex::encode([0; 32])))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
