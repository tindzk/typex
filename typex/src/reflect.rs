//! Structural shapes returned by [`Meta::reflect`] and [`MetaMut::reflect_mut`].
//!
//! [`Meta`] and [`MetaMut`] expose structure through a single entry point each.
//! The shape-specific operations live on the access traits in this module,
//! which callers do not need to import: `Object`, `ObjectRef`, `ObjectRefMut`
//! and the trait objects dispatch to them. Keeping `len`, `key` and similar
//! names off [`Meta`] and [`MetaMut`] means that importing those traits for
//! their derives never shadows inherent methods such as `str::len` when
//! auto-dereferencing reaches a `Meta` implementation first.
//!
//! Implement an access trait only in the module that hand-writes the
//! corresponding [`Meta`] or [`MetaMut`] implementation. The derive macros
//! refer to them by path.

// Keep public API names in scope so Rustdoc can resolve short intra-doc links.
#[allow(unused_imports)]
use crate::{
  AccessKind, MapEntryVisitor, Meta, MetaMut, MoveItemError, Object, ObjectRef, ObjectRefMut,
  ValueKind,
};
use alloc::string::String;
use alloc::vec::Vec;

/// Read-only structural shape of a [`Meta`] value.
///
/// The variant depends only on the concrete type, not on runtime state, and
/// determines [`ValueKind`].
pub enum Reflect<'a> {
  /// A value without exposed structure.
  Scalar,
  /// A value with named fields and optional positional items.
  Struct(&'a dyn StructAccess),
  /// A value with indexed items.
  Sequence(&'a dyn SequenceAccess),
  /// A value with string-addressable keyed entries.
  Map(&'a dyn MapAccess),
  /// An optional value, holding the contained value when present.
  Option(Option<ObjectRef<'a>>),
}

impl Reflect<'_> {
  /// Returns the structural kind that this shape represents.
  pub fn kind(&self) -> ValueKind {
    match self {
      Self::Scalar => ValueKind::Scalar,
      Self::Struct(_) => ValueKind::Struct,
      Self::Sequence(_) => ValueKind::Sequence,
      Self::Map(_) => ValueKind::Map,
      Self::Option(_) => ValueKind::Option,
    }
  }
}

/// Mutable structural shape of a [`MetaMut`] value.
pub enum ReflectMut<'a> {
  /// A value without mutable structure.
  ///
  /// Scalars report this, as do `BTreeSet` and `BinaryHeap`, whose elements
  /// determine their ordering, and `Rc` or `Arc` values that are not uniquely
  /// owned.
  Opaque,
  /// A value with mutable named fields and optional positional items.
  Struct(&'a mut dyn StructAccessMut),
  /// A value with mutable indexed items.
  Sequence(&'a mut dyn SequenceAccessMut),
  /// A value with mutable keyed entries.
  Map(&'a mut dyn MapAccessMut),
  /// An optional value.
  Option(&'a mut dyn OptionAccessMut),
}

/// Named field access for [`Reflect::Struct`].
// Callers check emptiness through the wrappers' `is_empty`.
#[allow(clippy::len_without_is_empty)]
pub trait StructAccess {
  /// Returns an exposed field by name.
  fn field(&self, name: &str) -> Option<ObjectRef<'_>>;

  /// Returns the exposed field names in declaration order.
  ///
  /// Derived enum implementations place the active variant name before its
  /// fields.
  fn field_names(&self) -> &'static [&'static str];

  /// Returns a positional item, such as a tuple field, at `index`.
  fn item(&self, _index: usize) -> Option<ObjectRef<'_>> {
    None
  }

  /// Returns the number of positional items, when the value has any.
  fn len(&self) -> Option<usize> {
    None
  }
}

/// Indexed item access for [`Reflect::Sequence`].
// Callers check emptiness through the wrappers' `is_empty`.
#[allow(clippy::len_without_is_empty)]
pub trait SequenceAccess {
  /// Returns the number of items.
  fn len(&self) -> usize;

  /// Returns the item at `index`.
  fn item(&self, index: usize) -> Option<ObjectRef<'_>>;
}

/// Keyed entry access for [`Reflect::Map`].
///
/// A map whose key type is not string-like must override [`Meta::eq_dyn`],
/// because the default comparison looks up entries by string key.
// Callers check emptiness through the wrappers' `is_empty`.
#[allow(clippy::len_without_is_empty)]
pub trait MapAccess {
  /// Returns the number of entries.
  fn len(&self) -> usize;

  /// Returns the value stored under a string-like `key`.
  fn key(&self, key: &str) -> Option<ObjectRef<'_>>;

  /// Returns the keys as strings, or `None` when a key is not string-like.
  fn keys(&self) -> Option<Vec<String>>;

  /// Passes each entry to `visitor` until it returns `false`.
  fn visit_entries(&self, visitor: &mut MapEntryVisitor<'_>);

  /// Returns the entry value at `index` for maps that also have positional
  /// order.
  fn item(&self, _index: usize) -> Option<ObjectRef<'_>> {
    None
  }

  /// Returns how callers address entries.
  ///
  /// Maps that also expose [`MapAccess::item`] report [`AccessKind::ItemKey`].
  fn access_kind(&self) -> AccessKind {
    AccessKind::Key
  }
}

/// Mutable named field access for [`ReflectMut::Struct`].
pub trait StructAccessMut {
  /// Returns a mutable field by name.
  fn field_mut(&mut self, name: &str) -> Option<ObjectRefMut<'_>>;

  /// Returns a mutable positional item at `index`.
  fn item_mut(&mut self, _index: usize) -> Option<ObjectRefMut<'_>> {
    None
  }
}

/// Mutable indexed item access for [`ReflectMut::Sequence`].
///
/// Insertion and appending accept an already-built [`Object`] and return it in
/// `Err` when the operation is unsupported, the index is out of bounds or the
/// concrete type differs from the item type.
pub trait SequenceAccessMut: SequenceAccess {
  /// Returns a mutable item at `index`.
  fn item_mut(&mut self, index: usize) -> Option<ObjectRefMut<'_>>;

  /// Inserts `value` at `index` and returns a mutable view of it.
  fn insert_item(&mut self, _index: usize, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    Err(value)
  }

  /// Appends `value` and returns a mutable view of it.
  fn push_item(&mut self, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    Err(value)
  }

  /// Removes the item at `index` and returns it.
  fn remove_item(&mut self, _index: usize) -> Option<Object> {
    None
  }

  /// Moves the item at `from` to the position before the item currently at
  /// `to`. `to` may equal the length to move the item to the end.
  fn move_item(&mut self, _from: usize, _to: usize) -> Result<(), MoveItemError> {
    Err(MoveItemError::Unsupported)
  }
}

/// Mutable keyed entry access for [`ReflectMut::Map`].
pub trait MapAccessMut {
  /// Returns a mutable value for a string-like `key`.
  fn key_mut(&mut self, key: &str) -> Option<ObjectRefMut<'_>>;

  /// Inserts `value` under `key`, replacing any existing value, and returns a
  /// mutable view of it.
  ///
  /// Returns `value` in `Err` if the key type cannot be built from a borrowed
  /// `&str` or the value type differs. Maps with `&'static str` keys reject
  /// insertion because a borrowed `key` cannot become `'static`.
  fn insert_key(&mut self, _key: &str, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    Err(value)
  }

  /// Removes the value stored under `key` and returns it.
  fn remove_key(&mut self, _key: &str) -> Option<Object> {
    None
  }
}

/// Mutable access for [`ReflectMut::Option`].
pub trait OptionAccessMut {
  /// Returns the contained value when present.
  fn value_mut(&mut self) -> Option<ObjectRefMut<'_>>;

  /// Stores `value` when no value is present and returns a mutable view of it.
  ///
  /// Returns `value` in `Err` when a value is already present or the concrete
  /// type differs.
  fn insert_value(&mut self, value: Object) -> Result<ObjectRefMut<'_>, Object>;

  /// Takes the contained value, leaving no value behind.
  fn take_value(&mut self) -> Option<Object>;
}
