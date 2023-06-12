use std::str::FromStr;

use common::rpc::{
    subxt::{self, Config, Error, OnlineClient, PolkadotConfig},
    ContractEvent, ContractEventData, InvalidSchema, Schema,
};
use db::{
    contract, node, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter,
    TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};
use futures_util::{pin_mut, TryStreamExt};
use itertools::Itertools;

use crate::utils::block_mapping_stream;

/// Errors that may occur during traversal process.
#[derive(Debug, Display, Error, From)]
pub enum TraverseError {
    /// Database-related error.
    DatabaseError(DbErr),

    /// Substrate RPC-related error.
    RpcError(subxt::Error),

    /// User provided invalid schema name.
    Schema(InvalidSchema),

    /// The provided node name is incorrect.
    #[display(fmt = "node not found")]
    NodeNotFound,
}

/// Traverse blocks before the confirmed block for events.
///
/// # Details
///
/// This method is provided for testing purposes, as dedicated archive servers
/// are required to correctly process old blocks in batches.
///
/// You can use [`traverse`] function to test your local Substrate node
/// event dispatching.
///
/// If necessary, you may set up a separate service for batch block analysis
/// and fill the database with models found in [`db`] crate.
pub async fn traverse(database: DatabaseConnection, name: String) -> Result<(), TraverseError> {
    let node = node::Entity::find()
        .filter(node::Column::Name.eq(name))
        .one(&database)
        .await?
        .ok_or(TraverseError::NodeNotFound)?;

    let api = OnlineClient::<PolkadotConfig>::from_url(&node.url).await?;

    let schema = Schema::from_str(&node.schema)?;

    let stream = block_mapping_stream(0..=node.confirmed_block as u64, &api);

    pin_mut!(stream);

    while let Some((_, block_hash)) = stream.try_next().await? {
        if let Ok(block_data) = parse_block(&api, &schema, block_hash).await {
            database
                .transaction::<_, _, TraverseError>(|txn| {
                    Box::pin(async move {
                        for (contract, deployer) in block_data.instantiations {
                            contract::Entity::update_many()
                                .col_expr(contract::Column::Owner, (&deployer[..]).into())
                                .filter(contract::Column::NodeId.eq(node.id))
                                .filter(contract::Column::Address.eq(&contract[..]))
                                .exec(txn)
                                .await?;
                        }

                        Ok(())
                    })
                })
                .await
                .into_raw_result()?;
        }
    }

    Ok(())
}

/// Parsed block data.
struct BlockData {
    /// Smart contract instantiations found in block.
    instantiations: Vec<([u8; 32], [u8; 32])>,
}

/// Attempt to parse block associated with the provided block hash using the provided schema.
async fn parse_block<T: Config + Send + Sync>(
    api: &OnlineClient<T>,
    schema: &Schema<T>,
    block_hash: T::Hash,
) -> Result<BlockData, Error> {
    let events = schema.block(api, Some(block_hash)).await?.events().await?;

    let instantiations = schema
        .events(&events, ContractEvent::Instantiated)
        .filter_map_ok(|event| match event {
            ContractEventData::Instantiated { contract, deployer } => {
                Some((contract.0, deployer.0))
            }
            _ => None,
        })
        .try_collect()?;

    Ok(BlockData { instantiations })
}
