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
    metadata::{LinkMetaKey, MetadataBuf, MetadataBufT, SysMetaVal},
};
use error::CascadeResult;
use fallible_iterator::FallibleIterator;
use holo_hash::{
    hash_type::{self, AnyDht},
    AnyDhtHash, EntryHash, HasHash, HeaderHash,
};
use holochain_p2p::{
    actor::{GetLinksOptions, GetMetaOptions, GetOptions},
    HolochainP2pCell,
};
use holochain_state::error::DatabaseResult;
use holochain_types::{
    element::{Element, GetElementResponse, RawGetEntryResponse, SignedHeaderHashed, WireElement},
    header::WireDelete,
    link::{GetLinksResponse, WireLinkMetaKey},
    metadata::{EntryDhtStatus, MetadataSet, TimedHeaderHash},
    EntryHashed, HeaderHashed,
};
use holochain_zome_types::{link::Link, Header};
use tracing::*;

#[cfg(test)]
mod network_tests;
#[cfg(all(test, outdated_tests))]
mod test;

pub mod error;

pub struct Cascade<'env: 'a, 'a, M = MetadataBuf<'env>, C = MetadataBuf<'env>>
where
    M: MetadataBufT,
    C: MetadataBufT,
{
    element_vault: &'a ChainCasBuf<'env>,
    meta_vault: &'a M,

    element_cache: &'a mut ChainCasBuf<'env>,
    meta_cache: &'a mut C,

    network: HolochainP2pCell,
}

/// The state of the cascade search
enum Search {
    /// The entry is found and we can stop
    Found(Element),
    /// We haven't found the entry yet and should
    /// continue searching down the cascade
    Continue(HeaderHash),
    /// We haven't found the entry and should
    /// not continue searching down the cascade
    // TODO This information is currently not passed back to
    // the caller however it might be useful.
    NotInCascade,
}

