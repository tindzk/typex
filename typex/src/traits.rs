//! Reflection traits and typed map access.
//!
//! Each default is compiled per implementing type, so `reflect` or
//! `reflect_mut` and the access method dispatch statically and only the
//! `_dyn` method goes through the vtable. A free function over `&dyn Meta` or
//! `&mut dyn MetaMut` would add dynamic calls for both.

// Keep public API names in scope so Rustdoc can resolve short intra-doc links.
#[allow(unused_imports)]
use crate::{
  AccessKind, AnyRef, ApplyError, FieldPathQueryMut, MapAccessMut, MapEntryVisitor, MutationBatch,
  Object, ObjectMut, ObjectOps, ObjectRef, ObjectRefMut, PatchOperation, PathSegment, Reflect,
  ReflectMut, ReflectiveError, SequenceAccess, SequenceAccessMut, StructAccess, TupleAccess,
  TypeInfo, ValueKind, VariantFields, VariantFieldsMut, apply_patch,
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

/// Compares two sequences by length, then items by index.
// Generic over both sequence shapes because trait-object upcasting requires
// Rust 1.86 and the MSRV is Rust 1.85.
#[inline]
fn items_eq<A, B>(a: &A, b: &B) -> bool
where
  A: SequenceAccess + ?Sized,
  B: SequenceAccess + ?Sized,
{
  a.len() == b.len()
    && (0..a.len()).all(|index| match (a.item(index), b.item(index)) {
      (Some(x), Some(y)) => x == y,
      _ => false,
    })
}

/// Compares field name lists, then fields with matching names.
// Generic over both access types so that struct and enum comparisons share it
// without trait-object upcasting, which requires Rust 1.86.
#[inline]
fn named_fields_eq<A, B>(a: &A, b: &B) -> bool
where
  A: StructAccess + ?Sized,
  B: StructAccess + ?Sized,
{
  let names = a.field_names();
  if names != b.field_names() {
    return false;
  }
  for &name in names {
    match (a.field(name), b.field(name)) {
      (Some(x), Some(y)) if x == y => {}
      _ => return false,
    }
  }
  true
}

/// Compares field counts, then fields at matching indices.
// Generic for the same reason as `named_fields_eq`.
#[inline]
fn positional_fields_eq<A, B>(a: &A, b: &B) -> bool
where
  A: TupleAccess + ?Sized,
  B: TupleAccess + ?Sized,
{
  let len = a.len();
  if len != b.len() {
    return false;
  }
  for index in 0..len {
    match (a.item(index), b.item(index)) {
      (Some(x), Some(y)) if x == y => {}
      _ => return false,
    }
  }
  true
}

/// Compares two values of the same type through their structural shapes.
// Generic over `T` so that each default `Meta::eq_dyn` calls `T::reflect`
// statically, which lets the compiler call the access trait of `this` directly.
#[inline]
fn structural_eq<T: Meta + ?Sized>(this: &T, other: &dyn Meta) -> bool {
  match (this.reflect(), other.reflect()) {
    (Reflect::Option(a), Reflect::Option(b)) => match (a, b) {
      (Some(a), Some(b)) => a == b,
      (None, None) => true,
      _ => false,
    },

    (Reflect::Struct(a), Reflect::Struct(b)) => named_fields_eq(a, b),
    (Reflect::Tuple(a), Reflect::Tuple(b)) => positional_fields_eq(a, b),
    (Reflect::Enum(a), Reflect::Enum(b)) => {
      a.variant_name() == b.variant_name()
        && match (a.fields(), b.fields()) {
          (VariantFields::Unit, VariantFields::Unit) => true,
          (VariantFields::Named(a), VariantFields::Named(b)) => named_fields_eq(a, b),
          (VariantFields::Positional(a), VariantFields::Positional(b)) => {
            positional_fields_eq(a, b)
          }
          _ => false,
        }
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
        equal = b.key(key).is_some_and(|other_value| value == other_value);
        equal
      });
      equal
    }

    (Reflect::Sequence(a), Reflect::Sequence(b)) => items_eq(a, b),
    (Reflect::KeyedSequence(a), Reflect::KeyedSequence(b)) => items_eq(a, b),

    _ => false,
  }
}

