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

#[derive(Debug, Display, Error, From)]
pub enum TraverseError {
    DatabaseError(DbErr),
    RpcError(subxt::Error),
    Schema(InvalidSchema),

    #[display(fmt = "node not found")]
    NodeNotFound,
}

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

struct BlockData {
    instantiations: Vec<([u8; 32], [u8; 32])>,
}

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
