use crate::{
  AnyRef, MapAccess, MapAccessMut, MapEntryVisitor, Meta, MetaMut, Object, ObjectRef, ObjectRefMut,
  OptionAccessMut, Reflect, ReflectMut, SequenceAccess, SequenceAccessMut, StructAccess,
  StructAccessMut, TypedMapAccess, TypedMapAccessMut, as_map_key, btree_map_get, btree_map_get_mut,
};
#[cfg(feature = "std")]
use crate::{hash_map_get, hash_map_get_mut};
use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet, BinaryHeap, LinkedList, VecDeque};
use alloc::rc::Rc;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::any::{Any, TypeId};
#[cfg(feature = "std")]
use core::hash::Hash;
#[cfg(feature = "std")]
use std::collections::HashMap;

macro_rules! impl_meta {
  ($($ty:ty),* $(,)?) => {
    $(
      impl Meta for $ty {
        fn reflect(&self) -> Reflect<'_> {
          Reflect::Scalar
        }

        fn eq_dyn(&self, other: &dyn Meta) -> bool {
          other.as_any().downcast_ref::<$ty>().is_some_and(|other| self == other)
        }

        fn into_any(self: Box<Self>) -> Box<dyn Any> {
          self
        }

        fn as_any(&self) -> &dyn Any {
          self
        }
      }

      impl MetaMut for $ty {
        set_body!();

        fn reflect_mut(&mut self) -> ReflectMut<'_> {
          ReflectMut::Opaque
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
          self
        }
      }
    )*
  };
}

impl_meta!(
  bool,
  char,
  i8,
  i16,
  i32,
  i64,
  i128,
  isize,
  u8,
  u16,
  u32,
  u64,
  u128,
  usize,
  f32,
  f64,
  &'static str,
  String,
);

// Downcasts `value` to `T` when possible, returning the original value otherwise.
fn into_typed<T: 'static>(value: Object) -> Result<T, Object> {
  if !value.is::<T>() {
    return Err(value);
  }

  Ok(*value.into_inner().into_any().downcast::<T>().unwrap())
}

macro_rules! impl_meta_forward {
  ($wrapper:ident) => {
    impl<T> Meta for $wrapper<T>
    where
      T: Meta + 'static,
    {
      fn reflect(&self) -> Reflect<'_> {
        (**self).reflect()
      }

      fn field_dyn(&self, name: &str) -> Option<ObjectRef<'_>> {
        (**self).field_dyn(name)
      }

      fn eq_dyn(&self, other: &dyn Meta) -> bool {
        other
          .as_any()
          .downcast_ref::<Self>()
          .is_some_and(|other| (**self).eq_dyn(&**other))
      }

      fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
      }

      fn as_any(&self) -> &dyn Any {
        self
      }
    }
  };
}

macro_rules! impl_meta_mut_forward {
  (wrapper = $wrapper:ident, mode = direct $(,)?) => {
    impl<T> MetaMut for $wrapper<T>
    where
      T: MetaMut + 'static,
    {
      set_body!();

      fn reflect_mut(&mut self) -> ReflectMut<'_> {
        (**self).reflect_mut()
      }

      fn field_mut_dyn(&mut self, name: &str) -> Option<ObjectRefMut<'_>> {
        (**self).field_mut_dyn(name)
      }

      fn item_mut_dyn(&mut self, index: usize) -> Option<ObjectRefMut<'_>> {
        (**self).item_mut_dyn(index)
      }

      fn key_mut_dyn(&mut self, key: &str) -> Option<ObjectRefMut<'_>> {
        (**self).key_mut_dyn(key)
      }

      fn as_any_mut(&mut self) -> &mut dyn Any {
        self
      }
    }
  };
  (wrapper = $wrapper:ident, mode = unique $(,)?) => {
    impl<T> MetaMut for $wrapper<T>
    where
      T: MetaMut + 'static,
    {
      set_body!();

      // A shared value exposes no mutable structure.
      fn reflect_mut(&mut self) -> ReflectMut<'_> {
        match $wrapper::get_mut(self) {
          Some(inner) => inner.reflect_mut(),
          None => ReflectMut::Opaque,
        }
      }

      fn field_mut_dyn(&mut self, name: &str) -> Option<ObjectRefMut<'_>> {
        $wrapper::get_mut(self)?.field_mut_dyn(name)
      }

      fn item_mut_dyn(&mut self, index: usize) -> Option<ObjectRefMut<'_>> {
        $wrapper::get_mut(self)?.item_mut_dyn(index)
      }

      fn key_mut_dyn(&mut self, key: &str) -> Option<ObjectRefMut<'_>> {
        $wrapper::get_mut(self)?.key_mut_dyn(key)
      }

      fn as_any_mut(&mut self) -> &mut dyn Any {
        self
      }
    }
  };
}

