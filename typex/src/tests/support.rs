use super::*;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::any::Any;

pub(super) fn move_item_by_remove_insert(
  value: &mut dyn SequenceAccessMut,
  from: usize,
  to: usize,
) -> Result<(), MoveItemError> {
  let len = value.len();
  if from >= len || to > len {
    return Err(MoveItemError::IndexOutOfBounds);
  }
  if from == to {
    return Ok(());
  }

  let item = value.remove_item(from).ok_or(MoveItemError::RemoveFailed)?;
  let insert_at = if from < to { to - 1 } else { to };
  match value.insert_item(insert_at, item) {
    Ok(_) => Ok(()),
    Err(item) => match value.insert_item(from, item) {
      Ok(_) => Err(MoveItemError::InsertFailed),
      Err(_) => Err(MoveItemError::RestoreFailed),
    },
  }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Number(pub(super) u16);

#[derive(Debug)]
pub(super) struct LyingType;

impl Meta for LyingType {
  fn type_info(&self) -> TypeInfo {
    TypeInfo::of::<Number>()
  }

  fn reflect(&self) -> Reflect<'_> {
    Reflect::Scalar
  }

  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

impl Meta for Number {
  fn reflect(&self) -> Reflect<'_> {
    Reflect::Scalar
  }

  fn eq_dyn(&self, other: &dyn Meta) -> bool {
    other.as_any().downcast_ref::<Number>() == Some(self)
  }

  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

impl MetaMut for Number {
  set_body!();

  fn reflect_mut(&mut self) -> ReflectMut<'_> {
    ReflectMut::Opaque
  }

  fn as_any_mut(&mut self) -> &mut dyn Any {
    self
  }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Text(pub(super) &'static str);

impl Meta for Text {
  fn reflect(&self) -> Reflect<'_> {
    Reflect::Scalar
  }

  fn eq_dyn(&self, other: &dyn Meta) -> bool {
    other.as_any().downcast_ref::<Text>() == Some(self)
  }

  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

impl MetaMut for Text {
  set_body!();

  fn reflect_mut(&mut self) -> ReflectMut<'_> {
    ReflectMut::Opaque
  }

  fn as_any_mut(&mut self) -> &mut dyn Any {
    self
  }
}

/// Manual [`Meta`]/[`MetaMut`] pair standing in for the structural derives.
/// `typex` cannot depend on its derive crate, so this gives field and item
/// mutation tests a concrete value to exercise.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Pair {
  pub(super) count: u16,
  pub(super) label: Text,
}

impl Meta for Pair {
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

impl StructAccess for Pair {
  fn field(&self, name: &str) -> Option<ObjectRef<'_>> {
    match name {
      "count" => Some(ObjectRef::new(&self.count)),
      "label" => Some(ObjectRef::new(&self.label)),
      _ => None,
    }
  }

  fn field_names(&self) -> &'static [&'static str] {
    &["count", "label"]
  }
}

impl MetaMut for Pair {
  set_body!();

  fn reflect_mut(&mut self) -> ReflectMut<'_> {
    ReflectMut::Struct(self)
  }

  fn as_any_mut(&mut self) -> &mut dyn Any {
    self
  }
}

impl StructAccessMut for Pair {
  fn field_mut(&mut self, name: &str) -> Option<ObjectRefMut<'_>> {
    match name {
      "count" => Some(ObjectRefMut::new(&mut self.count)),
      "label" => Some(ObjectRefMut::new(&mut self.label)),
      _ => None,
    }
  }
}

/// Hand-written enum with named, tuple and unit variants. It keeps the default
/// `eq_dyn` and navigation methods, which derived enums partly override.
#[derive(Debug, PartialEq)]
pub(super) enum Shape {
  Circle { radius: u8 },
  Pair(u8, u8),
  Empty,
}

impl Meta for Shape {
  fn reflect(&self) -> Reflect<'_> {
    Reflect::Enum(self)
  }

  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

impl EnumAccess for Shape {
  fn variant_name(&self) -> &'static str {
    match self {
      Self::Circle { .. } => "Circle",
      Self::Pair(..) => "Pair",
      Self::Empty => "Empty",
    }
  }

  fn fields(&self) -> VariantFields<'_> {
    match self {
      Self::Circle { .. } => VariantFields::Named(self),
      Self::Pair(..) => VariantFields::Positional(self),
      Self::Empty => VariantFields::Unit,
    }
  }
}

