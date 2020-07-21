use crate::core::ribosome::error::RibosomeResult;
use crate::core::ribosome::wasm_ribosome::WasmRibosome;
use crate::core::ribosome::CallContext;
use holochain_zome_types::QueryInput;
use holochain_zome_types::QueryOutput;
use std::sync::Arc;

pub fn query(
    _ribosome: Arc<WasmRibosome>,
    _call_context: Arc<CallContext>,
    _input: QueryInput,
) -> RibosomeResult<QueryOutput> {
    unimplemented!();
}
