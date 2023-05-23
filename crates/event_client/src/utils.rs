const TWOX_HASH_LEN: usize = 8;
const STORAGE_PREFIX_LEN: usize = 32;

pub(crate) fn extract_twox_account_id<T: AsRef<[u8]>>(key: T) -> Vec<u8> {
    key.as_ref()[STORAGE_PREFIX_LEN + TWOX_HASH_LEN..].to_owned()
}

pub(crate) fn extract_code_hash<T: AsRef<[u8]>>(key: T) -> Vec<u8> {
    key.as_ref()[STORAGE_PREFIX_LEN..].to_owned()
}
