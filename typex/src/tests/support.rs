use super::*;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::any::Any;

pub(super) fn move_item_by_remove_insert<T: MetaMut + ?Sized>(
  value: &mut T,
  from: usize,
  to: usize,
) -> Result<(), MoveItemError> {
  if value.kind() != ValueKind::Sequence {
    return Err(MoveItemError::Unsupported);
  }
  let len = value.len().ok_or(MoveItemError::Unsupported)?;
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

  fn kind(&self) -> ValueKind {
    ValueKind::Scalar
  }

  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

impl Meta for Number {
  fn kind(&self) -> ValueKind {
    ValueKind::Scalar
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

  fn as_any_mut(&mut self) -> &mut dyn Any {
    self
  }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Text(pub(super) &'static str);

impl Meta for Text {
  fn kind(&self) -> ValueKind {
    ValueKind::Scalar
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
  fn kind(&self) -> ValueKind {
    ValueKind::Struct
  }

  fn access_kind(&self) -> Option<AccessKind> {
    Some(AccessKind::Field)
  }

  fn field(&self, name: &str) -> Option<ObjectRef<'_>> {
    match name {
      "count" => Some(ObjectRef::new(&self.count as &dyn Meta)),
      "label" => Some(ObjectRef::new(&self.label as &dyn Meta)),
      _ => None,
    }
  }

  fn field_names(&self) -> &'static [&'static str] {
    &["count", "label"]
  }

  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

impl MetaMut for Pair {
  set_body!();

  fn field_mut(&mut self, name: &str) -> Option<ObjectRefMut<'_>> {
    match name {
      "count" => Some(ObjectRefMut::new(&mut self.count as &mut dyn MetaMut)),
      "label" => Some(ObjectRefMut::new(&mut self.label as &mut dyn MetaMut)),
      _ => None,
    }
  }

  fn as_any_mut(&mut self) -> &mut dyn Any {
    self
  }
}

/// Indexed collection keyed by a unique identifier, standing in for a
/// structural derive that reports [`AccessKind::ItemKey`]. Keeps
/// insertion order in `items` for index access while `by_key` maps each key
/// to its position for lookup without a linear scan.
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
  fn kind(&self) -> ValueKind {
    ValueKind::Sequence
  }

  fn access_kind(&self) -> Option<AccessKind> {
    Some(AccessKind::ItemKey)
  }

  fn item(&self, index: usize) -> Option<ObjectRef<'_>> {
    self
      .items
      .get(index)
      .map(|(_, value)| ObjectRef::new(value as &dyn Meta))
  }

  fn len(&self) -> Option<usize> {
    Some(self.items.len())
  }

  fn key(&self, key: &str) -> Option<ObjectRef<'_>> {
    let &index = self.by_key.get(key)?;
    Some(ObjectRef::new(&self.items[index].1 as &dyn Meta))
  }

  fn keys(&self) -> Option<Vec<String>> {
    Some(self.items.iter().map(|(id, _)| id.clone()).collect())
  }

  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

#[derive(Debug, PartialEq)]
pub(super) struct PushOnly {
  pub(super) values: Vec<u8>,
}

impl Meta for PushOnly {
  fn kind(&self) -> ValueKind {
    ValueKind::Sequence
  }

  fn len(&self) -> Option<usize> {
    Some(self.values.len())
  }

  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

impl MetaMut for PushOnly {
  set_body!();

  fn push_item(&mut self, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    if !value.is::<u8>() {
      return Err(value);
    }

    let value = *value.into_inner().into_any().downcast::<u8>().unwrap();
    self.values.push(value);
    Ok(ObjectRefMut::new(self.values.last_mut().unwrap()))
  }

  fn as_any_mut(&mut self) -> &mut dyn Any {
    self
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
  fn kind(&self) -> ValueKind {
    ValueKind::Struct
  }

  fn access_kind(&self) -> Option<AccessKind> {
    Some(AccessKind::Field)
  }

  fn field(&self, name: &str) -> Option<ObjectRef<'_>> {
    match name {
      "enabled" => Some(ObjectRef::new(&self.enabled as &dyn Meta)),
      "labels" => Some(ObjectRef::new(&self.labels as &dyn Meta)),
      "profile" => Some(ObjectRef::new(&self.profile as &dyn Meta)),
      "items" => Some(ObjectRef::new(&self.items as &dyn Meta)),
      _ => None,
    }
  }

  fn field_names(&self) -> &'static [&'static str] {
    &["enabled", "labels", "profile", "items"]
  }

  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

impl MetaMut for PatchConfig {
  set_body!();

  fn field_mut(&mut self, name: &str) -> Option<ObjectRefMut<'_>> {
    match name {
      "enabled" => Some(ObjectRefMut::new(&mut self.enabled as &mut dyn MetaMut)),
      "labels" => Some(ObjectRefMut::new(&mut self.labels as &mut dyn MetaMut)),
      "profile" => Some(ObjectRefMut::new(&mut self.profile as &mut dyn MetaMut)),
      "items" => Some(ObjectRefMut::new(&mut self.items as &mut dyn MetaMut)),
      _ => None,
    }
  }

  fn as_any_mut(&mut self) -> &mut dyn Any {
    self
  }
}

/// Sequence that rejects every insertion, so a failed move cannot restore its
/// item.
#[derive(Debug, PartialEq)]
pub(super) struct RejectInsert {
  pub(super) values: Vec<u8>,
}

impl Meta for RejectInsert {
  fn kind(&self) -> ValueKind {
    ValueKind::Sequence
  }

  fn len(&self) -> Option<usize> {
    Some(self.values.len())
  }

  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

impl MetaMut for RejectInsert {
  set_body!();

  fn move_item(&mut self, from: usize, to: usize) -> Result<(), MoveItemError> {
    move_item_by_remove_insert(self, from, to)
  }

  fn remove_item(&mut self, index: usize) -> Option<Object> {
    (index < self.values.len()).then(|| Object::new(self.values.remove(index)))
  }

  fn as_any_mut(&mut self) -> &mut dyn Any {
    self
  }
}
