use crate::prelude::*;

/// General function that can create any entry type.
///
/// This is used under the hood by [ `create_entry` ], [ `create_cap_grant` ] and [ `create_cap_claim` ].
///
/// The host builds a [ `Create` ] header for the passed entry value and commits a new element to the
/// chain.
///
/// Usually you don't need to use this function directly; it is the most general way to create an
/// entry and standardises the internals of higher level create functions.
pub fn create(entry_with_def_id: EntryWithDefId) -> ExternResult<HeaderHash> {
    HDK.with(|h| h.borrow().create(entry_with_def_id))
}

/// Update any entry type.
///
/// This is used under the hood by [ `update_entry` ], [ `update_cap_grant` ] and `update_cap_claim`.
/// @todo implement update_cap_claim
///
/// The host builds an [ `Update` ] header for the passed entry value and commits a new update to the
/// chain.
///
/// Usually you don't need to use this function directly; it is the most general way to update an
/// entry and standardises the internals of higher level create functions.
pub fn update(hash: HeaderHash, entry_with_def_id: EntryWithDefId) -> ExternResult<HeaderHash> {
    HDK.with(|h| h.borrow().update(UpdateInput::new(hash, entry_with_def_id)))
}

/// General function that can delete any entry type.
///
/// This is used under the hood by [ `delete_entry` ], [ `delete_cap_grant` ] and `delete_cap_claim`.
/// @todo implement delete_cap_claim
///
/// The host builds a [ `Delete` ] header for the passed entry and commits a new element to the chain.
///
/// Usually you don't need to use this function directly; it is the most general way to update an
/// entry and standardises the internals of higher level create functions.
pub fn delete(hash: HeaderHash) -> ExternResult<HeaderHash> {
    HDK.with(|h| h.borrow().delete(hash))
}

/// Create an app entry.
///
/// Apps define app entries by registering entry def ids with the `entry_defs` callback and serialize the
/// entry content when committing to the source chain.
///
/// This function accepts any input that implements [ `TryInto<EntryWithDefId>` ].
/// The default impls from the `#[hdk_entry( .. )]` and [ `entry_def!` ] macros include this.
///
/// With generic type handling it may make sense to directly construct [ `EntryWithDefId` ] and [ `create` ].
///
/// e.g.
/// ```ignore
/// #[hdk_entry(id = "foo")]
/// pub struct Foo(u32);
/// create_entry(Foo(50))?;
/// ```
///
/// See [ `get` ] and [ `get_details` ] for more information on CRUD.
pub fn create_entry<I, E>(input: I) -> ExternResult<HeaderHash>
where
    EntryWithDefId: TryFrom<I, Error = E>,
    WasmError: From<E>,
{
    create(EntryWithDefId::try_from(input)?)
}

/// Alias to delete
///
/// Takes the [ `HeaderHash` ] of the element to delete.
///
/// ```ignore
/// delete_entry(entry_hash(foo_entry)?)?;
/// ```
pub fn delete_entry(hash: HeaderHash) -> ExternResult<HeaderHash> {
    delete(hash)
}

/// Hash anything that that implements [ `TryInto<Entry>` ].
///
/// Hashes are typed in holochain, e.g. [ `HeaderHash` ] and [ `EntryHash` ] are different and yield different
/// bytes for a given value. This ensures correctness and allows type based dispatch in various
/// areas of the codebase.
///
/// Usually you want to hash a value that you want to reference on the DHT with [ `get` ] etc. because
/// it represents some domain-specific data sourced externally or generated within the wasm.
/// [ `HeaderHash` ] hashes are _always_ generated by the process of committing something to a local
/// chain. Every host function that commits an entry returns the new [ `HeaderHash` ]. The [ `HeaderHash` ] can
/// also be used with [ `get` ] etc. to retreive a _specific_ element from the DHT rather than the
/// oldest live element.
/// However there is no way to _generate_ a header hash directly from a header from inside wasm.
/// [ `Element` ] values (entry+header pairs returned by [ `get` ] etc.) contain prehashed header structs
/// called [ `HeaderHashed` ], which is composed of a [ `HeaderHash` ] alongside the "raw" [ `Header` ] value. Generally the pre-hashing is
/// more efficient than hashing headers ad-hoc as hashing always needs to be done at the database
/// layer, so we want to re-use that as much as possible.
/// The header hash can be extracted from the Element as `element.header_hashed().as_hash()`.
/// @todo is there any use-case that can't be satisfied by the `header_hashed` approach?
///
/// Anything that is annotated with #[hdk_entry( .. )] or entry_def!( .. ) implements this so is
/// compatible automatically.
///
/// [ `hash_entry` ] is "dumb" in that it doesn't check that the entry is defined, committed, on the DHT or
/// any other validation, it simply generates the hash for the serialized representation of
/// something in the same way that the DHT would.
///
/// It is strongly recommended that you use the [ `hash_entry` ] function to calculate hashes to avoid
/// inconsistencies between hashes in the wasm guest and the host.
/// For example, a lot of the crypto crates in rust compile to wasm so in theory could generate the
/// hash in the guest, but there is the potential that the serialization logic could be slightly
/// different, etc.
///
/// ```ignore
/// #[hdk_entry(id="foo")]
/// struct Foo;
///
/// let foo_hash = hash_entry(Foo)?;
/// ```
pub fn hash_entry<I, E>(input: I) -> ExternResult<EntryHash>
where
    Entry: TryFrom<I, Error = E>,
    WasmError: From<E>,
{
    HDK.with(|h| h.borrow().hash_entry(Entry::try_from(input)?))
}

