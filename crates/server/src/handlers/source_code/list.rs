use std::sync::Arc;

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
use serde::Serialize;

use crate::{auth::AuthenticatedUserId, pagination::Pagination};

#[derive(Serialize)]
pub struct SourceCodeData {
    pub id: i64,
    pub archive_hash: Vec<u8>,
}

#[derive(ErrorResponse, Display, From, Error)]
pub(super) enum SourceCodeListError {
    DatabaseError(DbErr),
}

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
        .and_then(|(id, archive_hash)| async move { Ok(SourceCodeData { id, archive_hash }) })
        .try_collect()
        .await
        .map(Json)
}
