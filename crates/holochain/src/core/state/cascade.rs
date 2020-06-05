//! # Cascade
//! This module is still a work in progress.
//! Here is some pseudocode we are using to build it.
//! ## Dimensions
//! get vs get_links
//! default vs options
//! fast vs strict (is set by app dev)
//!
//! ## Get
//! ### Default - Get's the latest version
//! Scratch Live -> Return
//! Scratch NotInCascade -> Goto Cas
//! Scratch _ -> None
//! Cas Live -> Return
//! Cas NotInCascade -> Goto cache
//! Cas _ -> None
//! Cache Live -> Return
//! Cache Pending -> Goto Network
//! Cache NotInCascade -> Goto Network
//! Cache _ -> None
//!
//! ## Get Links
//! ### Default - Get's the latest version
//! if I'm an authority
//! Scratch Found-> Return
//! Scratch NotInCascade -> Goto Cas
//! Cas Found -> Return
//! Cas NotInCascade -> Goto Network
//! else
//! Network Found -> Return
//! Network NotInCascade -> Goto Cache
//! Cache Found -> Return
//! Cache NotInCascade -> None
//!
//! ## Pagination
//! gets most recent N links with default N (50)
//! Page number
//! ## Loading
//! load_true loads the results into cache

use super::{
    chain_cas::ChainCasBuf,
    chain_meta::{ChainMetaBuf, ChainMetaBufT, EntryDhtStatus, LinkMetaKey, LinkMetaVal},
};
use holochain_state::error::DatabaseResult;
use holochain_types::{composite_hash::EntryHash, header::ZomeId, link::Tag, EntryHashed};
use tracing::*;

#[cfg(test)]
mod test;

pub struct Cascade<'env, C = ChainMetaBuf<'env>>
where
    C: ChainMetaBufT,
{
    primary: &'env ChainCasBuf<'env>,
    primary_meta: &'env C,

    cache: &'env ChainCasBuf<'env>,
    cache_meta: &'env C,
}

/// The state of the cascade search
enum Search {
    /// The entry is found and we can stop
    Found(EntryHashed),
    /// We haven't found the entry yet and should
    /// continue searching down the cascade
    Continue,
    /// We haven't found the entry and should
    /// not continue searching down the cascade
    // TODO This information is currently not passed back to
    // the caller however it might be useful.
    NotInCascade,
}

/// Should these functions be sync or async?
/// Depends on how much computation, and if writes are involved
impl<'env, C> Cascade<'env, C>
where
    C: ChainMetaBufT,
{
    /// Constructs a [Cascade], taking references to a CAS and a cache
    pub fn new(
        primary: &'env ChainCasBuf<'env>,
        primary_meta: &'env C,
        cache: &'env ChainCasBuf<'env>,
        cache_meta: &'env C,
    ) -> Self {
        Cascade {
            primary,
            primary_meta,
            cache,
            cache_meta,
        }
    }

    #[instrument(skip(self))]
    /// Gets an entry from the cas or cache depending on it's metadata
    // TODO asyncify slow blocking functions here
    // The default behavior is to skip deleted or replaced entries.
    // TODO: Implement customization of this behavior with an options/builder struct
    pub async fn dht_get(&self, entry_hash: &EntryHash) -> DatabaseResult<Option<EntryHashed>> {
        // Cas
        let search = self
            .primary
            .get_entry(entry_hash)
            .await?
            .and_then(|entry| {
                self.primary_meta
                    .get_dht_status(entry_hash)
                    .ok()
                    .map(|crud| {
                        if let EntryDhtStatus::Live = crud {
                            Search::Found(entry)
                        } else {
                            Search::NotInCascade
                        }
                    })
            })
            .unwrap_or_else(|| Search::Continue);

        // Cache
        match search {
            Search::Continue => Ok(self.cache.get_entry(entry_hash).await?.and_then(|entry| {
                self.cache_meta
                    .get_dht_status(entry_hash)
                    .ok()
                    .and_then(|crud| match crud {
                        EntryDhtStatus::Live => Some(entry),
                        _ => None,
                    })
            })),
            Search::Found(entry) => Ok(Some(entry)),
            Search::NotInCascade => Ok(None),
        }
    }

    /// Gets an links from the cas or cache depending on it's metadata
    // TODO asyncify slow blocking functions here
    // The default behavior is to skip deleted or replaced entries.
    // TODO: Implement customization of this behavior with an options/builder struct
    pub async fn dht_get_links<'a>(
        &self,
        key: &'a LinkMetaKey<'a>,
    ) -> DatabaseResult<Vec<LinkMetaVal>> {
        // Am I an authority?
        let authority = self.primary.contains(&key.base()).await?;
        if authority {
            // Cas
            let links = self.primary_meta.get_links(key)?;

            // Cache
            if links.is_empty() {
                self.cache_meta.get_links(key)
            } else {
                Ok(links)
            }
        } else {
            // Cache
            self.cache_meta.get_links(key)
        }
    }
}