/// Thin wrapper around update for app entries.
/// The hash is the [ `HeaderHash` ] of the deleted element, the input is a [ `TryInto<EntryWithDefId>` ].
///
/// Updates can reference Elements which contain Entry data -- namely, Creates and other Updates -- but
/// not Deletes or system Elements
///
/// As updates can reference elements on other agent's source chains across unpredictable network
/// topologies, they are treated as a tree structure.
///
/// Many updates can point to a single create/update and continue to accumulate as long as agents
/// author them against that element. It is up to happ developers to decide how to ensure the tree
/// branches are walked appropriately and that updates point to the correct element, whatever that
/// means for the happ.
///
/// ```ignore
/// #[hdk_entry(id = "foo")]
/// struct Foo(u32);
///
/// let foo_zero_header_hash: HeaderHash = commit_entry!(Foo(0))?;
/// let foo_ten_update_header_hash: HeaderHash = update_entry(foo_zero_header_hash, Foo(10))?;
/// ```
///
/// @todo in the future this will be true because we will have the concept of 'redirects':
/// Works as an app entry delete+create.
///
/// See [ `create_entry` ]
/// See [ `update` ]
/// See [ `delete_entry` ]
pub fn update_entry<I, E>(hash: HeaderHash, input: I) -> ExternResult<HeaderHash>
where
    EntryWithDefId: TryFrom<I, Error = E>,
    WasmError: From<E>,
{
    update(hash, EntryWithDefId::try_from(input)?)
}

/// Gets an element for a given entry or header hash.
///
/// The behaviour of get changes subtly per the _type of the passed hash_.
/// A header hash returns the element for that header, i.e. header+entry or header+None.
/// An entry hash returns the "oldest live" element, i.e. header+entry.
///
/// An element is no longer live once it is referenced by a valid delete element.
/// An update to an element does not change its liveness.
/// See [ `get_details` ] for more information about how CRUD elements reference each other.
///
/// Note: [ `get` ] __always triggers and blocks on a network call__.
///       @todo implement a 'get optimistic' that returns based on the current opinion of the world
///       and performs network calls in the background so they are available 'next time'.
///
/// Note: Deletes are considered in the liveness but Updates are not currently followed
///       automatically due to the need for the happ to disambiguate update logic.
///       @todo implement 'redirect' logic so that updates are followed by [ `get` ].
///
/// Note: Updates typically point to a different entry hash than what they are updating but not
///       always, e.g. consider changing `foo` to `bar` back to `foo`. The entry hashes in a crud
///       tree can be circular but the header hashes are never circular.
///       In this case, deleting the create for foo would make the second update pointing to foo
///       the "oldest live" element.
///
/// Note: "oldest live" only relates to disambiguating many creates and updates from many authors
///       pointing to a single entry, it is not the "current value" of an entry in a CRUD sense.
///       e.g. If "foo" is created then updated to "bar", a [ `get` ] on the hash of "foo" will return
///            "foo" as part of an element with the "oldest live" header.
///            To discover "bar" the agent needs to call `get_details` and decide how it wants to
///            collapse many potential creates, updates and deletes down into a single or filtered
///            set of updates, to "walk the tree".
///       e.g. Updates could include a proof of work and a tree would collapse to a simple
///            blockchain if the agent follows the "heaviest chain".
///       e.g. Updates could represent turns in a 2-player game and the update with the newest
///            timestamp countersigned by both players represents an opt-in chain of updates with
///            support for casual "undo" with player's consent.
///       e.g. Domain/user names could be claimed on a "first come, first serve" basis with only
///            creates and deletes allowed by validation rules, the "oldest live" element _does_
///            represent the element pointing at the first agent to claim a name, but it could also
///            be checked manually by the app with `get_details`.
///
/// Note: "oldest live" is only as good as the information available to the authorities the agent
///       contacts on their current network partition, there could always be an older live entry
///       on another partition, and of course the oldest live entry could be deleted and no longer
///       be live.
pub fn get<H>(hash: H, options: GetOptions) -> ExternResult<Option<Element>>
where
    AnyDhtHash: From<H>,
{
    HDK.with(|h| {
        h.borrow()
            .get(GetInput::new(AnyDhtHash::from(hash), options))
    })
}

