use std::sync::Arc;

use aide::{transform::TransformOperation, OperationIo};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use axum_derive_error::ErrorResponse;
use common::rpc::sp_core::{
    sr25519::{Pair, Public, Signature},
    Pair as _,
};
use db::{
    cli_token, public_key, sea_query::OnConflict, token, ActiveValue, ColumnTrait,
    DatabaseConnection, DbErr, EntityTrait, QueryFilter, QuerySelect, TransactionErrorExt,
    TransactionTrait,
};
use derive_more::{Display, Error, From};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::schema::example_error;

/// Errors that may occur during the authentication process.
#[derive(ErrorResponse, Display, From, Error, OperationIo)]
#[aide(output)]
pub(super) enum UserAuthenticationError {
    /// Database-related error.
    DatabaseError(DbErr),

    /// An invalid signature was submitted by user.
    #[status(StatusCode::UNPROCESSABLE_ENTITY)]
    #[display(fmt = "invalid signature")]
    InvalidSignature,

    /// Provided key doesn't have any related account.
    // OK is used here to allow web app to interact more simply.
    #[status(StatusCode::OK)]
    #[display(fmt = "no related account was found")]
    NoRelatedAccounts,
}

/// Query string deserialization struct for an optional CLI token.
#[derive(Deserialize, JsonSchema)]
pub(super) struct UserAuthenticationQuery {
    /// User-generated CLI token.
    #[serde(default)]
    #[schemars(example = "crate::schema::example_token")]
    cli_token: Option<String>,
}

/// Authentication request.
#[derive(Deserialize, JsonSchema)]
pub(super) struct UserAuthenticationRequest {
    /// Public key used to authenticate.
    #[schemars(example = "crate::schema::example_public_key", with = "String")]
    account: Public,

    /// Message signed with the provided public key for verification.
    ///
    /// Verification message consists of
    /// a string equal to the account address
    /// used for verification purposes.
    ///
    /// Example: `<Bytes>5FeLhJAs4CUHqpWmPDBLeL7NLAoHsB2ZuFZ5Mk62EgYemtFj</Bytes>`
    #[schemars(example = "crate::schema::example_signature", with = "String")]
    signature: Signature,
}

/// Conditional successful token exchange.
#[derive(Serialize, JsonSchema)]
#[serde(untagged)]
pub(super) enum UserAuthenticationResponse {
    /// Web UI authentication flow.
    Web {
        /// Authentication token.
        #[schemars(example = "crate::schema::example_token")]
        token: String,
    },

    /// CLI authentication flow.
    ///
    /// Authentication token is not provided here,
    /// as the CLI has to request it in a separate
    /// query to the `exchange` route.
    Cli,
}

/// Generate OAPI documentation for the [`login`] handler.
pub(super) fn docs(op: TransformOperation) -> TransformOperation {
    op.summary("Create new authentication token.")
        .description(
            r#"Use provided credentials to authenticate user and create
a new authentication token.

Provided credentials are validated to ensure that the provided signature
belongs to the provided public key.

This route returns different responses depending on the flow you want to use.
Regular authentication flow returns an authentication token from this route
as soon as the signature validation is successful. CLI authentication flow
does not return anything from this route, relying on client calling an `/auth/exchange` route.
To proceed with the CLI authentication flow, pass `cli_token` value as specified
in the query string documentation."#,
        )
        .response::<200, Json<UserAuthenticationResponse>>()
        .response_with::<422, Json<Value>, _>(|op| {
            op.description("The provided signature is invalid.")
                .example(example_error(UserAuthenticationError::InvalidSignature))
        })
}

