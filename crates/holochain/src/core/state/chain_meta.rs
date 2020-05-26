#![allow(clippy::ptr_arg)]
use futures::{future::LocalBoxFuture, FutureExt};
use holo_hash::HeaderHash;
use holochain_serialized_bytes::prelude::*;
use holochain_state::{
    buffer::KvvBuf,
    db::{CACHE_LINKS_META, CACHE_SYSTEM_META, PRIMARY_LINKS_META, PRIMARY_SYSTEM_META},
    error::{DatabaseError, DatabaseResult},
    prelude::*,
};
use holochain_types::{composite_hash::EntryHash, header::LinkAdd, shims::*, Header, HeaderHashed};
use mockall::mock;
use std::collections::{HashMap, HashSet};
use std::convert::TryInto;
use std::fmt::Debug;

type Tag = String;

#[derive(Debug)]
pub enum EntryDhtStatus {
    Live,
    Dead,
    Pending,
    Rejected,
    Abandoned,
    Conflict,
    Withdrawn,
    Purged,
}

// TODO Add Op to value
// Adds have hash of LinkAdd
// And target
// Dels have hash of LinkAdd
#[derive(Debug, Hash, PartialEq, Eq, Clone, Serialize, Deserialize)]
enum Op {
    Add(HeaderHash, EntryHash),
    Remove(HeaderHash),
}

#[allow(dead_code)]
struct LinkKey<'a> {
    base: &'a EntryHash,
    tag: Tag,
}

impl<'a> LinkKey<'a> {
    fn to_key(&self) -> Vec<u8> {
        // Possibly FIXME if this expect is actually not true
        let sb: SerializedBytes = self
            .base
            .try_into()
            .expect("entry addresses don't have the unserialize problem");
        let mut vec: Vec<u8> = sb.bytes().to_vec();
        vec.extend_from_slice(self.tag.as_ref());
        vec
    }
}

/*
TODO impliment these types
AddLink:
Base: hash
Target: hash
Type: (maybe?)
Tag: string
Addlink_Time: timestamp
Addlink_Action: hash

RemoveLink:
Base:
Target:
Type:
Tag:
AddLink_Time: timestamp
AddLink_Action: hash
RemoveLink_Action: timestamp
RemoveLink_Action: hash
*/

pub trait ChainMetaBufT<'env, R = Reader<'env>>
where
    R: Readable,
{
    // Links
    /// Get all te links on this base that match the tag
    fn get_links<Tag>(&self, base: &EntryHash, tag: Tag) -> DatabaseResult<HashSet<EntryHash>>
    where
        Tag: Into<String>;

    /// Add a link
    fn add_link<'a>(&'a mut self, link_add: LinkAdd) -> LocalBoxFuture<'a, DatabaseResult<()>>;

    // Sys
    fn get_crud(&self, entry_hash: EntryHash) -> DatabaseResult<EntryDhtStatus>;
}

pub struct ChainMetaBuf<'env, R = Reader<'env>>
where
    R: Readable,
{
    system_meta: KvvBuf<'env, Vec<u8>, SysMetaVal, R>,
    links_meta: KvvBuf<'env, Vec<u8>, Op, R>,
}

impl<'env, R> ChainMetaBuf<'env, R>
where
    R: Readable,
{
    pub(crate) fn new(
        reader: &'env R,
        system_meta: MultiStore,
        links_meta: MultiStore,
    ) -> DatabaseResult<Self> {
        Ok(Self {
            system_meta: KvvBuf::new(reader, system_meta)?,
            links_meta: KvvBuf::new(reader, links_meta)?,
        })
    }
    pub fn primary(reader: &'env R, dbs: &impl GetDb) -> DatabaseResult<Self> {
        let system_meta = dbs.get_db(&*PRIMARY_SYSTEM_META)?;
        let links_meta = dbs.get_db(&*PRIMARY_LINKS_META)?;
        Self::new(reader, system_meta, links_meta)
    }

    pub fn cache(reader: &'env R, dbs: &impl GetDb) -> DatabaseResult<Self> {
        let system_meta = dbs.get_db(&*CACHE_SYSTEM_META)?;
        let links_meta = dbs.get_db(&*CACHE_LINKS_META)?;
        Self::new(reader, system_meta, links_meta)
    }
}

