use std::str::FromStr;

use common::rpc::{
    subxt::{
        self,
        rpc::types::{BlockNumber, NumberOrHex},
        Config, Error, OnlineClient, PolkadotConfig,
    },
    ContractEvent, ContractEventData, InvalidSchema, Schema,
};
use db::{
    contract, node, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter,
    TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};
use futures_util::{pin_mut, stream::try_unfold, TryStreamExt};
use itertools::Itertools;

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

    let mut current_block = 0u64;
    let confirmed_block = node.confirmed_block as u64;

    let block_mapping_stream = try_unfold(
        (&mut current_block, api.rpc()),
        |(block_number, rpc)| async move {
            let mut block_hash = None;

            while block_hash.is_none() && *block_number <= confirmed_block {
                println!("block: {}", &block_number);

                block_hash = rpc
                    .block_hash(Some(BlockNumber::from(NumberOrHex::from(*block_number))))
                    .await?;

                *block_number += 1;
            }

            Result::<_, TraverseError>::Ok(block_hash.map(|val| (val, (block_number, rpc))))
        },
    );

    pin_mut!(block_mapping_stream);

    while let Some(block_hash) = block_mapping_stream.try_next().await? {
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
