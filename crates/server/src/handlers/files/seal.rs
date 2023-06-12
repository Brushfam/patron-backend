use std::sync::Arc;

use axum::extract::{Path, State};
use axum_derive_error::ErrorResponse;
use db::{
    build_session_token, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter,
    TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};

/// Errors that may occur during the file upload sealing process.
#[derive(ErrorResponse, Display, From, Error)]
pub(super) enum SealError {
    /// Database-related error.
    DatabaseError(DbErr),
}

/// Seal the provided build session token to prevent further file uploads.
///
/// After executing this route no additional files can be uploaded with the provided
/// build session token, preventing any modifications from custom scripts that user may execute
/// during the build process.
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
