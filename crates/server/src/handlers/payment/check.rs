use std::{array::TryFromSliceError, sync::Arc};

use aide::{transform::TransformOperation, OperationIo};
use axum::{extract::State, http::StatusCode, Extension, Json};
use axum_derive_error::ErrorResponse;
use common::{
    hash::blake2,
    rpc::{
        self, parity_scale_codec,
        parity_scale_codec::Decode,
        sp_core::crypto::AccountId32,
        substrate_api_client,
        substrate_api_client::{rpc::JsonrpseeClient, Api},
    },
};
use db::{
    node, public_key, user, ActiveValue, ColumnTrait, DatabaseConnection, DbErr, EntityTrait,
    QueryFilter, QuerySelect, SelectExt, TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};
use ink_metadata::LangError;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use tokio::{runtime::Handle, task::JoinError};

use crate::{auth::AuthenticatedUserId, schema::example_error};

/// JSON request body.
#[derive(Deserialize, JsonSchema)]
pub(super) struct PaymentCheckRequest {
    /// Node identifier used to check the membership payment.
    #[schemars(example = "crate::schema::example_database_identifier")]
    node_id: i64,

    /// Account identifier against which the check will be executed.
    #[schemars(example = "crate::schema::example_account", with = "String")]
    account: AccountId32,
}

/// Errors that may occur during the membership check process.
#[derive(ErrorResponse, Display, From, Error, OperationIo)]
#[aide(output)]
pub(super) enum PaymentCheckError {
    /// Database-related error.
    DatabaseError(DbErr),

    /// Substrate RPC-related error.
    #[display(fmt = "substrate rpc error: {:?}", _0)]
    Rpc(#[error(ignore)] substrate_api_client::Error),

    /// SCALE codec error.
    Scale(parity_scale_codec::Error),

    /// Contract address stored inside of a database is invalid.
    ContractAddress(TryFromSliceError),

    /// Unable to spawn Tokio task to handle RPC calls.
    JoinError(JoinError),

    /// Contract call error.
    #[display(fmt = "unable to call the contract")]
    CallError,

    /// Deleted user attempted to access the route.
    #[status(StatusCode::FORBIDDEN)]
    #[display(fmt = "user doesn't exist")]
    NonExistentUser,

    /// Provided account address is incorrect.
    #[status(StatusCode::BAD_REQUEST)]
    #[display(fmt = "invalid account was provided")]
    InvalidKey,

    /// Provided node identifier is incorrect.
    #[status(StatusCode::NOT_FOUND)]
    #[display(fmt = "invalid node id")]
    InvalidNodeId,

    /// Provided node identifier is not marked as the one that supports payments.
    #[status(StatusCode::BAD_REQUEST)]
    #[display(fmt = "provided node doesn't support payments")]
    NodeWithoutPayments,

    /// Membership check returned a negative result.
    #[status(StatusCode::BAD_REQUEST)]
    #[display(fmt = "payment required")]
    PaymentRequired,

    /// Paid user attempted to check the membership again.
    #[status(StatusCode::BAD_REQUEST)]
    #[display(fmt = "user already has membership available")]
    PaidAlready,
}

/// Generate OAPI documentation for the [`check`] handler.
pub(super) fn docs(op: TransformOperation) -> TransformOperation {
    op.summary("Check membership payment with the provided node.")
        .description("See self-hosted documentation for more information about the contract ABI.")
        .response::<200, ()>()
        .response_with::<400, Json<Value>, _>(|op| {
            op.description("Invalid account identifier was provided.")
                .example(example_error(PaymentCheckError::InvalidKey))
        })
        .response_with::<404, Json<Value>, _>(|op| {
            op.description("The provided node identifier is invalid.")
                .example(example_error(PaymentCheckError::InvalidNodeId))
        })
}

/// Check current authenticated user's membership.
///
/// Consult self-hosted documentation for more information on supported smart contract ABI.
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

            let (url, contract) = node::Entity::find_by_id(request.node_id)
                .select_only()
                .columns([node::Column::Url, node::Column::PaymentContract])
                .into_tuple::<(String, Option<Vec<u8>>)>()
                .one(txn)
                .await?
                .ok_or(PaymentCheckError::InvalidNodeId)?;

            let contract = contract.ok_or(PaymentCheckError::NodeWithoutPayments)?;

            // Make sure this matches the ABI of the check message.
            let mut data = Vec::with_capacity(36);
            data.extend_from_slice(&blake2("check".as_bytes())[0..4]);
            data.extend_from_slice(request.account.as_ref());

            let raw_response = tokio::task::spawn_blocking(|| {
                Handle::current().block_on(async move {
                    let client = JsonrpseeClient::new(&url)
                        .map_err(substrate_api_client::Error::RpcClient)?;
                    let api = Api::new(client).await?;

                    let val = rpc::call_contract(
                        &api,
                        AccountId32::new(contract.as_slice().try_into()?),
                        data,
                    )
                    .await?;

                    Result::<_, PaymentCheckError>::Ok(val)
                })
            })
            .await??
            .result
            .map_err(|_| PaymentCheckError::CallError)?
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
