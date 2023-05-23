use std::sync::Arc;

use axum::extract::{Path, State};
use axum_derive_error::ErrorResponse;
use db::{
    build_session_token, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter,
    TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};

#[derive(ErrorResponse, Display, From, Error)]
pub(super) enum SealError {
    DatabaseError(DbErr),
}

pub(super) async fn seal(
    State(db): State<Arc<DatabaseConnection>>,
    Path(token): Path<String>,
) -> Result<(), SealError> {
    db.transaction(|txn| {
        Box::pin(async move {
            build_session_token::Entity::delete_many()
                .filter(build_session_token::Column::Token.eq(token))
                .exec(txn)
                .await?;

            Ok(())
        })
    })
    .await
    .into_raw_result()
}
