use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use axum_derive_error::ErrorResponse;
use db::{
    event, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, PrimitiveDateTime, QueryFilter,
    QueryOrder, QuerySelect,
};
use derive_more::{Display, Error, From};
use futures_util::TryStreamExt;
use serde::Serialize;
use sp_core::{crypto::AccountId32, ByteArray};

/// Errors that may occur during the contract event list request handling.
#[derive(ErrorResponse, Display, From, Error)]
pub(super) enum ContractEventsError {
    /// Database-related error.
    DatabaseError(DbErr),
}

/// A single contract event.
#[derive(Serialize)]
pub struct ContractEvent {
    /// Serialized JSON body of a contract event.
    ///
    /// See [`db::event::EventBody`] for more information.
    body: String,

    /// Timestamp of a block in which the event was discovered.
    timestamp: i64,
}

/// Contract event list request handler.
pub(super) async fn events(
    Path(account): Path<AccountId32>,
    State(db): State<Arc<DatabaseConnection>>,
) -> Result<Json<Vec<ContractEvent>>, ContractEventsError> {
    let model = event::Entity::find()
        .select_only()
        .columns([event::Column::Body, event::Column::BlockTimestamp])
        .filter(event::Column::Account.eq(account.as_slice()))
        .order_by_desc(event::Column::BlockTimestamp)
        .limit(25)
        .into_tuple::<(String, PrimitiveDateTime)>()
        .stream(&*db)
        .await?
        .map_ok(|(body, date)| ContractEvent {
            body,
            timestamp: date.assume_utc().unix_timestamp(),
        })
        .try_collect()
        .await?;

    Ok(Json(model))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::testing::{create_database, ResponseBodyExt};

    use assert_json::assert_json;
    use axum::{body::Body, http::Request};
    use common::config::Config;
    use db::{
        code, contract, event, node, ActiveValue, DatabaseConnection, EntityTrait, OffsetDateTime,
        PrimitiveDateTime,
    };
    use sp_core::crypto::AccountId32;
    use tower::ServiceExt;

    async fn create_test_env(db: &DatabaseConnection) {
        let node = node::Entity::insert(node::ActiveModel {
            name: ActiveValue::Set(String::from("test")),
            url: ActiveValue::Set(String::from("ws://localhost:9944")),
            schema: ActiveValue::Set(String::from("test")),
            confirmed_block: ActiveValue::Set(0),
            ..Default::default()
        })
        .exec_with_returning(db)
        .await
        .expect("unable to insert node");

        code::Entity::insert(code::ActiveModel {
            hash: ActiveValue::Set(vec![0; 32]),
            code: ActiveValue::Set(vec![1, 2, 3]),
        })
        .exec_without_returning(db)
        .await
        .expect("unable to insert code");

        contract::Entity::insert(contract::ActiveModel {
            node_id: ActiveValue::Set(node.id),
            code_hash: ActiveValue::Set(vec![0; 32]),
            address: ActiveValue::Set(vec![1; 32]),
            owner: ActiveValue::Set(Some(vec![2; 32])),
            ..Default::default()
        })
        .exec_with_returning(db)
        .await
        .expect("unable to insert contract");

        let datetime = OffsetDateTime::from_unix_timestamp(0).expect("invalid date");

        event::Entity::insert(event::ActiveModel {
            node_id: ActiveValue::Set(node.id),
            account: ActiveValue::Set(vec![1; 32]),
            event_type: ActiveValue::Set(event::EventType::Instantiation),
            body: ActiveValue::Set(
                serde_json::to_string(&event::EventBody::Instantiation).unwrap(),
            ),
            block_timestamp: ActiveValue::Set(PrimitiveDateTime::new(
                datetime.date(),
                datetime.time(),
            )),
            ..Default::default()
        })
        .exec_without_returning(db)
        .await
        .expect("unable to insert an event");
    }

    #[tokio::test]
    async fn successful() {
        let db = create_database().await;

        create_test_env(&db).await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()))
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/contracts/events/{}", AccountId32::new([1; 32])))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_json!(response.json().await, [
            {
                "body": r#""Instantiation""#,
                "timestamp": 0
            }
        ])
    }

    #[tokio::test]
    async fn unknown() {
        let db = create_database().await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()))
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/contracts/events/{}", AccountId32::new([1; 32])))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_json!(response.json().await, [])
    }
}
