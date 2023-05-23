use std::{array::TryFromSliceError, fmt, str::FromStr, sync::Arc};

use axum::{extract::State, http::StatusCode, Extension, Json};
use axum_derive_error::ErrorResponse;
use common::rpc::{
    parity_scale_codec::{self, Decode},
    subxt::{self, ext::sp_runtime::DispatchError, OnlineClient, PolkadotConfig},
    InvalidSchema, Schema,
};
use db::{
    node, public_key, user, ActiveValue, ColumnTrait, DatabaseConnection, DbErr, EntityTrait,
    QueryFilter, QuerySelect, SelectExt, TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};
use ink_metadata::LangError;
use serde::Deserialize;
use sp_core::{blake2_256, crypto::AccountId32};

use crate::auth::AuthenticatedUserId;

#[derive(Deserialize)]
pub(super) struct PaymentCheckRequest {
    node_id: i64,
    account: AccountId32,
}

#[derive(ErrorResponse, Display, From, Error)]
pub(super) enum PaymentCheckError {
    DatabaseError(DbErr),
    Schema(InvalidSchema),
    Rpc(subxt::Error),
    Dispatch(WrappedDispatchError),
    Scale(parity_scale_codec::Error),
    ContractAddress(TryFromSliceError),

    #[display(fmt = "unable to call the contract")]
    CallError,

    #[status(StatusCode::FORBIDDEN)]
    #[display(fmt = "user doesn't exist")]
    NonExistentUser,

    #[status(StatusCode::BAD_REQUEST)]
    #[display(fmt = "invalid account was provided")]
    InvalidKey,

    #[status(StatusCode::NOT_FOUND)]
    #[display(fmt = "invalid node id")]
    InvalidNodeId,

    #[status(StatusCode::BAD_REQUEST)]
    #[display(fmt = "provided node doesn't support payments")]
    NodeWithoutPayments,

    #[status(StatusCode::BAD_REQUEST)]
    #[display(fmt = "payment required")]
    PaymentRequired,

    #[status(StatusCode::BAD_REQUEST)]
    #[display(fmt = "user already has membership available")]
    PaidAlready,
}

#[derive(Debug)]
pub(super) struct WrappedDispatchError(DispatchError);

impl fmt::Display for WrappedDispatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl std::error::Error for WrappedDispatchError {}

pub(super) async fn check(
    Extension(current_user): Extension<AuthenticatedUserId>,
    State(db): State<Arc<DatabaseConnection>>,
    Json(request): Json<PaymentCheckRequest>,
) -> Result<(), PaymentCheckError> {
    db.transaction(|txn| {
        Box::pin(async move {
            let user = user::Entity::find_by_id(current_user.id())
                .lock_exclusive()
                .one(txn)
                .await?
                .ok_or(PaymentCheckError::NonExistentUser)?;

            if user.paid {
                return Err(PaymentCheckError::PaidAlready);
            }

            let key_exists = public_key::Entity::find()
                .select_only()
                .filter(public_key::Column::UserId.eq(current_user.id()))
                .filter(public_key::Column::Address.eq(AsRef::<[u8]>::as_ref(&request.account)))
                .exists(txn)
                .await?;

            if !key_exists {
                return Err(PaymentCheckError::InvalidKey);
            }

            let (url, schema_name, contract) = node::Entity::find_by_id(request.node_id)
                .select_only()
                .columns([
                    node::Column::Url,
                    node::Column::Schema,
                    node::Column::PaymentContract,
                ])
                .into_tuple::<(String, String, Option<Vec<u8>>)>()
                .one(txn)
                .await?
                .ok_or(PaymentCheckError::InvalidNodeId)?;

            let contract = contract.ok_or(PaymentCheckError::NodeWithoutPayments)?;

            let rpc = OnlineClient::<PolkadotConfig>::from_url(url).await?;
            let schema = Schema::from_str(&schema_name)?;

            // Make sure this matches the ABI of the check message.
            let mut data = Vec::with_capacity(36);
            data.extend_from_slice(&blake2_256("check".as_bytes())[0..4]);
            data.extend_from_slice(request.account.as_ref());

            let raw_response = schema
                .call_contract(
                    &rpc,
                    subxt::utils::AccountId32(contract.as_slice().try_into()?),
                    data,
                )
                .await?
                .result
                .map_err(WrappedDispatchError)?
                .data;

            let response: Result<bool, LangError> = Decode::decode(&mut &*raw_response)?;

            if !response.map_err(|_| PaymentCheckError::CallError)? {
                return Err(PaymentCheckError::PaymentRequired);
            }

            let mut active_model: user::ActiveModel = user.into();
            active_model.paid = ActiveValue::Set(true);
            user::Entity::update(active_model).exec(txn).await?;

            Ok(())
        })
    })
    .await
    .into_raw_result()
}
