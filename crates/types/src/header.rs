//! Holochain's [`Header`] and its variations.
//!
//! All header variations contain the fields `author` and `timestamp`.
//! Furthermore, all variations besides pub struct `Dna` (which is the first header
//! in a chain) contain the field `prev_header`.

#![allow(missing_docs)]

use crate::{metadata::TimedHeaderHash, prelude::*};
use holo_hash::EntryHash;
use holochain_zome_types::entry_def::EntryVisibility;
use holochain_zome_types::header::*;

pub type HeaderHashed = HoloHashed<Header>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, SerializedBytes)]
/// A header of one of the two types that create a new entry.
pub enum NewEntryHeader {
    /// A header which simply creates a new entry
    Create(EntryCreate),
    /// A header which creates a new entry that is semantically related to a
    /// previously created entry or header
    Update(EntryUpdate),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, SerializedBytes, Ord, PartialOrd)]
/// A header of one of the two types that create a new entry.
pub enum WireNewEntryHeader {
    /// A header which simply creates a new entry
    Create(MinNewEntryHeader),
    /// A pair containing the minimum data which creates a new entry
    /// and the header it updates.
    /// Note: IntendedFor::Entry is implied.
    Update((MinNewEntryHeader, HeaderHash)),
}

/// The minimum unique data for new entry header
/// that share a common entry
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, SerializedBytes, Ord, PartialOrd)]
pub struct MinNewEntryHeader {
    /// Timestamp is first so that deriving Ord results in
    /// order by time
    pub timestamp: holochain_zome_types::timestamp::Timestamp,
    pub author: AgentPubKey,
    pub header_seq: u32,
    pub prev_header: HeaderHash,
    pub signature: Signature,
}

/// A element delete hash pair for sending over the network
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, SerializedBytes, Hash)]
pub struct WireDelete {
    pub element_delete_address: TimedHeaderHash,
    pub removes_address: HeaderHash,
}

impl NewEntryHeader {
    /// Get the entry on this header
    pub fn entry(&self) -> &EntryHash {
        match self {
            NewEntryHeader::Create(EntryCreate { entry_hash, .. })
            | NewEntryHeader::Update(EntryUpdate { entry_hash, .. }) => entry_hash,
        }
    }

    /// Get the visibility of this header
    pub fn visibility(&self) -> &EntryVisibility {
        match self {
            NewEntryHeader::Create(EntryCreate { entry_type, .. })
            | NewEntryHeader::Update(EntryUpdate { entry_type, .. }) => entry_type.visibility(),
        }
    }
}

impl From<NewEntryHeader> for Header {
    fn from(h: NewEntryHeader) -> Self {
        match h {
            NewEntryHeader::Create(h) => Header::EntryCreate(h),
            NewEntryHeader::Update(h) => Header::EntryUpdate(h),
        }
    }
}

impl From<(EntryCreate, Signature)> for MinNewEntryHeader {
    fn from((ec, signature): (EntryCreate, Signature)) -> Self {
        Self {
            timestamp: ec.timestamp,
            author: ec.author,
            header_seq: ec.header_seq,
            prev_header: ec.prev_header,
            signature,
        }
    }
}

impl From<(EntryUpdate, Signature)> for MinNewEntryHeader {
    fn from((eu, signature): (EntryUpdate, Signature)) -> Self {
        Self {
            timestamp: eu.timestamp,
            author: eu.author,
            header_seq: eu.header_seq,
            prev_header: eu.prev_header,
            signature,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        fixt::HeaderBuilderCommonFixturator,
        test_utils::{fake_dna_hash, fake_entry_content_hash},
    };
    use ::fixt::prelude::*;

    #[test]
    fn test_header_msgpack_roundtrip() {
        let orig: Header = Dna::from_builder(
            fake_dna_hash(1),
            HeaderBuilderCommonFixturator::new(Unpredictable)
                .next()
                .unwrap(),
        )
        .into();
        let bytes = rmp_serde::to_vec_named(&orig).unwrap();
        let res: Header = rmp_serde::from_read_ref(&bytes).unwrap();
        assert_eq!(orig, res);
    }

    #[test]
    fn test_entrycreate_msgpack_roundtrip() {
        let orig: Header = EntryCreate::from_builder(
            HeaderBuilderCommonFixturator::new(Unpredictable)
                .next()
                .unwrap(),
            EntryType::App(AppEntryType::new(
                0.into(),
                0.into(),
                EntryVisibility::Public,
            )),
            fake_entry_content_hash(1).into(),
        )
        .into();
        let bytes = rmp_serde::to_vec_named(&orig).unwrap();
        println!("{:?}", bytes);
        let res: Header = rmp_serde::from_read_ref(&bytes).unwrap();
        assert_eq!(orig, res);
    }

    #[test]
    fn test_entrycreate_serializedbytes_roundtrip() {
        let orig: Header = EntryCreate::from_builder(
            HeaderBuilderCommonFixturator::new(Unpredictable)
                .next()
                .unwrap(),
            EntryType::App(AppEntryType::new(
                0.into(),
                0.into(),
                EntryVisibility::Public,
            )),
            fake_entry_content_hash(1).into(),
        )
        .into();
        let bytes: SerializedBytes = orig.clone().try_into().unwrap();
        let res: Header = bytes.try_into().unwrap();
        assert_eq!(orig, res);
    }
}
