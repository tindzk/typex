use super::*;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::any::Any;

pub(super) fn move_item_by_remove_insert(
  value: &mut dyn MetaMut,
  from: usize,
  to: usize,
) -> Result<(), MoveItemError> {
  if value.as_meta().kind() != ValueKind::Sequence {
    return Err(MoveItemError::Unsupported);
  }
  let len = value.as_meta().len().ok_or(MoveItemError::Unsupported)?;
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

/// Indexed collection keyed by a unique identifier, standing in for a map
/// that reports [`AccessKind::ItemKey`]. Keeps insertion order in `items` for
/// index access while `by_key` maps each key to its position for lookup
/// without a linear scan.
#[derive(Debug, PartialEq)]
pub(super) struct IndexMap {
  pub(super) items: Vec<(String, Number)>,
  by_key: BTreeMap<String, usize>,
}

impl IndexMap {
  pub(super) fn new(items: Vec<(String, Number)>) -> Self {
    let by_key = items
      .iter()
      .enumerate()
      .map(|(index, (key, _))| (key.clone(), index))
      .collect();
    Self { items, by_key }
  }
}

impl Meta for IndexMap {
  fn reflect(&self) -> Reflect<'_> {
    Reflect::Map(self)
  }

  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

impl MapAccess for IndexMap {
  fn len(&self) -> usize {
    self.items.len()
  }

  fn key(&self, key: &str) -> Option<ObjectRef<'_>> {
    let &index = self.by_key.get(key)?;
    Some(ObjectRef::new(&self.items[index].1))
  }

  fn keys(&self) -> Option<Vec<String>> {
    Some(self.items.iter().map(|(id, _)| id.clone()).collect())
  }

  fn visit_entries(&self, visitor: &mut MapEntryVisitor<'_>) {
    for (key, value) in &self.items {
      if !visitor(AnyRef::new(key), ObjectRef::new(value)) {
        break;
      }
    }
  }

  fn item(&self, index: usize) -> Option<ObjectRef<'_>> {
    self
      .items
      .get(index)
      .map(|(_, value)| ObjectRef::new(value))
  }

  fn access_kind(&self) -> AccessKind {
    AccessKind::ItemKey
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