impl_meta_forward!(Box);
impl_meta_forward!(Rc);
impl_meta_forward!(Arc);
impl_meta_mut_forward!(wrapper = Box, mode = direct);
impl_meta_mut_forward!(wrapper = Rc, mode = unique);
impl_meta_mut_forward!(wrapper = Arc, mode = unique);

fn move_vec_item<T>(value: &mut [T], from: usize, to: usize) -> Result<(), crate::MoveItemError> {
  let len = value.len();
  if from >= len || to > len {
    return Err(crate::MoveItemError::IndexOutOfBounds);
  }
  if from == to {
    return Ok(());
  }

  if from < to {
    value[from..to].rotate_left(1);
  } else {
    value[to..=from].rotate_right(1);
  }
  Ok(())
}

fn move_vec_deque_item<T>(
  value: &mut VecDeque<T>,
  from: usize,
  to: usize,
) -> Result<(), crate::MoveItemError> {
  let len = value.len();
  if from >= len || to > len {
    return Err(crate::MoveItemError::IndexOutOfBounds);
  }
  if from == to {
    return Ok(());
  }

  let value = value.make_contiguous();
  if from < to {
    value[from..to].rotate_left(1);
  } else {
    value[to..=from].rotate_right(1);
  }
  Ok(())
}

fn move_linked_list_item<T>(
  value: &mut LinkedList<T>,
  from: usize,
  to: usize,
) -> Result<(), crate::MoveItemError> {
  let len = value.len();
  if from >= len || to > len {
    return Err(crate::MoveItemError::IndexOutOfBounds);
  }
  if from == to {
    return Ok(());
  }

  let mut tail = value.split_off(from);
  let item = tail.pop_front().expect("validated source index");
  value.append(&mut tail);
  let insert_at = if from < to { to - 1 } else { to };
  let mut suffix = value.split_off(insert_at);
  value.push_back(item);
  value.append(&mut suffix);
  Ok(())
}

// Compares two sequences item by item in iteration order. Callers check the
// lengths first.
fn ordered_eq<'a, T, I>(a: I, b: I) -> bool
where
  T: Meta + 'a,
  I: IntoIterator<Item = &'a T>,
{
  a.into_iter().zip(b).all(|(a, b)| a.eq_dyn(b))
}

// Compares two sequences as sorted multisets. `BinaryHeap` iteration order
// depends on insertion history, so equal heaps can iterate differently.
fn sorted_eq<'a, T, I>(a: I, b: I) -> bool
where
  T: Meta + Ord + 'a,
  I: IntoIterator<Item = &'a T>,
{
  let mut a = a.into_iter().collect::<Vec<_>>();
  let mut b = b.into_iter().collect::<Vec<_>>();
  a.sort_unstable();
  b.sort_unstable();
  ordered_eq(a, b)
}

