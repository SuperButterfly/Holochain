//! kdirect kdsyskind types

use crate::*;

/// A type that is compatible with the "data" field of a KdEntryContent,
/// with a given "kind".
pub trait AsKdSysKind: 'static + Send + std::fmt::Display {
    /// convert this KdSysKind into a json value
    fn to_json(&self) -> KdResult<serde_json::Value>;
}

/// A unifying enum that allows us to pull them out of data too
#[derive(Debug)]
pub enum KdSysKind {
    /// s.app sys kind
    App(KdSysKindApp),

    /// unrecognized sys kind
    Unrecognized(serde_json::Value),
}

impl std::fmt::Display for KdSysKind {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::App(a) => a.fmt(f),
            Self::Unrecognized(v) => v.fmt(f),
        }
    }
}

impl AsKdSysKind for KdSysKind {
    fn to_json(&self) -> KdResult<serde_json::Value> {
        match self {
            Self::App(a) => serde_json::to_value(a),
            Self::Unrecognized(v) => serde_json::to_value(v),
        }
        .map_err(KdError::other)
    }
}

macro_rules! as_kd_sys_kind {
    ($i:ident) => {
        impl ::std::fmt::Display for $i {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                let s = ::serde_json::to_string_pretty(self).map_err(|_| ::std::fmt::Error)?;
                f.write_str(&s)?;
                Ok(())
            }
        }

        impl AsKdSysKind for $i {
            fn to_json(&self) -> $crate::KdResult<::serde_json::Value> {
                ::serde_json::to_value(self).map_err($crate::KdError::other)
            }
        }
    };
}

/// Kitsune Direct 's.app' additional data struct
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct KdSysKindApp {
    /// The simple common name of this app
    #[serde(rename = "name")]
    pub name: String,
}

as_kd_sys_kind!(KdSysKindApp);
