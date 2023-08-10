//! Substrate node RPC utilities.
//!
//! This module provides various methods for communicating with Substrate nodes
//! that support `pallet-contracts`, allowing you to query data without worrying about
//! node specifics.
//!
//! # Metadata handling
//!
//! As node developers may release new updates, we constantly check for metadata version changes
//! when querying nodes.
//!
//! When metadata version change is detected, we fetch new metadata information from a node
//! while caching it in the process.

use std::{convert::identity, num::NonZeroUsize};

use frame_metadata::RuntimeMetadataPrefixed;
use futures_util::{
    stream::{self, try_unfold},
    Stream, StreamExt, TryStreamExt,
};
use lru::LruCache;
use pallet_contracts::Determinism;
use pallet_contracts_primitives::ContractExecResult;
use parity_scale_codec::{Compact, Decode, Encode};
use sp_core::crypto::AccountId32;
use sp_version::RuntimeVersion;
use substrate_api_client::{
    ac_compose_macros::rpc_params,
    ac_node_api::{Events, Metadata, StaticEvent},
    ac_primitives::{
        Bytes, Config, PolkadotConfig, RpcParams, StorageKey, SubstrateKitchensinkConfig, H256,
    },
    rpc::{Request, Subscribe},
    storage_key, Api, Error, GetChainInfo, GetStorage,
};

pub use parity_scale_codec;
pub use sp_core;
pub use substrate_api_client;

/// Default page size for fetching data by storage key prefix.
pub const PAGE_SIZE: u32 = 10;

/// WASM blob information received from an RPC node.
#[derive(Decode)]
struct PrefabWasmModule {
    _instruction_weights_version: Compact<u32>,
    _initial: Compact<u32>,
    _maximum: Compact<u32>,
    /// WASM bytecode value.
    code: Vec<u8>,
    _determinism: Determinism,
}

/// Deployed contract information from an RPC node.
#[derive(Decode)]
pub struct ContractInfo {
    _trie_id: Vec<u8>,
    /// Code hash associated with the current contract.
    pub code_hash: H256,
    _storage_bytes: u32,
    _storage_items: u32,
    _storage_byte_deposit: u128,
    _storage_item_deposit: u128,
    _storage_base_deposit: u128,
}

/// Get a [`Block`] information for the provided block hash.
///
/// If the provided hash is [`None`], the latest block is retrieved.
///
/// [`Block`]: Config::Block
pub async fn block<C: Request>(
    api: &Api<PolkadotConfig, C>,
    at: Option<H256>,
) -> Result<Option<<SubstrateKitchensinkConfig as Config>::Block>, Error> {
    api.get_block(at).await
}

/// Get information on the stored code at the provided block hash.
///
/// This method returns an asynchronous [`Stream`] of [`StorageKey`] (which can be decoded to receive the code hash value)
/// and WASM blob bytes.
pub async fn pristine_code_root<C: Request>(
    api: &Api<PolkadotConfig, C>,
    at: H256,
) -> Result<impl Stream<Item = Result<Vec<(StorageKey, Vec<u8>)>, Error>> + '_, Error> {
    let prefix = api
        .get_storage_map_key_prefix("Contracts", "CodeStorage")
        .await?;

    Ok(paged_key_values::<_, PrefabWasmModule, _, _>(prefix, api, at, |module| module.code).await)
}

/// Get WASM blob for the provided code hash at the provided block hash.
///
/// This method returns WASM blob bytes if present in the provided block.
pub async fn pristine_code<C: Request>(
    api: &Api<PolkadotConfig, C>,
    at: H256,
    code_hash: H256,
) -> Result<Option<Vec<u8>>, Error> {
    api.get_storage_map::<_, PrefabWasmModule>("Contracts", "CodeStorage", code_hash, Some(at))
        .await
        .map(|val| val.map(|module| module.code))
}

