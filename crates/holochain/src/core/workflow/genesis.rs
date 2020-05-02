/// Genesis Workflow: Initialize the source chain with the initial entries:
/// - Dna
/// - AgentValidationPkg
/// - AgentId
///
/// FIXME: understand the details of actually getting the DNA
/// FIXME: creating entries in the config db
use super::{
    error::{WorkflowError, WorkflowResult},
    WorkflowCaller, WorkflowEffects, WorkflowTriggers,
};
use crate::{conductor::api::CellConductorApiT, core::state::workspace::GenesisWorkspace};
use futures::future::FutureExt;
use holochain_types::{dna::Dna, entry::Entry, prelude::*};
use must_future::MustBoxFuture;

pub struct GenesisWorkflow<Api: CellConductorApiT> {
    api: Api,
    dna: Dna,
    agent_hash: AgentHash,
}


impl<'env, Api: CellConductorApiT + Send + Sync + 'env> WorkflowCaller<'env>
    for GenesisWorkflow<Api>
{
    type Output = ();
    type Workspace = GenesisWorkspace<'env>;
    type Triggers = ();

    fn workflow(
        self,
        mut workspace: Self::Workspace,
    ) -> MustBoxFuture<'env, WorkflowResult<'env, Self::Output, Self>> {
        async {
            let Self {
                api,
                dna,
                agent_hash,
            } = self;
            // TODO: this is a placeholder for a real DPKI request to show intent
            if api
                .dpki_request("is_agent_hash_valid".into(), agent_hash.to_string())
                .await?
                == "INVALID"
            {
                return Err(WorkflowError::AgentInvalid(agent_hash.clone()));
            }

            workspace
                .source_chain
                .put_entry(Entry::Dna(Box::new(dna)), &agent_hash)
                .await?;
            workspace
                .source_chain
                .put_entry(Entry::AgentKey(agent_hash.clone()), &agent_hash)
                .await?;

            let fx = WorkflowEffects {
                workspace,
                signals: Default::default(),
                callbacks: Default::default(),
                triggers: (),
                __lifetime: Default::default(),
            };
            let result = ();

            Ok((result, fx))
        }
        .boxed()
        .into()
    }
}

#[cfg(test)]
mod tests {

    use super::GenesisWorkflow;
    use crate::core::workflow::caller::{run_workflow_3, WorkflowCaller, run_workflow_4};
    use crate::{
        conductor::api::MockCellConductorApi,
        core::{
            state::{
                source_chain::SourceChain,
                workspace::{GenesisWorkspace, Workspace},
            },
            workflow::error::WorkflowError,
        },
    };
    use fallible_iterator::FallibleIterator;
    use holochain_state::{env::*, test_utils::test_cell_env};
    use holochain_types::{
        entry::Entry,
        observability,
        prelude::*,
        test_utils::{fake_agent_hash, fake_dna},
    };
    use tracing::*;

    #[tokio::test]
    async fn genesis_initializes_source_chain() -> Result<(), anyhow::Error> {
        observability::test_run()?;
        let arc = test_cell_env();
        let env = arc.guard().await;
        let dbs = arc.dbs().await?;
        let dna = fake_dna("a");
        let agent_hash = fake_agent_hash("a");

        {
            let reader = env.reader()?;
            let workspace = GenesisWorkspace::new(&reader, &dbs)?;
            let mut api = MockCellConductorApi::new();
            api.expect_sync_dpki_request()
                .returning(|_, _| Ok("mocked dpki request response".to_string()));
            let workflow = GenesisWorkflow {
                api,
                dna: dna.clone(),
                agent_hash: agent_hash.clone(),
            };
            let _: () = run_workflow_4(workflow, workspace, arc.clone()).await?;
            // let writer = env.writer()?;
            // fx.workspace.commit_txn(writer)?;
        }

        env.with_reader(|reader| {
            let source_chain = SourceChain::new(&reader, &dbs)?;
            assert_eq!(source_chain.agent_hash()?, agent_hash);
            source_chain.chain_head().expect("chain head should be set");
            let hashes: Vec<_> = source_chain
                .iter_back()
                .map(|h| {
                    debug!("header: {:?}", h);
                    Ok(h.entry_address().clone())
                })
                .collect()
                .unwrap();
            assert_eq!(
                hashes,
                vec![
                    holo_hash::EntryHash::try_from(Entry::AgentKey(agent_hash))
                        .unwrap()
                        .into(),
                    holo_hash::EntryHash::try_from(Entry::Dna(Box::new(dna)))
                        .unwrap()
                        .into(),
                ]
            );
            Result::<_, WorkflowError>::Ok(())
        })?;
        Ok(())
    }
}

/* TODO: make doc-able

Called from:

 - Conductor upon first ACTIVATION of an installed DNA (trace: follow)



Parameters (expected types/structures):

- DNA hash to pull from path to file (or HCHC [FUTURE] )

- AgentID [SEEDLING] (already registered in DeepKey [LEAPFROG])

- Membrane Access Payload (optional invitation code / to validate agent join) [possible for LEAPFROG]



Data X (data & structure) from Store Y:

- Get DNA from HCHC by DNA hash

- or Get DNA from filesystem by filename



----

Functions / Workflows:

- check that agent key is valid [MOCKED dpki] (via real dpki [LEAPFROG])

- retrieve DNA from file path [in the future from HCHC]

- initialize lmdb environment and dbs, save to conductor runtime config.

- commit DNA entry (w/ special enum header with NULL  prev_header)

- commit CapToken Grant for author (agent key) (w/ normal header)



    fn commit_DNA

    fn produce_header



Examples / Tests / Acceptance Criteria:

- check hash of DNA =



----



Persisted X Changes to Store Y (data & structure):

- source chain HEAD 2 new headers

- CAS commit headers and genesis entries: DNA & Author Capabilities Grant (Agent Key)



- bootstrapped peers from attempt to publish key and join network



Spawned Tasks (don't wait for result -signals/log/tracing=follow):

- ZomeCall:init (for processing app initialization with bridges & networking)

- DHT transforms of genesis entries in CAS



Returned Results (type & structure):

- None
*/
