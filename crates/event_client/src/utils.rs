use common::rpc::{
    sp_core::H256,
    substrate_api_client::{ac_primitives::PolkadotConfig, rpc::Request, Api, Error, GetChainInfo},
};
use futures_util::{stream, Stream, StreamExt, TryStreamExt};

/// TwoX hash length.
const TWOX_HASH_LEN: usize = 8;

/// Module + method information length.
const STORAGE_PREFIX_LEN: usize = 32;

/// Extract account id from the provided storage key.
///
/// For more information on the extraction algorithm consult [polkadot{.js}]'s
/// chain state section. You can preview the details of any storage key
/// of a deployed smart contract.
///
/// [polkadot{.js}]: https://polkadot.js.org
pub(crate) fn extract_twox_account_id<T: AsRef<[u8]>>(key: T) -> Vec<u8> {
    key.as_ref()[STORAGE_PREFIX_LEN + TWOX_HASH_LEN..].to_owned()
}

/// Extract code hash from the provided storage key.
///
/// For more information on the extraction algorithm consult [polkadot{.js}]'s
/// chain state section. You can preview the details of any storage key
/// of an uploaded WASM blob.
///
/// [polkadot{.js}]: https://polkadot.js.org
pub(crate) fn extract_code_hash<T: AsRef<[u8]>>(key: T) -> Vec<u8> {
    key.as_ref()[STORAGE_PREFIX_LEN..].to_owned()
}

/// Get a mapping stream from block number to block hash.
///
/// The stream may skip blocks, to which an RPC node did not provide a hash.
pub(crate) fn block_mapping_stream<'a, I: IntoIterator<Item = u32> + 'a, C: Request>(
    range: I,
    api: &'a Api<PolkadotConfig, C>,
) -> impl Stream<Item = Result<(u32, H256), Error>> + 'a {
    stream::iter(range)
        .map(Ok)
        .try_filter_map(move |block_number| async move {
            Ok(api
                .get_block_hash(Some(block_number))
                .await?
                .map(|hash| (block_number, hash)))
        })
}

#[cfg(test)]
mod tests {
    use common::rpc::sp_core::{
        crypto::{AccountId32, Ss58Codec},
        ByteArray,
    };

    #[test]
    fn extract_twox_account_id() {
        let account_id =
            AccountId32::from_ss58check("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY")
                .unwrap();
        let hex_key = "4342193e496fab7ec59d615ed0dc5530060e99e5378e562537cf3bc983e17b91518366b5b1bc7c99d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d";
        let key = hex::decode(hex_key).unwrap();
        assert_eq!(
            super::extract_twox_account_id(&key).as_slice(),
            account_id.as_slice()
        );
    }

    #[test]
    fn extract_code_hash() {
        let hex_key = "4342193e496fab7ec59d615ed0dc553022fca90611ba8b7942f8bdb3b97f65800000000000000000000000000000000000000000000000000000000000000000";
        let key = hex::decode(hex_key).unwrap();
        assert_eq!(super::extract_code_hash(&key), vec![0; 32]);
    }
}