macro_rules! impl_seq_meta {
  (
    type = $ty:ty,
    generics = [$($generics:tt)*],
    bounds = [$($bounds:tt)*],
    item = get,
    len = method $(,)?
  ) => {
    impl_seq_meta!(@meta $ty, [$($generics)*], [$($bounds)*]);

    impl<$($generics)*> SequenceAccess for $ty
    where
      $($bounds)*
    {
      fn len(&self) -> usize {
        self.len()
      }

      fn item(&self, index: usize) -> Option<ObjectRef<'_>> {
        self.get(index).map(|value| ObjectRef::new(value))
      }
    }
  };
  (
    type = $ty:ty,
    generics = [$($generics:tt)*],
    bounds = [$($bounds:tt)*],
    item = get,
    len = const $len:ident $(,)?
  ) => {
    impl_seq_meta!(@meta $ty, [$($generics)*], [$($bounds)*]);

    impl<$($generics)*> SequenceAccess for $ty
    where
      $($bounds)*
    {
      fn len(&self) -> usize {
        $len
      }

      fn item(&self, index: usize) -> Option<ObjectRef<'_>> {
        self.get(index).map(|value| ObjectRef::new(value))
      }
    }
  };
  (
    type = $ty:ty,
    generics = [$($generics:tt)*],
    bounds = [$($bounds:tt)*],
    item = iter,
    len = method,
    eq = $eq:ident $(,)?
  ) => {
    impl<$($generics)*> Meta for $ty
    where
      $($bounds)*
    {
      fn reflect(&self) -> Reflect<'_> {
        Reflect::Sequence(self)
      }

      fn eq_dyn(&self, other: &dyn Meta) -> bool {
        other
          .as_any()
          .downcast_ref::<Self>()
          .is_some_and(|other| self.len() == other.len() && $eq(self, other))
      }

      fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
      }

      fn as_any(&self) -> &dyn Any {
        self
      }
    }

    impl<$($generics)*> SequenceAccess for $ty
    where
      $($bounds)*
    {
      fn len(&self) -> usize {
        self.len()
      }

      fn item(&self, index: usize) -> Option<ObjectRef<'_>> {
        self.iter().nth(index).map(|value| ObjectRef::new(value))
      }
    }
  };
  (@meta $ty:ty, [$($generics:tt)*], [$($bounds:tt)*]) => {
    impl<$($generics)*> Meta for $ty
    where
      $($bounds)*
    {
      fn reflect(&self) -> Reflect<'_> {
        Reflect::Sequence(self)
      }

      fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
      }

      fn as_any(&self) -> &dyn Any {
        self
      }
    }
  };
}

// Implements `MetaMut` for a sequence whose mutable shape is `Self`.
macro_rules! impl_seq_meta_mut {
  (type = $ty:ty, generics = [$($generics:tt)*], bounds = [$($bounds:tt)*] $(,)?) => {
    impl<$($generics)*> MetaMut for $ty
    where
      $($bounds)*
    {
      set_body!();

      fn reflect_mut(&mut self) -> ReflectMut<'_> {
        ReflectMut::Sequence(self)
      }

      fn item_mut_dyn(&mut self, index: usize) -> Option<ObjectRefMut<'_>> {
        SequenceAccessMut::item_mut(self, index)
      }

      fn as_any_mut(&mut self) -> &mut dyn Any {
        self
      }
    }
  };
}

macro_rules! impl_indexed_meta_mut {
  (
    type = $ty:ty,
    generics = [$($generics:tt)*],
    bounds = [$($bounds:tt)*],
    access_mut = $access_mut:ident,
    push = $push:ident,
    last = $last:ident,
    insert = $insert:ident,
    move_item = $move_item:path,
    $($extra:item)*
  ) => {
    impl_seq_meta_mut!(type = $ty, generics = [$($generics)*], bounds = [$($bounds)*]);

    impl<$($generics)*> SequenceAccessMut for $ty
    where
      $($bounds)*
    {
      fn item_mut(&mut self, index: usize) -> Option<ObjectRefMut<'_>> {
        self
          .$access_mut(index)
          .map(|value| ObjectRefMut::new(value))
      }

      fn push_item(&mut self, value: Object) -> Result<ObjectRefMut<'_>, Object> {
        let value = into_typed(value)?;
        self.$push(value);

        Ok(ObjectRefMut::new(
          self.$last().expect("just pushed")
        ))
      }

      fn insert_item(&mut self, index: usize, value: Object) -> Result<ObjectRefMut<'_>, Object> {
        if index > self.len() {
          return Err(value);
        }

        let value = into_typed(value)?;
        self.$insert(index, value);

        Ok(ObjectRefMut::new(
          self.$access_mut(index).expect("just inserted"),
        ))
      }

      $($extra)*

      fn move_item(&mut self, from: usize, to: usize) -> Result<(), crate::MoveItemError> {
        $move_item(self, from, to)
      }
    }
  };
}

