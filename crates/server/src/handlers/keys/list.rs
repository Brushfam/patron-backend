use std::sync::Arc;

use axum::{
    extract::{Query, State},
    Extension, Json,
};
use axum_derive_error::ErrorResponse;
use db::{
    public_key, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QuerySelect,
};
use derive_more::{Display, Error, From};
use futures_util::TryStreamExt;
use serde::Serialize;
use sp_core::crypto::AccountId32;

use crate::{auth::AuthenticatedUserId, pagination::Pagination};

#[derive(Serialize)]
pub struct PublicKeyData {
    pub id: i64,
    pub address: AccountId32,
}

#[derive(ErrorResponse, Display, From, Error)]
pub(super) enum PublicKeyListError {
    DatabaseError(DbErr),

    #[display(fmt = "invalid public key size stored in db")]
    InvalidPublicKeySize,
}

pub(super) async fn list(
    Extension(current_user): Extension<AuthenticatedUserId>,
    State(db): State<Arc<DatabaseConnection>>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<Vec<PublicKeyData>>, PublicKeyListError> {
    public_key::Entity::find()
        .select_only()
        .columns([public_key::Column::Id, public_key::Column::Address])
        .filter(public_key::Column::UserId.eq(current_user.id()))
        .limit(pagination.limit())
        .offset(pagination.offset())
        .into_tuple::<(i64, Vec<u8>)>()
        .stream(&*db)
        .await?
        .err_into()
        .and_then(|(id, address)| async move {
            Ok(PublicKeyData {
                id,
                address: AccountId32::new(
                    address
                        .try_into()
                        .map_err(|_| PublicKeyListError::InvalidPublicKeySize)?,
                ),
            })
        })
        .try_collect()
        .await
        .map(Json)
}
