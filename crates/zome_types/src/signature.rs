//! Signature for authenticity of data
use holo_hash::AgentPubKey;
use holochain_serialized_bytes::prelude::*;

/// Input structure for creating a signature.
#[derive(Debug, PartialEq, Serialize, Deserialize, SerializedBytes, Clone)]
pub struct SignInput {
    /// The public key associated with the private key that should be used to
    /// generate the signature.
    pub key: holo_hash::AgentPubKey,

    /// The data that should be signed.
    pub data: SerializedBytes,
}

impl SignInput {
    /// construct a new SignInput struct.
    pub fn new<D>(key: holo_hash::AgentPubKey, data: D) -> Result<Self, SerializedBytesError>
    where
        D: TryInto<SerializedBytes, Error = SerializedBytesError>,
    {
        let data: SerializedBytes = data.try_into()?;
        Ok(Self { key, data })
    }

    /// construct a new SignInput struct from raw bytes.
    pub fn new_raw(key: holo_hash::AgentPubKey, data: Vec<u8>) -> Self {
        Self {
            key,
            data: UnsafeBytes::from(data).into(),
        }
    }

    /// key getter
    pub fn key(&self) -> &AgentPubKey {
        &self.key
    }

    /// data getter
    pub fn data(&self) -> &SerializedBytes {
        &self.data
    }
}

/// The raw bytes of a signature.
#[derive(Clone, Serialize, Deserialize, SerializedBytes, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Signature(#[serde(with = "serde_bytes")] pub Vec<u8>);

impl AsRef<[u8]> for Signature {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Debug for Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("Signature(0x"))?;
        for byte in &self.0 {
            f.write_fmt(format_args!("{:02x}", byte))?;
        }
        f.write_fmt(format_args!(")"))?;
        Ok(())
    }
}
