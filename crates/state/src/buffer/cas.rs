use super::{kv::KvBuf, BufKey, BufVal, BufferedStore};
use crate::{
    error::{DatabaseError, DatabaseResult},
    fatal_db_hash_construction_check, fatal_db_hash_integrity_check,
    prelude::{Reader, Writer},
};
use fallible_iterator::FallibleIterator;
use futures::future::FutureExt;
use holo_hash::Hashable;
use holo_hash_core::HoloHashCoreHash;
use must_future::MustBoxFuture;

/// A wrapper around a KvBuf where keys are always Addresses,
/// and values are always AddressableContent.
pub struct CasBuf<'env, H>(KvBuf<'env, H::HashType, H::Content, Reader<'env>>)
where
    H: Hashable + Send,
    H::HashType: BufKey,
    H::Content: BufVal + Send + Sync;

impl<'env, H> CasBuf<'env, H>
where
    H: Hashable + Send,
    H::HashType: BufKey,
    H::Content: BufVal + Send + Sync,
{
    /// Create a new CasBuf from a read-only transaction and a database reference
    pub fn new(reader: &'env Reader<'env>, db: rkv::SingleStore) -> DatabaseResult<Self> {
        Ok(Self(KvBuf::new(reader, db)?))
    }

    /// Get a value from the underlying [KvBuf]
    pub fn get(
        &'env self,
        hash: &'env H::HashType,
    ) -> MustBoxFuture<'env, DatabaseResult<Option<H>>> {
        async move {
            Ok(if let Some(content) = self.0.get(hash)? {
                Some(Self::deserialize_and_hash(hash.get_bytes(), content).await)
            } else {
                None
            })
        }
        .boxed()
        .into()
    }

    /// Put a value into the underlying [KvBuf]
    pub fn put(&mut self, h: H) {
        let (content, hash) = h.into_inner();
        self.0.put(hash, content)
    }

    /// Delete a value from the underlying [KvBuf]
    pub fn delete(&mut self, k: H::HashType) {
        self.0.delete(k)
    }

    /// Iterate over the underlying persisted data, NOT taking the scratch space into consideration
    pub fn iter_fail(
        &'env self,
    ) -> DatabaseResult<Box<dyn FallibleIterator<Item = H, Error = DatabaseError> + 'env>> {
        Ok(Box::new(FallibleIterator::map(self.0.iter()?, |(h, c)| {
            Ok(Self::deserialize_and_hash_blocking(h, c))
        })))
    }

    /// Iterate over the underlying persisted data, NOT taking the scratch space into consideration
    pub fn iter_fail_raw(
        &'env self,
    ) -> DatabaseResult<Box<dyn FallibleIterator<Item = H, Error = DatabaseError> + 'env>> {
        Ok(Box::new(FallibleIterator::map(
            self.0.iter_raw()?,
            |(h, c)| Ok(Self::deserialize_and_hash_blocking(h, c)),
        )))
    }

    fn deserialize_and_hash_blocking(hash: &[u8], content: H::Content) -> H {
        tokio_safe_block_on::tokio_safe_block_on(
            Self::deserialize_and_hash(hash, content),
            std::time::Duration::from_millis(500),
        )
        .expect("TODO: make into stream")
        // TODO: make this a stream?
    }

    async fn deserialize_and_hash(hash_bytes: &[u8], content: H::Content) -> H {
        let data = fatal_db_hash_construction_check!(
            "CasBuf::get",
            hash_bytes,
            H::with_data(content).await
        );
        fatal_db_hash_integrity_check!("CasBuf::get", hash_bytes, data.as_hash().get_bytes());
        data
    }

    /// Iterate over items which are staged for PUTs in the scratch space
    // HACK: unfortunate leaky abstraction here, but needed to allow comprehensive
    // iteration, by chaining this with an iter_raw
    pub fn iter_scratch_puts(&'env self) -> impl Iterator<Item = H> + 'env {
        self.0.iter_scratch_puts().map(|(hash_bytes, content)| {
            tokio_safe_block_on::tokio_safe_block_on(
                Self::deserialize_and_hash(&hash_bytes, (**content).clone()),
                std::time::Duration::from_millis(500),
            )
            .expect("TODO: make into stream")
        })
    }
}

impl<'env, H> BufferedStore<'env> for CasBuf<'env, H>
where
    H: Hashable + Send,
    H::HashType: BufKey,
    H::Content: BufVal + Send + Sync,
{
    type Error = DatabaseError;

    fn flush_to_txn(self, writer: &'env mut Writer) -> DatabaseResult<()> {
        self.0.flush_to_txn(writer)?;
        Ok(())
    }
}
