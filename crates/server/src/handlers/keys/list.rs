use std::sync::Arc;

use aide::{transform::TransformOperation, OperationIo};
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
use schemars::JsonSchema;
use serde::Serialize;
use sp_core::crypto::AccountId32;

use crate::{auth::AuthenticatedUserId, pagination::Pagination};

/// A single public key data.
#[derive(Serialize, JsonSchema)]
pub struct PublicKeyData {
    /// Public key identifier.
    #[schemars(example = "crate::schema::example_database_identifier")]
    pub id: i64,

    /// Account address.
    #[schemars(example = "crate::schema::example_account", with = "String")]
    pub address: AccountId32,
}

/// Errors that may occur during the public key list request handling.
#[derive(ErrorResponse, Display, From, Error, OperationIo)]
#[aide(output)]
pub(super) enum PublicKeyListError {
    /// Database-related error.
    DatabaseError(DbErr),

    /// Public key stored inside of a database has an invalid size.
    #[display(fmt = "invalid public key size stored in db")]
    InvalidPublicKeySize,
}

/// Generate OAPI documentation for the [`list`] handler.
pub(super) fn docs(op: TransformOperation) -> TransformOperation {
    op.summary("List public keys attached to the current user.")
        .response_with::<200, Json<Vec<PublicKeyData>>, _>(|op| op.description("Public key list."))
}

/// List public keys attached to the current authenticated user's account.
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
