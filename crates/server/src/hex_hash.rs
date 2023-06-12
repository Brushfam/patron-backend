use serde::{Deserialize, Serialize};

/// Wrapper to serialize and deserialize `[u8; 32]` value as a hex string.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct HexHash(#[serde(with = "hex")] pub [u8; 32]);