impl<'env, R> ChainMetaBufT<'env, R> for ChainMetaBuf<'env, R>
where
    R: Readable,
{
    // TODO find out whether we need link_type.
    fn get_links<Tag: Into<String>>(
        &self,
        base: &EntryHash,
        tag: Tag,
    ) -> DatabaseResult<HashSet<EntryHash>> {
        // TODO get removes
        // TODO get adds
        let key = LinkKey {
            base,
            tag: tag.into(),
        };
        let mut results = HashMap::new();
        let mut removes = vec![];
        self.links_meta
            .get(&key.to_key())?
            .map(|op| {
                op.map(|op| match op {
                    Op::Add(link_add_hash, entry) => {
                        results.insert(link_add_hash, entry);
                    }
                    Op::Remove(link_add_hash) => {
                        removes.push(link_add_hash);
                    }
                })
            })
            .collect::<Result<_, _>>()?;
        for link_add_hash in removes {
            results.remove(&link_add_hash);
        }
        Ok(results.into_iter().map(|(_, v)| v).collect())
    }

    // TODO: Figure out how to use this with MustFuture
    fn add_link<'a>(&'a mut self, link_add: LinkAdd) -> LocalBoxFuture<'a, DatabaseResult<()>> {
        let f = async move {
            let base = &link_add.base_address.clone();
            let target = link_add.target_address.clone();
            let tag = link_add.tag.clone();
            let link_add = HeaderHashed::with_data(Header::LinkAdd(link_add)).await?;
            let link_address: &HeaderHash = link_add.as_ref();
            let link_address = link_address.clone();
            let key = LinkKey { base, tag };

            self.links_meta
                .insert(key.to_key(), Op::Add(link_address, target));
            DatabaseResult::Ok(())
        };
        f.boxed_local()
    }

    fn get_crud(&self, _entry_hash: EntryHash) -> DatabaseResult<EntryDhtStatus> {
        unimplemented!()
    }
}

mock! {
    pub ChainMetaBuf
    {
        fn get_links(&self, base: &EntryHash, tag: Tag) -> DatabaseResult<HashSet<EntryHash>>;
        fn add_link(&mut self, link: LinkAdd) -> DatabaseResult<()>;
        fn get_crud(&self, entry_hash: EntryHash) -> DatabaseResult<EntryDhtStatus>;
    }
}

impl<'env, R> ChainMetaBufT<'env, R> for MockChainMetaBuf
where
    R: Readable,
{
    fn get_links<Tag: Into<String>>(
        &self,
        base: &EntryHash,
        tag: Tag,
    ) -> DatabaseResult<HashSet<EntryHash>> {
        self.get_links(base, tag.into())
    }
    fn get_crud(&self, entry_hash: EntryHash) -> DatabaseResult<EntryDhtStatus> {
        self.get_crud(entry_hash)
    }

    fn add_link<'a>(&'a mut self, link_add: LinkAdd) -> LocalBoxFuture<'a, DatabaseResult<()>> {
        async move { self.add_link(link_add) }.boxed_local()
    }
}

impl<'env, R: Readable> BufferedStore<'env> for ChainMetaBuf<'env, R> {
    type Error = DatabaseError;

    fn flush_to_txn(self, writer: &'env mut Writer) -> DatabaseResult<()> {
        self.system_meta.flush_to_txn(writer)?;
        self.links_meta.flush_to_txn(writer)?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::fixt::EntryFixturator;
    use fixt::prelude::*;
    use holo_hash::{AgentPubKeyFixturator, HeaderHashFixturator};
    use holochain_state::{buffer::BufferedStore, test_utils::test_cell_env};
    use holochain_types::{EntryHashed, Timestamp};
    use maplit::hashset;

    #[tokio::test(threaded_scheduler)]
    async fn can_add_and_get_link() {
        let arc = test_cell_env();
        let env = arc.guard().await;

        let (base_hash, target_hash) = tokio_safe_block_on::tokio_safe_block_on(
            async {
                let mut entry_fix = EntryFixturator::new(Unpredictable);
                (
                    EntryHashed::with_data(entry_fix.next().unwrap())
                        .await
                        .unwrap(),
                    EntryHashed::with_data(entry_fix.next().unwrap())
                        .await
                        .unwrap(),
                )
            },
            std::time::Duration::from_secs(1),
        )
        .unwrap();

        let tag = StringFixturator::new(Unpredictable).next().unwrap();
        let base_address: &EntryHash = base_hash.as_ref();
        let target_address: &EntryHash = target_hash.as_ref();
        let add_link = LinkAdd {
            author: AgentPubKeyFixturator::new(Unpredictable).next().unwrap(),
            timestamp: Timestamp::now(),
            header_seq: 0,
            prev_header: HeaderHashFixturator::new(Unpredictable).next().unwrap(),
            base_address: base_address.clone(),
            target_address: target_address.clone(),
            tag: tag.clone(),
            link_type: SerializedBytesFixturator::new(Unpredictable)
                .next()
                .unwrap(),
        };

        env.with_reader(|reader| {
            let meta_buf = ChainMetaBuf::primary(&reader, &env).unwrap();
            assert!(meta_buf
                .get_links(base_hash.as_ref(), tag.clone())
                .unwrap()
                .is_empty());
            DatabaseResult::Ok(())
        })
        .unwrap();

        {
            let reader = env.reader().unwrap();
            let mut meta_buf = ChainMetaBuf::primary(&reader, &env).unwrap();
            {
                let _ = meta_buf.add_link(add_link).await;
            }
            env.with_commit(|writer| meta_buf.flush_to_txn(writer))
                .unwrap();
        }

        env.with_reader(|reader| {
            let meta_buf = ChainMetaBuf::primary(&reader, &env).unwrap();
            assert_eq!(
                meta_buf.get_links(base_hash.as_ref(), tag.clone()).unwrap(),
                hashset! {target_address.clone()}
            );
            DatabaseResult::Ok(())
        })
        .unwrap();
    }
}
