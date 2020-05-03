//! Links interrelate entries in a source chain.

use crate::address::EntryAddress;
use holochain_serialized_bytes::prelude::*;
use regex::Regex;

type LinkType = String;
type LinkTag = String;

/// Links interrelate entries in a source chain.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, SerializedBytes)]
pub struct Link {
    base: EntryAddress,
    target: EntryAddress,
    link_type: LinkType,
    tag: LinkTag,
}

impl Link {
    /// Construct a new link.
    pub fn new(base: &EntryAddress, target: &EntryAddress, link_type: &str, tag: &str) -> Self {
        Link {
            base: base.to_owned(),
            target: target.to_owned(),
            link_type: link_type.to_owned(),
            tag: tag.to_owned(),
        }
    }

    /// Get the base address of this link.
    pub fn base(&self) -> &EntryAddress {
        &self.base
    }

    /// Get the target address of this link.
    pub fn target(&self) -> &EntryAddress {
        &self.target
    }

    /// Get the type of this link.
    pub fn link_type(&self) -> &LinkType {
        &self.link_type
    }

    /// Get the tag of this link.
    pub fn tag(&self) -> &LinkTag {
        &self.tag
    }
}

/// How do we match this link in queries?
pub enum LinkMatch<S: Into<String>> {
    /// Match all/any links.
    Any,

    /// Match exactly by string.
    Exactly(S),

    /// Match by regular expression.
    Regex(S),
}

impl<S: Into<String>> LinkMatch<S> {
    /// Build a regular expression string for this link match.
    #[allow(clippy::wrong_self_convention)]
    pub fn to_regex_string(self) -> Result<String, String> {
        let re_string: String = match self {
            LinkMatch::Any => ".*".into(),
            LinkMatch::Exactly(s) => "^".to_owned() + &regex::escape(&s.into()) + "$",
            LinkMatch::Regex(s) => s.into(),
        };
        // check that it is a valid regex
        match Regex::new(&re_string) {
            Ok(_) => Ok(re_string),
            Err(_) => Err("Invalid regex passed to get_links".into()),
        }
    }
}
