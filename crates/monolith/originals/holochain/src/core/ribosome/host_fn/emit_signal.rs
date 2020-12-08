use monolith::holochain::core::ribosome::RibosomeT;
use monolith::holochain::core::ribosome::{error::RibosomeResult, CallContext};
use monolith::holochain::core::signal::Signal;
use monolith::holochain_zome_types::EmitSignalInput;
use monolith::holochain_zome_types::EmitSignalOutput;
use std::sync::Arc;

pub fn emit_signal(
    _ribosome: Arc<impl RibosomeT>,
    call_context: Arc<CallContext>,
    input: EmitSignalInput,
) -> RibosomeResult<EmitSignalOutput> {
    let cell_id = call_context.host_access().cell_id().clone();
    let bytes = input.into_inner();
    let signal = Signal::App(cell_id, bytes);
    call_context.host_access().signal_tx().send(signal)?;
    Ok(EmitSignalOutput::new(()))
}
