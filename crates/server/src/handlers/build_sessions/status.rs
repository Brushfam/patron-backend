use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use axum_derive_error::ErrorResponse;
use db::{build_session, DatabaseConnection, DbErr, EntityTrait, QuerySelect};
use derive_more::{Display, Error, From};
use serde::Serialize;

#[derive(ErrorResponse, Display, From, Error)]
pub(super) enum BuildSessionStatusError {
    DatabaseError(DbErr),

    #[status(StatusCode::NOT_FOUND)]
    #[display(fmt = "build session not found")]
    BuildSessionNotFound,
}

#[derive(Serialize)]
pub(super) struct BuildSessionStatusResponse {
    status: build_session::Status,
    code_hash: Option<String>,
}

pub(super) async fn status(
    Path(id): Path<i64>,
    State(db): State<Arc<DatabaseConnection>>,
) -> Result<Json<BuildSessionStatusResponse>, BuildSessionStatusError> {
    let (status, code_hash) = build_session::Entity::find_by_id(id)
        .select_only()
        .columns([
            build_session::Column::Status,
            build_session::Column::CodeHash,
        ])
        .into_tuple::<(build_session::Status, Option<Vec<u8>>)>()
        .one(&*db)
        .await?
        .ok_or(BuildSessionStatusError::BuildSessionNotFound)?;

    Ok(Json(BuildSessionStatusResponse {
        status,
        code_hash: code_hash.map(hex::encode),
    }))
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

    async fn create_test_env(db: &DatabaseConnection) -> i64 {
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
            ..Default::default()
        })
        .exec_with_returning(db)
        .await
        .expect("unable to insert build session")
        .id
    }

    #[tokio::test]
    async fn successful() {
        let db = create_database().await;

        let build_session_id = create_test_env(&db).await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::new().unwrap()))
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/buildSessions/status/{}", build_session_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_json!(response.json().await, {
            "status": "completed",
            "code_hash": hex::encode([0; 32])
        });
    }

    #[tokio::test]
    async fn unknown() {
        let db = create_database().await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::new().unwrap()))
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/buildSessions/status/1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
