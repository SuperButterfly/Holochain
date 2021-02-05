//! Implements base-64 de/serialization for HoloHash
// NB: this module has a lot of copy-and-paste from `ser.rs`

use super::*;
use crate::HoloHash;
use crate::{error::HoloHashResult, HashType};

/// A wrapper around HoloHash to denote that deserialization should use
/// base-64 strings rather than raw byte arrays
//
// TODO: make HoloHash and HoloHashB64 both deserialize identically (can take either string or seq),
//       so that the only difference is how they serialize
#[derive(
    Debug,
    Clone,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    derive_more::Constructor,
    derive_more::Display,
    derive_more::From,
    derive_more::Into,
    derive_more::AsRef,
)]
pub struct HoloHashB64<T: HashType>(HoloHash<T>);

impl<T: HashType> HoloHashB64<T> {
    /// Read a HoloHash from base64 string
    pub fn from_b64_str(str: &str) -> HoloHashResult<Self> {
        let bytes = holo_hash_decode_unchecked(str)?;
        HoloHash::from_raw_39(bytes).map(Into::into)
    }
}

impl<T: HashType> serde::Serialize for HoloHashB64<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&holo_hash_encode(self.0.get_raw_39()))
    }
}

impl<'de, T: HashType> serde::Deserialize<'de> for HoloHashB64<T> {
    fn deserialize<D>(deserializer: D) -> Result<HoloHashB64<T>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_bytes(HoloHashB64Visitor(std::marker::PhantomData))
    }
}

struct HoloHashB64Visitor<T: HashType>(std::marker::PhantomData<T>);

impl<'de, T: HashType> serde::de::Visitor<'de> for HoloHashB64Visitor<T> {
    type Value = HoloHashB64<T>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a HoloHash of primitive hash_type")
    }

    fn visit_str<E>(self, b64: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let h = holo_hash_decode_unchecked(b64)
            .map_err(|e| serde::de::Error::custom(format!("HoloHash error: {:?}", e)))?;
        if !h.len() == 39 {
            Err(serde::de::Error::custom(
                "HoloHash serialized representation must be exactly 39 bytes",
            ))
        } else {
            let inner = HoloHash::from_raw_39(h.to_vec())
                .map_err(|e| serde::de::Error::custom(format!("HoloHash error: {:?}", e)))?;
            Ok(HoloHashB64(inner))
        }
    }
}

// NB: These could be macroized, but if we spell it out, we get better IDE
// support

/// Base64-ready version of AgentPubKey
pub type AgentPubKeyB64 = HoloHashB64<hash_type::Agent>;

/// Base64-ready version of DnaHash
pub type DnaHashB64 = HoloHashB64<hash_type::Dna>;

/// Base64-ready version of DhtOpHash
pub type DhtOpHashB64 = HoloHashB64<hash_type::DhtOp>;

/// Base64-ready version of EntryHash
pub type EntryHashB64 = HoloHashB64<hash_type::Entry>;

/// Base64-ready version of HeaderHash
pub type HeaderHashB64 = HoloHashB64<hash_type::Header>;

/// Base64-ready version of NetIdHash
pub type NetIdHashB64 = HoloHashB64<hash_type::NetId>;

/// Base64-ready version of WasmHash
pub type WasmHashB64 = HoloHashB64<hash_type::Wasm>;

/// Base64-ready version of AnyDhtHash
pub type AnyDhtHashB64 = HoloHashB64<hash_type::AnyDht>;
