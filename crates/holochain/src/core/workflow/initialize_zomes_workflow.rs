use super::{
    error::WorkflowRunResult, unsafe_invoke_zome_workspace::UnsafeInvokeZomeWorkspace,
    InvokeZomeWorkspace,
};
use crate::core::{
    queue_consumer::OneshotWriter,
    ribosome::{
        guest_callback::init::{InitInvocation, InitResult},
        RibosomeT,
    },
    state::workspace::{Workspace, WorkspaceError, WorkspaceResult},
};
use derive_more::Constructor;
use holochain_state::buffer::BufferedStore;
use holochain_state::prelude::{GetDb, Reader, Writer};
use holochain_types::{dna::DnaDef, header::builder};

#[derive(Constructor)]
pub struct InitializeZomesWorkflowArgs<Ribosome: RibosomeT> {
    pub dna_def: DnaDef,
    pub ribosome: Ribosome,
}

// TODO: #[instrument]
pub async fn initialize_zomes_workflow<'env, Ribosome: RibosomeT>(
    mut workspace: InitializeZomesWorkspace<'env>,
    writer: OneshotWriter,
    args: InitializeZomesWorkflowArgs<Ribosome>,
) -> WorkflowRunResult<InitResult> {
    let result = initialize_zomes_workflow_inner(&mut workspace, args).await?;

    // --- END OF WORKFLOW, BEGIN FINISHER BOILERPLATE ---

    // commit the workspace
    writer
        .with_writer(|writer| workspace.flush_to_txn(writer).expect("TODO"))
        .await?;

    Ok(result)
}

async fn initialize_zomes_workflow_inner<'env, Ribosome: RibosomeT>(
    workspace: &mut InitializeZomesWorkspace<'env>,
    args: InitializeZomesWorkflowArgs<Ribosome>,
) -> WorkflowRunResult<InitResult> {
    let InitializeZomesWorkflowArgs { dna_def, ribosome } = args;
    // Call the init callback
    let result = {
        // TODO: We need a better solution then reusung the InvokeZomeWorkspace (i.e. ghost actor)
        let (_g, raw_workspace) = UnsafeInvokeZomeWorkspace::from_mut(&mut workspace.0);
        let invocation = InitInvocation { dna_def };
        ribosome.run_init(raw_workspace, invocation)?
    };

    // Insert the init marker
    workspace
        .0
        .source_chain
        .put(builder::InitZomesComplete {}, None)
        .await?;

    Ok(result)
}

pub struct InitializeZomesWorkspace<'env>(pub(crate) InvokeZomeWorkspace<'env>);

impl<'env> Workspace<'env> for InitializeZomesWorkspace<'env> {
    /// Constructor
    #[allow(dead_code)]
    fn new(reader: &'env Reader<'env>, dbs: &impl GetDb) -> WorkspaceResult<Self> {
        Ok(Self(InvokeZomeWorkspace::new(reader, dbs)?))
    }

    fn flush_to_txn(self, writer: &mut Writer) -> Result<(), WorkspaceError> {
        self.0.source_chain.into_inner().flush_to_txn(writer)?;
        self.0.meta.flush_to_txn(writer)?;
        self.0.cache_cas.flush_to_txn(writer)?;
        self.0.cache_meta.flush_to_txn(writer)?;
        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::core::ribosome::MockRibosomeT;
    use crate::core::workflow::fake_genesis;
    use crate::fixt::DnaDefFixturator;
    use fixt::Unpredictable;
    use holochain_state::{env::ReadManager, test_utils::test_cell_env};
    use holochain_types::Header;
    use matches::assert_matches;

    #[tokio::test(threaded_scheduler)]
    async fn adds_init_marker() {
        let env = test_cell_env();
        let dbs = env.dbs().await;
        let env_ref = env.guard().await;
        let reader = env_ref.reader().unwrap();
        let mut workspace =
            InitializeZomesWorkspace(InvokeZomeWorkspace::new(&reader, &dbs).unwrap());
        let mut ribosome = MockRibosomeT::new();

        // Setup the ribosome mock
        ribosome
            .expect_run_init()
            .returning(move |_workspace, _invocation| Ok(InitResult::Pass));

        // Genesis
        fake_genesis(&mut workspace.0.source_chain).await.unwrap();

        let dna_def = DnaDefFixturator::new(Unpredictable).next().unwrap();

        let args = InitializeZomesWorkflowArgs { ribosome, dna_def };
        initialize_zomes_workflow_inner(&mut workspace, args)
            .await
            .unwrap();

        // Check init is added to the workspace
        assert_matches!(
            workspace.0.source_chain.get_index(3).await,
            Ok(Some(Header::InitZomesComplete(_)))
        );
    }
}