/// Get information on all available contracts at the provided block hash.
///
/// This method returns an asynchronous [`Stream`] of [`StorageKey`] (which can be decoded to receive the contract address value)
/// and associated contract information.
pub async fn contract_info_of_root<C: Request + Send + Sync>(
    api: &Api<PolkadotConfig, C>,
    at: H256,
) -> Result<impl Stream<Item = Result<Vec<(StorageKey, ContractInfo)>, Error>> + '_, Error> {
    let prefix = api
        .get_storage_map_key_prefix("Contracts", "ContractInfoOf")
        .await?;

    Ok(paged_key_values(prefix, api, at, identity).await)
}

/// Get information about the specific contract at the provided block hash.
///
/// This method returns associated contract information if present in the provided block.
pub async fn contract_info_of<C: Request>(
    api: &Api<PolkadotConfig, C>,
    at: H256,
    account_id: &AccountId32,
) -> Result<Option<ContractInfo>, Error> {
    api.get_storage_map("Contracts", "ContractInfoOf", account_id, Some(at))
        .await
}

/// Get UNIX timestamp in milliseconds for the provided block hash.
pub async fn block_timestamp_millis<C: Request>(
    api: &Api<PolkadotConfig, C>,
    at: H256,
) -> Result<u64, Error> {
    Ok(api
        .get_storage("Timestamp", "Now", Some(at))
        .await?
        .expect("timestamp is always expected to be present"))
}

/// Call the contract with the provided [`AccountId32`] and raw call data.
///
/// Provided raw call data should match the ABI of the contract.
pub async fn call_contract<C: Request + Subscribe>(
    api: &Api<PolkadotConfig, C>,
    contract: AccountId32,
    data: Vec<u8>,
) -> Result<ContractExecResult<<PolkadotConfig as Config>::Balance, ()>, Error> {
    #[derive(Encode)]
    pub struct CallRequest {
        origin: AccountId32,
        dest: AccountId32,
        value: u128,
        gas_limit: Option<u128>,
        storage_deposit_limit: Option<u128>,
        input_data: Vec<u8>,
    }

    let request = CallRequest {
        // Dummy address
        origin: contract.clone(),
        dest: contract,
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        input_data: data,
    };

    let mut params = RpcParams::new();

    params
        .insert("ContractsApi_call")
        .map_err(|val| Error::Other(Box::new(val)))?;
    params
        .insert(format!("0x{}", hex::encode(request.encode())))
        .map_err(|val| Error::Other(Box::new(val)))?;

    let bytes: String = api.client().request("state_call", params).await?;

    let result = ContractExecResult::decode(
        &mut &*hex::decode(bytes.strip_prefix("0x").unwrap_or(&bytes))
            .map_err(|val| Error::Other(Box::new(val)))?,
    )?;

    Ok(result)
}

/// Node metadata cache.
#[derive(Debug)]
pub struct MetadataCache {
    cache: LruCache<(u32, u32, u32), Metadata>,
}

impl MetadataCache {
    /// Create new [`MetadataCache`].
    pub fn new() -> Self {
        Default::default()
    }

    /// Get metadata associated with the provided block hash.
    ///
    /// This method requests node runtime version corresponding to the provided block,
    /// and either fetches it from node or retrieves from cache.
    pub async fn metadata<C: Request>(
        &mut self,
        api: &Api<PolkadotConfig, C>,
        at: H256,
    ) -> Result<Metadata, Error> {
        let RuntimeVersion {
            authoring_version,
            spec_version,
            impl_version,
            ..
        } = api
            .client()
            .request("state_getRuntimeVersion", rpc_params![at])
            .await?;

        if let Some(metadata) = self
            .cache
            .get(&(authoring_version, spec_version, impl_version))
        {
            Ok(metadata.clone())
        } else {
            let metadata_bytes: Bytes = api
                .client()
                .request("state_getMetadata", rpc_params![Some(at)])
                .await?;

            let runtime_metadata =
                RuntimeMetadataPrefixed::decode(&mut metadata_bytes.0.as_slice())?;
            let metadata: Metadata = runtime_metadata.try_into()?;

            self.cache.push(
                (authoring_version, spec_version, impl_version),
                metadata.clone(),
            );

            Ok(metadata)
        }
    }
}

