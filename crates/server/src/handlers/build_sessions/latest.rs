use std::{array::TryFromSliceError, sync::Arc};

use aide::{transform::TransformOperation, OperationIo};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use axum_derive_error::ErrorResponse;
use db::{
    build_session, source_code, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;

use crate::{hex_hash::HexHash, schema::example_error};

/// Code hash details.
#[derive(Serialize, JsonSchema)]
pub struct BuildSessionLatestData {
    /// Code hash corresponding to the provided source code archive hash.
    #[schemars(example = "crate::schema::example_hex_hash")]
    pub code_hash: HexHash,
}

/// Errors that may occur during the request handling.
#[derive(ErrorResponse, Display, From, Error, OperationIo)]
#[aide(output)]
pub(super) enum BuildSessionLatestError {
    /// Database-related error.
    DatabaseError(DbErr),

    /// Incorrect hash size stored inside of a database
    IncorrectArchiveHash(TryFromSliceError),

    /// Provided archive hash doesn't have any completed build sessions.
    #[status(StatusCode::NOT_FOUND)]
    #[display(fmt = "no related build sessions were found")]
    NoRelatedBuildSessions,
}

/// Generate OAPI documentation for the [`latest`] handler.
pub(super) fn docs(op: TransformOperation) -> TransformOperation {
    op.summary("Get the latest build session code hash.")
        .response::<200, Json<BuildSessionLatestData>>()
        .response_with::<404, Json<Value>, _>(|op| {
            op.description("No related build sessions were found.")
                .example(example_error(
                    BuildSessionLatestError::NoRelatedBuildSessions,
                ))
        })
}

/// Handler for getting the latest code hash that corresponds to the provided archive hash.
///
/// This handler searches only for successful build sessions, as code hashes are generated only for those.
pub(super) async fn latest(
    State(db): State<Arc<DatabaseConnection>>,
    Path(archive_hash): Path<HexHash>,
) -> Result<Json<BuildSessionLatestData>, BuildSessionLatestError> {
    db.transaction(|txn| {
        Box::pin(async move {
            let source_code_id = source_code::Entity::find()
                .select_only()
                .column(source_code::Column::Id)
                .filter(source_code::Column::ArchiveHash.eq(&archive_hash.0[..]))
                .into_tuple::<i64>()
                .one(txn)
                .await?
                .ok_or(BuildSessionLatestError::NoRelatedBuildSessions)?;

            let code_hash = build_session::Entity::find()
                .select_only()
                .column(build_session::Column::CodeHash)
                .filter(build_session::Column::CodeHash.is_not_null())
                .filter(build_session::Column::Status.eq(build_session::Status::Completed))
                .filter(build_session::Column::SourceCodeId.eq(source_code_id))
                .order_by_desc(build_session::Column::CreatedAt)
                .into_tuple::<Vec<u8>>()
                .one(txn)
                .await?
                .ok_or(BuildSessionLatestError::NoRelatedBuildSessions)?;

            Ok(Json(BuildSessionLatestData {
                code_hash: code_hash.as_slice().try_into()?,
            }))
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
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use common::config::Config;
    use db::{build_session, source_code, user, ActiveValue, DatabaseConnection, EntityTrait};
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

        source_code::Entity::insert(source_code::ActiveModel {
            user_id: ActiveValue::Set(Some(user.id)),
            archive_hash: ActiveValue::Set(vec![1; 32]),
            ..Default::default()
        })
        .exec_with_returning(db)
        .await
        .expect("unable to create source code");

        build_session::Entity::insert(build_session::ActiveModel {
            user_id: ActiveValue::Set(Some(user.id)),
            source_code_id: ActiveValue::Set(source_code_id),
            status: ActiveValue::Set(build_session::Status::Completed),
            cargo_contract_version: ActiveValue::Set(String::from("3.0.0")),
            code_hash: ActiveValue::Set(Some(vec![0; 32])),
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
                    .uri(format!("/buildSessions/latest/{}", hex::encode([0; 32])))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_json!(response.json().await, {
            "code_hash": hex::encode([0; 32]),
        });
    }

    #[tokio::test]
    async fn source_code_without_build_sessions() {
        let db = create_database().await;

        create_test_env(&db).await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()))
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/buildSessions/details/{}", hex::encode([1; 32])))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND)
    }
}
