//! The workflow and queue consumer for sys validation

use super::{
    error::WorkflowResult,
    integrate_dht_ops_workflow::{
        disintegrate_single_metadata, disintegrate_single_op, integrate_single_metadata,
        integrate_single_op,
    },
    produce_dht_ops_workflow::dht_op_light::light_to_op,
};
use crate::core::{
    queue_consumer::{OneshotWriter, TriggerSender, WorkComplete},
    state::{
        dht_op_integration::{IntegratedDhtOpsStore, IntegrationLimboStore, IntegrationLimboValue},
        element_buf::ElementBuf,
        metadata::MetadataBuf,
        validation_db::{ValidationLimboStatus, ValidationLimboStore, ValidationLimboValue},
        workspace::{Workspace, WorkspaceResult},
    },
};
use fallible_iterator::FallibleIterator;
use holo_hash::DhtOpHash;
use holochain_state::{
    buffer::{BufferedStore, KvBufFresh},
    db::{INTEGRATED_DHT_OPS, INTEGRATION_LIMBO},
    fresh_reader,
    prelude::*,
};
use holochain_types::{dht_op::DhtOp, validate::ValidationStatus, Timestamp};
use tracing::*;

#[instrument(skip(workspace, writer, trigger_integration))]
pub async fn app_validation_workflow(
    mut workspace: AppValidationWorkspace,
    writer: OneshotWriter,
    trigger_integration: &mut TriggerSender,
) -> WorkflowResult<WorkComplete> {
    warn!("unimplemented passthrough");

    let complete = app_validation_workflow_inner(&mut workspace).await?;
    // --- END OF WORKFLOW, BEGIN FINISHER BOILERPLATE ---

    // commit the workspace
    writer.with_writer(|writer| Ok(workspace.flush_to_txn(writer)?))?;

    // trigger other workflows
    trigger_integration.trigger();

    Ok(complete)
}
async fn app_validation_workflow_inner(
    workspace: &mut AppValidationWorkspace,
) -> WorkflowResult<WorkComplete> {
    let env = workspace.validation_limbo.env().clone();
    let (ops, mut awaiting_ops): (Vec<ValidationLimboValue>, Vec<ValidationLimboValue>) =
        fresh_reader!(env, |r| workspace
            .validation_limbo
            .drain_iter_filter(&r, |(_, vlv)| {
                match vlv.status {
                    // We only want sys validated or awaiting app dependency ops
                    ValidationLimboStatus::SysValidated
                    | ValidationLimboStatus::AwaitingAppDeps(_)
                    | ValidationLimboStatus::AwaitingProof => Ok(true),
                    ValidationLimboStatus::Pending | ValidationLimboStatus::AwaitingSysDeps(_) => {
                        Ok(false)
                    }
                }
            })?
            // Partition awaiting proof into a separate vec
            .partition(|vlv| match vlv.status {
                ValidationLimboStatus::AwaitingProof => Ok(false),
                _ => Ok(true),
            }))?;
    debug!(?ops, ?awaiting_ops);
    for mut vlv in ops {
        match &vlv.status {
            ValidationLimboStatus::AwaitingAppDeps(_) => {
                let op = light_to_op(vlv.op.clone(), &workspace.element_pending).await?;
                let hash = DhtOpHash::with_data(&op).await;
                workspace.to_val_limbo(hash, vlv).await?;
            }
            ValidationLimboStatus::SysValidated => {
                if vlv.awaiting_proof.awaiting_proof() {
                    vlv.status = ValidationLimboStatus::AwaitingProof;
                    awaiting_ops.push(vlv);
                } else {
                    let op = light_to_op(vlv.op.clone(), &workspace.element_pending).await?;
                    let hash = DhtOpHash::with_data(&op).await;
                    let iv = IntegrationLimboValue {
                        validation_status: ValidationStatus::Valid,
                        op: vlv.op,
                    };
                    workspace.to_int_limbo(hash, iv, op).await?;
                }
            }
            _ => unreachable!("Should not contain any other status"),
        }
    }
    // Check awaiting proof that might be able to be progressed now.
    // Including any awaiting proof from this run.
    for mut vlv in awaiting_ops {
        let mut still_awaiting = Vec::new();
        for dep in vlv.awaiting_proof.deps.drain(..) {
            if workspace.integration_limbo.contains(&dep)?
                || workspace.integrated_dht_ops.contains(&dep)?
            {
                // Discarding dep because we have proof it's integrated
            } else {
                // Dep is still not integrated so keep waiting
                still_awaiting.push(dep);
            }
        }
        let op = light_to_op(vlv.op.clone(), &workspace.element_pending).await?;
        let hash = DhtOpHash::with_data(&op).await;
        if still_awaiting.len() > 0 {
            vlv.awaiting_proof.deps = still_awaiting;
            workspace.to_val_limbo(hash, vlv).await?;
        } else {
            let iv = IntegrationLimboValue {
                validation_status: ValidationStatus::Valid,
                op: vlv.op,
            };
            workspace.to_int_limbo(hash, iv, op).await?;
        }
    }
    Ok(WorkComplete::Complete)
}

