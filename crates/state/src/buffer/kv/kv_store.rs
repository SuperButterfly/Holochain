use super::KvStoreT;
use crate::buffer::{check_empty_key, iter::SingleIterRaw};
use crate::{
    error::{DatabaseError, DatabaseResult},
    prelude::*,
};
use fallible_iterator::FallibleIterator;
use rkv::SingleStore;

/// Wrapper around an rkv SingleStore which provides strongly typed values
pub struct KvStore<K, V>
where
    K: BufKey,
    V: BufVal,
{
    db: SingleStore,
    __phantom: std::marker::PhantomData<(K, V)>,
}

impl<K, V> KvStoreT<K, V> for KvStore<K, V>
where
    K: BufKey,
    V: BufVal,
{
    /// Fetch data from DB as raw byte slice
    fn get_bytes<'env, R: Readable>(
        &self,
        reader: &'env R,
        k: &K,
    ) -> DatabaseResult<Option<&'env [u8]>> {
        check_empty_key(k)?;
        match self.db.get(reader, k)? {
            Some(rkv::Value::Blob(buf)) => Ok(Some(buf)),
            None => Ok(None),
            Some(_) => Err(DatabaseError::InvalidValue),
        }
    }

    /// Fetch data from DB, deserialize into V type
    fn get<R: Readable>(&self, reader: &R, k: &K) -> DatabaseResult<Option<V>> {
        check_empty_key(k)?;
        match self.get_bytes(reader, k)? {
            Some(bytes) => Ok(Some(holochain_serialized_bytes::decode(bytes)?)),
            None => Ok(None),
        }
    }

    /// Put V into DB as serialized data
    fn put(&self, writer: &mut Writer, k: &K, v: &V) -> DatabaseResult<()> {
        let buf = holochain_serialized_bytes::encode(v)?;
        let encoded = rkv::Value::Blob(&buf);
        self.db.put(writer, k, &encoded)?;
        Ok(())
    }

    /// Delete value from DB
    fn delete(&self, writer: &mut Writer, k: &K) -> DatabaseResult<()> {
        Ok(self.db.delete(writer, k)?)
    }

    /// Iterate over the underlying persisted data
    fn iter<'env, R: Readable>(&self, reader: &'env R) -> DatabaseResult<SingleIterRaw<'env, V>> {
        Ok(SingleIterRaw::new(
            self.db.iter_start(reader)?,
            self.db.iter_end(reader)?,
        ))
    }

    /// Iterate from a key onwards
    fn iter_from<'env, R: Readable>(
        &self,
        reader: &'env R,
        k: K,
    ) -> DatabaseResult<SingleIterRaw<'env, V>> {
        check_empty_key(&k)?;
        Ok(SingleIterRaw::new(
            self.db.iter_from(reader, k)?,
            self.db.iter_end(reader)?,
        ))
    }

    /// Iterate over the underlying persisted data in reverse
    fn iter_reverse<'env, R: Readable>(
        &self,
        reader: &'env R,
    ) -> DatabaseResult<fallible_iterator::Rev<SingleIterRaw<'env, V>>> {
        Ok(SingleIterRaw::new(self.db.iter_start(reader)?, self.db.iter_end(reader)?).rev())
    }
}

impl<K, V> KvStore<K, V>
where
    K: BufKey,
    V: BufVal,
{
    /// Create a new KvIntBufFresh from a read-only transaction and a database reference
    pub fn new(db: SingleStore) -> Self {
        Self {
            db,
            __phantom: std::marker::PhantomData,
        }
    }

    /// Accessor for raw Rkv DB
    pub fn db(&self) -> SingleStore {
        self.db
    }

    // TODO: This should be cfg test but can't because it's in a different crate
    /// Clear db, useful for tests
    pub fn delete_all(&mut self, writer: &mut Writer) -> DatabaseResult<()> {
        Ok(self.db.clear(writer)?)
    }
}