impl_seq_meta!(
  type = Vec<T>,
  generics = [T],
  bounds = [T: Meta + 'static],
  item = get,
  len = method,
);

impl_indexed_meta_mut!(
  type = Vec<T>,
  generics = [T],
  bounds = [T: MetaMut + 'static],
  access_mut = get_mut,
  push = push,
  last = last_mut,
  insert = insert,
  move_item = move_vec_item,
  fn remove_item(&mut self, index: usize) -> Option<Object> {
    (index < self.len()).then(|| Object::new(self.remove(index)))
  }
);

impl_seq_meta!(
  type = [T; N],
  generics = [T, const N: usize],
  bounds = [T: Meta + 'static],
  item = get,
  len = const N,
);
impl_seq_meta_mut!(
  type = [T; N],
  generics = [T, const N: usize],
  bounds = [T: MetaMut + 'static],
);

impl<T, const N: usize> SequenceAccessMut for [T; N]
where
  T: MetaMut + 'static,
{
  fn item_mut(&mut self, index: usize) -> Option<ObjectRefMut<'_>> {
    self.get_mut(index).map(|value| ObjectRefMut::new(value))
  }
}

impl_seq_meta!(
  type = VecDeque<T>,
  generics = [T],
  bounds = [T: Meta + 'static],
  item = get,
  len = method,
);
impl_indexed_meta_mut!(
  type = VecDeque<T>,
  generics = [T],
  bounds = [T: MetaMut + 'static],
  access_mut = get_mut,
  push = push_back,
  last = back_mut,
  insert = insert,
  move_item = move_vec_deque_item,
  fn remove_item(&mut self, index: usize) -> Option<Object> {
    self.remove(index).map(Object::new)
  }
);

impl_seq_meta!(
  type = LinkedList<T>,
  generics = [T],
  bounds = [T: Meta + 'static],
  item = iter,
  len = method,
  eq = ordered_eq,
);
impl_seq_meta_mut!(
  type = LinkedList<T>,
  generics = [T],
  bounds = [T: MetaMut + 'static],
);

impl<T> SequenceAccessMut for LinkedList<T>
where
  T: MetaMut + 'static,
{
  fn item_mut(&mut self, index: usize) -> Option<ObjectRefMut<'_>> {
    self
      .iter_mut()
      .nth(index)
      .map(|value| ObjectRefMut::new(value))
  }

  fn push_item(&mut self, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    let value = into_typed(value)?;
    self.push_back(value);

    Ok(ObjectRefMut::new(self.back_mut().expect("just pushed")))
  }

  fn insert_item(&mut self, index: usize, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    if index > self.len() {
      return Err(value);
    }

    let value = into_typed(value)?;
    let mut tail = self.split_off(index);
    self.push_back(value);
    self.append(&mut tail);

    Ok(ObjectRefMut::new(
      self.iter_mut().nth(index).expect("just inserted"),
    ))
  }

  fn remove_item(&mut self, index: usize) -> Option<Object> {
    (index < self.len()).then(|| {
      let mut tail = self.split_off(index);
      let value = tail.pop_front().expect("just split at a valid index");
      self.append(&mut tail);
      Object::new(value)
    })
  }

  fn move_item(&mut self, from: usize, to: usize) -> Result<(), crate::MoveItemError> {
    move_linked_list_item(self, from, to)
  }
}

impl_seq_meta!(
  type = BTreeSet<T>,
  generics = [T],
  bounds = [T: Meta + Ord + 'static],
  item = iter,
  len = method,
  eq = ordered_eq,
);

/// Mutating a `BTreeSet` element in place could break its sort order, and
/// appending has no index or key to target since position is decided by
/// ordering. The set therefore exposes no mutable structure; use
/// `to_mut::<BTreeSet<T>>()` for `insert`/`remove`.
impl<T> MetaMut for BTreeSet<T>
where
  T: Meta + Ord + 'static,
{
  set_body!();

  fn reflect_mut(&mut self) -> ReflectMut<'_> {
    ReflectMut::Opaque
  }

  fn as_any_mut(&mut self) -> &mut dyn Any {
    self
  }
}

impl_seq_meta!(
  type = BinaryHeap<T>,
  generics = [T],
  bounds = [T: Meta + Ord + 'static],
  item = iter,
  len = method,
  eq = sorted_eq,
);

/// `BinaryHeap` only allows mutable access to its max element, via
/// `peek_mut`, and `push` sifts the new value to an arbitrary position. The
/// heap therefore exposes no mutable structure; use
/// `to_mut::<BinaryHeap<T>>()` for the heap's own API.
impl<T> MetaMut for BinaryHeap<T>
where
  T: Meta + Ord + 'static,
{
  set_body!();

  fn reflect_mut(&mut self) -> ReflectMut<'_> {
    ReflectMut::Opaque
  }

  fn as_any_mut(&mut self) -> &mut dyn Any {
    self
  }
}

impl<T> Meta for Option<T>
where
  T: Meta + 'static,
{
  fn reflect(&self) -> Reflect<'_> {
    Reflect::Option(self.as_ref().map(|value| ObjectRef::new(value)))
  }

  fn field_dyn(&self, name: &str) -> Option<ObjectRef<'_>> {
    self.as_ref()?.field_dyn(name)
  }

  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

impl<T> MetaMut for Option<T>
where
  T: MetaMut + 'static,
{
  set_body!();

  fn reflect_mut(&mut self) -> ReflectMut<'_> {
    ReflectMut::Option(self)
  }

  fn field_mut_dyn(&mut self, name: &str) -> Option<ObjectRefMut<'_>> {
    self.as_mut()?.field_mut_dyn(name)
  }

  fn item_mut_dyn(&mut self, index: usize) -> Option<ObjectRefMut<'_>> {
    if index == 0 {
      OptionAccessMut::value_mut(self)
    } else {
      None
    }
  }

  fn key_mut_dyn(&mut self, key: &str) -> Option<ObjectRefMut<'_>> {
    self.as_mut()?.key_mut_dyn(key)
  }

  fn as_any_mut(&mut self) -> &mut dyn Any {
    self
  }
}

impl<T> OptionAccessMut for Option<T>
where
  T: MetaMut + 'static,
{
  fn value_mut(&mut self) -> Option<ObjectRefMut<'_>> {
    self.as_mut().map(|value| ObjectRefMut::new(value))
  }

  fn insert_value(&mut self, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    if self.is_some() {
      return Err(value);
    }

    let value = into_typed(value)?;
    Ok(ObjectRefMut::new(self.insert(value)))
  }

  fn take_value(&mut self) -> Option<Object> {
    self.take().map(Object::new)
  }
}

