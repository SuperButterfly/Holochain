#[macro_export]
/// Generate a "Hashed" wrapper struct around a `TryInto<SerializedBytes>` item.
/// Only includes a `with_pre_hashed` constructor.
///
/// ```
/// # use holochain_serialized_bytes::prelude::*;
/// # #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, SerializedBytes)]
/// # pub struct MyType;
/// holo_hash::make_hashed_base! {
///     Visibility(pub),
///     HashedName(MyTypeHashed),
///     ContentType(MyType),
///     HashType(holo_hash::EntryContentHash),
/// }
/// ```
macro_rules! make_hashed_base {
    (
        Visibility($($vis:tt)*),
        HashedName($n:ident),
        ContentType($t:ty),
        HashType($h:ty),
    ) => {
        /// "Hashed" wrapper type - provides access to the original item,
        /// plus the HoloHash of that item.
        #[derive(::std::fmt::Debug, ::std::clone::Clone)]
        $($vis)* struct $n($crate::GenericHashed<$t, $h>);

        impl $n {
            /// Produce a "Hashed" wrapper with a provided hash.
            pub fn with_pre_hashed(t: $t, h: $h) -> Self {
                Self($crate::GenericHashed::with_pre_hashed(t, h))
            }
        }

        impl $crate::Hashed for $n {
            type Content = $t;
            type HashType = $h;

            fn into_inner(self) -> (Self::Content, Self::HashType) {
                self.0.into_inner()
            }

            fn as_content(&self) -> &Self::Content {
                self.0.as_content()
            }

            fn as_hash(&self) -> &Self::HashType {
                self.0.as_hash()
            }
        }

        impl ::std::convert::From<$n> for ($t, $h) {
            fn from(n: $n) -> ($t, $h) {
                use $crate::Hashed;
                n.into_inner()
            }
        }

        impl ::std::ops::Deref for $n {
            type Target = $t;

            fn deref(&self) -> &Self::Target {
                use $crate::Hashed;
                self.as_content()
            }
        }

        impl ::std::convert::AsRef<$t> for $n {
            fn as_ref(&self) -> &$t {
                use $crate::Hashed;
                self.as_content()
            }
        }

        impl ::std::borrow::Borrow<$t> for $n {
            fn borrow(&self) -> &$t {
                use $crate::Hashed;
                self.as_content()
            }
        }

        impl ::std::convert::AsRef<$h> for $n {
            fn as_ref(&self) -> &$h {
                use $crate::Hashed;
                self.as_hash()
            }
        }

        impl ::std::borrow::Borrow<$h> for $n {
            fn borrow(&self) -> &$h {
                use $crate::Hashed;
                self.as_hash()
            }
        }

        impl ::std::cmp::PartialEq for $n {
            fn eq(&self, other: &Self) -> bool {
                self.0 == other.0
            }
        }

        impl ::std::cmp::Eq for $n {}

        impl ::std::hash::Hash for $n {
            fn hash<H: ::std::hash::Hasher>(&self, state: &mut H) {
                self.0.hash(state);
            }
        }
    };
}

#[macro_export]
/// Generate a "Hashed" wrapper struct around a `TryInto<SerializedBytes>` item.
/// Including a `with_data` hashing constructor.
///
/// The purpose of these hashed wrappers is to make an ergonomic and
/// generalized way to create data and cache the calculated hash of that
/// data along with it in a ways that's safe and let's us not have to
/// recalculate it many times.
///
/// Parameters:
/// - Visibility - specify the visibility for the new struct.
/// - HashedName - specify the name for the new struct.
/// - ContentType - the type that will be wrapped and hashed.
/// - HashType - the hash type to be generated.
///
/// ```
/// # use holochain_serialized_bytes::prelude::*;
/// # use holo_hash::HoloHashExt;
/// # #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, SerializedBytes)]
/// # pub struct MyType;
/// holo_hash::make_hashed! {
///     Visibility(pub),
///     HashedName(MyTypeHashed),
///     ContentType(MyType),
///     HashType(holo_hash::EntryContentHash),
/// }
/// ```
macro_rules! make_hashed {
    (
        Visibility($($vis:tt)*),
        HashedName($n:ident),
        ContentType($t:ty),
        HashType($h:ty),
    ) => {
        $crate::make_hashed_base! {
            Visibility($($vis)*),
            HashedName($n),
            ContentType($t),
            HashType($h),
        }

        impl $crate::Hashable for $n {

            /// Serialize and hash the given item, producing a "Hashed" wrapper.
            fn with_data(content: Self::Content) -> must_future::MustBoxFuture<'static, Result<Self, SerializedBytesError>>
            where Self: Sized {
                use ::std::convert::TryFrom;
                use futures::future::FutureExt;
                async {
                    let sb = ::holochain_serialized_bytes::SerializedBytes::try_from(&content)?;
                    Ok(Self::with_pre_hashed(content, Self::HashType::with_data(::holochain_serialized_bytes::UnsafeBytes::from(sb).into()).await))
                }
                .boxed().into()
            }
        }
    };
}
