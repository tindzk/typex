// Keep public API names in scope so Rustdoc can resolve short intra-doc links.
#[allow(unused_imports)]
use crate::{
  AccessKind, AnyRef, ApplyError, FieldPathQuery, FieldPathQueryMut, MapAccessMut, MapEntryVisitor,
  MutationBatch, Object, ObjectMut, ObjectOps, ObjectRef, ObjectRefMut, PatchOperation, Reflect,
  ReflectMut, ReflectiveError, SequenceAccessMut, TypeInfo, TypedPath, ValueKind, apply_patch,
};
use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
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

/// Compares two values of the same type through their structural shapes.
// Generic over `T` so that each default `Meta::eq_dyn` calls `T::reflect`
// statically, which lets the compiler call the access trait of `this` directly.
#[inline]
fn structural_eq<T: Meta + ?Sized>(this: &T, other: &dyn Meta) -> bool {
  let this_any = this.as_any();
  match (this.reflect(), other.reflect()) {
    (Reflect::Option(a), Reflect::Option(b)) => match (a, b) {
      (Some(a), Some(b)) => a.eq_dyn(b),
      (None, None) => true,
      _ => false,
    },

    (Reflect::Struct(a), Reflect::Struct(b)) => {
      if a.field_names() != b.field_names() {
        return false;
      }
      a.field_names()
        .iter()
        .all(|&name| match (a.field(name), b.field(name)) {
          (Some(x), Some(y)) if is_self_reference(x, this_any) => {
            is_self_reference(y, other.as_any())
          }
          (Some(x), Some(y)) => x.eq_dyn(y),
          _ => false,
        })
    }

    (Reflect::Map(a), Reflect::Map(b)) => {
      if a.len() != b.len() {
        return false;
      }

      let mut equal = true;
      a.visit_entries(&mut |key, value| {
        let Some(key) = string_map_key(key) else {
          equal = false;
          return false;
        };
        equal = b
          .key(key)
          .is_some_and(|other_value| value.eq_dyn(other_value));
        equal
      });
      equal
    }

    (Reflect::Sequence(a), Reflect::Sequence(b)) => {
      a.len() == b.len()
        && (0..a.len()).all(|index| match (a.item(index), b.item(index)) {
          (Some(x), Some(y)) => x.eq_dyn(y),
          _ => false,
        })
    }

    _ => false,
  }
}

/// Read-only reflection for runtime type information, structural navigation
/// and downcasting to a concrete type.
///
/// Implement or derive this trait to expose a type through read-only
/// reflection. Implementations are provided for common scalar and collection
/// types. [`Meta::reflect`] exposes the structure through the access traits in
/// [`Reflect`], which callers reach through [`Object`], [`ObjectRef`] or
/// `&dyn Meta` rather than by importing them.
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

  /// Returns this value's structural shape.
  ///
  /// The variant must depend only on `Self`'s concrete type, never on runtime
  /// state. A structural implementation may expose variant-dependent field
  /// names while returning the same variant.
  fn reflect(&self) -> Reflect<'_>;

  /// Returns an exposed field by name, forwarding through options.
  ///
  /// The default implementation goes through [`Meta::reflect`]. The derive
  /// overrides it with a direct lookup.
  fn field_dyn(&self, name: &str) -> Option<ObjectRef<'_>> {
    match self.reflect() {
      Reflect::Struct(value) => value.field(name),
      Reflect::Option(value) => value?.field(name),
      _ => None,
    }
  }

  /// Compares two [`Meta`] values structurally.
  ///
  /// Values with different [`Meta::type_info`] are never equal. Otherwise the
  /// comparison dispatches on the shape from [`Meta::reflect`]:
  ///
  /// - [`Reflect::Struct`] compares fields by name. A field that resolves to
  ///   the value itself, such as a derived enum's variant name, is treated as
  ///   equal.
  /// - [`Reflect::Map`] compares lengths, then looks up each visited key by
  ///   string. It supports `String` and `&'static str` keys.
  /// - [`Reflect::Sequence`] compares lengths, then items by index.
  /// - [`Reflect::Option`] compares presence and inner values.
  /// - [`Reflect::Scalar`] values are never equal, so scalar implementations
  ///   override this method.
  ///
  /// Built-in scalars, `BTreeMap`, `HashMap` and the derives override this
  /// method with equivalent direct comparisons. Custom opaque leaves need
  /// `#[typex(partial_eq)]` or a hand-written implementation. Float equality
  /// follows `PartialEq`, so `NaN != NaN`.
  fn eq_dyn(&self, other: &dyn Meta) -> bool {
    self.type_info() == other.type_info() && structural_eq(self, other)
  }

  /// Converts this [`Meta`] instance into a boxed `Any` for owned conversions;
  /// see also [`ObjectOps::to`].
  fn into_any(self: Box<Self>) -> Box<dyn Any>;

  /// Returns the value as a borrowed `Any` trait object.
  fn as_any(&self) -> &dyn Any;
}

