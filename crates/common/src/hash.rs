use blake2::{digest::typenum::U32, Blake2b, Digest};

pub fn blake2(data: &[u8]) -> [u8; 32] {
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(data);
    hasher.finalize().into()
}
