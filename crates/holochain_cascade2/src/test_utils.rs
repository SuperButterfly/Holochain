use holo_hash::hash_type::AnyDht;
use holo_hash::AgentPubKey;
use holo_hash::HasHash;
use holo_hash::HeaderHash;
use holochain_p2p::actor;
use holochain_p2p::HolochainP2pError;
use holochain_state::mutations::set_when_integrated;
use holochain_state::mutations::update_op_validation_status;
use holochain_types::dht_op::DhtOpHashed;
use holochain_types::env::EnvRead;
use holochain_types::env::EnvWrite;
use holochain_types::metadata::MetadataSet;
use holochain_types::prelude::ValidationPackageResponse;
use holochain_types::timestamp;
use holochain_zome_types::AgentActivityFilterDeterministic;
use holochain_zome_types::AgentActivityResponseDeterministic;
use holochain_zome_types::HeaderHashed;
use holochain_zome_types::SignedHeaderHashed;
use holochain_zome_types::ValidationStatus;

use crate::authority;
use crate::authority::WireDhtOp;
use crate::authority::WireLinkKey;
use crate::authority::WireLinkOps;
use crate::authority::WireOps;
use holochain_sqlite::db::WriteManager;
use holochain_sqlite::prelude::DatabaseResult;
use holochain_state::mutations::insert_op;

pub use activity_test_data::*;
pub use element_test_data::*;
pub use entry_test_data::*;

mod activity_test_data;
mod element_test_data;
mod entry_test_data;

#[derive(Clone)]
pub struct PassThroughNetwork {
    envs: Vec<EnvRead>,
    authority: bool,
}

impl PassThroughNetwork {
    pub fn authority_for_all(envs: Vec<EnvRead>) -> Self {
        Self {
            envs,
            authority: true,
        }
    }

    pub fn authority_for_nothing(envs: Vec<EnvRead>) -> Self {
        Self {
            envs,
            authority: false,
        }
    }
}

#[derive(Clone)]
pub struct MockNetwork(std::sync::Arc<tokio::sync::Mutex<MockHolochainP2pCellT2>>);

impl MockNetwork {
    pub fn new(mock: MockHolochainP2pCellT2) -> Self {
        Self(std::sync::Arc::new(tokio::sync::Mutex::new(mock)))
    }
}

#[mockall::automock]
#[async_trait::async_trait]
pub trait HolochainP2pCellT2 {
    async fn get_validation_package(
        &mut self,
        request_from: AgentPubKey,
        header_hash: HeaderHash,
    ) -> actor::HolochainP2pResult<ValidationPackageResponse>;

    async fn get(
        &mut self,
        dht_hash: holo_hash::AnyDhtHash,
        options: actor::GetOptions,
    ) -> actor::HolochainP2pResult<Vec<WireOps>>;

    async fn get_meta(
        &mut self,
        dht_hash: holo_hash::AnyDhtHash,
        options: actor::GetMetaOptions,
    ) -> actor::HolochainP2pResult<Vec<MetadataSet>>;

    async fn get_links(
        &mut self,
        link_key: WireLinkKey,
        options: actor::GetLinksOptions,
    ) -> actor::HolochainP2pResult<Vec<WireLinkOps>>;

    async fn get_agent_activity(
        &mut self,
        agent: AgentPubKey,
        query: AgentActivityFilterDeterministic,
        options: actor::GetActivityOptions,
    ) -> actor::HolochainP2pResult<Vec<AgentActivityResponseDeterministic>>;

    async fn authority_for_hash(
        &mut self,
        dht_hash: holo_hash::AnyDhtHash,
    ) -> actor::HolochainP2pResult<bool>;
}

#[async_trait::async_trait]
impl HolochainP2pCellT2 for PassThroughNetwork {
    async fn get_validation_package(
        &mut self,
        _request_from: AgentPubKey,
        _header_hash: HeaderHash,
    ) -> actor::HolochainP2pResult<ValidationPackageResponse> {
        todo!()
    }

