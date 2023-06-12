use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use axum_derive_error::ErrorResponse;
use db::{
    cli_token, token, DatabaseConnection, DbErr, EntityTrait, TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::validation::ValidatedJson;

/// Errors related to the token exchange.
#[derive(ErrorResponse, Display, From, Error)]
pub(super) enum ExchangeTokenError {
    /// Database-related error.
    DatabaseError(DbErr),

    /// Invalid CLI token was submitted.
    #[status(StatusCode::NOT_FOUND)]
    #[display(fmt = "provided CLI token was not found")]
    TokenNotFound,
}

/// JSON HTTP request.
#[derive(Deserialize, Validate)]
pub(super) struct ExchangeTokenRequest {
    /// User-generated CLI token.
    #[validate(length(equal = "db::cli_token::TOKEN_LENGTH"))]
    cli_token: String,
}

/// JSON HTTP response.
#[derive(Serialize)]
pub(super) struct ExchangeTokenResponse {
    /// Authentication token.
    token: String,
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
