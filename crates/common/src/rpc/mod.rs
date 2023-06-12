//! Schema abstraction over Substrate node RPC.
//!
//! # Schemas
//!
//! By introducing the concept of schemas we can utilize [`subxt`]'s typed
//! RPC bindings while simultaneously providing other crates with abstraction
//! to communicate with different nodes over a unified API.
//!
//! The core component of this module is the `Schema` enum, which allows
//! users to pick a preferred schema for communication. Be aware, that a schema from one
//! network may be supported by another network, even though their names are different.
//!
//! After creating an instance of `Schema`, you may use various methods such as
//! `Schema::block`, `Schema::pristine_code`, etc. to receive relevant information
//! without worrying about node specifics.

/// Supported schemas.
pub mod schemas;

use std::{
    fmt::{self, Display, Formatter},
    marker::PhantomData,
    pin::Pin,
    str::FromStr,
};

use futures_util::{stream::try_unfold, Stream, TryStreamExt};
use itertools::Itertools;
use pallet_contracts_primitives::ContractExecResult;
use parity_scale_codec::{Decode, Encode};
use subxt::{
    blocks::Block,
    client::OnlineClientT,
    events::Events,
    metadata::DecodeWithMetadata,
    storage::{KeyIter, StorageKey},
    utils::{AccountId32, H256},
    Config, Error,
};

pub use parity_scale_codec;
pub use subxt;

/// Default page size for fetching data with [`subxt`].
pub const PAGE_SIZE: u32 = 10;

/// Substrate node RPC schema.
pub enum Schema<T> {
    /// Astar schema, which can be initialized from an "astar" string.
    Astar,

    #[doc(hidden)]
    __Config(PhantomData<T>),
}

impl<T> Copy for Schema<T> {}

impl<T> Clone for Schema<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Astar => Self::Astar,
            _ => unreachable!(),
        }
    }
}

impl<T> FromStr for Schema<T> {
    type Err = InvalidSchema;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "astar" => Ok(Self::Astar),
            _ => Err(InvalidSchema),
        }
    }
}

/// An error that indicates that an invalid schema was provided.
#[derive(Debug)]
pub struct InvalidSchema;

impl Display for InvalidSchema {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "invalid schema")
    }
}

impl std::error::Error for InvalidSchema {}

/// Macro utilities to generate schema mappers.
///
/// See methods below for usage demonstration.
macro_rules! schema_map {
    (fetch $self:ident, $client:ident, $at:ident, $($network:ident, $schema:ident => $address:ident, $mapper:expr, [$($val:expr),+]),+) => {
        match $self {
            $(Schema::$network => {
                let address = schemas::$schema::storage()
                    .contracts()
                    .$address($($val),+);

                $client.storage()
                    .at($at)
                    .fetch(&address)
                    .await?
                    .map($mapper)
            }),+
            _ => unreachable!()
        }
    };

    (stream $self:ident, $client:ident, $at:ident, $($network:ident, $schema:ident => $address:ident, $mapper:expr),+) => {
        match $self {
            $(Schema::$network => {
                let address = schemas::$schema::storage()
                    .contracts()
                    .$address();

                Box::pin(
                    key_iter_to_stream(
                        $client.storage()
                            .at($at)
                            .iter(address, PAGE_SIZE)
                            .await?
                    )
                    .map_ok($mapper),
                )
            }),+
            _ => unreachable!()
        }
    };
}

impl<T: Config + Send + Sync> Schema<T> {
    /// Get a [`Block`] information for the provided block hash.
    ///
    /// If the provided hash is [`None`], the latest block is retrieved.
    pub async fn block<C: OnlineClientT<T>>(
        &self,
        client: &C,
        at: Option<T::Hash>,
    ) -> Result<Block<T, C>, Error> {
        if let Some(hash) = at {
            client.blocks().at(hash).await
        } else {
            client.blocks().at_latest().await
        }
    }

