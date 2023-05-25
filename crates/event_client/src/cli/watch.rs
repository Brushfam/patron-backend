use std::{
    future::{self, ready},
    str::FromStr,
};

use common::rpc::{
    subxt::{self, blocks::Block, config::Header, Config, OnlineClient, PolkadotConfig},
    ContractEvent, ContractEventData, InvalidSchema, Schema,
};
use db::{
    code, contract, event, node, sea_query::OnConflict, ActiveModelTrait, ActiveValue, ColumnTrait,
    DatabaseConnection, DbErr, EntityTrait, OffsetDateTime, PrimitiveDateTime, QueryFilter,
    TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};
use futures_util::{pin_mut, stream::FuturesUnordered, TryStreamExt};
use itertools::Itertools;
use tracing::{debug, info};

use crate::utils::block_mapping_stream;

#[derive(Debug, Display, Error, From)]
pub enum WatchError {
    DatabaseError(DbErr),
    RpcError(subxt::Error),
    JsonError(serde_json::Error),
    Schema(InvalidSchema),

    #[display(fmt = "node not found")]
    NodeNotFound,
}

/// Watch an RPC node for new events.
///
/// This method will attempt to fix any gaps that occured between
/// watch process activations.
pub async fn watch(database: DatabaseConnection, name: String) -> Result<(), WatchError> {
    let mut node = node::Entity::find()
        .filter(node::Column::Name.eq(&name))
        .one(&database)
        .await?
        .ok_or(WatchError::NodeNotFound)?;

    let api = OnlineClient::<PolkadotConfig>::from_url(&node.url).await?;

    let schema = Schema::from_str(&node.schema)?;

    let subscription = api.blocks().subscribe_finalized().await?;

    // Attempt to catch-up to the latest block.
    info!("attempting to catch-up to the latest block");
    let latest = api.blocks().at_latest().await?;
    let stream = block_mapping_stream(
        (node.confirmed_block + 1) as u64..=latest.number() as u64,
        &api,
    )
    .and_then(|(_, hash)| api.blocks().at(hash));

    pin_mut!(stream);

    while let Some(block) = stream.try_next().await? {
        debug!(block_number = %block.number(), "found a block to catch-up to");
        node = process_block(schema, node, &database, &api, block).await?;
    }

    // Proceed with the subscription, since an attempt to traverse missed blocks was already made.
    info!("processing new blocks from now on");
    let confirmed_block = node.confirmed_block;
    let mut stream =
        subscription.try_filter(|block| future::ready(block.number() as i64 > confirmed_block));

    while let Some(block) = stream.try_next().await? {
        debug!(block_number = %block.number(), "found new block");
        node = process_block(schema, node, &database, &api, block).await?;
    }

    Ok(())
}

/// Attempt to process one block from either traversal attempt, or
/// block subscription.
///
/// Returns new [`node::Model`], which represents an updated node
/// with up-to-date confirmed block counter.
async fn process_block<T>(
    schema: Schema<T>,
    node: node::Model,
    database: &DatabaseConnection,
    api: &OnlineClient<T>,
    block: Block<T, OnlineClient<T>>,
) -> Result<node::Model, WatchError>
where
    T: Config + Send + Sync,
    T::Header: Header<Number = u32>,
{
    let mut active_node: node::ActiveModel = node.clone().into();

    let block_millis = schema.block_timestamp_millis(api, block.hash()).await?;
    let raw_timestamp = unix_ts::Timestamp::from_millis(block_millis);
    let offset_timestamp = OffsetDateTime::from_unix_timestamp(raw_timestamp.seconds())
        .expect("invalid timestamp was provided");
    let block_timestamp = PrimitiveDateTime::new(offset_timestamp.date(), offset_timestamp.time());

    let events = block.events().await?;

    let code_uploads = schema
        .events(&events, ContractEvent::CodeStored)
        .filter_map_ok(|event| match event {
            ContractEventData::CodeStored { code_hash } => Some(code_hash),
            _ => None,
        })
        .map_ok(|code_hash| {
            let hash = block.hash();

            async move {
                schema
                    .pristine_code(api, hash, &code_hash)
                    .await
                    .map(|code| (code_hash.0, code))
            }
        })
        .try_collect::<_, FuturesUnordered<_>, _>()?
        .try_filter_map(|(hash, code)| ready(Ok(code.map(|val| (hash, val)))))
        .map_ok(|(hash, code)| code::ActiveModel {
            hash: ActiveValue::Set(hash.to_vec()),
            code: ActiveValue::Set(code),
        })
        .try_collect::<Vec<_>>()
        .await?;

    let instantiations = schema
        .events(&events, ContractEvent::Instantiated)
        .filter_map_ok(|event| match event {
            ContractEventData::Instantiated { contract, deployer } => Some((contract, deployer)),
            _ => None,
        })
        .map_ok(|(contract, deployer)| async {
            schema
                .contract_info_of(api, block.hash(), &contract)
                .await
                .map(|info| (contract, deployer, info))
        })
        .try_collect::<_, FuturesUnordered<_>, _>()?
        .try_filter_map(|(contract, deployer, info)| {
            ready(Ok(info.map(|val| (contract, deployer, val))))
        })
        .map_ok(|(contract, deployer, info)| contract::ActiveModel {
            code_hash: ActiveValue::Set(info.code_hash.0.to_vec()),
            node_id: ActiveValue::Set(node.id),
            address: ActiveValue::Set(contract.0.to_vec()),
            owner: ActiveValue::Set(Some(deployer.0.to_vec())),
            ..Default::default()
        })
        .try_collect::<Vec<_>>()
        .await?;

    let code_hash_updates: Vec<_> = schema
        .events(&events, ContractEvent::ContractCodeUpdated)
        .filter_map_ok(|event| match event {
            ContractEventData::ContractCodeUpdated {
                contract,
                new_code_hash,
            } => Some((contract.0, new_code_hash.0)),
            _ => None,
        })
        .try_collect()?;

    let terminations: Vec<_> = schema
        .events(&events, ContractEvent::Terminated)
        .filter_map_ok(|event| match event {
            ContractEventData::Terminated { contract } => Some(contract.0),
            _ => None,
        })
        .try_collect()?;

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
                        account: ActiveValue::Set(contract.to_vec()),
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
                        .filter(contract::Column::Address.eq(&contract[..]))
                        .exec(txn)
                        .await?;
                }

                if !terminations.is_empty() {
                    let termination_body = serde_json::to_string(&event::EventBody::Termination)?;

                    event::Entity::insert_many(terminations.iter().map(|model| {
                        event::ActiveModel {
                            node_id: ActiveValue::Set(node.id),
                            account: ActiveValue::Set(model.to_vec()),
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
                                .is_in(terminations.iter().map(|val| &val[..])),
                        )
                        .exec(txn)
                        .await?;
                }

                active_node.confirmed_block = ActiveValue::Set(block.number() as i64);

                Ok(active_node.update(txn).await?)
            })
        })
        .await
        .into_raw_result()
}
