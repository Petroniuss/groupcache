//! Serialization codec for peer-to-peer value transfer.
//!
//! By default, values are serialized using MessagePack (rmp-serde).
//! Enable the `bincode` feature for faster serialization at the cost
//! of a less compact wire format.

use serde::{Deserialize, Serialize};

/// Serialize a value to bytes for peer-to-peer transfer.
#[cfg(not(feature = "bincode"))]
pub(crate) fn serialize<T: Serialize>(value: &T) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    rmp_serde::to_vec(value).map_err(|e| Box::new(e) as _)
}

/// Deserialize a value from bytes received from a peer.
#[cfg(not(feature = "bincode"))]
pub(crate) fn deserialize<T: for<'a> Deserialize<'a>>(bytes: &[u8]) -> Result<T, Box<dyn std::error::Error + Send + Sync>> {
    rmp_serde::from_slice(bytes).map_err(|e| Box::new(e) as _)
}

/// Serialize a value to bytes for peer-to-peer transfer.
#[cfg(feature = "bincode")]
pub(crate) fn serialize<T: Serialize>(value: &T) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    bincode::serialize(value).map_err(|e| Box::new(e) as _)
}

/// Deserialize a value from bytes received from a peer.
#[cfg(feature = "bincode")]
pub(crate) fn deserialize<T: for<'a> Deserialize<'a>>(bytes: &[u8]) -> Result<T, Box<dyn std::error::Error + Send + Sync>> {
    bincode::deserialize(bytes).map_err(|e| Box::new(e) as _)
}