pub fn must_get_entry(entry_hash: EntryHash) -> ExternResult<EntryHashed> {
    HDK.with(|h| {
        h.borrow()
            .must_get_entry(MustGetEntryInput::new(entry_hash))
    })
}

pub fn must_get_header(header_hash: HeaderHash) -> ExternResult<SignedHeaderHashed> {
    HDK.with(|h| {
        h.borrow()
            .must_get_header(MustGetHeaderInput::new(header_hash))
    })
}

pub fn must_get_valid_element(header_hash: HeaderHash) -> ExternResult<Element> {
    HDK.with(|h| {
        h.borrow()
            .must_get_valid_element(MustGetValidElementInput::new(header_hash))
    })
}

/// Get an element from the hash AND the details for the entry or header hash passed in.
/// Returns [ `None` ] if the entry/header does not exist.
/// The details returned are a contextual mix of elements and header hashes, see below.
///
/// Note: The return details will be inferred by the hash type passed in, be careful to pass in the
///       correct hash type for the details you want.
///
/// Note: If a header hash is passed in the element returned is the specified element.
///       If an entry hash is passed in all the headers (so implicitly all the elements) are
///       returned for the entry that matches that hash.
///       See [ `get` ] for more information about what "oldest live" means.
///
/// The details returned include relevant creates, updates and deletes for the hash passed in.
///
/// Creates are initial header/entry combinations (elements) produced by commit_entry! and cannot
/// reference other headers.
/// Updates and deletes both reference a specific header+entry combination.
/// Updates must reference another create or update header+entry.
/// Deletes must reference a create or update header+entry (nothing can reference a delete).
///
/// Full elements are returned for direct references to the passed hash.
/// Header hashes are returned for references to references to the passed hash.
///
/// [ `Details` ] for a header hash return:
/// - the element for this header hash if it exists
/// - all update and delete _elements_ that reference that specified header
///
/// [ `Details` ] for an entry hash return:
/// - all creates, updates and delete _elements_ that reference that entry hash
/// - all update and delete _elements_ that reference the elements that reference the entry hash
///
/// Note: Entries are just values, so can be referenced by many CRUD headers by many authors.
///       e.g. the number 1 or string "foo" can be referenced by anyone publishing CRUD headers at
///       any time they need to represent 1 or "foo" for a create, update or delete.
///       If you need to disambiguate entry values, provide uniqueness in the entry value such as
///       a unique hash (e.g. current chain head), timestamp (careful about collisions!), or random
///       bytes/uuid (see random_bytes() and the uuid rust crate that supports uuids from bytes).
///
/// Note: There are multiple header types that exist and operate entirely outside of CRUD elements
///       so they cannot reference or be referenced by CRUD, so are immutable or have their own
///       mutation logic (e.g. link create/delete) and will not be included in [ `get_details` ] results
///       e.g. the DNA itself, links, migrations, etc.
///       However the element will still be returned by [ `get_details` ] if a header hash is passed,
///       these header-only elements will have [ `None` ] as the entry value.
pub fn get_details<H: Into<AnyDhtHash>>(
    hash: H,
    options: GetOptions,
) -> ExternResult<Option<Details>> {
    HDK.with(|h| h.borrow().get_details(GetInput::new(hash.into(), options)))
}

