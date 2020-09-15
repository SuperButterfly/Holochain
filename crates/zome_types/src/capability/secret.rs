use holochain_serialized_bytes::prelude::*;
use serde::de::Error;
use subtle::ConstantTimeEq;

/// The number of bits we want for a comfy secret.
pub const CAP_SECRET_BITS: usize = 512;
/// The number of bytes we want for a comfy secret.
pub const CAP_SECRET_BYTES: usize = CAP_SECRET_BITS / 8;
/// A fixed size array of bytes that a secret must be.
pub type CapSecretBytes = [u8; CAP_SECRET_BYTES];

/// A CapSecret is used by a caller to prove to a callee access to a committed CapGrant.
///
/// It is a random, unique identifier for the capability, which is shared by
/// the grantor to allow access to others. The grantor can optionally further restrict usage of the
/// secret to specific agents.
///
/// @todo enforce that secrets are unique across all grants in a chain.
#[derive(Clone, Copy, SerializedBytes)]
pub struct CapSecret(CapSecretBytes);

/// Serialize CapSecret as serde_bytes style.
/// This is needed because Serialize cannot be derived for "large" fixed size arrays.
impl serde::ser::Serialize for CapSecret {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

/// Custom Deserialize for CapSecret.
/// Requires serde_bytes style binary data and the correct number of bytes.
/// This is needed because Deserialize cannot be derived for "large" fixed size arrays.
impl<'de> serde::de::Deserialize<'de> for CapSecret {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let bytes: &[u8] = serde::de::Deserialize::deserialize(deserializer)?;
        if bytes.len() != CAP_SECRET_BYTES {
            return Err(D::Error::invalid_value(
                serde::de::Unexpected::Bytes(bytes),
                &"incorrect length cap secret",
            ));
        }
        let mut inner: [u8; CAP_SECRET_BYTES] = [0; CAP_SECRET_BYTES];
        inner.clone_from_slice(bytes);
        Ok(Self(inner))
    }
}

/// Trivial new type derivation.
impl From<[u8; CAP_SECRET_BYTES]> for CapSecret {
    fn from(b: [u8; CAP_SECRET_BYTES]) -> Self {
        Self(b)
    }
}

/// Constant time equality check for CapSecret.
/// This mitigates timing attacks where a remote agent can reverse engineer a valid grant by
/// measuring tiny changes in latency associated with optimised equality checks.
/// More matching bytes = more latency = vulnerability.
/// This type of attack has been successfully demonstrated over a network despite varied latencies.
impl PartialEq for CapSecret {
    fn eq(&self, other: &Self) -> bool {
        self.0.ct_eq(&other.0).into()
    }
}

impl Eq for CapSecret {}

impl std::fmt::Debug for CapSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.0.to_vec(), f)
    }
}

/// It's fairly common (e.g. when calling unrestricted functions) to need to generate a cap secret
/// that is not secret at all but it doesn't matter because it will be ignored by the recipient.
/// To try and make it as clear as possible that this is 'empty' and 'insecure' the pattern is to
/// coerce ().into() rather than CapSecret::default() or CapSecret::new().
///
/// This is relatively harmless on the claim side, sending an empty secret as a claim will pass for
/// Unrestricted zome calls and simply be Unauthorized on anything else.
///
/// This is DANGEROUS on the grant side.
/// Do NOT commit a cap grant with an empty/predictable secret.
///
/// @todo sys validation to prevent empty secret assigned/transferable cap grants
impl From<()> for CapSecret {
    fn from(_: ()) -> Self {
        Self::from([0; CAP_SECRET_BYTES])
    }
}