/// Mutable reflection extension of [`Meta`].
///
/// [`MetaMut::reflect_mut`] exposes mutable structural navigation, insertion
/// and removal through the access traits in [`ReflectMut`]. Callers reach
/// them through [`ObjectMut`], [`ObjectRefMut`] or `&mut dyn MetaMut`, where
/// unsupported navigation returns `None` and unsupported operations return
/// the supplied [`Object`] in `Err`.
///
/// Mutable field, item and key access only reaches existing entries. To add an
/// entry, insert or append a value, which must have the expected concrete type.
///
/// The derive macro implements this trait automatically. Manual
/// implementations must provide [`MetaMut::reflect_mut`],
/// [`MetaMut::set_dyn`], [`MetaMut::replace_dyn`], [`MetaMut::as_any_mut`]
/// and a hidden `as_meta` method that returns `self`.
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
/// # use typex::{Meta, MetaMut, Object, ObjectRefMut};
/// #[derive(Debug, Meta, MetaMut)]
/// struct Pair {
///   value: u16,
/// }
///
/// let mut pair = Rc::new(Pair { value: 23 });
/// ObjectRefMut::new(&mut pair)
///   .field_mut("value")
///   .unwrap()
///   .set(Object::new(41_u16))
///   .unwrap();
///
/// *ObjectRefMut::new(&mut pair)
///   .field_mut("value")
///   .unwrap()
///   .to_mut::<u16>()
///   .unwrap() = 42;
///
/// assert_eq!(pair.value, 42);
///
/// let _shared = Rc::clone(&pair);
/// assert!(ObjectRefMut::new(&mut pair).field_mut("value").is_err());
/// ```
pub trait MetaMut: Meta {
  /// Returns this value's mutable structural shape.
  fn reflect_mut(&mut self) -> ReflectMut<'_>;