/// Trait for binding static [ `EntryDef` ] property access for a type.
/// See [ `register_entry` ]
pub trait EntryDefRegistration {
    fn entry_def() -> crate::prelude::EntryDef;

    fn entry_def_id() -> crate::prelude::EntryDefId;

    fn entry_visibility() -> crate::prelude::EntryVisibility;

    fn crdt_type() -> crate::prelude::CrdtType;

    fn required_validations() -> crate::prelude::RequiredValidations;
}

/// Implements conversion traits to allow a struct to be handled as an app entry.
/// If you have some need to implement custom serialization logic or metadata injection
/// you can do so by implementing these traits manually instead.
///
/// This requires that TryFrom and TryInto [ `derive@SerializedBytes` ] is implemented for the entry type,
/// which implies that [ `serde::Serialize` ] and [ `serde::Deserialize` ] is also implemented.
/// These can all be derived and there is an attribute macro that both does the default defines.
#[macro_export]
macro_rules! app_entry {
    ( $t:ident ) => {
        impl TryFrom<&$crate::prelude::Entry> for $t {
            type Error = $crate::prelude::WasmError;
            fn try_from(entry: &$crate::prelude::Entry) -> Result<Self, Self::Error> {
                match entry {
                    $crate::prelude::Entry::App(eb) => Ok(Self::try_from(
                        $crate::prelude::SerializedBytes::from(eb.to_owned()),
                    )?),
                    _ => Err($crate::prelude::SerializedBytesError::Deserialize(format!(
                        "{:?} is not an Entry::App so has no serialized bytes",
                        entry
                    ))
                    .into()),
                }
            }
        }

        impl TryFrom<$crate::prelude::Entry> for $t {
            type Error = $crate::prelude::WasmError;
            fn try_from(entry: $crate::prelude::Entry) -> Result<Self, Self::Error> {
                Self::try_from(&entry)
            }
        }

        impl TryFrom<$crate::prelude::EntryHashed> for $t {
            type Error = $crate::prelude::WasmError;
            fn try_from(entry_hashed: $crate::prelude::EntryHashed) -> Result<Self, Self::Error> {
                Self::try_from(entry_hashed.as_content())
            }
        }

        impl TryFrom<&$crate::prelude::Element> for $t {
            type Error = $crate::prelude::WasmError;
            fn try_from(element: &$crate::prelude::Element) -> Result<Self, Self::Error> {
                Ok(match element.entry() {
                    ElementEntry::Present(serialized) => Self::try_from(serialized)?,
                    _ => return Err(Self::Error::Guest("Missing entry".into())),
                })
            }
        }

        impl TryFrom<$crate::prelude::Element> for $t {
            type Error = $crate::prelude::WasmError;
            fn try_from(element: $crate::prelude::Element) -> Result<Self, Self::Error> {
                (&element).try_into()
            }
        }

        impl TryFrom<&$t> for $crate::prelude::Entry {
            type Error = $crate::prelude::WasmError;
            fn try_from(t: &$t) -> Result<Self, Self::Error> {
                match AppEntryBytes::try_from(SerializedBytes::try_from(t)?) {
                    Ok(app_entry_bytes) => Ok(Self::App(app_entry_bytes)),
                    Err(entry_error) => match entry_error {
                        EntryError::SerializedBytes(serialized_bytes_error) => {
                            Err(WasmError::Serialize(serialized_bytes_error))
                        }
                        EntryError::EntryTooLarge(_) => {
                            Err(WasmError::Guest(entry_error.to_string()))
                        }
                    },
                }
            }
        }

        impl TryFrom<$t> for $crate::prelude::Entry {
            type Error = $crate::prelude::WasmError;
            fn try_from(t: $t) -> Result<Self, Self::Error> {
                Self::try_from(&t)
            }
        }
    };
}