/// Should these functions be sync or async?
/// Depends on how much computation, and if writes are involved
impl<'env: 'a, 'a, M, C> Cascade<'env, 'a, M, C>
where
    C: MetadataBufT,
    M: MetadataBufT,
{
    /// Constructs a [Cascade], taking references to all necessary databases
    pub fn new(
        element_vault: &'a ChainCasBuf<'env>,
        meta_vault: &'a M,
        element_cache: &'a mut ChainCasBuf<'env>,
        meta_cache: &'a mut C,
        network: HolochainP2pCell,
    ) -> Self {
        Cascade {
            element_vault,
            meta_vault,
            element_cache,
            meta_cache,
            network,
        }
    }

    async fn fetch_element_via_header(
        &mut self,
        hash: HeaderHash,
        options: GetOptions,
    ) -> CascadeResult<()> {
        let results = self.network.get(hash.into(), options).await?;

        // The element that we want to store
        let mut element: Option<Box<WireElement>> = None;

        // Search through the returns for the first delete
        let proof_of_delete = results.into_iter().find_map(|response| match response {
            // Has header
            GetElementResponse::GetHeader(Some(we)) => {
                let deleted = we.deleted().clone();
                // Store the first found element
                if element.is_none() {
                    // TODO: Validate that this is the same element across all returns
                    // TODO: Validate that the entry hash matches
                    // TODO: Check all headers have the correct hash
                    element = Some(we);
                }
                match deleted {
                    // Has proof of deleted entry
                    Some(deleted) => Some(deleted),
                    // No proof of delete so this is a live element
                    None => None,
                }
            }
            // Doesn't have header but not because it was deleted
            GetElementResponse::GetHeader(None) => None,
            r @ _ => {
                error!(
                    msg = "Got an invalid response to fetch element via header",
                    ?r
                );
                None
            }
        });

        // Add the element data to the caches if there was some
        if let Some(element) = element {
            let element = element.into_element().await;
            let (signed_header, maybe_entry) = element.clone().into_inner();
            let timed_header_hash: TimedHeaderHash = signed_header.header_hashed().clone().into();

            // If there is an entry we want to check the hash
            let entry = match maybe_entry {
                Some(entry) => {
                    let eh = EntryHashed::from_content(entry).await;
                    match signed_header.header().entry_data() {
                        // Entry hash matches
                        Some((hash, _)) if eh.as_hash() == hash => Some(eh),
                        // Entry hash doesn't match so don't store any metadata
                        None => None,
                        _ => {
                            warn!("Entry hash doesn't match header");
                            // Return before storing metadata
                            return Ok(());
                        }
                    }
                }
                None => None,
            };
            if let Some(entry) = &entry {
                let entry_hash = entry.as_hash().clone();
                // TODO: [B-02052] Should / could we just do an integrate_to_cache here?
                // Found a delete, add it to the cache.
                if let Some(WireDelete {
                    element_delete_address,
                    removes_address,
                }) = proof_of_delete
                {
                    self.meta_cache.register_raw_on_entry(
                        entry_hash.clone(),
                        SysMetaVal::Delete(element_delete_address.clone()),
                    )?;
                    self.meta_cache.register_raw_on_header(
                        removes_address,
                        SysMetaVal::Delete(element_delete_address),
                    );
                }
                self.meta_cache
                    .register_raw_on_entry(entry_hash, SysMetaVal::NewEntry(timed_header_hash))?;
            }

            // Put in element element_cache
            self.element_cache.put(signed_header, entry)?;
        }
        Ok(())
    }

    async fn fetch_element_via_entry(
        &mut self,
        hash: EntryHash,
        options: GetOptions,
    ) -> CascadeResult<()> {
        let elements = self.network.get(hash.clone().into(), options).await?;

        let mut maybe_entry_hashed: Option<EntryHashed> = None;

        for element in elements {
            match element {
                GetElementResponse::GetEntryFull(Some(raw)) => {
                    let RawGetEntryResponse {
                        live_headers,
                        deletes,
                        entry,
                        entry_type,
                        entry_hash,
                    } = *raw;
                    // We don't want to hash every entry so just hash one but check
                    // all the hashes match
                    let entry_hashed = match &maybe_entry_hashed {
                        Some(eh) => eh.clone(),
                        None => {
                            let eh = EntryHashed::from_content(entry).await;
                            maybe_entry_hashed = Some(eh.clone());
                            eh
                        }
                    };
                    // Check the hash matches
                    if entry_hash != *entry_hashed.as_hash() && entry_hash == hash {
                        warn!("Received element with hash that doesn't match");
                        maybe_entry_hashed = None;
                        continue;
                    }
                    for entry_header in live_headers {
                        let (new_entry_header, header_hash, signature) = entry_header
                            .create_new_entry_header(entry_type.clone(), entry_hash.clone())
                            .await;
                        self.meta_cache
                            .register_header(new_entry_header.clone())
                            .await?;
                        let header =
                            HeaderHashed::with_pre_hashed(new_entry_header.into(), header_hash);
                        // Add elements to element_cache
                        self.element_cache.put(
                            SignedHeaderHashed::with_presigned(header, signature),
                            Some(entry_hashed.clone()),
                        )?;
                    }
                    for WireDelete {
                        element_delete_address,
                        removes_address,
                    } in deletes
                    {
                        self.meta_cache.register_raw_on_header(
                            removes_address,
                            SysMetaVal::Delete(element_delete_address.clone()),
                        );
                        self.meta_cache.register_raw_on_entry(
                            entry_hash.clone(),
                            SysMetaVal::Delete(element_delete_address),
                        )?;
                    }
                }
                // Authority didn't have any headers for this entry
                GetElementResponse::GetEntryFull(None) => (),
                r @ GetElementResponse::GetHeader(_) => {
                    error!(
                        msg = "Got an invalid response to fetch element via entry",
                        ?r
                    );
                }
                r @ _ => unimplemented!("{:?} is unimplemented for fetching via entry", r),
            }
        }
        Ok(())
    }

    // TODO: Remove when used
    #[allow(dead_code)]
    async fn fetch_meta(
        &mut self,
        hash: AnyDhtHash,
        options: GetMetaOptions,
    ) -> CascadeResult<Vec<MetadataSet>> {
        let all_metadata = self.network.get_meta(hash.clone(), options).await?;

        // Only put raw meta data in element_cache and combine all results
        for metadata in all_metadata.iter().cloned() {
            let hash = hash.clone();
            // Put in meta element_cache
            let values = metadata
                .headers
                .into_iter()
                .map(|h| SysMetaVal::NewEntry(h))
                .chain(metadata.deletes.into_iter().map(|h| SysMetaVal::Delete(h)))
                .chain(metadata.updates.into_iter().map(|h| SysMetaVal::Update(h)));
            match *hash.hash_type() {
                hash_type::AnyDht::Entry(e) => {
                    let basis = hash.retype(e);
                    for v in values {
                        self.meta_cache.register_raw_on_entry(basis.clone(), v)?;
                    }
                }
                hash_type::AnyDht::Header => {
                    let basis = hash.retype(hash_type::Header);
                    for v in values {
                        self.meta_cache.register_raw_on_header(basis.clone(), v);
                    }
                }
            }
        }
        Ok(all_metadata)
    }

    async fn fetch_links(
        &mut self,
        link_key: WireLinkMetaKey,
        options: GetLinksOptions,
    ) -> CascadeResult<()> {
        let results = self.network.get_links(link_key, options).await?;
        for links in results {
            let GetLinksResponse {
                link_adds,
                link_removes,
            } = links;

            for (link_add, signature) in link_adds {
                let header = HeaderHashed::from_content(Header::LinkAdd(link_add.clone())).await;
                self.element_cache
                    .put(SignedHeaderHashed::with_presigned(header, signature), None)?;
                self.meta_cache.add_link(link_add).await?;
            }
            for (link_remove, signature) in link_removes {
                let header =
                    HeaderHashed::from_content(Header::LinkRemove(link_remove.clone())).await;
                self.element_cache
                    .put(SignedHeaderHashed::with_presigned(header, signature), None)?;
                self.meta_cache.remove_link(link_remove).await?;
            }
        }
        Ok(())
    }

    /// Get a header without checking its metadata
    pub async fn dht_get_header_raw(
        &self,
        header_address: &HeaderHash,
    ) -> DatabaseResult<Option<SignedHeaderHashed>> {
        match self.element_vault.get_header(header_address).await? {
            None => self.element_cache.get_header(header_address).await,
            r => Ok(r),
        }
    }

    /// Get an entry without checking its metadata
    pub async fn dht_get_entry_raw(
        &self,
        entry_hash: &EntryHash,
    ) -> DatabaseResult<Option<EntryHashed>> {
        match self.element_vault.get_entry(entry_hash).await? {
            None => self.element_cache.get_entry(entry_hash).await,
            r => Ok(r),
        }
    }

    async fn get_element_local_raw(&self, hash: &HeaderHash) -> CascadeResult<Option<Element>> {
        match self.element_vault.get_element(hash).await? {
            None => Ok(self.element_cache.get_element(hash).await?),
            r => Ok(r),
        }
    }

    /// Returns the oldest live [Element] for this [EntryHash] by getting the
    /// latest available metadata from authorities combined with this agents authored data.
    pub async fn dht_get_entry(
        &mut self,
        entry_hash: EntryHash,
        options: GetOptions,
    ) -> CascadeResult<Option<Element>> {
        // Update the cache from the network
        self.fetch_element_via_entry(entry_hash.clone(), options.clone())
            .await?;

        // Meta Cache
        let oldest_live_element = match self.meta_cache.get_dht_status(&entry_hash)? {
            EntryDhtStatus::Live => {
                let oldest_live_header = self
                    .meta_cache
                    .get_headers(entry_hash)?
                    .filter_map(|header| {
                        if let None = self
                            .meta_cache
                            .get_deletes_on_header(header.header_hash.clone())?
                            .next()?
                        {
                            Ok(Some(header))
                        } else {
                            Ok(None)
                        }
                    })
                    .min()?
                    .expect("Status is live but no headers?");

                // We have an oldest live header now get the element
                self.get_element_local_raw(&oldest_live_header.header_hash)
                    .await?
                    .map(Search::Found)
                    // It's not local so check the network
                    .unwrap_or(Search::Continue(oldest_live_header.header_hash))
            }
            EntryDhtStatus::Dead
            | EntryDhtStatus::Pending
            | EntryDhtStatus::Rejected
            | EntryDhtStatus::Abandoned
            | EntryDhtStatus::Conflict
            | EntryDhtStatus::Withdrawn
            | EntryDhtStatus::Purged => Search::NotInCascade,
        };

        // Network
        match oldest_live_element {
            Search::Found(element) => Ok(Some(element)),
            Search::Continue(oldest_live_header) => {
                self.dht_get_header(oldest_live_header, options).await
            }
            Search::NotInCascade => Ok(None),
        }
    }

    /// Returns the [Element] for this [HeaderHash] if it is live
    /// by getting the latest available metadata from authorities
    /// combined with this agents authored data.
    /// _Note: Deleted headers are a tombstone set_
    pub async fn dht_get_header(
        &mut self,
        header_hash: HeaderHash,
        options: GetOptions,
    ) -> CascadeResult<Option<Element>> {
        // Meta Cache
        if let Some(_) = self
            .meta_cache
            .get_deletes_on_header(header_hash.clone())?
            .next()?
        {
            // Final tombstone found
            return Ok(None);
        // Meta Vault
        } else if let Some(_) = self
            .meta_vault
            .get_deletes_on_header(header_hash.clone())?
            .next()?
        {
            // Final tombstone found
            return Ok(None);
        }
        // Network
        self.fetch_element_via_header(header_hash.clone(), options)
            .await?;
        // Check if header is alive after fetch
        let delete_on_header = self
            .meta_cache
            .get_deletes_on_header(header_hash.clone())?
            .next()?
            .is_none();

        if delete_on_header {
            Ok(None)
        } else {
            // See if the header was found?
            self.get_element_local_raw(&header_hash).await
        }
    }

    #[instrument(skip(self))]
    // Updates the cache with the latest network authority data
    // and returns what is in the cache.
    // This gives you the latest possible picture of the current dht state.
    // Data from your zome call is also added to the cache.
    pub async fn dht_get(
        &mut self,
        hash: AnyDhtHash,
        options: GetOptions,
    ) -> CascadeResult<Option<Element>> {
        match *hash.hash_type() {
            AnyDht::Entry(e) => {
                let hash = hash.retype(e);
                self.dht_get_entry(hash, options).await
            }
            AnyDht::Header => {
                let hash = hash.retype(hash_type::Header);
                self.dht_get_header(hash, options).await
            }
        }
    }

    /// Gets an links from the cas or cache depending on it's metadata
    // The default behavior is to skip deleted or replaced entries.
    // TODO: Implement customization of this behavior with an options/builder struct
    pub async fn dht_get_links<'link>(
        &mut self,
        key: &'link LinkMetaKey<'link>,
        options: GetLinksOptions,
    ) -> CascadeResult<Vec<Link>> {
        // Update the cache from the network
        self.fetch_links(key.into(), options).await?;

        // Meta Cache
        // Return any links from the meta cache that don't have removes.
        Ok(self
            .meta_cache
            .get_links(key)?
            .map(|l| Ok(l.into_link()))
            .collect()?)
    }
}

#[cfg(test)]
/// Helper function for easily setting up cascades during tests
pub fn test_dbs_and_mocks<'env>(
    reader: &'env holochain_state::transaction::Reader<'env>,
    dbs: &impl holochain_state::db::GetDb,
) -> (
    ChainCasBuf<'env>,
    super::metadata::MockMetadataBuf,
    ChainCasBuf<'env>,
    super::metadata::MockMetadataBuf,
) {
    let cas = ChainCasBuf::vault(&reader, dbs, true).unwrap();
    let element_cache = ChainCasBuf::cache(&reader, dbs).unwrap();
    let metadata = super::metadata::MockMetadataBuf::new();
    let metadata_cache = super::metadata::MockMetadataBuf::new();
    (cas, metadata, element_cache, metadata_cache)
}
