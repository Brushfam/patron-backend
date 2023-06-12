use std::sync::Arc;

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

use crate::hex_hash::HexHash;

/// Errors that may occur during the detail preview process.
#[derive(ErrorResponse, Display, From, Error)]
pub(super) enum BuildSessionDetailsError {
    /// Database-related error.
    DatabaseError(DbErr),

    /// Requested build session was not found.
    #[status(StatusCode::NOT_FOUND)]
    #[display(fmt = "build session not found")]
    BuildSessionNotFound,
}

/// Build session details handler.
///
/// This route is suitable to acquire the information on tooling
/// versions used during the smart contract build process.
pub(super) async fn details(
    Path(code_hash): Path<HexHash>,
    State(db): State<Arc<DatabaseConnection>>,
) -> Result<Json<build_session::BuildSessionInfo>, BuildSessionDetailsError> {
    let model = build_session::Entity::find()
        .select_only()
        .columns([
            build_session::Column::SourceCodeId,
            build_session::Column::CargoContractVersion,
            build_session::Column::RustcVersion,
        ])
        .filter(build_session::Column::CodeHash.eq(&code_hash.0[..]))
        .order_by_desc(build_session::Column::CreatedAt)
        .into_model()
        .one(&*db)
        .await?
        .ok_or(BuildSessionDetailsError::BuildSessionNotFound)?;

    Ok(Json(model))
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
            archive_hash: ActiveValue::Set(Vec::new()),
            ..Default::default()
        })
        .exec_with_returning(db)
        .await
        .expect("unable to create source code")
        .id;

        build_session::Entity::insert(build_session::ActiveModel {
            user_id: ActiveValue::Set(Some(user.id)),
            source_code_id: ActiveValue::Set(source_code_id),
            status: ActiveValue::Set(build_session::Status::New),
            cargo_contract_version: ActiveValue::Set(String::from("3.0.0")),
            rustc_version: ActiveValue::Set(String::from("1.69.0")),
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
                    .uri(format!("/buildSessions/details/{}", hex::encode([0; 32])))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_json!(response.json().await, {
            "source_code_id": 1,
            "rustc_version": "1.69.0",
            "cargo_contract_version": "3.0.0"
        });
    }

    #[tokio::test]
    async fn unknown() {
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
