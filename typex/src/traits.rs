// Keep public API names in scope so Rustdoc can resolve short intra-doc links.
#[allow(unused_imports)]
use crate::{
  AccessKind, AnyRef, ApplyError, FieldPathQuery, FieldPathQueryMut, MapEntryVisitor,
  MutationBatch, Object, ObjectMut, ObjectOps, ObjectRef, ObjectRefMut, PatchOperation,
  ReflectiveError, TypeInfo, TypedPath, ValueKind, apply_patch,
};
use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::any::{Any, TypeId};
#[cfg(feature = "std")]
use core::hash::Hash;
#[cfg(feature = "std")]
use std::collections::HashMap;

/// Traverses a nested field, item or key path from a [`Meta`] value.
///
/// The blanket implementation makes this helper available to every [`Meta`]
/// value. Raw paths return [`ObjectRef`] values, while [`TypedPath`] queries
/// return the terminal Rust reference directly.
pub trait FieldPath: Meta {
  /// Traverses a nested field, item or key path from the current value; see
  /// [`FieldPathQuery`].
  fn field_path<'a, Q: FieldPathQuery<'a>>(&'a self, query: Q) -> Option<Q::Output>
  where
    Self: Sized,
  {
    ObjectRef::new(self).field_path(query)
  }
}

impl<T> FieldPath for T where T: Meta + ?Sized {}

/// Mutable counterpart of [`FieldPath`] for [`MetaMut`] values.
///
/// See [`FieldPathQueryMut`] for the accepted paths, their return types and
/// their errors.
pub trait FieldPathMut: MetaMut {
  /// Traverses a nested field, item or key path from the current value; see
  /// [`FieldPathQueryMut`].
  fn field_path_mut<'r, Q: FieldPathQueryMut<'r>>(
    &'r mut self,
    query: Q,
  ) -> Result<Q::Output, ReflectiveError>
  where
    Self: Sized,
  {
    query.resolve(ObjectRefMut::new(self))
  }
}

impl<T> FieldPathMut for T where T: MetaMut + ?Sized {}

/// Failure returned when a sequential value cannot move an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveItemError {
  /// The value does not support moving items.
  Unsupported,
  /// Either the source or destination index is outside the sequence.
  IndexOutOfBounds,
  /// The item could not be removed from the sequence.
  RemoveFailed,
  /// The item could not be inserted at its destination and is back at its
  /// source index.
  InsertFailed,
  /// The item could not be inserted at its destination or restored at its
  /// source index. The sequence no longer contains it.
  RestoreFailed,
}

pub(crate) fn as_map_key<K: 'static>(candidate: &K) -> Option<String> {
  let any = candidate as &dyn Any;

  if let Some(value) = any.downcast_ref::<String>() {
    Some(value.clone())
  } else {
    any
      .downcast_ref::<&'static str>()
      .map(|value| (*value).to_owned())
  }
}

/// Checks whether `field` resolves to `parent` itself, as a derived enum's
/// variant name does.
fn is_self_reference(field: ObjectRef<'_>, parent: &dyn Any) -> bool {
  let field = field.as_meta().as_any();
  Any::type_id(field) == Any::type_id(parent) && core::ptr::addr_eq(field, parent)
}

fn string_map_key<'a>(key: AnyRef<'a>) -> Option<&'a str> {
  key
    .to_ref::<String>()
    .map(String::as_str)
    .or_else(|| key.to_ref::<&'static str>().copied())
}