impl Default for MetadataCache {
    fn default() -> Self {
        Self {
            cache: LruCache::new(NonZeroUsize::new(5).unwrap()),
        }
    }
}

/// Fetch events associated with the provided block hash.
///
/// Since events layout may differ between different runtime upgrades,
/// this method accepts [`MetadataCache`] to correctly query node for the corresponding metadata.
pub async fn events<C: Request>(
    api: &Api<PolkadotConfig, C>,
    at: H256,
    metadata_cache: &mut MetadataCache,
) -> Result<Events<H256>, Error> {
    let key = storage_key("System", "Events");
    let event_bytes = api
        .get_opaque_storage_by_key(key, Some(at))
        .await?
        .ok_or(Error::BlockNotFound)?;

    Ok(Events::new(
        metadata_cache.metadata(api, at).await?,
        Default::default(),
        event_bytes,
    ))
}

/// Contract instantiation event.
#[derive(Decode)]
pub struct Instantiated {
    /// [`AccountId32`] value of the deployer.
    pub deployer: AccountId32,

    /// [`AccountId32`] value of the contract itself.
    pub contract: AccountId32,
}

impl StaticEvent for Instantiated {
    const PALLET: &'static str = "Contracts";
    const EVENT: &'static str = "Instantiated";
}

/// WASM code upload event.
#[derive(Decode)]
pub struct CodeStored {
    /// Code hash value of the uploaded WASM code.
    pub code_hash: H256,
}

impl StaticEvent for CodeStored {
    const PALLET: &'static str = "Contracts";
    const EVENT: &'static str = "CodeStored";
}

/// Associated code hash of a contract was changed.
#[derive(Decode)]
pub struct ContractCodeUpdated {
    /// [`AccountId32`] value of the associated contract.
    pub contract: AccountId32,

    /// New code hash value associated with the current contract.
    pub new_code_hash: H256,

    /// Previous code hash value.
    pub old_code_hash: H256,
}

impl StaticEvent for ContractCodeUpdated {
    const PALLET: &'static str = "Contracts";
    const EVENT: &'static str = "ContractCodeUpdated";
}

/// Contract termination event.
#[derive(Decode)]
pub struct Terminated {
    /// [`AccountId32`] value of a contract that got terminated.
    pub contract: AccountId32,
    _beneficiary: AccountId32,
}

impl StaticEvent for Terminated {
    const PALLET: &'static str = "Contracts";
    const EVENT: &'static str = "Terminated";
}

// Get storage keys and values with the provided prefix, mapping values in process.
async fn paged_key_values<C: Request, V: Decode, T, F: FnMut(V) -> T + 'static>(
    prefix: StorageKey,
    api: &Api<PolkadotConfig, C>,
    at: H256,
    map: F,
) -> impl Stream<Item = Result<Vec<(StorageKey, T)>, Error>> + '_ {
    try_unfold(
        (None, prefix, map),
        move |(start_key, prefix, mut map)| async move {
            let storage_keys = api
                .get_storage_keys_paged(Some(prefix.clone()), PAGE_SIZE, start_key, Some(at))
                .await?;

            if storage_keys.is_empty() {
                return Ok(None);
            }

            let start_key = storage_keys.last().cloned();

            let values = stream::iter(storage_keys)
                .then(move |storage_key| async move {
                    let value = api
                        .get_storage_by_key(storage_key.clone(), Some(at))
                        .await?
                        .expect("unable to find value corresponding to the provided storage key");

                    Result::<_, Error>::Ok((storage_key, value))
                })
                .map_ok(|(key, val)| (key, map(val)))
                .try_collect()
                .await?;

            Result::<_, Error>::Ok(Some((values, (start_key, prefix, map))))
        },
    )
}
