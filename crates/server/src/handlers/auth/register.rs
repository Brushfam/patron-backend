use std::sync::Arc;

use aide::{transform::TransformOperation, OperationIo};
use axum::{extract::State, Json};
use axum_derive_error::ErrorResponse;
use db::{
    token, user, DatabaseConnection, DbErr, EntityTrait, TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};
use schemars::JsonSchema;
use serde::Serialize;

/// Errors that may occur during the user registration process.
#[derive(ErrorResponse, Display, From, Error, OperationIo)]
#[aide(output)]
pub(super) enum UserRegistrationError {
    /// Database-related error.
    DatabaseError(DbErr),
}

/// Registered user's authentication token response.
#[derive(Serialize, JsonSchema)]
pub(super) struct UserRegistrationResponse {
    /// Authentication token.
    #[schemars(example = "crate::schema::example_token")]
    token: String,
}

/// Generate OAPI documentation for the [`register`] handler.
pub(super) fn docs(op: TransformOperation) -> TransformOperation {
    op.summary("Register new user.")
        .description(
            r#"This route does not request any data from a client,
thus registering user immediately. Be aware, that a newly registered user does not
have any public keys attached to their account, meaning that you have to attach one
as soon as possible to ensure that a user account does not get lost."#,
        )
        .response::<200, Json<UserRegistrationResponse>>()
}

/// User registration handler.
///
/// This route will return an authentication token for a newly registered
/// users to provide an ability to verify a public key for an account.
pub(super) async fn register(
    State(db): State<Arc<DatabaseConnection>>,
) -> Result<Json<UserRegistrationResponse>, UserRegistrationError> {
    db.transaction(|txn| {
        Box::pin(async move {
            let user =
                user::Entity::insert(<db::user::ActiveModel as std::default::Default>::default())
                    .exec_with_returning(txn)
                    .await?;

            let (model, token) = token::generate_token(user.id);

            token::Entity::insert(model)
                .exec_without_returning(txn)
                .await?;

            Ok(Json(UserRegistrationResponse { token }))
        })
    })
    .await
    .into_raw_result()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::testing::{create_database, ResponseBodyExt};

    use assert_json::{assert_json, validators};
    use axum::{body::Body, http::Request};
    use common::config::Config;
    use db::token::TOKEN_LENGTH;
    use tower::ServiceExt;

    #[tokio::test]
    async fn register() {
        let db = create_database().await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/register")
                    .body(Body::empty())
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
}
