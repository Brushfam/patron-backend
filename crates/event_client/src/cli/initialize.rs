use std::str::FromStr;

use common::rpc::{
    subxt::{self, utils::AccountId32, OnlineClient, PolkadotConfig},
    InvalidSchema, Schema, PAGE_SIZE,
};
use db::{
    code, contract, node, sea_query::OnConflict, ActiveValue, DatabaseConnection, DbErr,
    EntityTrait, TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};
use futures_util::{TryFutureExt, TryStreamExt};

use crate::utils::{extract_code_hash, extract_twox_account_id};

/// Errors thay may occur during initialization process.
#[derive(Debug, Display, Error, From)]
pub enum InitializeError {
    /// Database-related error.
    DatabaseError(DbErr),

    /// Substrate RPC-related error.
    RpcError(subxt::Error),

    /// User provided invalid schema name.
    Schema(InvalidSchema),

    /// Invalid payment contract account id was provided.
    #[display(fmt = "invalid account id for payment contract")]
    InvalidPaymentAddress,
}

/// Initialize an RPC node from the provided data.
///
/// # Details
///
/// This method obtains information about the latest block available and
/// acquires smart contract information and uploaded WASM blob details
/// related to that block.
///
/// You have to run this command every time you add a new node to the database,
/// since [`initialize`] function initializes node information too.
///
/// No traversal of previous blocks is being done by this command.
pub async fn initialize(
    database: DatabaseConnection,
    name: String,
    url: String,
    schema_name: String,
    payment_address: Option<String>,
) -> Result<(), InitializeError> {
    let api = OnlineClient::<PolkadotConfig>::from_url(&url).await?;

    let schema = Schema::from_str(&schema_name)?;

    let latest_block = schema.block(&api, None).await?;

    let payment_address = payment_address
        .as_deref()
        .map(AccountId32::from_str)
        .transpose()
        .map_err(|_| InitializeError::InvalidPaymentAddress)?
        .map(|addr| addr.0.to_vec());

    // Attempt to add all the information in a single transaction.
    //
    // Confirmed block value is set to the value of latest block number
    // we acquired earlier.
    database
        .transaction(|txn| {
            Box::pin(async move {
                let node = node::Entity::insert(node::ActiveModel {
                    name: ActiveValue::Set(name),
                    url: ActiveValue::Set(url),
                    schema: ActiveValue::Set(schema_name),
                    payment_contract: ActiveValue::Set(payment_address),
                    confirmed_block: ActiveValue::Set(latest_block.number() as i64),
                    ..Default::default()
                })
                .on_conflict(
                    OnConflict::column(node::Column::Name)
                        .update_columns([
                            node::Column::Url,
                            node::Column::Schema,
                            node::Column::PaymentContract,
                            node::Column::ConfirmedBlock,
                        ])
                        .to_owned(),
                )
                .exec_with_returning(txn)
                .await?;

                schema
                    .pristine_code_root(&api, latest_block.hash())
                    .await?
                    .err_into::<InitializeError>()
                    .map_ok(|(key, wasm)| code::ActiveModel {
                        hash: ActiveValue::Set(extract_code_hash(key)),
                        code: ActiveValue::Set(wasm),
                    })
                    .try_chunks(PAGE_SIZE as usize)
                    .map_err(|err| err.1)
                    .and_then(|chunk| {
                        code::Entity::insert_many(chunk)
                            .on_conflict(
                                OnConflict::column(code::Column::Hash)
                                    .do_nothing()
                                    .to_owned(),
                            )
                            .exec_without_returning(txn)
                            .map_ok(|_| ())
                            .err_into()
                    })
                    .try_collect()
                    .await?;

                schema
                    .contract_info_of_root(&api, latest_block.hash())
                    .await?
                    .err_into::<InitializeError>()
                    .map_ok(|(key, contract)| contract::ActiveModel {
                        code_hash: ActiveValue::Set(contract.code_hash.0.to_vec()),
                        node_id: ActiveValue::Set(node.id),
                        address: ActiveValue::Set(extract_twox_account_id(key)),
                        ..Default::default()
                    })
                    .try_chunks(PAGE_SIZE as usize)
                    .map_err(|err| err.1)
                    .and_then(|chunk| {
                        contract::Entity::insert_many(chunk)
                            .on_conflict(
                                OnConflict::columns([
                                    contract::Column::NodeId,
                                    contract::Column::Address,
                                ])
                                .do_nothing()
                                .to_owned(),
                            )
                            .exec_without_returning(txn)
                            .map_ok(|_| ())
                            .err_into()
                    })
                    .try_collect()
                    .await?;

                Ok(())
            })
        })
        .await
        .into_raw_result()
}
