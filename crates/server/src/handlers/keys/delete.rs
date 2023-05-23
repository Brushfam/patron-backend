use std::sync::Arc;

use axum::{extract::State, Extension, Json};
use axum_derive_error::ErrorResponse;
use db::{
    public_key, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter,
    TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};
use serde::Deserialize;
use sp_core::sr25519::Public;

use crate::auth::AuthenticatedUserId;

#[derive(ErrorResponse, Display, From, Error)]
pub(super) enum PublicKeyDeletionError {
    DatabaseError(DbErr),
}

#[derive(Deserialize)]
pub(super) struct PublicKeyDeletionRequest {
    account: Public,
}

pub(super) async fn delete(
    Extension(current_user): Extension<AuthenticatedUserId>,
    State(db): State<Arc<DatabaseConnection>>,
    Json(request): Json<PublicKeyDeletionRequest>,
) -> Result<(), PublicKeyDeletionError> {
    db.transaction(|txn| {
        Box::pin(async move {
            public_key::Entity::delete_many()
                .filter(public_key::Column::UserId.eq(current_user.id()))
                .filter(public_key::Column::Address.eq(&request.account.0[..]))
                .exec(txn)
                .await?;

            Ok(())
        })
    })
    .await
    .into_raw_result()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::testing::{create_database, RequestBodyExt, ResponseBodyExt};

    use assert_json::assert_json;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use common::config::Config;
    use db::{public_key, token, user, ActiveValue, DatabaseConnection, EntityTrait};
    use serde_json::json;
    use sp_core::crypto::{AccountId32, Ss58Codec};
    use tower::Service;

    const ACCOUNT_ID: &str = "5FeLhJAs4CUHqpWmPDBLeL7NLAoHsB2ZuFZ5Mk62EgYemtFj";

    async fn create_test_env(db: &DatabaseConnection) -> String {
        let user = user::Entity::insert(user::ActiveModel::default())
            .exec_with_returning(db)
            .await
            .expect("unable to create user");

        let (model, token) = token::generate_token(user.id);

        token::Entity::insert(model)
            .exec_without_returning(db)
            .await
            .expect("unable to insert token");

        let account = AccountId32::from_ss58check(ACCOUNT_ID).unwrap();
        let account_buf: &[u8] = account.as_ref();

        public_key::Entity::insert(public_key::ActiveModel {
            user_id: ActiveValue::Set(user.id),
            address: ActiveValue::Set(account_buf.to_vec()),
            ..Default::default()
        })
        .exec_without_returning(db)
        .await
        .expect("unable to create public key");

        token
    }

    #[tokio::test]
    async fn list_and_delete() {
        let db = create_database().await;

        let token = create_test_env(&db).await;

        let mut service = crate::app_router(Arc::new(db), Arc::new(Config::new().unwrap()));

        let response = service
            .call(
                Request::builder()
                    .method("GET")
                    .uri("/keys")
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_json!(response.json().await, [
            {
                "id": 1,
                "address": ACCOUNT_ID
            }
        ]);

        let response = service
            .call(
                Request::builder()
                    .method("DELETE")
                    .uri("/keys")
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from_json(json!({
                        "account": ACCOUNT_ID,
                    })))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let response = service
            .call(
                Request::builder()
                    .method("GET")
                    .uri("/keys")
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_json!(response.json().await, []);
    }
}