/// Read-only reflection for runtime type information, structural navigation
/// and downcasting to a concrete type.
///
/// Implement or derive this trait to expose a type through read-only
/// reflection. Implementations are provided for common scalar and collection
/// types. [`Meta::reflect`] exposes the structure through the access traits in
/// [`Reflect`], which callers reach through [`Object`] or [`ObjectRef`] rather
/// than by importing them.
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
  /// The default implementation goes through [`Meta::reflect`] and costs one
  /// dynamic call, so structural types do not need to override it.
  fn field_dyn(&self, name: &str) -> Option<ObjectRef<'_>> {
    match self.reflect() {
      Reflect::Struct(value) => value.field(name),
      Reflect::Enum(value) => match value.fields() {
        VariantFields::Named(fields) => fields.field(name),
        _ => None,
      },
      Reflect::Option(value) => value?.field(name),
      _ => None,
    }
  }

  /// Returns the structural shape of the value; see [`ObjectRef::kind`].
  ///
  /// The default goes through [`Meta::reflect`] and, like the other defaults
  /// in this trait, is compiled per implementing type. An override must return
  /// the same result as the default.
  fn kind_dyn(&self) -> ValueKind {
    self.reflect().kind()
  }

  /// Returns the number of exposed structural items; see [`ObjectRef::len`].
  ///
  /// The default goes through [`Meta::reflect`]. An override must return the
  /// same result as the default.
  fn len_dyn(&self) -> Option<usize> {
    match self.reflect() {
      Reflect::Tuple(value) => Some(value.len()),
      Reflect::Enum(value) => match value.fields() {
        VariantFields::Positional(fields) => Some(fields.len()),
        _ => None,
      },
      Reflect::Sequence(value) => Some(value.len()),
      Reflect::KeyedSequence(value) => Some(value.len()),
      Reflect::Map(value) => Some(value.len()),
      Reflect::Option(value) => Some(usize::from(value.is_some())),
      Reflect::Struct(_) | Reflect::Scalar => None,
    }
  }

  /// Returns the exposed field names, forwarding through options; see
  /// [`ObjectRef::field_names`].
  ///
  /// The default goes through [`Meta::reflect`]. An override must return the
  /// same result as the default.
  fn field_names_dyn(&self) -> &'static [&'static str] {
    match self.reflect() {
      Reflect::Struct(value) => value.field_names(),
      Reflect::Enum(value) => match value.fields() {
        VariantFields::Named(fields) => fields.field_names(),
        _ => &[],
      },
      Reflect::Option(Some(value)) => value.field_names(),
      _ => &[],
    }
  }

  /// Passes each map entry to `visitor`, forwarding through options; see
  /// [`ObjectRef::visit_map_entries`].
  ///
  /// The default goes through [`Meta::reflect`]. An override must return the
  /// same result and visit the same entries as the default.
  fn visit_map_entries_dyn(&self, visitor: &mut MapEntryVisitor<'_>) -> bool {
    match self.reflect() {
      Reflect::Map(value) => {
        value.visit_entries(visitor);
        true
      }
      Reflect::Option(Some(value)) => value.visit_map_entries(visitor),
      _ => false,
    }
  }

  /// Returns the access kind of the value, forwarding through options; see
  /// [`ObjectRef::access_kind`].
  ///
  /// The default goes through [`Meta::reflect`] and reduces to a constant for
  /// most types. An override must return the same result as the default.
  fn access_kind_dyn(&self) -> Option<AccessKind> {
    match self.reflect() {
      Reflect::Scalar => None,
      Reflect::Struct(_) => Some(AccessKind::Field),
      Reflect::Tuple(_) | Reflect::Sequence(_) => Some(AccessKind::Index),
      Reflect::Enum(value) => Some(match value.fields() {
        VariantFields::Positional(_) => AccessKind::Index,
        VariantFields::Unit | VariantFields::Named(_) => AccessKind::Field,
      }),
      Reflect::KeyedSequence(_) => Some(AccessKind::KeyedItem),
      Reflect::Map(_) => Some(AccessKind::Key),
      Reflect::Option(value) => value?.access_kind(),
    }
  }

  /// Returns whether the active variant is named `name`, or `None` when the
  /// value is not an enum.
  ///
  /// Path traversal calls this for [`PathSegment::Variant`] segments, so the
  /// check costs one dynamic call. The default goes through [`Meta::reflect`].
  fn is_variant_dyn(&self, name: &str) -> Option<bool> {
    match self.reflect() {
      Reflect::Enum(value) => Some(value.variant_name() == name),
      _ => None,
    }
  }

  /// Returns an exposed item at `index`. An option exposes its contained
  /// value at index 0.
  ///
  /// The default implementation goes through [`Meta::reflect`] and costs one
  /// dynamic call; see [`Meta::field_dyn`].
  fn item_dyn(&self, index: usize) -> Option<ObjectRef<'_>> {
    match self.reflect() {
      Reflect::Tuple(value) => value.item(index),
      Reflect::Enum(value) => match value.fields() {
        VariantFields::Positional(fields) => fields.item(index),
        _ => None,
      },
      Reflect::Sequence(value) => value.item(index),
      Reflect::KeyedSequence(value) => value.item(index),
      Reflect::Option(value) => value.filter(|_| index == 0),
      Reflect::Struct(_) | Reflect::Map(_) | Reflect::Scalar => None,
    }
  }

  /// Returns a value for a string-like `key`, forwarding through options.
  ///
  /// The default implementation goes through [`Meta::reflect`] and costs one
  /// dynamic call; see [`Meta::field_dyn`].
  fn key_dyn(&self, key: &str) -> Option<ObjectRef<'_>> {
    match self.reflect() {
      Reflect::Map(value) => value.key(key),
      Reflect::KeyedSequence(value) => value.key(key),
      Reflect::Option(value) => value?.key(key),
      _ => None,
    }
  }

  /// Compares two [`Meta`] values structurally, returning `false` when their
  /// runtime type metadata differs. The default applies the
  /// [structural equality](crate#structural-equality) rules to their shapes.
  /// Override this method to compare opaque values or map keys that are not
  /// string-like.
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
/// them through [`ObjectMut`], which returns `None` for unsupported navigation
/// and the supplied [`Object`] in `Err` for unsupported operations, or through
/// [`ObjectRefMut`], which returns a [`ReflectiveError`].
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
  /// The default implementation goes through [`MetaMut::reflect_mut`] and
  /// costs one dynamic call; see [`Meta::field_dyn`].
  fn field_mut_dyn(&mut self, name: &str) -> Option<ObjectRefMut<'_>> {
    match self.reflect_mut() {
      ReflectMut::Struct(value) => value.field_mut(name),
      ReflectMut::Enum(value) => match value.fields_mut() {
        VariantFieldsMut::Named(fields) => fields.field_mut(name),
        _ => None,
      },
      ReflectMut::Option(value) => value.value_mut()?.inner.field_mut_dyn(name),
      _ => None,
    }
  }

  /// Returns a mutable item at `index`. An option exposes its contained value
  /// at index 0.
  ///
  /// The default implementation goes through [`MetaMut::reflect_mut`] and
  /// costs one dynamic call; see [`Meta::field_dyn`].
  fn item_mut_dyn(&mut self, index: usize) -> Option<ObjectRefMut<'_>> {
    match self.reflect_mut() {
      ReflectMut::Tuple(value) => value.item_mut(index),
      ReflectMut::Enum(value) => match value.fields_mut() {
        VariantFieldsMut::Positional(fields) => fields.item_mut(index),
        _ => None,
      },
      ReflectMut::Sequence(value) => value.item_mut(index),
      ReflectMut::KeyedSequence(value) => value.item_mut(index),
      ReflectMut::Option(value) if index == 0 => value.value_mut(),
      _ => None,
    }
  }

  /// Returns a mutable value for `key`, forwarding through options.
  ///
  /// The default implementation goes through [`MetaMut::reflect_mut`] and
  /// costs one dynamic call; see [`Meta::field_dyn`].
  fn key_mut_dyn(&mut self, key: &str) -> Option<ObjectRefMut<'_>> {
    match self.reflect_mut() {
      ReflectMut::Map(value) => value.key_mut(key),
      ReflectMut::KeyedSequence(value) => value.key_mut(key),
      ReflectMut::Option(value) => value.value_mut()?.inner.key_mut_dyn(key),
      _ => None,
    }
  }

  /// Inserts `value` under `key`; see [`MapAccessMut::insert_key`]. Options
  /// forward the insertion to their contained value.
  fn insert_key_dyn(&mut self, key: &str, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    match self.reflect_mut() {
      ReflectMut::Map(map) => map.insert_key(key, value),
      ReflectMut::Option(option) => match option.value_mut() {
        Some(inner) => inner.inner.insert_key_dyn(key, value),
        None => Err(value),
      },
      _ => Err(value),
    }
  }

  /// Inserts `value` at `index`; see [`SequenceAccessMut::insert_item`]. An
  /// option without a value accepts an insertion at index 0.
  fn insert_item_dyn(&mut self, index: usize, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    match self.reflect_mut() {
      ReflectMut::Sequence(sequence) => sequence.insert_item(index, value),
      ReflectMut::KeyedSequence(sequence) => sequence.insert_item(index, value),
      ReflectMut::Option(option) if index == 0 => option.insert_value(value),
      _ => Err(value),
    }
  }

  /// Appends `value`; see [`SequenceAccessMut::push_item`].
  fn push_item_dyn(&mut self, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    match self.reflect_mut() {
      ReflectMut::Sequence(sequence) => sequence.push_item(value),
      ReflectMut::KeyedSequence(sequence) => sequence.push_item(value),
      _ => Err(value),
    }
  }

  /// Removes and returns the value stored under `key`. Options forward the
  /// removal to their contained value.
  fn remove_key_dyn(&mut self, key: &str) -> Option<Object> {
    match self.reflect_mut() {
      ReflectMut::Map(value) => value.remove_key(key),
      ReflectMut::Option(value) => value.value_mut()?.inner.remove_key_dyn(key),
      _ => None,
    }
  }

  /// Removes and returns the item at `index`. An option gives up its
  /// contained value at index 0.
  fn remove_item_dyn(&mut self, index: usize) -> Option<Object> {
    match self.reflect_mut() {
      ReflectMut::Sequence(value) => value.remove_item(index),
      ReflectMut::KeyedSequence(value) => value.remove_item(index),
      ReflectMut::Option(value) if index == 0 => value.take_value(),
      _ => None,
    }
  }

  /// Moves an item; see [`SequenceAccessMut::move_item`].
  fn move_item_dyn(&mut self, from: usize, to: usize) -> Result<(), MoveItemError> {
    match self.reflect_mut() {
      ReflectMut::Sequence(value) => value.move_item(from, to),
      ReflectMut::KeyedSequence(value) => value.move_item(from, to),
      _ => Err(MoveItemError::Unsupported),
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