impl<T, E> Meta for Result<T, E>
where
  T: Meta + 'static,
  E: Meta + 'static,
{
  fn reflect(&self) -> Reflect<'_> {
    Reflect::Struct(self)
  }

  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

impl<T, E> StructAccess for Result<T, E>
where
  T: Meta + 'static,
  E: Meta + 'static,
{
  fn field(&self, name: &str) -> Option<ObjectRef<'_>> {
    match (self, name) {
      (Ok(value), "Ok") => Some(ObjectRef::new(value)),
      (Err(error), "Err") => Some(ObjectRef::new(error)),
      _ => None,
    }
  }

  fn field_names(&self) -> &'static [&'static str] {
    match self {
      Ok(_) => &["Ok"],
      Err(_) => &["Err"],
    }
  }

  fn item(&self, index: usize) -> Option<ObjectRef<'_>> {
    match (self, index) {
      (Ok(value), 0) => Some(ObjectRef::new(value)),
      (Err(error), 0) => Some(ObjectRef::new(error)),
      _ => None,
    }
  }

  fn len(&self) -> Option<usize> {
    Some(1)
  }
}

impl<T, E> MetaMut for Result<T, E>
where
  T: MetaMut + 'static,
  E: MetaMut + 'static,
{
  set_body!();

  fn reflect_mut(&mut self) -> ReflectMut<'_> {
    ReflectMut::Struct(self)
  }

  fn as_any_mut(&mut self) -> &mut dyn Any {
    self
  }
}

