use super::HostContext;
use super::WasmRibosome;
use crate::core::ribosome::error::RibosomeResult;
use crate::core::ribosome::RibosomeT;
use holo_hash::holo_hash_core::HeaderHash;
use holochain_zome_types::commit::CommitEntryResult;
use holochain_zome_types::entry::Entry;
use holochain_zome_types::validate::ValidateEntryResult;
use holochain_zome_types::CommitEntryInput;
use holochain_zome_types::CommitEntryOutput;
use std::sync::Arc;

pub async fn commit_entry(
    ribosome: Arc<WasmRibosome>,
    host_context: Arc<HostContext>,
    input: CommitEntryInput,
) -> RibosomeResult<CommitEntryOutput> {
    let entry: Entry = input.into_inner();
    let validate = ribosome.run_validate(host_context.zome_name.clone(), &entry)?;
    Ok(CommitEntryOutput::new(match validate {
        // @todo actually commit an entry and put the header here
        ValidateEntryResult::Valid => CommitEntryResult::Success(HeaderHash::new(vec![0xdb; 36])),
        invalid => CommitEntryResult::ValidateFailed(invalid),
    }))
}
