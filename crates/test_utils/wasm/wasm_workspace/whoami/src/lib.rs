use hdk3::prelude::*;

// returns the current agent info
#[hdk_extern]
fn whoami(_: ()) -> ExternResult<AgentInfo> {
    Ok(agent_info!()?)
}

// returns the agent info reported by the given pub key
// in theory the output is the same as the input
// it's just that the output comes _from the opinion of the remote agent_
#[hdk_extern]
fn whoarethey(agent_pubkey: AgentPubKey) -> ExternResult<AgentInfo> {
    let result: SerializedBytes = call_remote!(
        agent_pubkey,
        zome_info!()?.zome_name,
        "whoami".to_string(),
        generate_cap_secret!()?,
        ().try_into()?
    )?;

    Ok(result.try_into()?)
}
