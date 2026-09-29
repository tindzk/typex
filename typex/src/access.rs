use crate::{ObjectRef, TypeInfo};
// Keep public API names in scope for short intra-doc links.
#[allow(unused_imports)]
use crate::{Meta, TypedMapAccess};
use core::any::{Any, TypeId};
use core::fmt;

/// Classifies the structural shape exposed by a [`Meta`] implementation.
///
/// Use [`ObjectRef::access_kind`] and the relevant accessors to discover how a
/// value can be traversed.
///
/// # Example
///
/// ```
/// # use typex::{ValueKind, Meta, Object};
/// let value = Object::new(42_u8);
/// assert_eq!(value.kind(), ValueKind::Scalar);
/// ```
///
/// ```
/// # use typex::{ValueKind, Object};
/// # use typex::Meta;
/// #[derive(Meta)]
/// struct Book {
///   title: &'static str,
/// }
///
/// let value = Object::new(Book { title: "Rust" });
/// assert_eq!(value.kind(), ValueKind::Struct);
/// ```
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ValueKind {
  /// A primitive value or opaque user-defined type represented as a leaf
  /// during reflection.
  Scalar,
  /// An optional value. `Some(value)` forwards structural access to its inner
  /// value. `None` represents the empty state and has length zero.
  Option,
  /// A value with named fields exposed through [`ObjectRef::field`] and
  /// [`ObjectRef::field_names`].
  Struct,
  /// A value with keyed entries exposed through [`ObjectRef::key`], [`ObjectRef::keys`]
  /// and [`ObjectRef::len`]. [`TypedMapAccess`] provides access for concrete key
  /// types.
  Map,
  /// An ordered value with indexed items exposed through [`ObjectRef::item`] and
  /// [`ObjectRef::len`].
  Sequence,
}

/// Describes how a reflected value can be accessed.
///
/// # Example
///
/// ```
/// # use typex::{AccessKind, Object};
/// # use typex::Meta;
/// #
/// # #[derive(Meta)]
/// # struct Book {
/// #   title: &'static str,
/// # }
/// let value = Object::new(Book { title: "Rust" });
///
/// assert_eq!(value.access_kind(), Some(AccessKind::Field));
/// ```
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AccessKind {
  /// Access fields by name and enumerate field names via [`ObjectRef::field`] and
  /// [`ObjectRef::field_names`]. Typically used for structs.
  Field,
  /// Access values by key and enumerate keys and length via [`ObjectRef::key`],
  /// [`ObjectRef::keys`] and [`ObjectRef::len`]. Typically used for maps.
  Key,
  /// Access items by index and report their length via [`ObjectRef::item`] and
  /// [`ObjectRef::len`]. Typically used for sequences.
  Item,
  /// Access items by key or by index via [`ObjectRef::key`] and [`ObjectRef::item`],
  /// and enumerate keys and length via [`ObjectRef::keys`] and [`ObjectRef::len`]. Each
  /// key is unique. Typically used for ordered collections with key and index
  /// access.
  ItemKey,
}

/// Borrowed, type-erased reference to a value that exposes runtime type
/// information and downcasts to the concrete type.
///
/// # Example
///
/// ```
/// # use typex::{AnyRef, TypeInfo};
/// let value = 42_u32;
/// let value_ref = AnyRef::new(&value);
///
/// assert!(value_ref.is::<u32>());
/// assert_eq!(value_ref.to_ref::<u32>(), Some(&value));
/// assert_eq!(value_ref.type_info(), TypeInfo::of::<u32>());
/// ```
#[derive(Clone, Copy)]
pub struct AnyRef<'a> {
  any: &'a dyn Any,
  info: TypeInfo,
}

impl<'a> AnyRef<'a> {
  /// Creates a borrowed type-erased reference.
  pub fn new<T: 'static>(value: &'a T) -> Self {
    Self {
      any: value,
      info: TypeInfo::of::<T>(),
    }
  }

  /// Returns runtime type metadata for the referenced value.
  pub fn type_info(&self) -> TypeInfo {
    self.info
  }

  /// Returns the Rust type name of the referenced value.
  pub fn type_name(&self) -> &'static str {
    self.info.type_name()
  }

  /// Returns the unique type ID of the referenced value.
  pub fn id(&self) -> TypeId {
    self.info.id()
  }

  /// Downcasts the referenced value to `T`.
  pub fn to_ref<T: 'static>(&self) -> Option<&'a T> {
    self.any.downcast_ref::<T>()
  }

  /// Checks whether the referenced value has type `T`.
  pub fn is<T: 'static>(&self) -> bool {
    self.info == TypeInfo::of::<T>()
  }
}

impl fmt::Debug for AnyRef<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(self.type_name())
  }
}

/// Callback for enumerating map entries through reflection.
///
/// Return `true` to continue visiting entries or `false` to stop early.
pub type MapEntryVisitor<'visitor> =
  dyn for<'entry> FnMut(AnyRef<'entry>, ObjectRef<'entry>) -> bool + 'visitor;
