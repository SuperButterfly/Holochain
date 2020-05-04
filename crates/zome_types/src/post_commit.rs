use holo_hash_core::HeaderHash;
use holochain_serialized_bytes::prelude::*;

#[derive(PartialEq, Serialize, Deserialize, SerializedBytes)]
pub enum PostCommitCallbackResult {
    Success(HeaderHash),
    Fail(HeaderHash, String),
}
