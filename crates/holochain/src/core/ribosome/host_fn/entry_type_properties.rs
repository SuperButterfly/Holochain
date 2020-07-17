use crate::core::ribosome::error::RibosomeResult;
use crate::core::ribosome::wasm_ribosome::WasmRibosome;
use crate::core::ribosome::CallContext;
use holochain_zome_types::EntryTypePropertiesInput;
use holochain_zome_types::EntryTypePropertiesOutput;
use std::sync::Arc;

pub fn entry_type_properties(
    _ribosome: Arc<WasmRibosome>,
    _host_context: Arc<CallContext>,
    _input: EntryTypePropertiesInput,
) -> RibosomeResult<EntryTypePropertiesOutput> {
    unimplemented!();
}