/// Implements a whole lot of sane defaults for a struct or enum that should behave as an entry,
/// *without* implementing the app entry conversion interface.
///
/// This allows crates to easily define a struct as an entry separately to binding that struct
/// as an entry type in a dependent crate.
///
/// For most normal applications, you should use the [ `entry_def!` ] macro instead.
#[macro_export]
macro_rules! register_entry {
    ( $t:ident $def:expr ) => {
        impl $crate::prelude::EntryDefRegistration for $t {
            fn entry_def() -> $crate::prelude::EntryDef {
                $def
            }

            fn entry_def_id() -> $crate::prelude::EntryDefId {
                Self::entry_def().id
            }

            fn entry_visibility() -> $crate::prelude::EntryVisibility {
                Self::entry_def().visibility
            }

            fn crdt_type() -> $crate::prelude::CrdtType {
                Self::entry_def().crdt_type
            }

            fn required_validations() -> $crate::prelude::RequiredValidations {
                Self::entry_def().required_validations
            }
        }

        impl From<$t> for $crate::prelude::EntryDef
        where
            $t: $crate::prelude::EntryDefRegistration,
        {
            fn from(_: $t) -> Self {
                $t::entry_def()
            }
        }

        impl From<&$t> for $crate::prelude::EntryDef
        where
            $t: $crate::prelude::EntryDefRegistration,
        {
            fn from(_: &$t) -> Self {
                $t::entry_def()
            }
        }

        impl From<$t> for $crate::prelude::EntryDefId
        where
            $t: $crate::prelude::EntryDefRegistration,
        {
            fn from(_: $t) -> Self {
                $t::entry_def_id()
            }
        }

        impl From<&$t> for $crate::prelude::EntryDefId
        where
            $t: $crate::prelude::EntryDefRegistration,
        {
            fn from(_: &$t) -> Self {
                $t::entry_def_id()
            }
        }

        impl TryFrom<&$t> for $crate::prelude::EntryWithDefId
        where
            $t: $crate::prelude::EntryDefRegistration,
        {
            type Error = $crate::prelude::WasmError;
            fn try_from(t: &$t) -> Result<Self, Self::Error> {
                Ok(Self::new($t::entry_def_id(), t.try_into()?))
            }
        }

        impl TryFrom<$t> for $crate::prelude::EntryWithDefId {
            type Error = $crate::prelude::WasmError;
            fn try_from(t: $t) -> Result<Self, Self::Error> {
                (&t).try_into()
            }
        }

        impl From<$t> for $crate::prelude::EntryVisibility
        where
            $t: $crate::prelude::EntryDefRegistration,
        {
            fn from(_: $t) -> Self {
                $t::entry_visibility()
            }
        }

        impl From<&$t> for $crate::prelude::EntryVisibility
        where
            $t: $crate::prelude::EntryDefRegistration,
        {
            fn from(_: &$t) -> Self {
                $t::entry_visibility()
            }
        }

        impl From<$t> for $crate::prelude::CrdtType
        where
            $t: $crate::prelude::EntryDefRegistration,
        {
            fn from(_: $t) -> Self {
                $t::crdt_type()
            }
        }

        impl From<&$t> for $crate::prelude::CrdtType
        where
            $t: $crate::prelude::EntryDefRegistration,
        {
            fn from(_: &$t) -> Self {
                $t::crdt_type()
            }
        }

        impl From<$t> for $crate::prelude::RequiredValidations
        where
            $t: $crate::prelude::EntryDefRegistration,
        {
            fn from(_: $t) -> Self {
                $t::required_validations()
            }
        }

        impl From<&$t> for $crate::prelude::RequiredValidations
        where
            $t: $crate::prelude::EntryDefRegistration,
        {
            fn from(_: &$t) -> Self {
                $t::required_validations()
            }
        }
    };
}

/// Implements a whole lot of sane defaults for a struct or enum that should behave as an entry.
/// All the entry def fields are available as dedicated methods on the type and matching From impls
/// are provided for each. This allows for both Foo::entry_def() and EntryDef::from(Foo::new())
/// style logic which are both useful in different scenarios.
///
/// For example, the Foo::entry_def() style works best in the entry_defs callback as it doesn't
/// require an instantiated Foo in order to get the definition.
/// On the other hand, EntryDef::from(Foo::new()) works better when e.g. using create_entry() as
/// an instance of Foo already exists and we need the entry def id back for creates and updates.
///
/// If you don't want to use the macro you can simply implement similar fns youself.
///
/// This is not a trait at the moment, it could be in the future but for now these functions and
/// impls are just a loose set of conventions.
///
/// It's actually entirely possible to interact with core directly without any of these.
/// e.g. [ `create_entry` ] is just building a tuple of [ `EntryDefId` ] and [ `Entry::App` ] under the hood.
///
/// This requires that TryFrom and TryInto [ `derive@SerializedBytes` ] is implemented for the entry type,
/// which implies that [ `serde::Serialize` ] and [ `serde::Deserialize` ] is also implemented.
/// These can all be derived and there is an attribute macro that both does the default defines.
///
///  e.g. the following are equivalent
///
/// ```ignore
/// #[hdk_entry(id = "foo", visibility = "private", required_validations = 6, )]
/// pub struct Foo;
/// ```
///
/// ```ignore
/// #[derive(SerializedBytes, serde::Serialize, serde::Deserialize)]
/// pub struct Foo;
/// entry_def!(Foo EntryDef {
///   id: "foo".into(),
///   visibility: EntryVisibility::Private,
///   ..Default::default()
/// });
/// ```
#[macro_export]
macro_rules! entry_def {
    ( $t:ident $def:expr ) => {
        app_entry!($t);
        register_entry!($t $def);
    };
}

