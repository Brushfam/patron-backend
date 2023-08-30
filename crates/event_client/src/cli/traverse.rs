use common::rpc::{
    self,
    sp_core::{ByteArray, H256},
    substrate_api_client::{
        self,
        ac_primitives::PolkadotConfig,
        rpc::{JsonrpseeClient, Request},
        Api, Error,
    },
    Instantiated, MetadataCache,
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
    #[display(fmt = "rpc error: {:?}", _0)]
    RpcError(#[error(ignore)] substrate_api_client::Error),

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

    let client = JsonrpseeClient::new(&node.url).map_err(substrate_api_client::Error::RpcClient)?;
    let api = Api::new(client).await?;

    let stream = block_mapping_stream(0..=node.confirmed_block as u32, &api);

    pin_mut!(stream);

    let mut metadata_cache = MetadataCache::new();

    while let Some((_, block_hash)) = stream.try_next().await? {
        if let Ok(block_data) = parse_block(&api, block_hash, &mut metadata_cache).await {
            database
                .transaction::<_, _, TraverseError>(|txn| {
                    Box::pin(async move {
                        for instantiation in block_data.instantiations {
                            contract::Entity::update_many()
                                .col_expr(
                                    contract::Column::Owner,
                                    (instantiation.deployer.as_slice()).into(),
                                )
                                .filter(contract::Column::NodeId.eq(node.id))
                                .filter(
                                    contract::Column::Address.eq(instantiation.contract.as_slice()),
                                )
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
    instantiations: Vec<Instantiated>,
}

/// Attempt to parse block associated with the provided block hash.
async fn parse_block<C: Request>(
    api: &Api<PolkadotConfig, C>,
    block_hash: H256,
    metadata_cache: &mut MetadataCache,
) -> Result<BlockData, Error> {
    let metadata = metadata_cache.metadata(api, block_hash).await?;

    let events = rpc::events(api, block_hash, metadata.clone()).await?;

    let instantiations = events.find().try_collect()?;

    Ok(BlockData { instantiations })
}
