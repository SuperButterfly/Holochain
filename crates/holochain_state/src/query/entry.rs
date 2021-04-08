use holo_hash::*;
use holochain_sqlite::rusqlite::named_params;
use holochain_types::dht_op::DhtOpType;
use holochain_zome_types::*;
use std::fmt::Debug;

use super::*;

#[derive(Debug, Clone)]
pub struct GetEntryQuery(EntryHash);

impl GetEntryQuery {
    pub fn new(hash: EntryHash) -> Self {
        Self(hash)
    }
}

impl Query for GetEntryQuery {
    type Data = SignedHeaderHashed;
    type State = Maps<SignedHeaderHashed>;
    type Output = Option<Element>;

    fn create_query(&self) -> &str {
        "
            SELECT Header.blob AS header_blob FROM DhtOp
            JOIN Header On DhtOp.header_hash = Header.hash
            WHERE DhtOp.type = :store_entry
            AND
            DhtOp.basis_hash = :entry_hash
            AND
            DhtOp.validation_status = :status
        "
    }

    fn delete_query(&self) -> &str {
        "
            SELECT Header.blob AS header_blob FROM DhtOp
            JOIN Header On DhtOp.header_hash = Header.hash
            WHERE DhtOp.type = :delete
            AND
            DhtOp.basis_hash = :entry_hash
            AND
            DhtOp.validation_status = :status
        "
    }

    fn create_params(&self) -> Vec<Params> {
        let params = named_params! {
            ":store_entry": DhtOpType::StoreEntry,
            ":entry_hash": self.0,
            ":status": ValidationStatus::Valid,
        };
        params.to_vec()
    }

    fn delete_params(&self) -> Vec<Params> {
        let params = named_params! {
            ":delete": DhtOpType::RegisterDeletedEntryHeader,
            ":entry_hash": self.0,
            ":status": ValidationStatus::Valid,
        };
        params.to_vec()
    }

    fn as_map(&self) -> Arc<dyn Fn(&Row) -> StateQueryResult<Self::Data>> {
        Arc::new(row_to_signed_header("header_blob"))
    }

    fn as_filter(&self) -> Box<dyn Fn(&Self::Data) -> bool> {
        let entry_filter = self.0.clone();
        let f = move |header: &SignedHeaderHashed| match header.header() {
            Header::Create(Create { entry_hash, .. }) => *entry_hash == entry_filter,
            Header::Delete(Delete {
                deletes_entry_address,
                ..
            }) => *deletes_entry_address == entry_filter,
            _ => false,
        };
        Box::new(f)
    }

    fn init_fold(&self) -> StateQueryResult<Self::State> {
        Ok(Maps::new())
    }

    fn fold(
        &self,
        mut state: Self::State,
        shh: SignedHeaderHashed,
    ) -> StateQueryResult<Self::State> {
        let hash = shh.as_hash().clone();
        match shh.header() {
            Header::Create(_) => {
                if !state.deletes.contains(&hash) {
                    state.creates.insert(hash, shh);
                }
            }
            Header::Delete(delete) => {
                state.creates.remove(&delete.deletes_address);
                state.deletes.insert(delete.deletes_address.clone());
            }
            _ => panic!("TODO: Turn this into an error"),
        }
        Ok(state)
    }

    fn render<S>(&self, state: Self::State, stores: S) -> StateQueryResult<Self::Output>
    where
        S: Store,
    {
        // Choose an arbitrary header
        let header = state.creates.into_iter().map(|(_, v)| v).next();
        match header {
            Some(header) => {
                // TODO: Handle error where header doesn't have entry hash.
                let entry_hash = header.header().entry_hash().unwrap();
                let entry = stores
                    .get_entry(&entry_hash)?
                    .expect("TODO: Handle case where entry wasn't found but we had headers");
                Ok(Some(Element::new(header, Some(entry))))
            }
            None => Ok(None),
        }
    }
}
