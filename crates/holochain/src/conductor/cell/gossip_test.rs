use crate::{
    conductor::p2p_store::{AgentKv, AgentKvKey},
    test_utils::{conductor_setup::ConductorTestData, new_invocation},
};
use fallible_iterator::FallibleIterator;
use hdk3::prelude::*;
use holochain_state::buffer::KvStoreT;
use holochain_state::fresh_reader_test;
use holochain_wasm_test_utils::TestWasm;
use matches::assert_matches;
use test_wasm_common::{AnchorInput, TestString};

#[tokio::test(threaded_scheduler)]
async fn gossip_test() {
    observability::test_run().ok();
    const NUM: usize = 1;
    let zomes = vec![TestWasm::Anchor];
    let mut conductor_test = ConductorTestData::new(zomes, false).await;
    let handle = conductor_test.handle.clone();
    let alice_call_data = &conductor_test.alice_call_data;
    let alice_cell_id = &alice_call_data.cell_id;

    // ALICE adding anchors

    let anchor_invocation = |anchor: &str, cell_id, i: usize| {
        let anchor = AnchorInput(anchor.into(), i.to_string());
        new_invocation(cell_id, "anchor", anchor, TestWasm::Anchor)
    };

    for i in 0..NUM {
        let invocation = anchor_invocation("alice", alice_cell_id, i).unwrap();
        let response = handle.call_zome(invocation).await.unwrap().unwrap();
        assert_matches!(response, ZomeCallResponse::Ok(_));
    }

    // Give publish time to finish
    tokio::time::delay_for(std::time::Duration::from_secs(1)).await;

    // Bring Bob online
    conductor_test.bring_bob_online().await;
    let bob_call_data = conductor_test.bob_call_data.as_ref().unwrap();
    let bob_cell_id = &bob_call_data.cell_id;

    // Give gossip some time to finish
    tokio::time::delay_for(std::time::Duration::from_secs(1)).await;

    // Bob list anchors
    let invocation = new_invocation(
        bob_cell_id,
        "list_anchor_addresses",
        TestString("alice".into()),
        TestWasm::Anchor,
    )
    .unwrap();
    let response = handle.call_zome(invocation).await.unwrap().unwrap();
    match response {
        ZomeCallResponse::Ok(r) => {
            let response: SerializedBytes = r.into_inner();
            let hashes: EntryHashes = response.try_into().unwrap();
            assert_eq!(hashes.0.len(), NUM);
        }
        _ => unreachable!(),
    }

    ConductorTestData::shutdown_conductor(handle).await;
}

#[tokio::test(threaded_scheduler)]
async fn agent_info_test() {
    observability::test_run().ok();
    let zomes = vec![TestWasm::Anchor];
    let mut conductor_test = ConductorTestData::new(zomes, false).await;
    let handle = conductor_test.handle.clone();
    let alice_call_data = &conductor_test.alice_call_data;
    let alice_cell_id = &alice_call_data.cell_id;
    let alice_agent_id = alice_cell_id.agent_pubkey();

    // Kitsune types
    let dna_kit: kitsune_p2p::KitsuneSpace = alice_call_data
        .ribosome
        .dna_file
        .dna_hash()
        .clone()
        .into_inner()
        .into();
    let alice_kit: kitsune_p2p::KitsuneAgent = alice_agent_id.clone().into_inner().into();

    let p2p_env = handle.get_p2p_env().await;
    let p2p_kv = AgentKv::new(p2p_env.clone().into()).unwrap();

    let key: AgentKvKey = (&dna_kit, &alice_kit).into();

    let agent_info = fresh_reader_test!(p2p_env, |r| {
        p2p_kv
            .as_store_ref()
            .iter(&r)
            .unwrap()
            .map(|(k, v)| Ok((k.to_vec(), v)))
            .collect::<Vec<_>>()
            .unwrap()
    });
    dbg!(agent_info);

    // Give publish time to finish
    tokio::time::delay_for(std::time::Duration::from_secs(1)).await;

    // Bring Bob online
    conductor_test.bring_bob_online().await;
    let bob_call_data = conductor_test.bob_call_data.as_ref().unwrap();
    let bob_cell_id = &bob_call_data.cell_id;

    let p2p_kv = AgentKv::new(p2p_env.clone().into()).unwrap();

    let agent_info = fresh_reader_test!(p2p_env, |r| {
        p2p_kv
            .as_store_ref()
            .iter(&r)
            .unwrap()
            .map(|(k, v)| Ok((k.to_vec(), v)))
            .collect::<Vec<_>>()
            .unwrap()
    });
    dbg!(agent_info);

    // Give gossip some time to finish
    tokio::time::delay_for(std::time::Duration::from_secs(1)).await;

    ConductorTestData::shutdown_conductor(handle).await;
}
