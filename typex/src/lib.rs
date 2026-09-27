//! `typex` is a lightweight Rust library for inspecting, traversing, comparing
//! and mutating values at runtime without knowing their concrete types. It
//! supports `no_std` environments with `alloc`.
//!
//! ## Available types
//!
//! - Reflection traits: [`Meta`] and [`MetaMut`]
//! - Owned objects: [`Object`], [`ObjectMut`] and [`SendObject`]
//! - Object trait: [`ObjectOps`]
//! - Borrowed views: [`ObjectRef`] and [`ObjectRefMut`]
//! - Typed map access: [`TypedMapAccess`] and [`TypedMapAccessMut`]
//! - Paths: [`PathSegment`], [`OwnedPath`] and [`TypedPath`]
//! - Mutations: [`PatchOperation`], [`MutationBatch`] and [`MutationRollback`]
//! - Type metadata: [`TypeInfo`], [`ValueKind`] and [`AccessKind`]
//! - Structural shapes: [`Reflect`] and [`ReflectMut`]
//! - Shape access: [`StructAccess`], [`TupleAccess`], [`EnumAccess`],
//!   [`SequenceAccess`], [`KeyedSequenceAccess`] and [`MapAccess`], with
//!   [`VariantFields`] for the fields of an enum variant
//! - Mutable shape access: [`StructAccessMut`], [`TupleAccessMut`],
//!   [`EnumAccessMut`], [`SequenceAccessMut`], [`KeyedSequenceAccessMut`],
//!   [`MapAccessMut`] and [`OptionAccessMut`], with [`VariantFieldsMut`]
//! - Type-keyed maps: [`TypeMap`]
//!
//! ## Derive macros
//!
//! The [`Meta`] and [`MetaMut`] derive macros are provided by
//! [`typex_derive`](https://docs.rs/typex_derive). Enable the `derive` feature
//! to use them as [`Meta`] and [`MetaMut`].
//!
//! ## Structural equality
//!
//! [`Meta::eq_dyn`] compares values with matching runtime type metadata. Its
//! default implementation dispatches on the shape from [`Meta::reflect`]:
//!
//! - [`Reflect::Struct`] compares field names, then fields by name.
//! - [`Reflect::Tuple`] compares lengths, then fields by index.
//! - [`Reflect::Enum`] compares active variant names, then their fields by
//!   name or index.
//! - [`Reflect::Map`] compares lengths, then looks up each visited key by
//!   string. It supports `String` and `&'static str` keys.
//! - [`Reflect::Sequence`] and [`Reflect::KeyedSequence`] compare lengths,
//!   then items by index.
//! - [`Reflect::Option`] compares presence and inner values.
//! - [`Reflect::Scalar`] returns `false`; scalar implementations provide
//!   their own value comparisons.
//!
//! Built-in scalars, tuples, `Result` and the derives compare values directly,
//! avoiding dispatch through the access traits. `BTreeMap` and `HashMap`
//! compare native keys and reflective values. Custom opaque values provide
//! equality through `#[typex(partial_eq)]` or a hand-written implementation.
//! Float equality follows `PartialEq`, so `NaN != NaN`.
//!
//! ## Feature flags
//!
//! - `std` (default): Enables `HashMap` support and implementations of
//!   `std::error::Error` for the crate's error types
//! - `derive`: Re-exports the [`Meta`] and [`MetaMut`] derive macros
//!
//! ## Minimum supported Rust version (MSRV)
//!
//! Rust v1.85+ is required.
//!
//! ## Example
//!
//! ```rust
//! use typex::{Meta, Object, ObjectOps};
//!
//! #[derive(Clone, Debug, Meta)]
//! struct Scope {
//!   name: &'static str,
//! }
//!
//! #[derive(Debug, Meta)]
//! struct User {
//!   id: u16,
//!   scopes: Vec<Scope>,
//! }
//!
//! let object = Object::new(User {
//!   id: 42,
//!   scopes: vec![Scope { name: "read" }],
//! });
//!
//! assert!(object.is::<User>());
//! assert_eq!(object.type_name(), core::any::type_name::<User>());
//! assert_eq!(object.field_names(), &["id", "scopes"]);
//!
//! assert_eq!(object.field("id").unwrap().to_ref::<u16>(), Some(&42));
//!
//! let scope_name = |object: &Object| {
//!   object
//!     .field("scopes")?
//!     .item(0)?
//!     .field("name")?
//!     .to_ref::<&'static str>()
//!     .copied()
//! };
//! assert_eq!(scope_name(&object), Some("read"));
//!
//! // Use typed paths to traverse fields without an explicit downcast
//! assert_eq!(object.field_path(User::FIELD_ID.path()), Some(&42));
//! assert_eq!(
//!   object.field_path(User::FIELD_SCOPES.item(0).then(Scope::FIELD_NAME)),
//!   Some(&"read")
//! );
//! ```
#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;
extern crate self as typex;

#[doc(hidden)]
pub mod __private {
  pub use alloc::borrow::ToOwned;
  pub use alloc::boxed::Box;
  pub use alloc::string::{String, ToString};
  pub use alloc::vec;
  pub use alloc::vec::Vec;
  pub use core::any::Any;
  pub use core::clone::Clone;
}

#[macro_use]
mod traits;
mod access;
mod collections;
mod mutation_batch;
mod objects;
mod patch;
mod path;
mod reflect;
mod type_info;
mod type_map;

pub use access::*;
pub use mutation_batch::*;
pub use objects::*;
pub use patch::*;
pub use path::*;
pub use reflect::*;
pub use traits::*;
pub use type_info::TypeInfo;
pub use type_map::*;

#[cfg(feature = "derive")]
pub use typex_derive::{Meta, MetaMut};

#[cfg(test)]
mod tests;
