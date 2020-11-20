//! holochain_types::dna::zome is a set of structs for working with holochain dna.

use std::sync::Arc;

use derive_more::Constructor;
use holochain_serialized_bytes::prelude::*;
use holochain_zome_types::zome::ZomeName;

use self::inline_zome::InlineZome;

use super::{error::DnaResult, DnaError};

pub mod inline_zome;

/// An internal type, joining a ZomeDef with its name
#[derive(
    Serialize,
    Deserialize,
    Hash,
    Clone,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    shrinkwraprs::Shrinkwrap,
)]
pub struct Zome {
    name: ZomeName,
    #[shrinkwrap(main_field)]
    def: ZomeDef,
}

impl Zome {
    /// Constructor
    pub fn new(name: ZomeName, def: ZomeDef) -> Self {
        Self { name, def }
    }

    /// Accessor
    pub fn zome_name(&self) -> &ZomeName {
        &self.name
    }

    /// Accessor
    pub fn zome_def(&self) -> &ZomeDef {
        &self.def
    }

    /// Split into components
    pub fn into_inner(self) -> (ZomeName, ZomeDef) {
        (self.name, self.def)
    }
}

impl From<(ZomeName, ZomeDef)> for Zome {
    fn from(pair: (ZomeName, ZomeDef)) -> Self {
        Self::new(pair.0, pair.1)
    }
}

impl From<Zome> for (ZomeName, ZomeDef) {
    fn from(zome: Zome) -> Self {
        zome.into_inner()
    }
}

impl From<Zome> for ZomeName {
    fn from(zome: Zome) -> Self {
        zome.name
    }
}

impl From<Zome> for ZomeDef {
    fn from(zome: Zome) -> Self {
        zome.def
    }
}

/// Represents an individual "zome" definition.
#[derive(
    Serialize, Deserialize, Hash, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, derive_more::From,
)]
// This can be untagged, since the only valid serialization target is WasmZome
#[serde(untagged)]
pub enum ZomeDef {
    /// A zome defined by Wasm bytecode
    Wasm(WasmZome),

    /// A zome defined by Rust closures
    #[serde(skip)]
    Inline(Arc<InlineZome>),
}

impl ZomeDef {
    /// If this is a Wasm zome, return the WasmHash.
    /// If not, return an error with the provided zome name
    pub fn wasm_hash(&self, zome_name: &ZomeName) -> DnaResult<holo_hash::WasmHash> {
        match self {
            ZomeDef::Wasm(WasmZome { wasm_hash }) => Ok(wasm_hash.clone()),
            _ => Err(DnaError::NonWasmZome(zome_name.clone())),
        }
    }
}

/// A zome defined by Wasm bytecode
#[derive(
    Serialize, Deserialize, Hash, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, SerializedBytes,
)]
pub struct WasmZome {
    /// The WasmHash representing the WASM byte code for this zome.
    pub wasm_hash: holo_hash::WasmHash,
}

/// Access a call has to host functions
#[derive(Debug, Copy, Clone, Constructor, PartialEq)]
pub struct HostFnAccess {
    /// Can access agent information
    pub agent_info: Permission,
    /// Can access the workspace
    pub read_workspace: Permission,
    /// Can write and workspace
    pub write_workspace: Permission,
    /// Can write to the network
    pub write_network: Permission,
    /// Can access dna and zome specific data
    pub dna_bindings: Permission,
    /// All other non-deterministic functions
    pub non_determinism: Permission,
    /// Access to functions that use the keystore in the conductor
    pub keystore: Permission,
}

#[derive(Debug, Copy, Clone, PartialEq)]
/// Permission granted to a call
pub enum Permission {
    /// Host functions with this access will be included
    Allow,
    /// Host functions with this access will be unreachable
    Deny,
}

impl ZomeDef {
    /// create a Zome from a holo_hash WasmHash instead of a holo_hash one
    pub fn from_hash(wasm_hash: holo_hash::WasmHash) -> Self {
        WasmZome { wasm_hash }.into()
    }
}

impl HostFnAccess {
    /// Allow all access
    pub fn all() -> Self {
        HostFnAccess {
            read_workspace: Permission::Allow,
            write_workspace: Permission::Allow,
            agent_info: Permission::Allow,
            non_determinism: Permission::Allow,
            write_network: Permission::Allow,
            keystore: Permission::Allow,
            dna_bindings: Permission::Allow,
        }
    }

    /// Deny all access
    pub fn none() -> Self {
        HostFnAccess {
            read_workspace: Permission::Deny,
            write_workspace: Permission::Deny,
            agent_info: Permission::Deny,
            non_determinism: Permission::Deny,
            write_network: Permission::Deny,
            keystore: Permission::Deny,
            dna_bindings: Permission::Deny,
        }
    }
}