// Lookups by `&str` support only `String` and `&'static str` keys. Other key
// types return `None`.
macro_rules! map_key_lookup {
  ($get:ident, $get_mut:ident, $map:ident $(, $extra:ident : $extra_bound:path)* ; $($key_bound:path),+) => {
    pub(crate) fn $get<'a, K, V, $($extra),*>(map: &'a $map<K, V, $($extra),*>, key: &str) -> Option<&'a V>
    where
      K: $($key_bound +)+ 'static,
      V: 'static,
      $($extra: $extra_bound + 'static,)*
    {
      if let Some(map) = (map as &dyn Any).downcast_ref::<$map<String, V, $($extra),*>>() {
        return map.get(key);
      }
      if let Some(map) = (map as &dyn Any).downcast_ref::<$map<&'static str, V, $($extra),*>>() {
        return map.get(key);
      }
      None
    }

    pub(crate) fn $get_mut<'a, K, V, $($extra),*>(map: &'a mut $map<K, V, $($extra),*>, key: &str) -> Option<&'a mut V>
    where
      K: $($key_bound +)+ 'static,
      V: 'static,
      $($extra: $extra_bound + 'static,)*
    {
      if TypeId::of::<K>() == TypeId::of::<String>() {
        let map = (map as &mut dyn Any)
          .downcast_mut::<$map<String, V, $($extra),*>>()
          .unwrap();
        return map.get_mut(key);
      }
      if TypeId::of::<K>() == TypeId::of::<&'static str>() {
        let map = (map as &mut dyn Any)
          .downcast_mut::<$map<&'static str, V, $($extra),*>>()
          .unwrap();
        return map.get_mut(key);
      }
      None
    }
  };
}

map_key_lookup!(btree_map_get, btree_map_get_mut, BTreeMap; Ord);

#[cfg(feature = "std")]
map_key_lookup!(hash_map_get, hash_map_get_mut, HashMap, S: core::hash::BuildHasher; Eq, Hash);

/// Typed key lookup for map-like values.
pub trait TypedMapAccess<K> {
  /// Returns a value for `key` when the value supports map-like access with
  /// this concrete key type.
  fn key_typed(&self, key: &K) -> Option<ObjectRef<'_>>;
}

/// Typed mutable key lookup for map-like values.
///
/// # Example
///
/// ```
/// # use std::collections::BTreeMap;
/// # use typex::{Object, TypedMapAccessMut};
/// let mut counts = BTreeMap::from([(7_u32, 11_u8)]);
///
/// counts
///   .key_typed_mut(&7)
///   .unwrap()
///   .set(Object::new(12_u8))
///   .unwrap();
/// assert_eq!(counts[&7], 12);
/// ```
pub trait TypedMapAccessMut<K> {
  /// Returns a mutable value for `key` when the value supports map-like access
  /// with this concrete key type.
  fn key_typed_mut(&mut self, key: &K) -> Option<ObjectRefMut<'_>>;
}

/// Read-only reflection operations for runtime type information, structural
/// navigation and downcasting to a concrete type.
///
/// Implement or derive this trait to expose a type through read-only
/// reflection. Implementations are provided for common scalar and collection
/// types.
///
/// Use [`Object`] for an owned reflective value or [`ObjectRef`] for a borrowed
/// view.
///
/// For mutable structural access, implement or derive [`MetaMut`], the mutable
/// counterpart to [`Meta`].
///
/// # Example
///
/// ```
/// # use typex::{Object, ObjectOps, TypeInfo};
/// # use typex::Meta;
/// #[derive(Meta)]
/// #[typex(opaque)]
/// struct Number(u16);
///
/// let mut objects = Vec::<Object>::new();
/// objects.push(Object::new(Number(23)));
///
/// assert_eq!(objects[0].type_name(), core::any::type_name::<Number>());
/// assert_eq!(objects[0].type_info(), TypeInfo::of::<Number>());
/// assert_eq!(objects[0].to_ref::<Number>().unwrap().0, 23);
/// ```
pub trait Meta: Any {
  /// Returns runtime type metadata for the value.
  fn type_info(&self) -> TypeInfo {
    TypeInfo::of::<Self>()
  }