/// Shorthand to implement the entry defs callback similar to the vec![ .. ] macro but for entries.
///
/// e.g. the following are the same
///
/// ```ignore
/// entry_defs![ Foo::entry_def() ];
/// ```
///
/// ```ignore
/// #[hdk_extern]
/// fn entry_defs(_: ()) -> ExternResult<EntryDefsCallbackResult> {
///   Ok(vec![ Foo::entry_def() ].into())
/// }
/// ```
#[macro_export]
macro_rules! entry_defs {
    [ $( $def:expr ),* ] => {
        #[hdk_extern]
        pub fn entry_defs(_: ()) -> $crate::prelude::ExternResult<$crate::prelude::EntryDefsCallbackResult> {
            Ok($crate::prelude::EntryDefsCallbackResult::from(vec![ $( $def ),* ]))
        }
    };
}

/// Attempts to lookup the [ `EntryDefIndex` ] given an [ `EntryDefId` ].
///
/// The [ `EntryDefId` ] is a [ `String` ] newtype and the [ `EntryDefIndex` ] is a u8 newtype.
/// The [ `EntryDefIndex` ] is used to reference the entry type in headers on the DHT and as the index of the type exported to tooling.
/// The [ `EntryDefId` ] is the 'human friendly' string that the [ `entry_defs!` ] callback maps to the index.
///
/// The host actually has no idea how to do this mapping, it is provided by the wasm!
///
/// Therefore this is a macro that calls the [ `entry_defs!` ] callback as defined within a zome directly from the zome.
/// It is a macro so that we can call a function with a known name `crate::entry_defs` from the HDK before the function is defined.
///
/// Obviously this assumes and requires that a compliant [ `entry_defs!` ] callback _is_ defined at the root of the crate.
#[macro_export]
macro_rules! entry_def_index {
    ( $t:ty ) => {
        match crate::entry_defs(()) {
            Ok($crate::prelude::EntryDefsCallbackResult::Defs(entry_defs)) => {
                match entry_defs.entry_def_index_from_id(<$t>::entry_def_id()) {
                    Some(entry_def_index) => Ok::<
                        $crate::prelude::EntryDefIndex,
                        $crate::prelude::WasmError,
                    >(entry_def_index),
                    None => {
                        $crate::prelude::tracing::error!(
                            entry_def_type = stringify!($t),
                            ?entry_defs,
                            "Failed to lookup index for entry def id."
                        );
                        Err::<$crate::prelude::EntryDefIndex, $crate::prelude::WasmError>(
                            $crate::prelude::WasmError::Guest(
                                "Failed to lookup index for entry def id.".into(),
                            ),
                        )
                    }
                }
            }
            Err(error) => {
                $crate::prelude::tracing::error!(?error, "Failed to lookup entry defs.");
                Err::<$crate::prelude::EntryDefIndex, $crate::prelude::WasmError>(error)
            }
        }
    };
}

#[macro_export]
macro_rules! entry_type {
    ( $t:ty ) => {
        match $crate::prelude::entry_def_index!($t) {
            Ok(id) => match $crate::prelude::zome_info() {
                Ok(ZomeInfo { zome_id, .. }) => Ok($crate::prelude::EntryType::App(
                    $crate::prelude::AppEntryType::new(id, zome_id, <$t>::entry_visibility()),
                )),
                Err(e) => Err(e),
                _ => unreachable!(),
            },
            Err(e) => Err(e),
        }
    };
}
