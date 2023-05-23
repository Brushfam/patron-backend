use std::sync::Arc;

use axum::{
    extract::State,
    headers::{authorization::Bearer, Authorization},
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    TypedHeader,
};
use axum_derive_error::ErrorResponse;
use common::config::Config;
use db::{
    public_key, token, user, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter,
    QuerySelect, SelectExt, TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AuthenticatedUserId(i64);

impl AuthenticatedUserId {
    /// Get raw user identifier value.
    pub fn id(&self) -> i64 {
        self.0
    }
}

#[derive(ErrorResponse, Display, From, Error)]
pub(super) enum AuthenticationError {
    DatabaseError(DbErr),

    #[status(StatusCode::FORBIDDEN)]
    #[display(fmt = "invalid authentication token was provided")]
    InvalidAuthenticationToken,

    #[status(StatusCode::FORBIDDEN)]
    #[display(fmt = "at least one verified key is required to access")]
    MissingKeys,

    #[status(StatusCode::FORBIDDEN)]
    #[display(fmt = "paid membership is required to access")]
    PaymentRequired,
}

pub(super) async fn require_authentication<
    const REQUIRE_VERIFIED_KEY: bool,
    const REQUIRE_PAYMENT: bool,
    B,
>(
    State((db, config)): State<(Arc<DatabaseConnection>, Arc<Config>)>,
    TypedHeader(authorization): TypedHeader<Authorization<Bearer>>,
    mut req: Request<B>,
    next: Next<B>,
) -> Result<Response, AuthenticationError> {
    let user_id = db
        .transaction::<_, _, AuthenticationError>(|txn| {
            Box::pin(async move {
                let bearer = authorization.token();

                let user_id: i64 = token::Entity::find()
                    .select_only()
                    .column(token::Column::UserId)
                    .filter(token::Column::Token.eq(bearer))
                    .into_tuple()
                    .one(txn)
                    .await?
                    .ok_or(AuthenticationError::InvalidAuthenticationToken)?;

                if REQUIRE_VERIFIED_KEY {
                    let has_verified_keys = public_key::Entity::find()
                        .select_only()
                        .filter(public_key::Column::UserId.eq(user_id))
                        .exists(txn)
                        .await?;

                    if !has_verified_keys {
                        return Err(AuthenticationError::MissingKeys);
                    }
                }

                if REQUIRE_PAYMENT && config.payments {
                    let paid = user::Entity::find_by_id(user_id)
                        .select_only()
                        .filter(user::Column::Paid.eq(true))
                        .exists(txn)
                        .await?;

                    if !paid {
                        return Err(AuthenticationError::PaymentRequired);
                    }
                }

                Ok(user_id)
            })
        })
        .await
        .into_raw_result()?;

    req.extensions_mut().insert(AuthenticatedUserId(user_id));

    Ok(next.run(req).await)
}
