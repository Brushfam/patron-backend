use std::sync::Arc;

use aide::{transform::TransformOperation, OperationIo};
use axum::{extract::State, http::StatusCode, Json};
use axum_derive_error::ErrorResponse;
use db::{
    cli_token, token, DatabaseConnection, DbErr, EntityTrait, TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use validator::Validate;

use crate::{schema::example_error, validation::ValidatedJson};

/// Errors related to the token exchange.
#[derive(ErrorResponse, Display, From, Error, OperationIo)]
#[aide(output)]
pub(super) enum ExchangeTokenError {
    /// Database-related error.
    DatabaseError(DbErr),

    /// Invalid CLI token was submitted.
    #[status(StatusCode::NOT_FOUND)]
    #[display(fmt = "provided CLI token was not found")]
    TokenNotFound,
}

/// Token exchange request.
#[derive(Deserialize, Validate, JsonSchema)]
pub(super) struct ExchangeTokenRequest {
    /// User-generated CLI token.
    #[validate(length(equal = "db::cli_token::TOKEN_LENGTH"))]
    #[schemars(example = "crate::schema::example_token")]
    cli_token: String,
}

/// Successful token exchange.
#[derive(Serialize, JsonSchema)]
pub(super) struct ExchangeTokenResponse {
    /// Authentication token.
    #[schemars(example = "crate::schema::example_token")]
    token: String,
}

/// Generate OAPI documentation for the [`exchange`] handler.
pub(super) fn docs(op: TransformOperation) -> TransformOperation {
    op.summary("Exchange CLI token for an authentication one.")
        .description(
            r#"This route is periodically invoked by the CLI during the authentication flow
to exchange a locally-generated token for an authentication one, which
can be used to authenticate with any other route later."#,
        )
        .response::<200, Json<ExchangeTokenResponse>>()
        .response_with::<404, Json<Value>, _>(|op| {
            op.description("Invalid CLI token.")
                .example(example_error(ExchangeTokenError::TokenNotFound))
        })
}

/// CLI token exchange handler.
///
/// This handler will exchange the token provided by the CLI
/// for an authentication one if user previously finished an authentication
/// flow with the same CLI token.
pub(super) async fn exchange(
    State(db): State<Arc<DatabaseConnection>>,
    ValidatedJson(request): ValidatedJson<ExchangeTokenRequest>,
) -> Result<Json<ExchangeTokenResponse>, ExchangeTokenError> {
    db.transaction(|txn| {
        Box::pin(async move {
            let (cli_token_model, token_model) = cli_token::Entity::find_by_id(request.cli_token)
                .find_also_related(token::Entity)
                .one(txn)
                .await?
                .ok_or(ExchangeTokenError::TokenNotFound)?;

            let token_model = token_model.ok_or(ExchangeTokenError::TokenNotFound)?;

            cli_token::Entity::delete(cli_token::ActiveModel::from(cli_token_model))
                .exec(txn)
                .await?;

            Ok(Json(ExchangeTokenResponse {
                token: token_model.token,
            }))
        })
    })
    .await
    .into_raw_result()
}
