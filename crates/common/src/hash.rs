use blake2::{digest::typenum::U32, Blake2b, Digest};

/// Creates a Blake2b 256-bit hash from the provided input.
///
/// This function is useful to determine the WASM blob code hash,
/// since its algorithm is identical to the one used in Substrate nodes.
pub fn blake2(data: &[u8]) -> [u8; 32] {
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(data);
    hasher.finalize().into()
}
