use crate::core::state::source_chain::{ChainInvalidReason, SourceChainError, SourceChainResult};
use holo_hash::EntryHash;
use holo_hash::HeaderHash;
use holochain_serialized_bytes::prelude::*;
use holochain_state::{
    buffer::{BufferedStore, CasBuf},
    db::{
        DbManager, CACHE_CHAIN_ENTRIES, CACHE_CHAIN_HEADERS, PRIMARY_CHAIN_ENTRIES,
        PRIMARY_CHAIN_HEADERS,
    },
    error::{DatabaseError, DatabaseResult},
    exports::SingleStore,
    prelude::{Readable, Reader, Writer},
};
use holochain_types::{
    chain_header::HeaderAddress,
    chain_header::{ChainHeader, HeaderWithEntry},
    entry::Entry,
    entry::EntryAddress,
};

pub type EntryCas<'env, R> = CasBuf<'env, Entry, R>;
pub type HeaderCas<'env, R> = CasBuf<'env, ChainHeader, R>;

/// A convenient pairing of two CasBufs, one for entries and one for headers
pub struct ChainCasBuf<'env, R: Readable = Reader<'env>> {
    entries: EntryCas<'env, R>,
    headers: HeaderCas<'env, R>,
}

impl<'env, R: Readable> ChainCasBuf<'env, R> {
    fn new(
        reader: &'env R,
        entries_store: SingleStore,
        headers_store: SingleStore,
    ) -> DatabaseResult<Self> {
        Ok(Self {
            entries: CasBuf::new(reader, entries_store)?,
            headers: CasBuf::new(reader, headers_store)?,
        })
    }

    pub fn primary(reader: &'env R, dbs: &DbManager) -> DatabaseResult<Self> {
        let entries = *dbs.get(&*PRIMARY_CHAIN_ENTRIES)?;
        let headers = *dbs.get(&*PRIMARY_CHAIN_HEADERS)?;
        Self::new(reader, entries, headers)
    }

    pub fn cache(reader: &'env R, dbs: &DbManager) -> DatabaseResult<Self> {
        let entries = *dbs.get(&*CACHE_CHAIN_ENTRIES)?;
        let headers = *dbs.get(&*CACHE_CHAIN_HEADERS)?;
        Self::new(reader, entries, headers)
    }

    pub fn get_entry(&self, entry_address: EntryAddress) -> DatabaseResult<Option<Entry>> {
        self.entries.get(&entry_address.into())
    }

    pub fn contains(&self, entry_address: EntryAddress) -> DatabaseResult<bool> {
        self.entries.get(&entry_address.into()).map(|e| e.is_some())
    }

    pub fn get_header(&self, header_address: HeaderAddress) -> DatabaseResult<Option<ChainHeader>> {
        self.headers.get(&header_address.into())
    }

    /// Given a ChainHeader, return the corresponding HeaderWithEntry
    pub fn header_with_entry(
        &self,
        header: ChainHeader,
    ) -> SourceChainResult<Option<HeaderWithEntry>> {
        if let Some(entry) = self.get_entry(header.entry_address().to_owned())? {
            Ok(Some(HeaderWithEntry::new(header, entry)))
        } else {
            Err(SourceChainError::InvalidStructure(
                ChainInvalidReason::MissingData(header.entry_address().to_owned()),
            ))
        }
    }

    pub fn get_header_with_entry(
        &self,
        header_address: &HeaderAddress,
    ) -> SourceChainResult<Option<HeaderWithEntry>> {
        if let Some(header) = self.get_header(header_address.to_owned())? {
            self.header_with_entry(header)
        } else {
            Ok(None)
        }
    }

    pub fn put(&mut self, v: (ChainHeader, Entry)) -> DatabaseResult<()> {
        let (header, entry) = v;
        self.entries.put((&entry).try_into()?, entry);
        self.headers.put((&header).try_into()?, header);
        Ok(())
    }

    // TODO: consolidate into single delete which handles entry and header together
    pub fn delete_entry(&mut self, k: EntryHash) {
        self.entries.delete(k.into())
    }

    pub fn delete_header(&mut self, k: HeaderHash) {
        self.headers.delete(k.into())
    }

    pub fn headers(&self) -> &HeaderCas<'env, R> {
        &self.headers
    }

    pub fn entries(&self) -> &EntryCas<'env, R> {
        &self.entries
    }
}

impl<'env, R: Readable> BufferedStore<'env> for ChainCasBuf<'env, R> {
    type Error = DatabaseError;

    fn flush_to_txn(self, writer: &'env mut Writer) -> DatabaseResult<()> {
        self.entries.flush_to_txn(writer)?;
        self.headers.flush_to_txn(writer)?;
        Ok(())
    }
}
