use super::HostContext;
use super::WasmRibosome;
use holochain_zome_types::SendInput;
use holochain_zome_types::SendOutput;
use std::sync::Arc;

pub async fn send(
    _ribosome: Arc<WasmRibosome>,
    _host_context: Arc<HostContext>,
    _input: SendInput,
) -> SendOutput {
    unimplemented!();
}