    /// Get information on the stored code at the provided block hash.
    pub async fn pristine_code_root<C: OnlineClientT<T>>(
        &self,
        client: &C,
        at: T::Hash,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<(StorageKey, Vec<u8>), Error>> + Send>>, Error>
    {
        Ok(schema_map!(
            stream self, client, at,
            Astar, astar => pristine_code_root, |(key, wasm)| (key, wasm.0)
        ))
    }

    /// Get WASM blob for the provided code hash at the provided block hash.
    pub async fn pristine_code<C: OnlineClientT<T>>(
        &self,
        client: &C,
        at: T::Hash,
        code_hash: &H256,
    ) -> Result<Option<Vec<u8>>, Error> {
        Ok(schema_map!(
            fetch self, client, at,
            Astar, astar => pristine_code, |wasm| wasm.0, [code_hash]
        ))
    }

    /// Get information on all available contracts at the provided block hash.
    pub async fn contract_info_of_root<C: OnlineClientT<T>>(
        &self,
        client: &C,
        at: T::Hash,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<(StorageKey, ContractInfo), Error>> + Send>>, Error>
    {
        Ok(schema_map!(
            stream self, client, at,
            Astar, astar => contract_info_of_root, |(key, info)| (key, ContractInfo { code_hash: info.code_hash })
        ))
    }

    /// Get information about the specific contract at the provided block hash.
    pub async fn contract_info_of<C: OnlineClientT<T>>(
        &self,
        client: &C,
        at: T::Hash,
        account_id: &AccountId32,
    ) -> Result<Option<ContractInfo>, Error> {
        Ok(schema_map!(
            fetch self, client, at,
            Astar, astar => contract_info_of, |info| ContractInfo { code_hash: info.code_hash }, [account_id]
        ))
    }

    /// Get UNIX timestamp in milliseconds for the provided block hash.
    pub async fn block_timestamp_millis<C: OnlineClientT<T>>(
        &self,
        client: &C,
        at: T::Hash,
    ) -> Result<u64, Error> {
        Ok(match self {
            Schema::Astar => client
                .storage()
                .at(at)
                .fetch(&schemas::astar::storage().timestamp().now())
                .await?
                .unwrap_or(0),
            _ => unreachable!(),
        })
    }

    /// Call the contract with the provided [`AccountId32`] and raw call data.
    ///
    /// Provided raw call data should match the ABI of the contract.
    pub async fn call_contract<C: OnlineClientT<T>>(
        &self,
        client: &C,
        contract: AccountId32,
        data: Vec<u8>,
    ) -> Result<ContractExecResult<u128>, Error> {
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

        let response = client
            .rpc()
            .state_call("ContractsApi_call", Some(&request.encode()), None)
            .await?;

        Ok(ContractExecResult::decode(&mut &*response.0)?)
    }

    /// Map schema-specific event types into generic contract-related events.
    ///
    /// This method will return only the information about the events related to
    /// the passed `needle` argument. You can use [`Iterator::filter_map`] to filter
    /// returned events after calling this method.
    pub fn events<'e>(
        &self,
        events: &'e Events<T>,
        needle: ContractEvent,
    ) -> Box<dyn Iterator<Item = Result<ContractEventData, Error>> + 'e> {
        macro_rules! boxed_events {
            ($($network:ident => $schema:ident, [$($event:ident => $mapper:expr),+]),+) => {
                match (self, needle) {
                    $($((Schema::$network, ContractEvent::$event) => {
                        Box::new(
                            events
                                .find::<schemas::$schema::contracts::events::$event>()
                                .map_ok($mapper)
                        )
                    }),+),+
                    _ => unreachable!()
                }
            };
        }

        boxed_events!(
            Astar => astar, [
                CodeStored => |event| ContractEventData::CodeStored {
                    code_hash: event.code_hash,
                },
                Instantiated => |event| ContractEventData::Instantiated {
                    contract: event.contract,
                    deployer: event.deployer,
                },
                ContractCodeUpdated => |event| ContractEventData::ContractCodeUpdated {
                    contract: event.contract,
                    new_code_hash: event.new_code_hash,
                },
                Terminated => |event| ContractEventData::Terminated {
                    contract: event.contract
                }
            ]
        )
    }
}

/// Generic contract info.
pub struct ContractInfo {
    /// Code hash used for the contract.
    pub code_hash: H256,
}

/// Generic contract events.
///
/// This enum is used to filter relevant events using [`Schema::events`] method.
pub enum ContractEvent {
    /// Search for WASM blob uploads.
    CodeStored,

    /// Search for contract instantiations.
    Instantiated,

    /// Search for contract code hash updates.
    ContractCodeUpdated,

    /// Search for contract terminations.
    Terminated,
}

/// Contract event data.
pub enum ContractEventData {
    /// New WASM blob was uploaded.
    CodeStored {
        /// Code hash of the uploaded WASM blob.
        code_hash: H256,
    },

    /// New contract was instantiated.
    Instantiated {
        /// Contract account id.
        contract: AccountId32,

        /// Account id of an entity that deployed the contract.
        deployer: AccountId32,
    },

    /// Contract hash was updated.
    ContractCodeUpdated {
        /// Related contract account id.
        contract: AccountId32,

        /// New code hash for the related contract.
        new_code_hash: H256,
    },

    /// Contract was terminated.
    Terminated {
        /// Account id of a terminated contract.
        contract: AccountId32,
    },
}

/// Transform a [`KeyIter`] into an asynchronous [`Stream`].
fn key_iter_to_stream<C, Client, ReturnTy>(
    key_iter: KeyIter<C, Client, ReturnTy>,
) -> impl Stream<Item = Result<(StorageKey, ReturnTy), Error>>
where
    C: Config,
    Client: OnlineClientT<C>,
    ReturnTy: DecodeWithMetadata,
{
    try_unfold(key_iter, |mut state| async move {
        state.next().await.map(|val| val.map(|val| (val, state)))
    })
}