  /// Returns the Rust type name from `core::any::type_name`.
  fn type_name(&self) -> &'static str {
    self.type_info().type_name()
  }

  /// Returns the type's [`TypeId`].
  fn id(&self) -> core::any::TypeId {
    self.type_info().id()
  }

  /// Returns this value's structural shape.
  ///
  /// The result depends only on `Self`'s concrete type, never on runtime state.
  /// A structural implementation may expose variant-dependent field names
  /// while retaining the same kind.
  fn kind(&self) -> ValueKind;

  /// Returns the named or indexed access exposed by this value, if any.
  fn access_kind(&self) -> Option<AccessKind> {
    None
  }

  /// Returns the contained value when this is `Some(value)`.
  fn option_value(&self) -> Option<ObjectRef<'_>> {
    None
  }

  /// Returns an exposed field by name.
  fn field(&self, _name: &str) -> Option<ObjectRef<'_>> {
    None
  }

  /// Returns the exposed field names in declaration order.
  ///
  /// Derived enum implementations place the active variant name before its fields.
  fn field_names(&self) -> &'static [&'static str] {
    &[]
  }

  /// Returns an item at `index` for sequential access.
  fn item(&self, _index: usize) -> Option<ObjectRef<'_>> {
    None
  }

  /// Returns the number of exposed structural items, when available.
  ///
  /// For maps, this is the number of keyed entries. An implementation whose
  /// kind is [`ValueKind::Map`] must return `Some` with that count.
  fn len(&self) -> Option<usize> {
    None
  }

  /// Checks whether a value has no exposed structural items, when available.
  fn is_empty(&self) -> Option<bool> {
    self.len().map(|len| len == 0)
  }

  /// Returns a value for `key` during map-like access.
  fn key(&self, _key: &str) -> Option<ObjectRef<'_>> {
    None
  }

  /// Returns the available keys for map-like access.
  fn keys(&self) -> Option<Vec<String>> {
    None
  }

  /// Passes each map entry to `visitor` until it returns `false`.
  ///
  /// Returns `false` when the value has no map-like access. Stopping early
  /// still returns `true`.
  ///
  /// A map implementation must return `true` and visit every keyed entry
  /// unless the visitor stops iteration. The default [`Meta::eq_dyn`] supports
  /// only `String` and `&'static str` keys, so maps with other key types must
  /// override [`Meta::eq_dyn`].
  fn visit_map_entries(&self, _visitor: &mut MapEntryVisitor<'_>) -> bool {
    false
  }

  /// Compares two [`Meta`] values structurally.
  ///
  /// Values with different [`Meta::type_info`] are never equal. Otherwise the
  /// comparison dispatches on [`Meta::kind`]:
  ///
  /// - [`ValueKind::Struct`] compares fields by name. A field that
  ///   resolves to the value itself, such as a derived enum's variant name,
  ///   is treated as equal.
  /// - [`ValueKind::Map`] compares lengths, then the default implementation
  ///   looks up each key from [`Meta::visit_map_entries`] with [`Meta::key`]. It
  ///   supports `String` and `&'static str` keys.
  /// - [`ValueKind::Sequence`] compares lengths, then items by index.
  /// - [`ValueKind::Option`] compares presence and inner values.
  /// - The default implementation returns `false` for [`ValueKind::Scalar`];
  ///   scalar implementations compare their values by overriding this method.
  ///
  /// The default implementation returns `false` for maps and sequences
  /// without [`Meta::len`]. Built-in scalars, `BTreeMap` and `HashMap` override
  /// this method. Custom opaque leaves need `#[typex(partial_eq)]` or a
  /// hand-written [`Meta::eq_dyn`] implementation. Float equality follows
  /// `PartialEq`, so `NaN != NaN`.
  fn eq_dyn(&self, other: &dyn Meta) -> bool {
    if self.type_info() != other.type_info() {
      return false;
    }

    match self.kind() {
      ValueKind::Scalar => false,

      ValueKind::Option => match (self.option_value(), other.option_value()) {
        (Some(a), Some(b)) => a.eq_dyn(b),
        (None, None) => true,
        _ => false,
      },

      ValueKind::Struct => {
        if self.field_names() != other.field_names() {
          return false;
        }
        self
          .field_names()
          .iter()
          .all(|&name| match (self.field(name), other.field(name)) {
            (Some(a), Some(b)) if is_self_reference(a, self.as_any()) => {
              is_self_reference(b, other.as_any())
            }
            (Some(a), Some(b)) => a.eq_dyn(b),
            _ => false,
          })
      }

      ValueKind::Map => {
        let (Some(self_len), Some(other_len)) = (self.len(), other.len()) else {
          return false;
        };

        if self_len != other_len {
          return false;
        }

        let mut equal = true;
        if !self.visit_map_entries(&mut |key, value| {
          let Some(key) = string_map_key(key) else {
            equal = false;
            return false;
          };
          equal = other
            .key(key)
            .is_some_and(|other_value| value.eq_dyn(other_value));
          equal
        }) {
          return false;
        }
        equal
      }

      ValueKind::Sequence => {
        let (Some(self_len), Some(other_len)) = (self.len(), other.len()) else {
          return false;
        };
        if self_len != other_len {
          return false;
        }

        (0..self_len).all(|index| match (self.item(index), other.item(index)) {
          (Some(a), Some(b)) => a.eq_dyn(b),
          _ => false,
        })
      }
    }
  }

  /// Converts this [`Meta`] instance into a boxed `Any` for owned conversions;
  /// see also [`ObjectOps::to`].
  fn into_any(self: Box<Self>) -> Box<dyn Any>;

  /// Returns the value as a borrowed `Any` trait object.
  fn as_any(&self) -> &dyn Any;
}

