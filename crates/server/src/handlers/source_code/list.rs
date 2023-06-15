use std::{array::TryFromSliceError, sync::Arc};

use aide::{transform::TransformOperation, OperationIo};
use axum::{
    extract::{Query, State},
    Extension, Json,
};
use axum_derive_error::ErrorResponse;
use db::{
    source_code, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QuerySelect,
};
use derive_more::{Display, Error, From};
use futures_util::TryStreamExt;
use schemars::JsonSchema;
use serde::Serialize;

use crate::{auth::AuthenticatedUserId, hex_hash::HexHash, pagination::Pagination};

/// A single source code archive data.
#[derive(Serialize, JsonSchema)]
pub struct SourceCodeData {
    /// Source code identifier.
    #[schemars(example = "crate::schema::example_database_identifier")]
    pub id: i64,

    /// Blake2b256 hash of an uploaded archive.
    #[schemars(example = "crate::schema::example_hex_hash")]
    pub archive_hash: HexHash,
}

/// Errors that may occur during the list process.
#[derive(ErrorResponse, Display, From, Error, OperationIo)]
#[aide(output)]
pub(super) enum SourceCodeListError {
    /// Database-related error.
    DatabaseError(DbErr),

    /// Incorrect hash size stored inside of a database
    IncorrectArchiveHash(TryFromSliceError),
}

/// Generate OAPI documentation for the [`list`] handler.
pub(super) fn docs(op: TransformOperation) -> TransformOperation {
    op.summary("List source code archives uploaded by the current user.")
        .response_with::<200, Json<Vec<SourceCodeData>>, _>(|op| {
            op.description("Source code archive list response.")
        })
}

/// List source code archives related to the current authenticated user.
pub(super) async fn list(
    Extension(current_user): Extension<AuthenticatedUserId>,
    State(db): State<Arc<DatabaseConnection>>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<Vec<SourceCodeData>>, SourceCodeListError> {
    source_code::Entity::find()
        .select_only()
        .columns([source_code::Column::Id, source_code::Column::ArchiveHash])
        .filter(source_code::Column::UserId.eq(current_user.id()))
        .limit(pagination.limit())
        .offset(pagination.offset())
        .into_tuple::<(i64, Vec<u8>)>()
        .stream(&*db)
        .await?
        .err_into()
        .and_then(|(id, archive_hash)| async move {
            Ok(SourceCodeData {
                id,
                archive_hash: archive_hash.as_slice().try_into()?,
            })
        })
        .try_collect()
        .await
        .map(Json)
}
