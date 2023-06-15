use std::array::TryFromSliceError;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Hexidecimal representation of a 32-byte array.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct HexHash(
    #[serde(with = "hex")]
    #[schemars(with = "String")]
    pub [u8; 32],
);

impl TryFrom<&[u8]> for HexHash {
    type Error = TryFromSliceError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        value.try_into().map(Self)
    }
}
