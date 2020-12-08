use crate::holochain::core::ribosome::error::RibosomeResult;
use crate::holochain::core::ribosome::CallContext;
use crate::holochain::core::ribosome::RibosomeT;
use crate::holochain_zome_types::UnreachableInput;
use crate::holochain_zome_types::UnreachableOutput;
use std::sync::Arc;

pub fn unreachable(
    _ribosome: Arc<impl RibosomeT>,
    _call_context: Arc<CallContext>,
    _input: UnreachableInput,
) -> RibosomeResult<UnreachableOutput> {
    unreachable!();
}
