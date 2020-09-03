//! The workflow and queue consumer for DhtOp integration

use super::*;

use crate::{
    conductor::manager::ManagedTaskResult,
    core::workflow::integrate_dht_ops_workflow::{
        integrate_dht_ops_workflow, IntegrateDhtOpsWorkspace,
    },
};
use futures::future::Either;
use holochain_state::env::EnvironmentWrite;
use holochain_state::env::ReadManager;
use tokio::task::JoinHandle;
use tracing::*;

/// Spawn the QueueConsumer for DhtOpIntegration workflow
#[instrument(skip(env, stop))]
pub fn spawn_integrate_dht_ops_consumer(
    env: EnvironmentWrite,
    mut stop: sync::broadcast::Receiver<()>,
) -> (
    TriggerSender,
    tokio::sync::oneshot::Receiver<()>,
    JoinHandle<ManagedTaskResult>,
) {
    let (tx, mut rx) = TriggerSender::new();
    let (tx_first, rx_first) = tokio::sync::oneshot::channel();
    let mut tx_first = Some(tx_first);
    let mut trigger_self = tx.clone();
    let handle = tokio::spawn(async move {
        loop {
            let workspace = IntegrateDhtOpsWorkspace::new(env.clone().into())
                .expect("Could not create Workspace");
            if let WorkComplete::Incomplete =
                integrate_dht_ops_workflow(workspace, env.clone().into())
                    .await
                    .expect("Error running Workflow")
            {
                trigger_self.trigger()
            };
            // notify the Cell that the first loop has completed
            if let Some(tx_first) = tx_first.take() {
                let _ = tx_first.send(());
            }
            // Check for shutdown or next job
            let next_job = rx.listen();
            let kill = stop.recv();
            tokio::pin!(next_job);
            tokio::pin!(kill);

            if let Either::Left((Err(_), _)) | Either::Right((_, _)) =
                futures::future::select(next_job, kill).await
            {
                tracing::warn!("Cell is shutting down: stopping queue consumer.");
                break;
            };
        }
        Ok(())
    });
    (tx, rx_first, handle)
}
