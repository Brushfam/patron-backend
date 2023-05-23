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

const PAGE_SIZE: u32 = 10;

pub enum Schema<T> {
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

#[derive(Debug)]
pub struct InvalidSchema;

impl Display for InvalidSchema {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "invalid schema")
    }
}

impl std::error::Error for InvalidSchema {}

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

pub struct ContractInfo {
    pub code_hash: H256,
}

pub enum ContractEvent {
    CodeStored,
    Instantiated,
    ContractCodeUpdated,
    Terminated,
}

pub enum ContractEventData {
    CodeStored {
        code_hash: H256,
    },

    Instantiated {
        contract: AccountId32,
        deployer: AccountId32,
    },

    ContractCodeUpdated {
        contract: AccountId32,
        new_code_hash: H256,
    },

    Terminated {
        contract: AccountId32,
    },
}

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