/// Mutable reflection extension of [`Meta`].
///
/// In addition to mutable structural navigation, this trait supports insertion,
/// removal and value replacement. Unsupported navigation returns `None`.
/// Unsupported operations and type mismatches return the supplied [`Object`] in
/// `Err`.
///
/// Mutable field, item and key access only reaches existing entries. To add an
/// entry, insert or append a value, which must have the expected concrete type.
///
/// The derive macro implements this trait automatically. Manual
/// implementations must provide [`MetaMut::replace`], [`MetaMut::as_any_mut`]
/// and a hidden `as_meta` method that returns `self`.
///
/// Use [`ObjectMut`] for an owned mutable reflective value or [`ObjectRefMut`]
/// for borrowed mutable access.
///
/// # Examples
///
/// ```
/// # use typex::{Meta, MetaMut, Object, ObjectRefMut};
/// #[derive(Debug, Meta, MetaMut)]
/// #[typex(opaque)]
/// struct Number(u16);
///
/// let mut number = Number(23);
///
/// ObjectRefMut::new(&mut number)
///   .set(Object::new(Number(42)))
///   .unwrap();
///
/// assert_eq!(number.0, 42);
/// ```
///
/// Shared containers such as `Rc` and `Arc` expose mutable children only while
/// they are uniquely owned:
///
/// ```
/// # use std::rc::Rc;
/// # use typex::{Meta, MetaMut, Object};
/// #[derive(Debug, Meta, MetaMut)]
/// struct Pair {
///   value: u16,
/// }
///
/// let mut pair = Rc::new(Pair { value: 23 });
/// typex::MetaMut::field_mut(&mut pair, "value")
///   .unwrap()
///   .set(Object::new(41_u16))
///   .unwrap();
///
/// *typex::MetaMut::field_mut(&mut pair, "value")
///   .unwrap()
///   .to_mut::<u16>()
///   .unwrap() = 42;
///
/// assert_eq!(pair.value, 42);
///
/// let _shared = Rc::clone(&pair);
/// assert!(typex::MetaMut::field_mut(&mut pair, "value").is_none());
/// ```
pub trait MetaMut: Meta {
  /// Returns a mutable field by name.
  fn field_mut(&mut self, _name: &str) -> Option<ObjectRefMut<'_>> {
    None
  }

  /// Returns a mutable item at `index` for sequential access.
  fn item_mut(&mut self, _index: usize) -> Option<ObjectRefMut<'_>> {
    None
  }

  /// Returns a mutable value for `key` during map-like access.
  fn key_mut(&mut self, _key: &str) -> Option<ObjectRefMut<'_>> {
    None
  }

  /// Inserts `value` under `key`, replacing any existing value, and returns a
  /// mutable view of it.
  ///
  /// Returns `value` in `Err` if keyed insertion is unsupported, the key type
  /// cannot be built from a borrowed `&str` or the value type differs. Maps
  /// with `&'static str` keys reject insertion because a borrowed `key` cannot
  /// become `'static`.
  fn insert_key(&mut self, _key: &str, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    Err(value)
  }

  /// Inserts `value` at `index` for sequential access, returning a mutable
  /// view of the inserted item.
  ///
  /// Returns the original `value` in `Err` if this type does not support
  /// indexed insertion, if `index` is out of bounds for insertion or if
  /// `value`'s concrete type does not match the expected item type.
  fn insert_item(&mut self, _index: usize, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    Err(value)
  }

  /// Appends `value` for sequential access, returning a mutable view of the
  /// appended value.
  ///
  /// Returns the original `value` in `Err` if this type does not support
  /// appending or if `value`'s concrete type does not match the expected
  /// item type.
  fn push_item(&mut self, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    Err(value)
  }

  /// Removes the value stored under `key` for map-like access, returning it
  /// as an owned reflective object.
  fn remove_key(&mut self, _key: &str) -> Option<Object> {
    None
  }

  /// Removes the item at `index` for sequential access, returning it as an
  /// owned reflective object.
  fn remove_item(&mut self, _index: usize) -> Option<Object> {
    None
  }

  /// Moves the item at `from` to the position before the item currently at
  /// `to`. `to` may equal the length to move the item to the end.
  ///
  /// The default implementation reports [`MoveItemError::Unsupported`]. A
  /// sequential implementation can use a native operation that preserves the
  /// same index semantics.
  fn move_item(&mut self, _from: usize, _to: usize) -> Result<(), MoveItemError> {
    Err(MoveItemError::Unsupported)
  }

  /// Overwrites the whole value with `value` and drops the previous value.
  ///
  /// Returns `value` in `Err` if its concrete type differs from `Self`.
  fn set(&mut self, value: Object) -> Result<(), Object>;

  /// Replaces the whole value and returns the previous value.
  ///
  /// Returns `value` in `Err` if its concrete type differs from `Self`. Use
  /// [`MetaMut::set`] when the previous value is not needed.
  fn replace(&mut self, value: Object) -> Result<Object, Object>;

  /// Applies an ordered list of [`PatchOperation`] values without requiring
  /// `Clone` on the target. A failed operation does not undo earlier ones.
  /// Use [`MutationBatch`] for transactional application.
  fn apply<'p, I>(&mut self, operations: I) -> Result<(), ApplyError<'p>>
  where
    Self: Sized,
    I: IntoIterator<Item = PatchOperation<'p>>,
  {
    apply_patch(self as &mut dyn MetaMut, operations)
  }

  /// Returns this value as a [`Meta`] trait object.
  ///
  /// Converting `&dyn MetaMut` into `&dyn Meta` through trait upcasting
  /// requires Rust 1.86, but the crate's MSRV is 1.85. Implementations return
  /// `self`.
  #[doc(hidden)]
  fn as_meta(&self) -> &dyn Meta;

  /// Returns the value as a mutable borrowed `Any` trait object.
  fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl dyn MetaMut + '_ {
  /// Downcasts the mutable reference to `T`.
  pub fn to_mut<T: 'static>(&mut self) -> Option<&mut T> {
    self.as_any_mut().downcast_mut::<T>()
  }

  /// Traverses a nested field, item or key path; see [`FieldPathQueryMut`].
  pub fn field_path_mut<'r, Q: FieldPathQueryMut<'r>>(
    &'r mut self,
    query: Q,
  ) -> Result<Q::Output, ReflectiveError> {
    query.resolve(ObjectRefMut::new(self))
  }
}

/// Provides shared method bodies for concrete [`MetaMut`] implementations.
macro_rules! set_body {
  () => {
    fn replace(&mut self, value: Object) -> Result<Object, Object> {
      if !value.is::<Self>() {
        return Err(value);
      }

      let replacement = *value.into_inner().into_any().downcast::<Self>().unwrap();
      Ok(Object::new(core::mem::replace(self, replacement)))
    }

    fn set(&mut self, value: Object) -> Result<(), Object> {
      if !value.is::<Self>() {
        return Err(value);
      }

      *self = *value.into_inner().into_any().downcast::<Self>().unwrap();
      Ok(())
    }

    fn as_meta(&self) -> &dyn Meta {
      self
    }
  };
}