/// User authentication handler.
///
/// This handler will accept a verified key
/// and return an authentication token for the relevant user.
pub(super) async fn login(
    State(db): State<Arc<DatabaseConnection>>,
    Query(query): Query<UserAuthenticationQuery>,
    Json(request): Json<UserAuthenticationRequest>,
) -> Result<Json<UserAuthenticationResponse>, UserAuthenticationError> {
    db.transaction(|txn| {
        Box::pin(async move {
            let user_id: i64 = public_key::Entity::find()
                .select_only()
                .column(public_key::Column::UserId)
                .filter(public_key::Column::Address.eq(&request.account.0[..]))
                .into_tuple()
                .one(txn)
                .await?
                .ok_or(UserAuthenticationError::NoRelatedAccounts)?;

            if Pair::verify(
                &request.signature,
                format!("<Bytes>{}</Bytes>", &request.account),
                &request.account,
            ) {
                let (active_model, token) = token::generate_token(user_id);

                let model = token::Entity::insert(active_model)
                    .exec_with_returning(txn)
                    .await?;

                let response = if let Some(token) = query.cli_token {
                    cli_token::Entity::insert(cli_token::ActiveModel {
                        token: ActiveValue::Set(token),
                        authentication_token_id: ActiveValue::Set(model.id),
                    })
                    .on_conflict(
                        OnConflict::column(cli_token::Column::Token)
                            .do_nothing()
                            .to_owned(),
                    )
                    .exec_without_returning(txn)
                    .await?;

                    UserAuthenticationResponse::Cli
                } else {
                    UserAuthenticationResponse::Web { token }
                };

                Ok(Json(response))
            } else {
                Err(UserAuthenticationError::InvalidSignature)
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
    use common::{
        config::Config,
        rpc::sp_core::crypto::{AccountId32, Ss58Codec},
    };
    use db::{
        cli_token, public_key, token::TOKEN_LENGTH, user, ActiveValue, DatabaseConnection,
        EntityTrait,
    };
    use rand::{
        distributions::{Alphanumeric, DistString},
        thread_rng,
    };
    use serde_json::json;
    use tower::{Service, ServiceExt};

    const ACCOUNT_ID: &str = "5FeLhJAs4CUHqpWmPDBLeL7NLAoHsB2ZuFZ5Mk62EgYemtFj";

    async fn create_test_account(db: &DatabaseConnection) {
        let user = user::Entity::insert(user::ActiveModel::default())
            .exec_with_returning(db)
            .await
            .expect("unable to create user");

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
    }

    #[tokio::test]
    async fn successful() {
        let db = create_database().await;

        create_test_account(&db).await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/login")
                    .header("Content-Type", "application/json")
                    .body(Body::from_json(json!({
                        "account": ACCOUNT_ID,
                        "signature": "0x6aa1134d5082aae91dc710cf70d79d2abf6c261cc58eeb13d25ef4dfc8eeed54de76e49f186cde3efd41f6008598ab8d895c78b4354f26e868ead1d8e6410d8a"
                    })))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_json!(response.json().await, {
            "token": validators::string(|val| {
                (val.len() == TOKEN_LENGTH)
                    .then_some(())
                    .ok_or(String::from("invalid length"))
            })
        });
    }

    #[tokio::test]
    async fn invalid_account() {
        let db = create_database().await;

        create_test_account(&db).await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/login")
                    .header("Content-Type", "application/json")
                    .body(Body::from_json(json!({
                        "account": "123",
                        "signature": "0x6aa1134d5082aae91dc710cf70d79d2abf6c261cc58eeb13d25ef4dfc8eeed54de76e49f186cde3efd41f6008598ab8d895c78b4354f26e868ead1d8e6410d8a"
                    })))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn invalid_signature() {
        let db = create_database().await;

        create_test_account(&db).await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/login")
                    .header("Content-Type", "application/json")
                    .body(Body::from_json(json!({
                        "account": ACCOUNT_ID,
                        "signature": "123"
                    })))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn unmatching_signature() {
        let db = create_database().await;

        create_test_account(&db).await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/login")
                    .header("Content-Type", "application/json")
                    .body(Body::from_json(json!({
                        "account": ACCOUNT_ID,
                        "signature": "0x6aa1134d5082aae91dc710cf70d79d2abf6c261cc58eeb13d25ef4dfc8eeed54de76e49f186cde3efd41f6008598ab8d895c78b4354f26e868ead1d8e6410d8b"
                    })))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn missing_account() {
        let db = create_database().await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/login")
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
    }

    #[tokio::test]
    async fn exchange() {
        let db = create_database().await;

        create_test_account(&db).await;

        let cli_token = Alphanumeric.sample_string(&mut thread_rng(), cli_token::TOKEN_LENGTH);

        let mut service = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()));

        let login_response = service
            .call(
                Request::builder()
                    .method("POST")
                    .uri(format!("/auth/login?cli_token={cli_token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from_json(json!({
                        "account": ACCOUNT_ID,
                        "signature": "0x6aa1134d5082aae91dc710cf70d79d2abf6c261cc58eeb13d25ef4dfc8eeed54de76e49f186cde3efd41f6008598ab8d895c78b4354f26e868ead1d8e6410d8a",
                    })))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(login_response.status(), StatusCode::OK);

        let exchange_response = service
            .call(
                Request::builder()
                    .method("POST")
                    .uri("/auth/exchange")
                    .header("Content-Type", "application/json")
                    .body(Body::from_json(json!({ "cli_token": &cli_token })))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_json!(exchange_response.json().await, {
            "token": validators::string(|val| {
                (val.len() == TOKEN_LENGTH)
                    .then_some(())
                    .ok_or(String::from("invalid length"))
            })
        });
    }

    #[tokio::test]
    async fn cli_token_repetition() {
        let db = create_database().await;

        create_test_account(&db).await;

        let cli_token = Alphanumeric.sample_string(&mut thread_rng(), cli_token::TOKEN_LENGTH);

        let mut service = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()));

        let login_response = service
            .call(
                Request::builder()
                    .method("POST")
                    .uri(format!("/auth/login?cli_token={cli_token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from_json(json!({
                        "account": ACCOUNT_ID,
                        "signature": "0x6aa1134d5082aae91dc710cf70d79d2abf6c261cc58eeb13d25ef4dfc8eeed54de76e49f186cde3efd41f6008598ab8d895c78b4354f26e868ead1d8e6410d8a",
                    })))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(login_response.status(), StatusCode::OK);

        let login_response = service
            .call(
                Request::builder()
                    .method("POST")
                    .uri(format!("/auth/login?cli_token={cli_token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from_json(json!({
                        "account": ACCOUNT_ID,
                        "signature": "0x6aa1134d5082aae91dc710cf70d79d2abf6c261cc58eeb13d25ef4dfc8eeed54de76e49f186cde3efd41f6008598ab8d895c78b4354f26e868ead1d8e6410d8a",
                    })))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(login_response.status(), StatusCode::OK);
    }
}
