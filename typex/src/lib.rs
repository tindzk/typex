//! `typex` is a lightweight Rust library for inspecting, traversing, comparing
//! and mutating values at runtime without knowing their concrete types. It
//! supports `no_std` environments with `alloc`.
//!
//! ## Available types
//!
//! - Reflection traits: [`Meta`] and [`MetaMut`]
//! - Structural shapes: [`Reflect`] and [`ReflectMut`], with access traits such
//!   as [`SequenceAccess`] for hand-written implementations
//! - Owned objects: [`Object`], [`ObjectMut`] and [`SendObject`]
//! - Object traits: [`ObjectOps`], [`FieldPath`] and [`FieldPathMut`]
//! - Borrowed views: [`ObjectRef`] and [`ObjectRefMut`]
//! - Paths: [`PathSegment`], [`OwnedPath`] and [`TypedPath`]
//! - Mutations: [`PatchOperation`], [`MutationBatch`] and [`MutationRollback`]
//! - Type metadata: [`TypeInfo`], [`ValueKind`] and [`AccessKind`]
//! - Type-keyed maps: [`TypeMap`]
//!
//! ## Derive macros
//!
//! The [`Meta`] and [`MetaMut`] derive macros are provided by
//! [`typex_derive`](https://docs.rs/typex_derive). Enable the `derive` feature
//! to use them as [`Meta`] and [`MetaMut`].
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
