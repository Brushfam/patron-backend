use std::{pin::pin, str::FromStr};

use common::rpc::{
    self,
    sp_core::crypto::AccountId32,
    substrate_api_client::{self, ac_primitives::Block, rpc::JsonrpseeClient, Api},
    MetadataCache,
};
use db::{
    code, contract, node, sea_query::OnConflict, ActiveValue, DatabaseConnection, DbErr,
    EntityTrait, TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};
use futures_util::TryStreamExt;

use crate::utils::{extract_code_hash, extract_twox_account_id};

/// Errors thay may occur during initialization process.
#[derive(Debug, Display, Error, From)]
pub enum InitializeError {
    /// Database-related error.
    DatabaseError(DbErr),

    /// Substrate RPC-related error.
    #[display(fmt = "rpc error: {:?}", _0)]
    RpcError(#[error(ignore)] substrate_api_client::Error),

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
    payment_address: Option<String>,
) -> Result<(), InitializeError> {
    let client = JsonrpseeClient::new(&url).map_err(substrate_api_client::Error::RpcClient)?;
    let api = Api::new(client).await?;

    let mut metadata_cache = MetadataCache::new();

    let latest_block = rpc::block(&api, None)
        .await?
        .expect("at least one block is expected");

    let block_hash = latest_block.hash();

    let metadata = metadata_cache.metadata(&api, block_hash).await?;

    let payment_address = payment_address
        .as_deref()
        .map(AccountId32::from_str)
        .transpose()
        .map_err(|_| InitializeError::InvalidPaymentAddress)?
        .map(|addr| <[u8; 32]>::from(addr).to_vec());

    let node = database
        .transaction::<_, _, InitializeError>(|txn| {
            Box::pin(async move {
                let node = node::Entity::insert(node::ActiveModel {
                    name: ActiveValue::Set(name),
                    url: ActiveValue::Set(url),
                    payment_contract: ActiveValue::Set(payment_address),
                    confirmed_block: ActiveValue::Set(latest_block.header.number as i64),
                    ..Default::default()
                })
                .on_conflict(
                    OnConflict::column(node::Column::Name)
                        .update_columns([
                            node::Column::Url,
                            node::Column::PaymentContract,
                            node::Column::ConfirmedBlock,
                        ])
                        .to_owned(),
                )
                .exec_with_returning(txn)
                .await?;

                Ok(node)
            })
        })
        .await
        .into_raw_result()?;

    let mut wasm_blobs = pin!(rpc::pristine_code_root(&api, block_hash, metadata).await?);

    while let Some(chunk) = wasm_blobs.try_next().await? {
        database
            .transaction::<_, _, InitializeError>(|txn| {
                Box::pin(async move {
                    code::Entity::insert_many(chunk.into_iter().map(|(key, wasm)| {
                        code::ActiveModel {
                            hash: ActiveValue::Set(extract_code_hash(key)),
                            code: ActiveValue::Set(wasm),
                        }
                    }))
                    .on_conflict(
                        OnConflict::column(code::Column::Hash)
                            .do_nothing()
                            .to_owned(),
                    )
                    .exec_without_returning(txn)
                    .await?;

                    Ok(())
                })
            })
            .await
            .into_raw_result()?;
    }

    let mut contracts = pin!(rpc::contract_info_of_root(&api, block_hash, metadata).await?);

    while let Some(chunk) = contracts.try_next().await? {
        database
            .transaction::<_, _, InitializeError>(|txn| {
                Box::pin(async move {
                    contract::Entity::insert_many(chunk.into_iter().map(|(key, contract)| {
                        contract::ActiveModel {
                            code_hash: ActiveValue::Set(contract.code_hash.0.to_vec()),
                            node_id: ActiveValue::Set(node.id),
                            address: ActiveValue::Set(extract_twox_account_id(key)),
                            ..Default::default()
                        }
                    }))
                    .on_conflict(
                        OnConflict::columns([contract::Column::NodeId, contract::Column::Address])
                            .do_nothing()
                            .to_owned(),
                    )
                    .exec_without_returning(txn)
                    .await?;

                    Ok(())
                })
            })
            .await
            .into_raw_result()?;
    }

    Ok(())
}
