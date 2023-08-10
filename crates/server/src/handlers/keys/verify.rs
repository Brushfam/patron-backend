use std::sync::Arc;

use aide::{transform::TransformOperation, OperationIo};
use axum::{extract::State, http::StatusCode, Extension, Json};
use axum_derive_error::ErrorResponse;
use common::rpc::sp_core::{
    sr25519::{Pair, Public, Signature},
    Pair as _,
};
use db::{
    public_key, user, ActiveValue, ColumnTrait, DatabaseConnection, DbErr, EntityTrait,
    QueryFilter, QuerySelect, SelectExt, TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::{auth::AuthenticatedUserId, schema::example_error};

/// Errors that may occur during the public key verification process.
#[derive(ErrorResponse, Display, From, Error, OperationIo)]
#[aide(output)]
pub(super) enum PublicKeyVerificationError {
    /// Database-related error.
    DatabaseError(DbErr),

    /// The provided public key is already in use by this or another account.
    #[status(StatusCode::FORBIDDEN)]
    #[display(fmt = "account already exists")]
    AccountExists,

    /// User provided an invalid signature.
    #[status(StatusCode::UNPROCESSABLE_ENTITY)]
    #[display(fmt = "invalid signature")]
    InvalidSignature,
}

/// JSON request body.
#[derive(Deserialize, JsonSchema)]
pub(super) struct PublicKeyVerificationRequest {
    /// Public key text value.
    #[schemars(example = "crate::schema::example_public_key", with = "String")]
    account: Public,

    /// Signed verification message.
    ///
    /// Verification message consists of
    /// a string equal to the account address
    /// used for verification purposes.
    ///
    /// Example: `<Bytes>5FeLhJAs4CUHqpWmPDBLeL7NLAoHsB2ZuFZ5Mk62EgYemtFj</Bytes>`
    #[schemars(example = "crate::schema::example_signature", with = "String")]
    signature: Signature,
}

/// Generate OAPI documentation for the [`docs`] handler.
pub(super) fn docs(op: TransformOperation) -> TransformOperation {
    op.summary("Verify a new public key.")
        .response::<200, ()>()
        .response_with::<403, Json<Value>, _>(|op| {
            op.description("The provided public key is already attached.")
                .example(example_error(PublicKeyVerificationError::AccountExists))
        })
        .response_with::<422, Json<Value>, _>(|op| {
            op.description("An invalid signature was provided.")
                .example(example_error(PublicKeyVerificationError::InvalidSignature))
        })
}

/// Verify a public key and attach it to the current authenticated user's account on success.
///
/// For more information on the format used for verification
/// signature see [`PublicKeyVerificationRequest`].
pub(super) async fn verify(
    Extension(current_user): Extension<AuthenticatedUserId>,
    State(db): State<Arc<DatabaseConnection>>,
    Json(request): Json<PublicKeyVerificationRequest>,
) -> Result<(), PublicKeyVerificationError> {
    if Pair::verify(
        &request.signature,
        format!("<Bytes>{}</Bytes>", &request.account),
        &request.account,
    ) {
        db.transaction(|txn| {
            Box::pin(async move {
                let user_exists = user::Entity::find_by_id(current_user.id())
                    .select_only()
                    .exists(txn)
                    .await?;

                let key_exists = public_key::Entity::find()
                    .select_only()
                    .filter(public_key::Column::Address.eq(&request.account.0[..]))
                    .exists(txn)
                    .await?;

                if user_exists && !key_exists {
                    public_key::Entity::insert(public_key::ActiveModel {
                        user_id: ActiveValue::Set(current_user.id()),
                        address: ActiveValue::Set(request.account.0.to_vec()),
                        ..Default::default()
                    })
                    .exec_without_returning(txn)
                    .await?;

                    Ok(())
                } else {
                    Err(PublicKeyVerificationError::AccountExists)
                }
            })
        })
        .await
        .into_raw_result()
    } else {
        Err(PublicKeyVerificationError::InvalidSignature)
    }
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
    use db::{token, user, DatabaseConnection, EntityTrait};
    use serde_json::json;
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

        token
    }

    #[tokio::test]
    async fn list_and_verify() {
        let db = create_database().await;

        let token = create_test_env(&db).await;

        let mut service = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()));

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

        let response = service
            .call(
                Request::builder()
                    .method("POST")
                    .uri("/keys")
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from_json(json!({
                        "account": ACCOUNT_ID,
                        "signature": "0x6aa1134d5082aae91dc710cf70d79d2abf6c261cc58eeb13d25ef4dfc8eeed54de76e49f186cde3efd41f6008598ab8d895c78b4354f26e868ead1d8e6410d8a"
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

        assert_json!(response.json().await, [
            {
                "id": 1,
                "address": ACCOUNT_ID
            }
        ]);
    }
}