impl StructAccess for Shape {
  fn field(&self, name: &str) -> Option<ObjectRef<'_>> {
    match (self, name) {
      (Self::Circle { radius }, "radius") => Some(ObjectRef::new(radius)),
      _ => None,
    }
  }

  fn field_names(&self) -> &'static [&'static str] {
    match self {
      Self::Circle { .. } => &["radius"],
      _ => &[],
    }
  }
}

impl TupleAccess for Shape {
  fn item(&self, index: usize) -> Option<ObjectRef<'_>> {
    match (self, index) {
      (Self::Pair(first, _), 0) => Some(ObjectRef::new(first)),
      (Self::Pair(_, second), 1) => Some(ObjectRef::new(second)),
      _ => None,
    }
  }

  fn len(&self) -> usize {
    match self {
      Self::Pair(..) => 2,
      _ => 0,
    }
  }
}

impl MetaMut for Shape {
  set_body!();

  fn reflect_mut(&mut self) -> ReflectMut<'_> {
    ReflectMut::Enum(self)
  }

  fn as_any_mut(&mut self) -> &mut dyn Any {
    self
  }
}

impl EnumAccessMut for Shape {
  fn fields_mut(&mut self) -> VariantFieldsMut<'_> {
    match self {
      Self::Circle { .. } => VariantFieldsMut::Named(self),
      Self::Pair(..) => VariantFieldsMut::Positional(self),
      Self::Empty => VariantFieldsMut::Unit,
    }
  }
}

impl StructAccessMut for Shape {
  fn field_mut(&mut self, name: &str) -> Option<ObjectRefMut<'_>> {
    match (self, name) {
      (Self::Circle { radius }, "radius") => Some(ObjectRefMut::new(radius)),
      _ => None,
    }
  }
}

impl TupleAccessMut for Shape {
  fn item_mut(&mut self, index: usize) -> Option<ObjectRefMut<'_>> {
    match (self, index) {
      (Self::Pair(first, _), 0) => Some(ObjectRefMut::new(first)),
      (Self::Pair(_, second), 1) => Some(ObjectRefMut::new(second)),
      _ => None,
    }
  }
}

/// Ordered list of `(key, value)` records where each key is unique. Items can
/// be looked up by index or by key, so the access kind is
/// [`AccessKind::KeyedItem`]. The value kind is [`ValueKind::Sequence`], so
/// item patches and positional mutation still apply.
#[derive(Debug, PartialEq)]
pub(super) struct KeyedSequence {
  pub(super) items: Vec<(String, Number)>,
}

impl KeyedSequence {
  fn position(&self, key: &str) -> Option<usize> {
    self.items.iter().position(|(item_key, _)| item_key == key)
  }
}

impl Meta for KeyedSequence {
  fn reflect(&self) -> Reflect<'_> {
    Reflect::KeyedSequence(self)
  }

  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

impl SequenceAccess for KeyedSequence {
  fn len(&self) -> usize {
    self.items.len()
  }

  fn item(&self, index: usize) -> Option<ObjectRef<'_>> {
    self.items.get(index).map(|value| ObjectRef::new(value))
  }
}

impl KeyedSequenceAccess for KeyedSequence {
  fn key(&self, key: &str) -> Option<ObjectRef<'_>> {
    self.item(self.position(key)?)
  }

  fn keys(&self) -> Vec<String> {
    self.items.iter().map(|(key, _)| key.clone()).collect()
  }
}

impl MetaMut for KeyedSequence {
  set_body!();

  fn reflect_mut(&mut self) -> ReflectMut<'_> {
    ReflectMut::KeyedSequence(self)
  }

  fn as_any_mut(&mut self) -> &mut dyn Any {
    self
  }
}

impl SequenceAccessMut for KeyedSequence {
  fn item_mut(&mut self, index: usize) -> Option<ObjectRefMut<'_>> {
    self
      .items
      .get_mut(index)
      .map(|value| ObjectRefMut::new(value))
  }

  fn insert_item(&mut self, index: usize, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    if index > self.items.len() || !value.is::<(String, Number)>() {
      return Err(value);
    }
    let record = value.to_ref::<(String, Number)>().unwrap();
    if self.position(&record.0).is_some() {
      return Err(value);
    }
    self
      .items
      .insert(index, value.to::<(String, Number)>().unwrap());
    Ok(ObjectRefMut::new(&mut self.items[index]))
  }

  fn push_item(&mut self, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    self.insert_item(self.items.len(), value)
  }

  fn remove_item(&mut self, index: usize) -> Option<Object> {
    (index < self.items.len()).then(|| Object::new(self.items.remove(index)))
  }

  fn move_item(&mut self, from: usize, to: usize) -> Result<(), MoveItemError> {
    move_item_by_remove_insert(self, from, to)
  }
}

