use std::sync::Arc;

use aide::{transform::TransformOperation, OperationIo};
use axum::extract::{Path, State};
use axum_derive_error::ErrorResponse;
use db::{
    build_session_token, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter,
    TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};

/// Errors that may occur during the file upload sealing process.
#[derive(ErrorResponse, Display, From, Error, OperationIo)]
#[aide(output)]
pub(super) enum SealError {
    /// Database-related error.
    DatabaseError(DbErr),
}

/// Generate OAPI documentation for the [`seal`] handler.
pub(super) fn docs(op: TransformOperation) -> TransformOperation {
    op.summary("Seal the provided build session token.")
        .description(
            r#"Sealing the build session token prevents
any further file uploads from the build session container.
            
Make sure to always seal build session tokens
to protect the database from malicious file uploads within a build session container."#,
        )
        .response::<200, ()>()
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
