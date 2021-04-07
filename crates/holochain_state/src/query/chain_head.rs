use holo_hash::*;
use holochain_sqlite::rusqlite::*;
use holochain_zome_types::*;
use std::fmt::Debug;

use super::*;

#[derive(Debug, Clone)]
pub struct ChainHeadQuery(AgentPubKey);

impl ChainHeadQuery {
    pub fn new(agent: AgentPubKey) -> Self {
        Self(agent)
    }
}

impl Query for ChainHeadQuery {
    type Data = SignedHeader;
    type State = Option<SignedHeader>;
    type Output = Option<HeaderHash>;

    fn create_query(&self) -> &str {
        "
            SELECT H.blob FROM Header AS H
            JOIN DhtOp as D
            ON D.header_hash = H.hash
            JOIN (
                SELECT author, MAX(seq) FROM Header
                GROUP BY author
            ) AS H2
            ON H.author = H2.author
            WHERE H.author = :author AND D.is_authored = 1
        "
    }

    fn create_params(&self) -> Vec<Params> {
        let params = named_params! {
            ":author": self.0,
        };
        params.to_vec()
    }

    fn init_fold(&self) -> StateQueryResult<Self::State> {
        Ok(None)
    }

    fn as_filter(&self) -> Box<dyn Fn(&Self::Data) -> bool> {
        let author = self.0.clone();
        // NB: it's a little redundant to filter on author, since we should never
        // be putting any headers by other authors in our scratch, but it
        // certainly doesn't hurt to be consistent.
        let f = move |header: &SignedHeader| *header.header().author() == author;
        Box::new(f)
    }

    fn fold(&self, state: Self::State, sh: SignedHeader) -> StateQueryResult<Self::State> {
        // Simple maximum finding
        Ok(Some(match state {
            None => sh,
            Some(old) => {
                if sh.header().header_seq() > old.header().header_seq() {
                    sh
                } else {
                    old
                }
            }
        }))
    }

    fn render<S>(&self, state: Self::State, _stores: S) -> StateQueryResult<Self::Output>
    where
        S: Stores<Self>,
        S::O: StoresIter<Self::Data>,
    {
        Ok(state.map(|sh| HeaderHash::with_data_sync(sh.header())))
    }

    fn as_map(&self) -> Arc<dyn Fn(&Row) -> StateQueryResult<Self::Data>> {
        Arc::new(row_to_header("blob"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insert::{insert_header, insert_op_lite};
    use ::fixt::prelude::*;
    use holochain_sqlite::{schema::SCHEMA_CELL, scratch::Scratch};
    use holochain_types::dht_op::DhtOpLight;

    #[test]
    fn test_chain_head_query() {
        observability::test_run().ok();
        let mut conn = Connection::open_in_memory().unwrap();
        SCHEMA_CELL.initialize(&mut conn, None).unwrap();

        let mut txn = conn
            .transaction_with_behavior(TransactionBehavior::Exclusive)
            .unwrap();

        let author = fixt!(AgentPubKey);

        // Create 5 consecutive headers for the authoring agent,
        // as well as 5 other random headers, interspersed.
        let shhs: Vec<_> = vec![
            fixt!(HeaderBuilderCommon),
            fixt!(HeaderBuilderCommon),
            fixt!(HeaderBuilderCommon),
            fixt!(HeaderBuilderCommon),
            fixt!(HeaderBuilderCommon),
        ]
        .into_iter()
        .enumerate()
        .flat_map(|(seq, random_header)| {
            let mut chain_header = random_header.clone();
            chain_header.header_seq = seq as u32;
            chain_header.author = author.clone();
            vec![chain_header, random_header]
        })
        .map(|b| {
            SignedHeaderHashed::with_presigned(
                // A chain made entirely of InitZomesComplete headers is totally invalid,
                // but we don't need a valid chain for this test,
                // we just need an ordered sequence of headers
                HeaderHashed::from_content_sync(InitZomesComplete::from_builder(b).into()),
                fixt!(Signature),
            )
        })
        .collect();

        let expected_head = shhs[8].clone();

        for shh in &shhs[..6] {
            let hash = shh.header_address();
            let op = DhtOpLight::StoreElement(hash.clone(), None, hash.clone().into());
            insert_header(&mut txn, shh.clone());
            insert_op_lite(&mut txn, op, fixt!(DhtOpHash), true);
        }

        let mut scratch = Scratch::<SignedHeader>::new();

        // It's also totally invalid for a call_zome scratch to contain headers
        // from other authors, but it doesn't matter here
        for shh in &shhs[6..] {
            scratch.add_item(shh.clone().into());
        }

        let query = ChainHeadQuery::new(author);

        let head = query.run(DbScratch::new(&[&mut txn], &scratch)).unwrap();
        assert_eq!(head.as_ref(), Some(expected_head.as_hash()));
    }
}
