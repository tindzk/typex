//! Structural shapes returned by [`Meta::reflect`] and [`MetaMut::reflect_mut`].
//!
//! Operations such as `len` and `key` live on the access traits in this module
//! rather than on [`Meta`] and [`MetaMut`], so importing [`Meta`] or
//! [`MetaMut`] never shadows inherent methods such as `str::len`. Callers use
//! the inherent methods of [`Object`], [`ObjectRef`] and [`ObjectRefMut`],
//! which dispatch to the access traits.
//!
//! A type with a hand-written [`Meta`] or [`MetaMut`] implementation must also
//! implement the access trait for each shape it returns, such as
//! [`SequenceAccess`] for [`Reflect::Sequence`].

// Keep public API names in scope so Rustdoc can resolve short intra-doc links.
#[allow(unused_imports)]
use crate::{
  AccessKind, MapEntryVisitor, Meta, MetaMut, MoveItemError, Object, ObjectRef, ObjectRefMut,
  PathSegment, ValueKind,
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
  /// A value with named fields, or a unit struct without fields.
  Struct(&'a dyn StructAccess),
  /// An enum value exposing its active variant and that variant's fields.
  Enum(&'a dyn EnumAccess),
  /// A tuple or tuple struct with positional fields.
  Tuple(&'a dyn TupleAccess),
  /// A value with indexed items.
  Sequence(&'a dyn SequenceAccess),
  /// A value with indexed items and unique string keys.
  KeyedSequence(&'a dyn KeyedSequenceAccess),
  /// A value with string-addressable keyed entries.
  Map(&'a dyn MapAccess),
  /// An optional value, holding the contained value when present.
  Option(Option<ObjectRef<'a>>),
}

impl Reflect<'_> {
  /// Returns the structural shape of the value.
  #[inline]
  pub fn kind(&self) -> ValueKind {
    match self {
      Self::Scalar => ValueKind::Scalar,
      Self::Struct(_) => ValueKind::Struct,
      Self::Tuple(_) => ValueKind::Tuple,
      Self::Enum(_) => ValueKind::Enum,
      Self::Sequence(_) | Self::KeyedSequence(_) => ValueKind::Sequence,
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
  /// A value with mutable named fields, or a unit struct without fields.
  Struct(&'a mut dyn StructAccessMut),
  /// A tuple or tuple struct with mutable positional fields.
  Tuple(&'a mut dyn TupleAccessMut),
  /// An enum value with mutable fields in its active variant.
  Enum(&'a mut dyn EnumAccessMut),
  /// A value with mutable indexed items.
  Sequence(&'a mut dyn SequenceAccessMut),
  /// A value with mutable indexed items and unique string keys.
  KeyedSequence(&'a mut dyn KeyedSequenceAccessMut),
  /// A value with mutable keyed entries.
  Map(&'a mut dyn MapAccessMut),
  /// An optional value whose contained value is inserted and taken through
  /// [`OptionAccessMut`].
  Option(&'a mut dyn OptionAccessMut),
}

/// Field access for [`Reflect::Struct`] and for the active variant of a
/// [`Reflect::Enum`] with named fields.
pub trait StructAccess {
  /// Returns the field named `name`.
  fn field(&self, name: &str) -> Option<ObjectRef<'_>>;

  /// Returns the field names in declaration order.
  fn field_names(&self) -> &'static [&'static str];
}

/// Positional field access for [`Reflect::Tuple`] and for tuple enum variants.
// Callers check emptiness through the wrappers' `is_empty`.
#[allow(clippy::len_without_is_empty)]
pub trait TupleAccess {
  /// Returns the field at `index`.
  fn item(&self, index: usize) -> Option<ObjectRef<'_>>;

  /// Returns the number of fields.
  fn len(&self) -> usize;
}

/// Fields of the active variant of an enum.
pub enum VariantFields<'a> {
  /// A unit variant without fields.
  Unit,
  /// A variant with named fields.
  Named(&'a dyn StructAccess),
  /// A tuple variant with positional fields.
  Positional(&'a dyn TupleAccess),
}

/// Variant access for [`Reflect::Enum`].
///
/// An enum implements [`StructAccess`] for variants with named fields and
/// [`TupleAccess`] for tuple variants. Callers reach both only through
/// [`Self::fields`], so neither needs to handle variants of the other kind.
pub trait EnumAccess {
  /// Returns the name of the active variant.
  fn variant_name(&self) -> &'static str;

  /// Returns the fields of the active variant.
  fn fields(&self) -> VariantFields<'_>;
}

/// Indexed item access for [`Reflect::Sequence`], such as a `Vec` or
/// `VecDeque`.
// Callers check emptiness through the wrappers' `is_empty`.
#[allow(clippy::len_without_is_empty)]
pub trait SequenceAccess {
  /// Returns the number of items.
  fn len(&self) -> usize;

  /// Returns the item at `index`.
  fn item(&self, index: usize) -> Option<ObjectRef<'_>>;
}

/// Indexed and keyed item access for [`Reflect::KeyedSequence`], such as a
/// list of records with unique IDs.
///
/// Keys derive from items and must stay unique, including after an item is
/// mutated in place. Key lookup is unspecified when duplicate keys exist.
///
/// Keyed sequences report [`AccessKind::KeyedItem`] and [`ValueKind::Sequence`]
/// as their access kind and structural shape. Mutable keyed sequences support
/// positional insertion, removal and moves.
///
/// Default structural equality compares lengths, then items at matching indices
/// through [`SequenceAccess::item`].
pub trait KeyedSequenceAccess: SequenceAccess {
  /// Returns the item stored under `key`, or `None` when the key is absent.
  fn key(&self, key: &str) -> Option<ObjectRef<'_>>;

  /// Returns the item keys in index order.
  fn keys(&self) -> Vec<String>;
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
}

/// Mutable named field access for [`ReflectMut::Struct`] and for enum variants
/// with named fields.
pub trait StructAccessMut {
  /// Returns the mutable field named `name`.
  fn field_mut(&mut self, name: &str) -> Option<ObjectRefMut<'_>>;
}

/// Mutable positional field access for [`ReflectMut::Tuple`] and for tuple enum
/// variants.
pub trait TupleAccessMut {
  /// Returns the mutable field at `index`.
  fn item_mut(&mut self, index: usize) -> Option<ObjectRefMut<'_>>;
}

/// Mutable fields of the active variant of an enum.
pub enum VariantFieldsMut<'a> {
  /// A unit variant without fields.
  Unit,
  /// A variant with named fields.
  Named(&'a mut dyn StructAccessMut),
  /// A tuple variant with positional fields.
  Positional(&'a mut dyn TupleAccessMut),
}

/// Mutable variant access for [`ReflectMut::Enum`].
pub trait EnumAccessMut: EnumAccess {
  /// Returns the mutable fields of the active variant.
  fn fields_mut(&mut self) -> VariantFieldsMut<'_>;
}

/// Mutable indexed item access for [`ReflectMut::Sequence`].
///
/// Insertion and appending accept an already-built [`Object`] and return it in
/// `Err` when the operation is unsupported, the index is out of bounds or the
/// concrete type differs from the item type.
pub trait SequenceAccessMut: SequenceAccess {
  /// Returns a mutable item at `index`, or `None` when the index is out of
  /// bounds or mutable item access is unsupported. The default returns `None`.
  fn item_mut(&mut self, _index: usize) -> Option<ObjectRefMut<'_>> {
    None
  }

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

/// Mutable indexed and keyed item access for [`ReflectMut::KeyedSequence`].
///
/// [`SequenceAccessMut::insert_item`] and [`SequenceAccessMut::push_item`]
/// must return the supplied value in `Err` when its key is already present.
pub trait KeyedSequenceAccessMut: SequenceAccessMut + KeyedSequenceAccess {
  /// Returns a mutable item stored under `key`, or `None` when the key is absent.
  fn key_mut(&mut self, key: &str) -> Option<ObjectRefMut<'_>>;
}

// Direct dispatch avoids the upcast cost in mutation batches.
macro_rules! with_sequence_mut {
  ($shape:expr, |$sequence:ident| $body:block, $fallback:block) => {
    match $shape {
      $crate::ReflectMut::Sequence($sequence) => $body,
      $crate::ReflectMut::KeyedSequence($sequence) => $body,
      _ => $fallback,
    }
  };
}

pub(crate) use with_sequence_mut;

// Sharing patch dispatch reduces instruction counts on newer compilers.
#[cfg(typex_trait_upcasting)]
macro_rules! with_patch_sequence_mut {
  ($shape:expr, |$sequence:ident| $body:block, $fallback:block) => {{
    let $sequence: &mut dyn $crate::SequenceAccessMut = match $shape {
      $crate::ReflectMut::Sequence(sequence) => sequence,
      $crate::ReflectMut::KeyedSequence(sequence) => sequence,
      _ => $fallback,
    };
    $body
  }};
}

#[cfg(typex_trait_upcasting)]
pub(crate) use with_patch_sequence_mut;
#[cfg(not(typex_trait_upcasting))]
pub(crate) use with_sequence_mut as with_patch_sequence_mut;

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