impl KeyedSequenceAccessMut for KeyedSequence {
  fn key_mut(&mut self, key: &str) -> Option<ObjectRefMut<'_>> {
    let index = self.position(key)?;
    self.item_mut(index)
  }
}

#[derive(Debug, PartialEq)]
pub(super) struct PushOnly {
  pub(super) values: Vec<u8>,
}

impl Meta for PushOnly {
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

impl SequenceAccess for PushOnly {
  fn len(&self) -> usize {
    self.values.len()
  }

  fn item(&self, index: usize) -> Option<ObjectRef<'_>> {
    self.values.get(index).map(|value| ObjectRef::new(value))
  }
}

impl MetaMut for PushOnly {
  set_body!();

  fn reflect_mut(&mut self) -> ReflectMut<'_> {
    ReflectMut::Sequence(self)
  }

  fn as_any_mut(&mut self) -> &mut dyn Any {
    self
  }
}

impl SequenceAccessMut for PushOnly {
  fn item_mut(&mut self, _index: usize) -> Option<ObjectRefMut<'_>> {
    None
  }

  fn push_item(&mut self, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    if !value.is::<u8>() {
      return Err(value);
    }

    let value = *value.into_inner().into_any().downcast::<u8>().unwrap();
    self.values.push(value);
    Ok(ObjectRefMut::new(self.values.last_mut().unwrap()))
  }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PatchConfig {
  pub(super) enabled: bool,
  pub(super) labels: BTreeMap<String, u8>,
  pub(super) profile: Pair,
  pub(super) items: Vec<u8>,
}

impl Meta for PatchConfig {
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

impl StructAccess for PatchConfig {
  fn field(&self, name: &str) -> Option<ObjectRef<'_>> {
    match name {
      "enabled" => Some(ObjectRef::new(&self.enabled)),
      "labels" => Some(ObjectRef::new(&self.labels)),
      "profile" => Some(ObjectRef::new(&self.profile)),
      "items" => Some(ObjectRef::new(&self.items)),
      _ => None,
    }
  }

  fn field_names(&self) -> &'static [&'static str] {
    &["enabled", "labels", "profile", "items"]
  }
}

impl MetaMut for PatchConfig {
  set_body!();

  fn reflect_mut(&mut self) -> ReflectMut<'_> {
    ReflectMut::Struct(self)
  }

  fn as_any_mut(&mut self) -> &mut dyn Any {
    self
  }
}

impl StructAccessMut for PatchConfig {
  fn field_mut(&mut self, name: &str) -> Option<ObjectRefMut<'_>> {
    match name {
      "enabled" => Some(ObjectRefMut::new(&mut self.enabled)),
      "labels" => Some(ObjectRefMut::new(&mut self.labels)),
      "profile" => Some(ObjectRefMut::new(&mut self.profile)),
      "items" => Some(ObjectRefMut::new(&mut self.items)),
      _ => None,
    }
  }
}

/// Sequence that rejects every insertion, so a failed move cannot restore its
/// item.
#[derive(Debug, PartialEq)]
pub(super) struct RejectInsert {
  pub(super) values: Vec<u8>,
}

impl Meta for RejectInsert {
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

impl SequenceAccess for RejectInsert {
  fn len(&self) -> usize {
    self.values.len()
  }

  fn item(&self, index: usize) -> Option<ObjectRef<'_>> {
    self.values.get(index).map(|value| ObjectRef::new(value))
  }
}

impl MetaMut for RejectInsert {
  set_body!();

  fn reflect_mut(&mut self) -> ReflectMut<'_> {
    ReflectMut::Sequence(self)
  }

  fn as_any_mut(&mut self) -> &mut dyn Any {
    self
  }
}

impl SequenceAccessMut for RejectInsert {
  fn item_mut(&mut self, index: usize) -> Option<ObjectRefMut<'_>> {
    self
      .values
      .get_mut(index)
      .map(|value| ObjectRefMut::new(value))
  }

  fn move_item(&mut self, from: usize, to: usize) -> Result<(), MoveItemError> {
    move_item_by_remove_insert(self, from, to)
  }

  fn remove_item(&mut self, index: usize) -> Option<Object> {
    (index < self.values.len()).then(|| Object::new(self.values.remove(index)))
  }
}