impl<T, E> StructAccessMut for Result<T, E>
where
  T: MetaMut + 'static,
  E: MetaMut + 'static,
{
  fn field_mut(&mut self, name: &str) -> Option<ObjectRefMut<'_>> {
    match (self, name) {
      (Ok(value), "Ok") => Some(ObjectRefMut::new(value)),
      (Err(error), "Err") => Some(ObjectRefMut::new(error)),
      _ => None,
    }
  }

  fn item_mut(&mut self, index: usize) -> Option<ObjectRefMut<'_>> {
    match (self, index) {
      (Ok(value), 0) => Some(ObjectRefMut::new(value)),
      (Err(error), 0) => Some(ObjectRefMut::new(error)),
      _ => None,
    }
  }
}

macro_rules! impl_tuple_meta {
  ($(($($idx:tt => $name:literal : $ty:ident),+)),* $(,)?) => {
    $(
      impl<$($ty),+> Meta for ($($ty,)+)
      where
        $($ty: Meta + 'static,)+
      {
        fn reflect(&self) -> Reflect<'_> {
          Reflect::Struct(self)
        }

        fn into_any(self: Box<Self>) -> Box<dyn Any> {
          self
        }

        fn as_any(&self) -> &dyn Any {
          self
        }
      }

      impl<$($ty),+> StructAccess for ($($ty,)+)
      where
        $($ty: Meta + 'static,)+
      {
        fn field(&self, name: &str) -> Option<ObjectRef<'_>> {
          match name {
            $($name => Some(ObjectRef::new(&self.$idx)),)+
            _ => None,
          }
        }

        fn field_names(&self) -> &'static [&'static str] {
          &[$($name),+]
        }

        fn item(&self, index: usize) -> Option<ObjectRef<'_>> {
          match index {
            $($idx => Some(ObjectRef::new(&self.$idx)),)+
            _ => None,
          }
        }

        fn len(&self) -> Option<usize> {
          Some(<[_]>::len(&[$($idx),+]))
        }
      }

      impl<$($ty),+> MetaMut for ($($ty,)+)
      where
        $($ty: MetaMut + 'static,)+
      {
        set_body!();

        fn reflect_mut(&mut self) -> ReflectMut<'_> {
          ReflectMut::Struct(self)
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
          self
        }
      }

      impl<$($ty),+> StructAccessMut for ($($ty,)+)
      where
        $($ty: MetaMut + 'static,)+
      {
        fn field_mut(&mut self, name: &str) -> Option<ObjectRefMut<'_>> {
          match name {
            $($name => Some(ObjectRefMut::new(&mut self.$idx)),)+
            _ => None,
          }
        }

        fn item_mut(&mut self, index: usize) -> Option<ObjectRefMut<'_>> {
          match index {
            $($idx => Some(ObjectRefMut::new(&mut self.$idx)),)+
            _ => None,
          }
        }
      }
    )*
  };
}

impl_tuple_meta!(
  (0 => "0": T0),
  (0 => "0": T0, 1 => "1": T1),
  (0 => "0": T0, 1 => "1": T1, 2 => "2": T2),
  (0 => "0": T0, 1 => "1": T1, 2 => "2": T2, 3 => "3": T3),
  (0 => "0": T0, 1 => "1": T1, 2 => "2": T2, 3 => "3": T3, 4 => "4": T4),
  (0 => "0": T0, 1 => "1": T1, 2 => "2": T2, 3 => "3": T3, 4 => "4": T4, 5 => "5": T5),
  (0 => "0": T0, 1 => "1": T1, 2 => "2": T2, 3 => "3": T3, 4 => "4": T4, 5 => "5": T5, 6 => "6": T6),
  (0 => "0": T0, 1 => "1": T1, 2 => "2": T2, 3 => "3": T3, 4 => "4": T4, 5 => "5": T5, 6 => "6": T6, 7 => "7": T7),
);

