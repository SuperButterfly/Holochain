use crate::core::ribosome::error::RibosomeResult;
use crate::core::ribosome::wasm_ribosome::WasmRibosome;
use crate::core::ribosome::HostContext;
use holo_hash::HasHash;
use holochain_zome_types::Entry;
use holochain_zome_types::EntryHashInput;
use holochain_zome_types::EntryHashOutput;
use std::sync::Arc;

pub fn entry_hash(
    _ribosome: Arc<WasmRibosome>,
    _host_context: Arc<HostContext>,
    input: EntryHashInput,
) -> RibosomeResult<EntryHashOutput> {
    let entry: Entry = input.into_inner();

    let entry_hash = tokio_safe_block_on::tokio_safe_block_forever_on(async move {
        holochain_types::entry::EntryHashed::with_data(entry).await
    })?
    .into_hash();

    Ok(EntryHashOutput::new(entry_hash))
}

#[cfg(test)]
#[cfg(feature = "slow_tests")]
pub mod wasm_test {
    use super::*;
    use crate::core::ribosome::host_fn::entry_hash::entry_hash;
    use crate::core::ribosome::HostContextFixturator;
    use crate::core::state::workspace::Workspace;
    use crate::fixt::EntryFixturator;
    use crate::fixt::WasmRibosomeFixturator;
    use holo_hash::EntryHash;
    use holochain_state::env::ReadManager;
    use holochain_wasm_test_utils::TestWasm;
    use holochain_zome_types::EntryHashInput;
    use holochain_zome_types::EntryHashOutput;
    use std::convert::TryInto;
    use std::sync::Arc;
    use test_wasm_common::TestString;

    #[tokio::test(threaded_scheduler)]
    /// we can get an entry hash out of the fn directly
    async fn entry_hash_test() {
        let ribosome = WasmRibosomeFixturator::new(crate::fixt::curve::Zomes(vec![]))
            .next()
            .unwrap();
        let host_context = HostContextFixturator::new(fixt::Unpredictable)
            .next()
            .unwrap();
        let entry = EntryFixturator::new(fixt::Predictable).next().unwrap();
        let input = EntryHashInput::new(entry);

        let output: EntryHashOutput =
            entry_hash(Arc::new(ribosome), Arc::new(host_context), input).unwrap();

        assert_eq!(output.into_inner().get_raw().to_vec().len(), 36,);
    }

    #[tokio::test(threaded_scheduler)]
    /// we can get an entry hash out of the fn via. a wasm call
    async fn ribosome_entry_hash_test() {
        let env = holochain_state::test_utils::test_cell_env();
        let dbs = env.dbs().await;
        let env_ref = env.guard().await;
        let reader = env_ref.reader().unwrap();
        let mut workspace = crate::core::workflow::InvokeZomeWorkspace::new(&reader, &dbs).unwrap();

        let (_g, raw_workspace) = crate::core::workflow::unsafe_invoke_zome_workspace::UnsafeInvokeZomeWorkspace::from_mut(&mut workspace);

        let entry = EntryFixturator::new(fixt::Predictable).next().unwrap();
        let input = EntryHashInput::new(entry);
        let output: EntryHashOutput =
            crate::call_test_ribosome!(raw_workspace, TestWasm::Imports, "entry_hash", input);
        assert_eq!(output.into_inner().get_raw().to_vec().len(), 36,);
    }

    #[tokio::test(threaded_scheduler)]
    /// the hash path underlying anchors wraps entry_hash
    async fn ribosome_hash_path_pwd_test() {
        let env = holochain_state::test_utils::test_cell_env();
        let dbs = env.dbs().await;
        let env_ref = env.guard().await;
        let reader = env_ref.reader().unwrap();
        let mut workspace = crate::core::workflow::InvokeZomeWorkspace::new(&reader, &dbs).unwrap();

        let (_g, raw_workspace) = crate::core::workflow::unsafe_invoke_zome_workspace::UnsafeInvokeZomeWorkspace::from_mut(&mut workspace);

        let input = TestString::from("foo.bar".to_string());
        let output: EntryHash =
            crate::call_test_ribosome!(raw_workspace, TestWasm::HashPath, "hash", input);

        let expected_path = hdk3::hash_path::path::Path::from("foo.bar");

        let expected_hash = tokio_safe_block_on::tokio_safe_block_forever_on(async move {
            holochain_types::entry::EntryHashed::with_data(Entry::App(
                (&expected_path).try_into().unwrap(),
            ))
            .await
        })
        .unwrap()
        .into_hash();

        assert_eq!(expected_hash.into_inner(), output.into_inner(),);
    }
}
