//! Everything to do with App (hApp) installation and uninstallation
//!
//! An App is a essentially a collection of Cells which are intended to be
//! available for a particular Holochain use-case, such as a microservice used
//! by some UI in a broader application.
//!
//! Each Cell maintains its own identity separate from any App.
//! Access to Cells can be shared between different Apps.

mod app_bundle;
mod app_manifest;
pub mod dna_gamut;
pub use app_bundle::*;
pub use app_manifest::app_manifest_validated::*;
pub use app_manifest::*;

use crate::dna::{DnaFile, YamlProperties};
use derive_more::Into;
use holo_hash::{AgentPubKey, DnaHash};
use holochain_serialized_bytes::SerializedBytes;
use holochain_zome_types::cell::CellId;
use std::{collections::HashMap, path::PathBuf};

/// The unique identifier for an installed app in this conductor
pub type InstalledAppId = String;

/// A friendly (nick)name used by UIs to refer to the Cells which make up the app
pub type CellNick = String;

/// The source of the DNA to be installed, either as binary data, or from a path
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DnaSource {
    /// register the dna loaded from a file on disk
    Path(PathBuf),
    /// register the dna as provided in the DnaFile data structure
    DnaFile(DnaFile),
    /// register the dna from an existing registered DNA (assumes properties will be set)
    Hash(DnaHash),
}

/// The instructions on how to get the DNA to be registered
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RegisterDnaPayload {
    /// UUID to override when installing this Dna
    pub uuid: Option<String>,
    /// Properties to override when installing this Dna
    pub properties: Option<YamlProperties>,
    /// The dna source
    pub source: DnaSource,
}

/// A collection of [DnaHash]es paired with an [AgentPubKey] and an app id
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct InstallAppPayload {
    /// The unique identifier for an installed app in this conductor
    pub installed_app_id: InstalledAppId,

    /// The agent to use when creating Cells for this App
    pub agent_key: AgentPubKey,

    /// The Dna paths in this app
    pub dnas: Vec<InstallAppDnaPayload>,
}

/// An [AppBundle] along with an [AgentPubKey] and optional [InstalledAppId]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct InstallAppBundlePayload {
    /// The unique identifier for an installed app in this conductor.
    pub bundle: AppBundle,

    /// The agent to use when creating Cells for this App.
    pub agent_key: AgentPubKey,

    /// The unique identifier for an installed app in this conductor.
    /// If not specified, it will be derived from the app name in the bundle manifest.
    pub installed_app_id: Option<InstalledAppId>,

    /// Include proof-of-membrane-membership data for cells that require it,
    /// keyed by the CellNick specified in the app bundle manifest.
    pub membrane_proofs: HashMap<CellNick, MembraneProof>,
}

/// Information needed to specify a Dna as part of an App
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct InstallAppDnaPayload {
    /// The path of the DnaFile
    pub path: Option<PathBuf>, // This is a deprecated field should register dna contents via register
    /// The path of the DnaFile
    pub hash: Option<DnaHash>, // When path is removed, this will become non-opt
    /// The CellNick which will be assigned to this Dna when installed
    pub nick: CellNick,
    /// Properties to override when installing this Dna
    pub properties: Option<YamlProperties>,
    /// App-specific proof-of-membrane-membership, if required by this app
    pub membrane_proof: Option<MembraneProof>,
}

impl InstallAppDnaPayload {
    /// Create a payload with no YamlProperties or MembraneProof. Good for tests.
    pub fn path_only(path: PathBuf, nick: CellNick) -> Self {
        Self {
            path: Some(path),
            hash: None,
            nick,
            properties: None,
            membrane_proof: None,
        }
    }
    /// Create a payload with no JsonProperties or MembraneProof. Good for tests.
    pub fn hash_only(hash: DnaHash, nick: CellNick) -> Self {
        Self {
            path: None,
            hash: Some(hash),
            nick,
            properties: None,
            membrane_proof: None,
        }
    }
}

/// App-specific payload for proving membership in the membrane of the app
pub type MembraneProof = SerializedBytes;

/// Data about an installed Cell
#[derive(Clone, Debug, Into, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InstalledCell(CellId, CellNick);

impl InstalledCell {
    /// Constructor
    pub fn new(cell_id: CellId, cell_handle: CellNick) -> Self {
        Self(cell_id, cell_handle)
    }

    /// Get the CellId
    pub fn into_id(self) -> CellId {
        self.0
    }

    /// Get the CellNick
    pub fn into_nick(self) -> CellNick {
        self.1
    }

    /// Get the inner data as a tuple
    pub fn into_inner(self) -> (CellId, CellNick) {
        (self.0, self.1)
    }

    /// Get the CellId
    pub fn as_id(&self) -> &CellId {
        &self.0
    }

    /// Get the CellNick
    pub fn as_nick(&self) -> &CellNick {
        &self.1
    }
}

/// A collection of [InstalledCell]s paired with an app id
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InstalledApp {
    /// The unique identifier for an installed app in this conductor
    pub installed_app_id: InstalledAppId,
    /// Cell data for this app
    pub cell_data: Vec<InstalledCell>,
}