macro_rules! impl_map_meta {
  (
    type = $ty:ty,
    generics = [$($generics:tt)*],
    bounds = [$($bounds:tt)*],
    lookup = $lookup:ident,
    equality = $eq:ident $(,)?
  ) => {
    impl<$($generics)*> Meta for $ty
    where
      $($bounds)*
    {
      fn reflect(&self) -> Reflect<'_> {
        Reflect::Map(self)
      }

      fn eq_dyn(&self, other: &dyn Meta) -> bool {
        $eq(self, other)
      }

      fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
      }

      fn as_any(&self) -> &dyn Any {
        self
      }
    }

    impl<$($generics)*> MapAccess for $ty
    where
      $($bounds)*
    {
      fn len(&self) -> usize {
        self.len()
      }

      fn key(&self, key: &str) -> Option<ObjectRef<'_>> {
        $lookup(self, key).map(|value| ObjectRef::new(value))
      }

      fn keys(&self) -> Option<Vec<String>> {
        self.keys().map(as_map_key).collect()
      }

      fn visit_entries(&self, visitor: &mut MapEntryVisitor<'_>) {
        for (key, value) in self {
          if !visitor(AnyRef::new(key), ObjectRef::new(value)) {
            break;
          }
        }
      }
    }
  };
}

/// Compares keys via `K`'s own `Ord`/`PartialEq` and values via
/// [Meta::eq_dyn], bypassing `keys`/`key`'s string round-trip.
/// Both maps iterate in the same `K`-sorted order, so this is a single
/// zipped pass rather than the generic default's per-key string lookup,
/// and it works for any `K`, not just string-like keys.
fn btree_map_eq<K, V>(this: &BTreeMap<K, V>, other: &dyn Meta) -> bool
where
  K: Ord + 'static,
  V: Meta + 'static,
{
  let Some(other) = other.as_any().downcast_ref::<BTreeMap<K, V>>() else {
    return false;
  };

  this.len() == other.len()
    && this
      .iter()
      .zip(other)
      .all(|((key, value), (other_key, other_value))| key == other_key && value.eq_dyn(other_value))
}

/// Compares native keys with `Eq`/`Hash` and values with [Meta::eq_dyn].
/// This avoids `keys`/`key`'s string conversion and is
/// independent of `HashMap` iteration order. Each key lookup takes `O(1)`
/// time on average, so equality takes `O(n)` time on average.
#[cfg(feature = "std")]
fn hash_map_eq<K, V, S>(this: &HashMap<K, V, S>, other: &dyn Meta) -> bool
where
  K: Eq + Hash + 'static,
  V: Meta + 'static,
  S: core::hash::BuildHasher + 'static,
{
  let Some(other) = other.as_any().downcast_ref::<HashMap<K, V, S>>() else {
    return false;
  };

  this.len() == other.len()
    && this.iter().all(|(key, value)| {
      other
        .get(key)
        .is_some_and(|other_value| value.eq_dyn(other_value))
    })
}

macro_rules! impl_typed_map_access {
  (
    type = $ty:ty,
    generics = [$($generics:tt)*],
    read_bounds = [$($read_bounds:tt)*],
    mut_bounds = [$($mut_bounds:tt)*] $(,)?
  ) => {
    impl<$($generics)*> TypedMapAccess<K> for $ty
    where
      $($read_bounds)*
    {
      fn key_typed(&self, key: &K) -> Option<ObjectRef<'_>> {
        self
          .get(key)
          .map(|value| ObjectRef::new(value))
      }
    }

    impl<$($generics)*> TypedMapAccessMut<K> for $ty
    where
      $($mut_bounds)*
    {
      fn key_typed_mut(&mut self, key: &K) -> Option<ObjectRefMut<'_>> {
        self
          .get_mut(key)
          .map(|value| ObjectRefMut::new(value))
      }
    }
  };
}

