use crate::core::ribosome::error::{RibosomeError, RibosomeResult};
use crate::core::{
    ribosome::{HostContext, RibosomeT},
    state::metadata::{LinkMetaKey, LinkMetaVal},
    workflow::InvokeZomeWorkspace,
};
use futures::future::FutureExt;
use holochain_state::error::DatabaseResult;
use holochain_zome_types::links::LinkTag;
use holochain_zome_types::GetLinksInput;
use holochain_zome_types::GetLinksOutput;
use must_future::MustBoxFuture;
use std::convert::TryInto;
use std::sync::Arc;
use holo_hash_core::HoloHashCore;

pub fn get_links<'a>(
    ribosome: Arc<impl RibosomeT>,
    host_context: Arc<HostContext>,
    input: GetLinksInput,
) -> RibosomeResult<GetLinksOutput> {
    let (base_address, tag) = input.into_inner();

    let base_address = base_address.try_into()?;

    // Get zome id
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

    let call =
        |workspace: &'a InvokeZomeWorkspace| -> MustBoxFuture<'a, DatabaseResult<Vec<LinkMetaVal>>> {
            async move {
                let cascade = workspace.cascade();
                let key = LinkMetaKey::BaseZomeTag(&base_address, zome_id, &tag);
                // safe block on
                cascade
                    .dht_get_links(&key)
                    .await
            }
            .boxed()
            .into()
        };
    let links = tokio_safe_block_on::tokio_safe_block_forever_on(async move {
        unsafe { host_context.workspace.apply_ref(call).await }
    })??;

    let links: Vec<HoloHashCore> = links.into_iter().map(|l| l.target.into()).collect();

    Ok(GetLinksOutput::new(links))
}