  /// Returns a mutable field by name, forwarding through options.
  ///
  /// The default implementation goes through [`MetaMut::reflect_mut`]. The
  /// derive overrides it with a direct lookup.
  fn field_mut_dyn(&mut self, name: &str) -> Option<ObjectRefMut<'_>> {
    match self.reflect_mut() {
      ReflectMut::Struct(value) => value.field_mut(name),
      ReflectMut::Option(value) => value.value_mut()?.inner.field_mut_dyn(name),
      _ => None,
    }
  }

  /// Returns a mutable item at `index`. An option exposes its contained value
  /// at index 0.
  ///
  /// The default implementation goes through [`MetaMut::reflect_mut`].
  fn item_mut_dyn(&mut self, index: usize) -> Option<ObjectRefMut<'_>> {
    match self.reflect_mut() {
      ReflectMut::Struct(value) => value.item_mut(index),
      ReflectMut::Sequence(value) => value.item_mut(index),
      ReflectMut::Option(value) if index == 0 => value.value_mut(),
      _ => None,
    }
  }

  /// Returns a mutable value for `key`, forwarding through options.
  ///
  /// The default implementation goes through [`MetaMut::reflect_mut`].
  fn key_mut_dyn(&mut self, key: &str) -> Option<ObjectRefMut<'_>> {
    match self.reflect_mut() {
      ReflectMut::Map(value) => value.key_mut(key),
      ReflectMut::Option(value) => value.value_mut()?.inner.key_mut_dyn(key),
      _ => None,
    }
  }

  /// Overwrites the whole value with `value` and drops the previous value.
  ///
  /// Returns `value` in `Err` if its concrete type differs from `Self`.
  fn set_dyn(&mut self, value: Object) -> Result<(), Object>;

  /// Replaces the whole value and returns the previous value.
  ///
  /// Returns `value` in `Err` if its concrete type differs from `Self`. Use
  /// [`MetaMut::set_dyn`] when the previous value is not needed.
  fn replace_dyn(&mut self, value: Object) -> Result<Object, Object>;

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

  /// Returns the mutable shape of the innermost contained value, looking
  /// through options. An option without a value reports
  /// [`ReflectMut::Opaque`].
  // A loop rather than recursion keeps this function inlinable.
  #[inline]
  fn reflect_mut_through_option(&mut self) -> ReflectMut<'_> {
    let mut shape = self.reflect_mut();
    loop {
      match shape {
        ReflectMut::Option(option) => match option.value_mut() {
          Some(inner) => shape = inner.inner.reflect_mut(),
          None => return ReflectMut::Opaque,
        },
        shape => return shape,
      }
    }
  }

  /// Returns a mutable field by name.
  ///
  /// Options forward the lookup to their contained value.
  #[inline]
  pub fn field_mut(&mut self, name: &str) -> Option<ObjectRefMut<'_>> {
    self.field_mut_dyn(name)
  }

  /// Returns a mutable item at `index`.
  ///
  /// An option exposes its contained value at index 0.
  #[inline]
  pub fn item_mut(&mut self, index: usize) -> Option<ObjectRefMut<'_>> {
    self.item_mut_dyn(index)
  }

  /// Returns a mutable value for `key`.
  ///
  /// Options forward the lookup to their contained value.
  #[inline]
  pub fn key_mut(&mut self, key: &str) -> Option<ObjectRefMut<'_>> {
    self.key_mut_dyn(key)
  }

  /// Inserts `value` under `key`; see [`MapAccessMut::insert_key`].
  ///
  /// Options forward the insertion to their contained value.
  pub fn insert_key(&mut self, key: &str, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    match self.reflect_mut_through_option() {
      ReflectMut::Map(map) => map.insert_key(key, value),
      _ => Err(value),
    }
  }

  /// Inserts `value` at `index`; see [`SequenceAccessMut::insert_item`].
  ///
  /// An option without a value accepts an insertion at index 0.
  pub fn insert_item(&mut self, index: usize, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    match self.reflect_mut() {
      ReflectMut::Sequence(sequence) => sequence.insert_item(index, value),
      ReflectMut::Option(option) if index == 0 => option.insert_value(value),
      _ => Err(value),
    }
  }

  /// Appends `value`; see [`SequenceAccessMut::push_item`].
  pub fn push_item(&mut self, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    match self.reflect_mut() {
      ReflectMut::Sequence(sequence) => sequence.push_item(value),
      _ => Err(value),
    }
  }

  /// Removes and returns the value stored under `key`.
  ///
  /// Options forward the removal to their contained value.
  pub fn remove_key(&mut self, key: &str) -> Option<Object> {
    match self.reflect_mut_through_option() {
      ReflectMut::Map(value) => value.remove_key(key),
      _ => None,
    }
  }

  /// Removes and returns the item at `index`.
  ///
  /// An option gives up its contained value at index 0.
  pub fn remove_item(&mut self, index: usize) -> Option<Object> {
    match self.reflect_mut() {
      ReflectMut::Sequence(value) => value.remove_item(index),
      ReflectMut::Option(value) if index == 0 => value.take_value(),
      _ => None,
    }
  }

  /// Moves an item; see [`SequenceAccessMut::move_item`].
  pub fn move_item(&mut self, from: usize, to: usize) -> Result<(), MoveItemError> {
    match self.reflect_mut() {
      ReflectMut::Sequence(value) => value.move_item(from, to),
      _ => Err(MoveItemError::Unsupported),
    }
  }

  /// Overwrites the whole value; see [`MetaMut::set_dyn`].
  pub fn set(&mut self, value: Object) -> Result<(), Object> {
    self.set_dyn(value)
  }

  /// Replaces the whole value and returns the previous value; see
  /// [`MetaMut::replace_dyn`].
  pub fn replace(&mut self, value: Object) -> Result<Object, Object> {
    self.replace_dyn(value)
  }

  /// Applies an ordered list of [`PatchOperation`] values. A failed operation
  /// does not undo earlier ones. Use [`MutationBatch`] for transactional
  /// application.
  pub fn apply<'p, I>(&mut self, operations: I) -> Result<(), ApplyError<'p>>
  where
    I: IntoIterator<Item = PatchOperation<'p>>,
  {
    apply_patch(self, operations)
  }
}

/// Provides shared method bodies for concrete [`MetaMut`] implementations.
macro_rules! set_body {
  () => {
    fn replace_dyn(&mut self, value: Object) -> Result<Object, Object> {
      if !value.is::<Self>() {
        return Err(value);
      }

      let replacement = *value.into_inner().into_any().downcast::<Self>().unwrap();
      Ok(Object::new(core::mem::replace(self, replacement)))
    }

    fn set_dyn(&mut self, value: Object) -> Result<(), Object> {
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