    async fn get(
        &mut self,
        dht_hash: holo_hash::AnyDhtHash,
        options: actor::GetOptions,
    ) -> actor::HolochainP2pResult<Vec<WireOps>> {
        let mut out = Vec::new();
        match *dht_hash.hash_type() {
            AnyDht::Entry => {
                for env in &self.envs {
                    let r = authority::handle_get_entry(
                        env.clone(),
                        dht_hash.clone().into(),
                        (&options).into(),
                    )
                    .map_err(|e| HolochainP2pError::Other(e.into()))?;
                    out.push(WireOps::Entry(r));
                }
            }
            AnyDht::Header => {
                for env in &self.envs {
                    let r = authority::handle_get_element(
                        env.clone(),
                        dht_hash.clone().into(),
                        (&options).into(),
                    )
                    .map_err(|e| HolochainP2pError::Other(e.into()))?;
                    out.push(WireOps::Element(r));
                }
            }
        }
        Ok(out)
    }
    async fn get_meta(
        &mut self,
        _dht_hash: holo_hash::AnyDhtHash,
        _options: actor::GetMetaOptions,
    ) -> actor::HolochainP2pResult<Vec<MetadataSet>> {
        todo!()
    }
    async fn get_links(
        &mut self,
        link_key: WireLinkKey,
        options: actor::GetLinksOptions,
    ) -> actor::HolochainP2pResult<Vec<WireLinkOps>> {
        let mut out = Vec::new();
        for env in &self.envs {
            let r = authority::handle_get_links(env.clone(), link_key.clone(), (&options).into())
                .map_err(|e| HolochainP2pError::Other(e.into()))?;
            out.push(r);
        }
        Ok(out)
    }
    async fn get_agent_activity(
        &mut self,
        agent: AgentPubKey,
        query: AgentActivityFilterDeterministic,
        options: actor::GetActivityOptions,
    ) -> actor::HolochainP2pResult<Vec<AgentActivityResponseDeterministic>> {
        let mut out = Vec::new();
        for env in &self.envs {
            let r = authority::handle_get_agent_activity_deterministic(
                env.clone(),
                agent.clone(),
                query.clone(),
                (&options).into(),
            )
            .map_err(|e| HolochainP2pError::Other(e.into()))?;
            out.push(r);
        }
        Ok(out)
    }

    async fn authority_for_hash(
        &mut self,
        _dht_hash: holo_hash::AnyDhtHash,
    ) -> actor::HolochainP2pResult<bool> {
        Ok(self.authority)
    }
}

pub fn fill_db(env: &EnvWrite, op: DhtOpHashed) {
    env.conn()
        .unwrap()
        .with_commit(|txn| {
            let hash = op.as_hash().clone();
            insert_op(txn, op, false).unwrap();
            update_op_validation_status(txn, hash.clone(), ValidationStatus::Valid).unwrap();
            set_when_integrated(txn, hash, timestamp::now()).unwrap();
            DatabaseResult::Ok(())
        })
        .unwrap();
}

pub fn fill_db_rejected(env: &EnvWrite, op: DhtOpHashed) {
    env.conn()
        .unwrap()
        .with_commit(|txn| {
            let hash = op.as_hash().clone();
            insert_op(txn, op, false).unwrap();
            update_op_validation_status(txn, hash.clone(), ValidationStatus::Rejected).unwrap();
            set_when_integrated(txn, hash, timestamp::now()).unwrap();
            DatabaseResult::Ok(())
        })
        .unwrap();
}

pub fn fill_db_pending(env: &EnvWrite, op: DhtOpHashed) {
    env.conn()
        .unwrap()
        .with_commit(|txn| {
            let hash = op.as_hash().clone();
            insert_op(txn, op, false).unwrap();
            update_op_validation_status(txn, hash.clone(), ValidationStatus::Valid).unwrap();
            DatabaseResult::Ok(())
        })
        .unwrap();
}

pub fn fill_db_as_author(env: &EnvWrite, op: DhtOpHashed) {
    env.conn()
        .unwrap()
        .with_commit(|txn| {
            insert_op(txn, op, true).unwrap();
            DatabaseResult::Ok(())
        })
        .unwrap();
}

#[async_trait::async_trait]
impl HolochainP2pCellT2 for MockNetwork {
    async fn get_validation_package(
        &mut self,
        request_from: AgentPubKey,
        header_hash: HeaderHash,
    ) -> actor::HolochainP2pResult<ValidationPackageResponse> {
        self.0
            .lock()
            .await
            .get_validation_package(request_from, header_hash)
            .await
    }

    async fn get(
        &mut self,
        dht_hash: holo_hash::AnyDhtHash,
        options: actor::GetOptions,
    ) -> actor::HolochainP2pResult<Vec<WireOps>> {
        self.0.lock().await.get(dht_hash, options).await
    }

    async fn get_meta(
        &mut self,
        dht_hash: holo_hash::AnyDhtHash,
        options: actor::GetMetaOptions,
    ) -> actor::HolochainP2pResult<Vec<MetadataSet>> {
        self.0.lock().await.get_meta(dht_hash, options).await
    }

    async fn get_links(
        &mut self,
        link_key: WireLinkKey,
        options: actor::GetLinksOptions,
    ) -> actor::HolochainP2pResult<Vec<WireLinkOps>> {
        self.0.lock().await.get_links(link_key, options).await
    }

    async fn get_agent_activity(
        &mut self,
        agent: AgentPubKey,
        query: AgentActivityFilterDeterministic,
        options: actor::GetActivityOptions,
    ) -> actor::HolochainP2pResult<Vec<AgentActivityResponseDeterministic>> {
        self.0
            .lock()
            .await
            .get_agent_activity(agent, query, options)
            .await
    }

    async fn authority_for_hash(
        &mut self,
        dht_hash: holo_hash::AnyDhtHash,
    ) -> actor::HolochainP2pResult<bool> {
        self.0.lock().await.authority_for_hash(dht_hash).await
    }
}

pub fn wire_op_to_shh(op: &WireDhtOp) -> SignedHeaderHashed {
    SignedHeaderHashed::with_presigned(
        HeaderHashed::from_content_sync(op.header.clone()),
        op.signature.clone(),
    )
}
