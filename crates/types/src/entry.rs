//! An Entry is a unit of data in a Holochain Source Chain.
//!
//! This module contains all the necessary definitions for Entry, which broadly speaking
//! refers to any data which will be written into the ContentAddressableStorage, or the EntityAttributeValueStorage.
//! It defines serialization behaviour for entries. Here you can find the complete list of
//! entry_types, and special entries, like deletion_entry and cap_entry.

//use crate::dna::Dna;
use holo_hash::*;
use holochain_serialized_bytes::prelude::*;

/// Structure holding the entry portion of a chain element.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, SerializedBytes)]
#[allow(clippy::large_enum_variant)]
#[serde(tag = "entry_type", content = "entry")]
pub enum Entry {
    /// The AgentKey system entry, the second entry of every source chain,
    /// which grants authoring capability for this agent. (Name TBD)
    AgentKey(AgentHash),
    /// The application entry data for entries that aren't system created entries
    App(SerializedBytes),
}

impl Entry {
    /// Get the EntryAddress of this entry
    // FIXME: use async with_data, or consider wrapper type
    // https://github.com/Holo-Host/holochain-2020/pull/86#discussion_r413226841
    pub fn entry_address(&self) -> EntryAddress {
        match self {
            Entry::AgentKey(key) => EntryAddress::Agent(key.to_owned()),
            Entry::App(serialized_bytes) => {
                EntryAddress::Entry(EntryHash::with_data_sync(&serialized_bytes.bytes()))
            }
        }
    }
}

/// wraps hashes that can be used as addresses for entries e.g. in a CAS
#[derive(Debug, Clone, derive_more::From, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntryAddress {
    /// standard entry hash
    Entry(EntryHash),
    /// agents are entries too
    Agent(AgentHash),
}

impl From<EntryAddress> for HoloHash {
    fn from(entry_address: EntryAddress) -> HoloHash {
        match entry_address {
            EntryAddress::Entry(entry_hash) => entry_hash.into(),
            EntryAddress::Agent(agent_hash) => agent_hash.into(),
            //      EntryAddress::Dna(dna_hash) => dna_hash.into(),
        }
    }
}

impl TryFrom<&Entry> for EntryAddress {
    type Error = SerializedBytesError;
    fn try_from(entry: &Entry) -> Result<Self, Self::Error> {
        Ok(EntryAddress::Entry(EntryHash::try_from(entry)?))
    }
}

impl AsRef<[u8]> for &EntryAddress {
    fn as_ref(&self) -> &[u8] {
        match self {
            EntryAddress::Entry(entry_hash) => entry_hash.as_ref(),
            EntryAddress::Agent(agent_hash) => agent_hash.as_ref(),
        }
    }
}

impl std::fmt::Display for EntryAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            EntryAddress::Entry(entry_hash) => write!(f, "{}", entry_hash),
            EntryAddress::Agent(agent_hash) => write!(f, "{}", agent_hash),
            //  EntryAddress::Dna(dna_hash) => write!(f, "{}", dna_hash),
        }
    }
}
