use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct HexHash(#[serde(with = "hex")] pub [u8; 32]);
