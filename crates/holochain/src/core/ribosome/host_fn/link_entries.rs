use crate::core::ribosome::error::{RibosomeError, RibosomeResult};
use crate::core::{
    ribosome::{HostContext, RibosomeT},
    workflow::InvokeZomeWorkspace,
    SourceChainResult,
};
use futures::future::BoxFuture;
use futures::future::FutureExt;
use holochain_types::{composite_hash::HeaderAddress, header::builder};
use holochain_zome_types::LinkEntriesInput;
use holochain_zome_types::LinkEntriesOutput;
use std::convert::TryInto;
use std::sync::Arc;

#[allow(clippy::extra_unused_lifetimes)]
pub fn link_entries<'a>(
    ribosome: Arc<impl RibosomeT>,
    host_context: Arc<HostContext>,
    input: LinkEntriesInput,
) -> RibosomeResult<LinkEntriesOutput> {
    let (base_address, target_address, tag) = input.into_inner();
    let base_address = base_address.try_into()?;
    let target_address = target_address.try_into()?;

    // extract the zome position
    let zome_id: holochain_types::header::ZomeId = match ribosome
        .dna_file()
        .dna
        .zomes
        .iter()
        .position(|(name, _)| name == &host_context.zome_name)
    {
        Some(index) => holochain_types::header::ZomeId::from(index as u8),
        None => Err(RibosomeError::ZomeNotExists(host_context.zome_name.clone()))?,
    };

    // Construct the link add
    let header_builder = builder::LinkAdd::new(base_address, target_address, zome_id, tag);

    let call = |workspace: &'a mut InvokeZomeWorkspace| -> BoxFuture<'a, SourceChainResult<HeaderAddress>> {
        async move {
            let source_chain = &mut workspace.source_chain;
            // push the header into the source chain
            source_chain.put(header_builder, None).await
        }
        .boxed()
    };
    let link_hash =
        tokio_safe_block_on::tokio_safe_block_forever_on(tokio::task::spawn(async move {
            unsafe { host_context.workspace.apply_mut(call).await }
        }))???;

    // return the hash of the committed link
    // note that validation is handled by the workflow
    // if the validation fails this commit will be rolled back by virtue of the lmdb transaction
    // being atomic
    Ok(LinkEntriesOutput::new(link_hash.into()))
}
