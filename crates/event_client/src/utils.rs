use common::rpc::subxt::{
    self,
    client::OnlineClientT,
    rpc::types::{BlockNumber, NumberOrHex},
    Config,
};
use futures_util::{stream, Stream, StreamExt, TryStreamExt};

const TWOX_HASH_LEN: usize = 8;
const STORAGE_PREFIX_LEN: usize = 32;

pub(crate) fn extract_twox_account_id<T: AsRef<[u8]>>(key: T) -> Vec<u8> {
    key.as_ref()[STORAGE_PREFIX_LEN + TWOX_HASH_LEN..].to_owned()
}

pub(crate) fn extract_code_hash<T: AsRef<[u8]>>(key: T) -> Vec<u8> {
    key.as_ref()[STORAGE_PREFIX_LEN..].to_owned()
}

/// Get a mapping stream from block number to block hash.
///
/// The stream may blocks, to which an RPC node did not provide a hash.
pub(crate) fn block_mapping_stream<
    'a,
    I: IntoIterator<Item = u64> + 'a,
    T: Config,
    C: OnlineClientT<T>,
>(
    range: I,
    api: &'a C,
) -> impl Stream<Item = Result<(u64, T::Hash), subxt::Error>> + 'a {
    stream::iter(range.into_iter())
        .map(Ok)
        .try_filter_map(move |block_number| async move {
            Ok(api
                .rpc()
                .block_hash(Some(BlockNumber::from(NumberOrHex::from(block_number))))
                .await?
                .map(|hash| (block_number, hash)))
        })
}
