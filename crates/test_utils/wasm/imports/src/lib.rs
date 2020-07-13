#[macro_use]
extern crate lazy_static;

use holochain_wasmer_guest::*;
use holochain_zome_types::globals::ZomeInfo;
use holochain_zome_types::*;

// only the host functions we require in order to pull/push data across the host/guest boundary
memory_externs!();

macro_rules! guest_functions {
    ( $( [ $host_fn:ident, $guest_fn:ident, $input_type:ty, $output_type:ty ] ),* ) => {
        $(
            host_externs!($host_fn);
            #[no_mangle]
            pub extern "C" fn $guest_fn(host_allocation_ptr: GuestPtr) -> GuestPtr {
                let input = {
                    let v: HostInput = host_args!(host_allocation_ptr);
                    let deserialized = <$input_type>::try_from(v.into_inner());
                    try_result!(deserialized, "failed to deserialize host inputs")
                };
                let output: $output_type = try_result!(
                    host_call!(
                        $host_fn,
                        input
                    ),
                    format!("failed to call host function {}", stringify!($host_fn))
                );
                let output_sb: SerializedBytes = try_result!(
                    output.try_into(),
                    "failed to serialize output for extern response"
                );
                ret!(GuestOutput::new(output_sb));
            }
        )*
    }
}

guest_functions!(
    [__zome_info, zome_info, ZomeInfoInput, ZomeInfoOutput],
    [__agent_info, agent_info, AgentInfoInput, AgentInfoOutput],
    [__call, call, CallInput, CallOutput],
    [__capability, capability, CapabilityInput, CapabilityOutput],
    [
        __commit_entry,
        commit_entry,
        CommitEntryInput,
        CommitEntryOutput
    ],
    [__decrypt, decrypt, DecryptInput, DecryptOutput],
    [__encrypt, encrypt, EncryptInput, EncryptOutput],
    [__show_env, show_env, ShowEnvInput, ShowEnvOutput],
    [__property, property, PropertyInput, PropertyOutput],
    [__query, query, QueryInput, QueryOutput],
    [
        __remove_link,
        remove_link,
        RemoveLinkInput,
        RemoveLinkOutput
    ],
    [__random_bytes, random_bytes, RandomBytesInput, RandomBytesOutput],
    [__sign, sign, SignInput, SignOutput],
    [__schedule, schedule, ScheduleInput, ScheduleOutput],
    [
        __update_entry,
        update_entry,
        UpdateEntryInput,
        UpdateEntryOutput
    ],
    [
        __emit_signal,
        emit_signal,
        EmitSignalInput,
        EmitSignalOutput
    ],
    [
        __remove_entry,
        remove_entry,
        RemoveEntryInput,
        RemoveEntryOutput
    ],
    [
        __link_entries,
        link_entries,
        LinkEntriesInput,
        LinkEntriesOutput
    ],
    [__keystore, keystore, KeystoreInput, KeystoreOutput],
    [__get_links, get_links, GetLinksInput, GetLinksOutput],
    [__get_entry, get_entry, GetEntryInput, GetEntryOutput],
    [
        __entry_hash,
        entry_hash,
        EntryHashInput,
        EntryHashOutput
    ],
    [__sys_time, sys_time, SysTimeInput, SysTimeOutput],
    [__debug, debug, DebugInput, DebugOutput],
    [
        __unreachable,
        unreachable,
        UnreachableInput,
        UnreachableOutput
    ]
);

// this is the type of thing you'd expect to see in an HDK to cache the global constants
// TODO: Does this actually work? I thought wasm lost it's memory between calls? freesig
lazy_static! {
    pub(crate) static ref ZOME_INFO: ZomeInfo = {
        let output: ZomeInfoOutput = host_call!(__zome_info, ZomeInfoInput::new(())).unwrap();
        output.into_inner()
    };
}
