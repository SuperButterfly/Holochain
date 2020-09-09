use crate::core::ribosome::error::RibosomeResult;
use crate::core::workflow::integrate_dht_ops_workflow::integrate_to_cache;
use crate::core::{
    ribosome::{CallContext, RibosomeT},
    workflow::CallZomeWorkspace,
    SourceChainResult,
};
use holochain_zome_types::header::builder;
use holochain_zome_types::LinkEntriesInput;
use holochain_zome_types::LinkEntriesOutput;
use std::sync::Arc;

#[allow(clippy::extra_unused_lifetimes)]
pub fn link_entries<'a>(
    ribosome: Arc<impl RibosomeT>,
    call_context: Arc<CallContext>,
    input: LinkEntriesInput,
) -> RibosomeResult<LinkEntriesOutput> {
    let (base_address, target_address, tag) = input.into_inner();

    // extract the zome position
    let zome_id = ribosome.zome_name_to_id(&call_context.zome_name)?;

    // Construct the link add
    let header_builder = builder::LinkAdd::new(base_address, target_address, zome_id, tag);

    let header_hash =
        tokio_safe_block_on::tokio_safe_block_forever_on(tokio::task::spawn(async move {
            let mut guard = call_context.host_access.workspace().write().await;
            let workspace: &mut CallZomeWorkspace = &mut guard;
            // push the header into the source chain
            let header_hash = workspace.source_chain.put(header_builder, None).await?;
            let element = workspace
                .source_chain
                .get_element(&header_hash)?
                .expect("Element we just put in SourceChain must be gettable");
            integrate_to_cache(
                &element,
                workspace.source_chain.elements(),
                &mut workspace.cache_meta,
            )
            .await
            .map_err(Box::new)?;
            SourceChainResult::Ok(header_hash)
        }))??;

    // return the hash of the committed link
    // note that validation is handled by the workflow
    // if the validation fails this commit will be rolled back by virtue of the lmdb transaction
    // being atomic
    Ok(LinkEntriesOutput::new(header_hash))
}

// we rely on the tests for get_links and get_link_details