pub struct AppValidationWorkspace {
    pub integrated_dht_ops: IntegratedDhtOpsStore,
    pub integration_limbo: IntegrationLimboStore,
    pub validation_limbo: ValidationLimboStore,
    // Integrated data
    pub element_vault: ElementBuf,
    pub meta_vault: MetadataBuf,
    // Data pending validation
    pub element_pending: ElementBuf<PendingPrefix>,
    pub meta_pending: MetadataBuf<PendingPrefix>,
    // Data that has progressed past validation and is pending Integration
    pub element_validated: ElementBuf<ValidatedPrefix>,
    pub meta_validated: MetadataBuf<ValidatedPrefix>,
    // Cached data
    pub element_cache: ElementBuf,
    pub meta_cache: MetadataBuf,
}

impl AppValidationWorkspace {
    pub fn new(env: EnvironmentRead, dbs: &impl GetDb) -> WorkspaceResult<Self> {
        let db = dbs.get_db(&*INTEGRATED_DHT_OPS)?;
        let integrated_dht_ops = KvBufFresh::new(env.clone(), db);
        let db = dbs.get_db(&*INTEGRATION_LIMBO)?;
        let integration_limbo = KvBufFresh::new(env.clone(), db);

        let validation_limbo = ValidationLimboStore::new(env.clone(), dbs)?;

        let element_vault = ElementBuf::vault(env.clone(), dbs, false)?;
        let meta_vault = MetadataBuf::vault(env.clone(), dbs)?;
        let element_cache = ElementBuf::cache(env.clone(), dbs)?;
        let meta_cache = MetadataBuf::cache(env.clone(), dbs)?;

        let element_pending = ElementBuf::pending(env.clone(), dbs)?;
        let meta_pending = MetadataBuf::pending(env.clone(), dbs)?;

        let element_validated = ElementBuf::validated(env.clone(), dbs)?;
        let meta_validated = MetadataBuf::validated(env, dbs)?;

        Ok(Self {
            integrated_dht_ops,
            integration_limbo,
            validation_limbo,
            element_vault,
            meta_vault,
            element_pending,
            meta_pending,
            element_validated,
            meta_validated,
            element_cache,
            meta_cache,
        })
    }

    async fn to_val_limbo(
        &mut self,
        hash: DhtOpHash,
        mut vlv: ValidationLimboValue,
    ) -> WorkflowResult<()> {
        vlv.last_try = Some(Timestamp::now());
        vlv.num_tries += 1;
        self.validation_limbo.put(hash, vlv)?;
        Ok(())
    }

    async fn to_int_limbo(
        &mut self,
        hash: DhtOpHash,
        iv: IntegrationLimboValue,
        op: DhtOp,
    ) -> WorkflowResult<()> {
        disintegrate_single_metadata(iv.op.clone(), &self.element_pending, &mut self.meta_pending)
            .await?;
        disintegrate_single_op(iv.op.clone(), &mut self.element_pending);
        integrate_single_op(op, &mut self.element_validated).await?;
        integrate_single_metadata(
            iv.op.clone(),
            &self.element_validated,
            &mut self.meta_validated,
        )
        .await?;
        self.integration_limbo.put(hash, iv)?;
        Ok(())
    }
}

impl Workspace for AppValidationWorkspace {
    fn flush_to_txn(self, writer: &mut Writer) -> WorkspaceResult<()> {
        warn!("unimplemented passthrough");
        self.validation_limbo.0.flush_to_txn(writer)?;
        self.integration_limbo.flush_to_txn(writer)?;
        self.element_pending.flush_to_txn(writer)?;
        self.meta_pending.flush_to_txn(writer)?;
        self.element_validated.flush_to_txn(writer)?;
        self.meta_validated.flush_to_txn(writer)?;
        Ok(())
    }
}