macro_rules! impl_map_mut {
  (
    type = $ty:ty,
    string_map = $string_map:ty,
    static_str_map = $static_str_map:ty,
    generics = [$($generics:tt)*],
    bounds = [$($bounds:tt)*],
    lookup = $lookup:ident $(,)?
  ) => {
    impl<$($generics)*> MetaMut for $ty
    where
      $($bounds)*
    {
      set_body!();

      fn reflect_mut(&mut self) -> ReflectMut<'_> {
        ReflectMut::Map(self)
      }

      fn key_mut_dyn(&mut self, key: &str) -> Option<ObjectRefMut<'_>> {
        MapAccessMut::key_mut(self, key)
      }

      fn as_any_mut(&mut self) -> &mut dyn Any {
        self
      }
    }

    impl<$($generics)*> MapAccessMut for $ty
    where
      $($bounds)*
    {
      fn key_mut(&mut self, key: &str) -> Option<ObjectRefMut<'_>> {
        $lookup(self, key).map(|value| ObjectRefMut::new(value))
      }

      fn insert_key(&mut self, key: &str, value: Object) -> Result<ObjectRefMut<'_>, Object> {
        if !value.is::<V>() {
          return Err(value);
        }

        if TypeId::of::<K>() == TypeId::of::<String>() {
          // The type ID check guarantees that this downcast succeeds.
          let value = *value.into_inner().into_any().downcast::<V>().unwrap();
          let map = (self as &mut dyn Any)
            .downcast_mut::<$string_map>()
            .unwrap();
          map.insert(key.to_owned(), value);
          return Ok(MapAccessMut::key_mut(self, key).expect("just inserted"));
        }

        // Any other `K` can't be built from a borrowed `&str`, so there's no
        // key to insert with.
        Err(value)
      }

      fn remove_key(&mut self, key: &str) -> Option<Object> {
        if TypeId::of::<K>() == TypeId::of::<String>() {
          let map = (self as &mut dyn Any)
            .downcast_mut::<$string_map>()
            .unwrap();
          return map.remove(key).map(Object::new);
        }
        if TypeId::of::<K>() == TypeId::of::<&'static str>() {
          let map = (self as &mut dyn Any)
            .downcast_mut::<$static_str_map>()
            .unwrap();
          return map.remove(key).map(Object::new);
        }

        // Any other `K` can't be matched against a borrowed `&str`.
        None
      }
    }
  };
}
impl_map_meta!(
  type = BTreeMap<K, V>,
  generics = [K, V],
  bounds = [K: Ord + 'static, V: Meta + 'static],
  lookup = btree_map_get,
  equality = btree_map_eq,
);

impl_typed_map_access!(
  type = BTreeMap<K, V>,
  generics = [K, V],
  read_bounds = [K: Ord + 'static, V: Meta + 'static],
  mut_bounds = [K: Ord + 'static, V: MetaMut + 'static],
);

impl_map_mut!(
  type = BTreeMap<K, V>,
  string_map = BTreeMap<String, V>,
  static_str_map = BTreeMap<&'static str, V>,
  generics = [K, V],
  bounds = [K: Ord + 'static, V: MetaMut + 'static],
  lookup = btree_map_get_mut,
);

#[cfg(feature = "std")]
impl_map_meta!(
  type = HashMap<K, V, S>,
  generics = [K, V, S],
  bounds = [K: Eq + Hash + 'static, V: Meta + 'static, S: core::hash::BuildHasher + 'static],
  lookup = hash_map_get,
  equality = hash_map_eq,
);

#[cfg(feature = "std")]
impl_typed_map_access!(
  type = HashMap<K, V, S>,
  generics = [K, V, S],
  read_bounds = [K: Eq + Hash + 'static, V: Meta + 'static, S: core::hash::BuildHasher + 'static],
  mut_bounds = [K: Eq + Hash + 'static, V: MetaMut + 'static, S: core::hash::BuildHasher + 'static],
);

#[cfg(feature = "std")]
impl_map_mut!(
  type = HashMap<K, V, S>,
  string_map = HashMap<String, V, S>,
  static_str_map = HashMap<&'static str, V, S>,
  generics = [K, V, S],
  bounds = [K: Eq + Hash + 'static, V: MetaMut + 'static, S: core::hash::BuildHasher + 'static],
  lookup = hash_map_get_mut,
);
