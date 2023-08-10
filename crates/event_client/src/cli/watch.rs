use std::{future::ready, iter};

use common::rpc::{
    self,
    sp_core::ByteArray,
    substrate_api_client::{
        self,
        ac_primitives::{Block, Config, Header, PolkadotConfig},
        rpc::{HandleSubscription, JsonrpseeClient, Request},
        Api, GetChainInfo, SubscribeChain,
    },
    CodeStored, ContractCodeUpdated, Instantiated, MetadataCache, Terminated,
};
use db::{
    code, contract, event, node, sea_query::OnConflict, ActiveModelTrait, ActiveValue, ColumnTrait,
    DatabaseConnection, DbErr, EntityTrait, OffsetDateTime, PrimitiveDateTime, QueryFilter,
    TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};
use futures_util::{pin_mut, stream, TryStreamExt};
use itertools::Itertools;
use tracing::{debug, info};

use crate::utils::block_mapping_stream;

/// Errors that may occur during the watch process.
#[derive(Debug, Display, Error, From)]
pub enum WatchError {
    /// Database-related error.
    DatabaseError(DbErr),

    /// Substrate RPC-related error.
    #[display(fmt = "rpc error: {:?}", _0)]
    RpcError(#[error(ignore)] substrate_api_client::Error),

    /// JSON serialization error.
    JsonError(serde_json::Error),

    /// The provided node name is incorrect.
    #[display(fmt = "node not found")]
    NodeNotFound,
}

/// Watch an RPC node for new smart contract-related events.
///
/// # Details
///
/// [`watch`] function will first identify the latest block available
/// and check if any catch-up attempt is necessary at all.
///
/// If catch-up process is required, [`watch`] function will stream
/// blocks starting from the confirmed block and up to the latest block.
///
/// As soon as all missed blocks are processed, [`watch`] will start listening
/// and processing only new blocks from now on.
pub async fn watch(database: DatabaseConnection, name: String) -> Result<(), WatchError> {
    let mut node = node::Entity::find()
        .filter(node::Column::Name.eq(&name))
        .one(&database)
        .await?
        .ok_or(WatchError::NodeNotFound)?;

    let client = JsonrpseeClient::new(&node.url).map_err(substrate_api_client::Error::RpcClient)?;
    let api = Api::<PolkadotConfig, _>::new(client).await?;

    let mut metadata_cache = MetadataCache::new();

    let mut subscription = api.subscribe_finalized_heads()?;

    // Attempt to catch-up to the latest block.
    info!("attempting to catch-up to the latest block");
    let latest = api
        .get_block(None)
        .await?
        .expect("at least one block is expected");
    let stream = block_mapping_stream(
        (node.confirmed_block + 1) as u32..=latest.header.number,
        &api,
    )
    .try_filter_map(|(_, hash)| rpc::block(&api, Some(hash)));

    pin_mut!(stream);

    while let Some(block) = stream.try_next().await? {
        debug!(block_number = %block.header().number(), "found a block to catch-up to");
        node = process_block(node, &database, &api, block.header(), &mut metadata_cache).await?;
    }

    // Proceed with the subscription, since an attempt to traverse missed blocks was already made.
    info!("processing new blocks from now on");

    let confirmed_block = node.confirmed_block as u32;
    let mut subscription_iter =
        iter::from_fn(|| subscription.next()).filter_ok(|header| header.number() > confirmed_block);

    while let Some(header) = subscription_iter
        .next()
        .transpose()
        .map_err(substrate_api_client::Error::RpcClient)?
    {
        debug!(block_number = %header.number(), "found new block");
        node = process_block(node, &database, &api, &header, &mut metadata_cache).await?;
    }

    Ok(())
}

/// Attempt to process one block from either traversal attempt, or
/// block subscription.
///
/// Returns new [`node::Model`], which represents an updated node
/// with up-to-date confirmed block counter.
async fn process_block<C: Request>(
    node: node::Model,
    database: &DatabaseConnection,
    api: &Api<PolkadotConfig, C>,
    block_header: &<PolkadotConfig as Config>::Header,
    metadata_cache: &mut MetadataCache,
) -> Result<node::Model, WatchError> {
    let mut active_node: node::ActiveModel = node.clone().into();

    let block_hash = block_header.hash();
    let block_number = block_header.number();

    let block_millis = rpc::block_timestamp_millis(api, block_hash).await?;
    let raw_timestamp = unix_ts::Timestamp::from_millis(block_millis);
    let offset_timestamp = OffsetDateTime::from_unix_timestamp(raw_timestamp.seconds())
        .expect("invalid timestamp was provided");
    let block_timestamp = PrimitiveDateTime::new(offset_timestamp.date(), offset_timestamp.time());

    let events = rpc::events(api, block_hash, metadata_cache).await?;

    let code_uploads = stream::iter(events.find::<CodeStored>())
        .err_into()
        .and_then(|CodeStored { code_hash }| async move {
            rpc::pristine_code(api, block_hash, code_hash)
                .await
                .map(|code| (code_hash.0, code))
        })
        .try_filter_map(|(hash, code)| ready(Ok(code.map(|val| (hash, val)))))
        .map_ok(|(hash, code)| code::ActiveModel {
            hash: ActiveValue::Set(hash.to_vec()),
            code: ActiveValue::Set(code),
        })
        .try_collect::<Vec<_>>()
        .await?;

    let instantiations = stream::iter(events.find::<Instantiated>())
        .err_into()
        .and_then(|Instantiated { deployer, contract }| async move {
            rpc::contract_info_of(api, block_hash, &contract)
                .await
                .map(|info| (contract, deployer, info))
        })
        .try_filter_map(|(contract, deployer, info)| {
            ready(Ok(info.map(|val| (contract, deployer, val))))
        })
        .map_ok(|(contract, deployer, info)| contract::ActiveModel {
            code_hash: ActiveValue::Set(info.code_hash.0.to_vec()),
            node_id: ActiveValue::Set(node.id),
            address: ActiveValue::Set(contract.as_slice().to_vec()),
            owner: ActiveValue::Set(Some(deployer.as_slice().to_vec())),
            ..Default::default()
        })
        .try_collect::<Vec<_>>()
        .await?;

    let code_hash_updates: Vec<_> = events
        .find::<ContractCodeUpdated>()
        .map_ok(
            |ContractCodeUpdated {
                 contract,
                 new_code_hash,
                 ..
             }| { (contract, new_code_hash) },
        )
        .try_collect()
        .map_err(substrate_api_client::Error::NodeApi)?;

    let terminations: Vec<_> = events
        .find::<Terminated>()
        .map_ok(|Terminated { contract, .. }| contract)
        .try_collect()
        .map_err(substrate_api_client::Error::NodeApi)?;

    database
        .transaction(|txn| {
            Box::pin(async move {
                if !code_uploads.is_empty() {
                    code::Entity::insert_many(code_uploads)
                        .on_conflict(
                            OnConflict::column(code::Column::Hash)
                                .do_nothing()
                                .to_owned(),
                        )
                        .exec_without_returning(txn)
                        .await?;
                }

                if !instantiations.is_empty() {
                    let instantiation_body =
                        serde_json::to_string(&event::EventBody::Instantiation)?;

                    event::Entity::insert_many(instantiations.iter().map(|model| {
                        event::ActiveModel {
                            node_id: ActiveValue::Set(node.id),
                            account: model.address.clone(),
                            event_type: ActiveValue::Set(event::EventType::Instantiation),
                            body: ActiveValue::Set(instantiation_body.clone()),
                            block_timestamp: ActiveValue::Set(block_timestamp),
                            ..Default::default()
                        }
                    }))
                    .exec_without_returning(txn)
                    .await?;

                    contract::Entity::insert_many(instantiations)
                        .on_conflict(
                            OnConflict::columns([
                                contract::Column::NodeId,
                                contract::Column::Address,
                            ])
                            .update_column(contract::Column::CodeHash)
                            .to_owned(),
                        )
                        .exec_without_returning(txn)
                        .await?;
                }

                for (contract, new_code_hash) in code_hash_updates {
                    event::ActiveModel {
                        node_id: ActiveValue::Set(node.id),
                        account: ActiveValue::Set(contract.as_slice().to_vec()),
                        event_type: ActiveValue::Set(event::EventType::CodeHashUpdate),
                        body: ActiveValue::Set(serde_json::to_string(
                            &event::EventBody::CodeHashUpdate {
                                new_code_hash: hex::encode(new_code_hash),
                            },
                        )?),
                        block_timestamp: ActiveValue::Set(block_timestamp),
                        ..Default::default()
                    }
                    .insert(txn)
                    .await?;

                    contract::Entity::update_many()
                        .col_expr(contract::Column::CodeHash, (&new_code_hash[..]).into())
                        .filter(contract::Column::NodeId.eq(node.id))
                        .filter(contract::Column::Address.eq(contract.as_slice()))
                        .exec(txn)
                        .await?;
                }

                if !terminations.is_empty() {
                    let termination_body = serde_json::to_string(&event::EventBody::Termination)?;

                    event::Entity::insert_many(terminations.iter().map(|model| {
                        event::ActiveModel {
                            node_id: ActiveValue::Set(node.id),
                            account: ActiveValue::Set(model.as_slice().to_vec()),
                            event_type: ActiveValue::Set(event::EventType::Termination),
                            body: ActiveValue::Set(termination_body.clone()),
                            block_timestamp: ActiveValue::Set(block_timestamp),
                            ..Default::default()
                        }
                    }))
                    .exec_without_returning(txn)
                    .await?;

                    contract::Entity::delete_many()
                        .filter(contract::Column::NodeId.eq(node.id))
                        .filter(
                            contract::Column::Address
                                .is_in(terminations.iter().map(|val| val.as_slice())),
                        )
                        .exec(txn)
                        .await?;
                }

                active_node.confirmed_block = ActiveValue::Set(block_number as i64);

                Ok(active_node.update(txn).await?)
            })
        })
        .await
        .into_raw_result()
}
