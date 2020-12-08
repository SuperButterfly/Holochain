use crate::holochain::core::ribosome::error::RibosomeResult;
use crate::holochain::core::ribosome::CallContext;
use crate::holochain::core::ribosome::RibosomeT;
use holo_hash::HasHash;
use crate::holochain_zome_types::zome_info::ZomeInfo;
use crate::holochain_zome_types::ZomeInfoInput;
use crate::holochain_zome_types::ZomeInfoOutput;
use std::sync::Arc;

pub fn zome_info(
    ribosome: Arc<impl RibosomeT>,
    call_context: Arc<CallContext>,
    _input: ZomeInfoInput,
) -> RibosomeResult<ZomeInfoOutput> {
    Ok(ZomeInfoOutput::new(ZomeInfo {
        dna_name: ribosome.dna_def().name.clone(),
        zome_name: call_context.zome.zome_name().clone(),
        dna_hash: ribosome.dna_def().as_hash().clone(),
        zome_id: ribosome.zome_to_id(&call_context.zome)?,
        properties: ribosome.dna_def().properties.clone(),
        // @TODO
        // public_token: "".into(),
    }))
}

#[cfg(test)]
#[cfg(feature = "slow_tests")]
pub mod test {
    use crate::holochain::fixt::ZomeCallHostAccessFixturator;
    use ::fixt::prelude::*;
    use crate::holochain_wasm_test_utils::TestWasm;
    use crate::holochain_zome_types::ZomeInfoOutput;

    #[tokio::test(threaded_scheduler)]
    async fn invoke_import_zome_info_test() {
        let test_env = crate::holochain_state::test_utils::test_cell_env();
        let env = test_env.env();
        let mut workspace =
            crate::holochain::core::workflow::CallZomeWorkspace::new(env.clone().into()).unwrap();
        crate::holochain::core::workflow::fake_genesis(&mut workspace.source_chain)
            .await
            .unwrap();

        let workspace_lock = crate::holochain::core::workflow::CallZomeWorkspaceLock::new(workspace);

        let mut host_access = fixt!(ZomeCallHostAccess);
        host_access.workspace = workspace_lock;
        let zome_info: ZomeInfoOutput =
            crate::call_test_ribosome!(host_access, TestWasm::ZomeInfo, "zome_info", ());
        assert_eq!(zome_info.inner_ref().dna_name, "test",);
    }
}
