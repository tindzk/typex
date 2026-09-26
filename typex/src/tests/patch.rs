use super::support::*;
use super::*;
use core::any::Any;

#[derive(Debug, PartialEq)]
struct RejectMiddleInsert {
  values: Vec<u8>,
}

impl Meta for RejectMiddleInsert {
  fn kind(&self) -> ValueKind {
    ValueKind::Sequence
  }

  fn access_kind(&self) -> Option<AccessKind> {
    Some(AccessKind::Item)
  }

  fn item(&self, index: usize) -> Option<ObjectRef<'_>> {
    self
      .values
      .get(index)
      .map(|value| ObjectRef::new(value as &dyn Meta))
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

impl MetaMut for RejectMiddleInsert {
  set_body!();

  fn move_item(&mut self, from: usize, to: usize) -> Result<(), MoveItemError> {
    move_item_by_remove_insert(self, from, to)
  }

  fn item_mut(&mut self, index: usize) -> Option<ObjectRefMut<'_>> {
    self
      .values
      .get_mut(index)
      .map(|value| ObjectRefMut::new(value as &mut dyn MetaMut))
  }

  fn insert_item(&mut self, index: usize, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    if index == 1 || index > self.values.len() || !value.is::<u8>() {
      return Err(value);
    }

    self.values.insert(index, value.to::<u8>().unwrap());
    Ok(ObjectRefMut::new(&mut self.values[index]))
  }

  fn remove_item(&mut self, index: usize) -> Option<Object> {
    (index < self.values.len()).then(|| Object::new(self.values.remove(index)))
  }

  fn as_any_mut(&mut self) -> &mut dyn Any {
    self
  }
}

#[test]
fn immediate_apply_keeps_successful_operations_when_a_later_operation_fails() {
  let mut pair = Pair {
    count: 1,
    label: Text("before"),
  };
  let patch = [
    PatchOperation::set([PathSegment::Field("count")], 7_u16),
    PatchOperation::set([PathSegment::Field("label")], 8_u16),
  ];

  let error = ObjectRefMut::new(&mut pair).apply(patch).unwrap_err();

  assert!(matches!(error, ApplyError::TypeMismatch { .. }));
  assert_eq!(pair.count, 7);
  assert_eq!(pair.label, Text("before"));
}

#[test]
fn apply_updates_only_fields_present_in_a_patch() {
  let mut pair = Pair {
    count: 1,
    label: Text("before"),
  };
  let patch = vec![PatchOperation::set(
    vec![PathSegment::Field("count")],
    7_u16,
  )];

  let target: &mut dyn MetaMut = &mut pair;
  ObjectRefMut::new(target).apply(patch).unwrap();

  assert_eq!(pair.count, 7);
  assert_eq!(pair.label, Text("before"));
}

#[test]
fn apply_upserts_map_entries() {
  let mut map = BTreeMap::from([(String::from("primary"), 1_u16)]);
  let patch = vec![
    PatchOperation::insert_key(Vec::new(), "primary", 2_u16),
    PatchOperation::insert_key(Vec::new(), "secondary", 3_u16),
  ];

  ObjectRefMut::new(&mut map).apply(patch).unwrap();

  assert_eq!(map.get("primary"), Some(&2));
  assert_eq!(map.get("secondary"), Some(&3));
}

#[test]
fn apply_supports_sequence_edits_and_map_removal() {
  let mut values = vec![1_u16, 2, 3];
  let patch = vec![
    PatchOperation::set(vec![PathSegment::Item(1)], 7_u16),
    PatchOperation::insert_item(Vec::new(), 2, 8_u16),
    PatchOperation::remove_item(Vec::new(), 0),
  ];

  ObjectRefMut::new(&mut values).apply(patch).unwrap();

  assert_eq!(values, vec![7, 8, 3]);

  let mut map = BTreeMap::from([
    (String::from("keep"), 1_u16),
    (String::from("remove"), 2_u16),
  ]);
  ObjectRefMut::new(&mut map)
    .apply([PatchOperation::remove_key([], "remove")])
    .unwrap();
  assert_eq!(map, BTreeMap::from([(String::from("keep"), 1_u16)]));
}

#[test]
fn apply_reports_missing_paths() {
  let mut pair = Pair {
    count: 1,
    label: Text("before"),
  };
  let patch = vec![PatchOperation::set(
    vec![PathSegment::Field("missing")],
    7_u16,
  )];

  let error = ObjectRefMut::new(&mut pair).apply(patch).unwrap_err();

  assert!(matches!(
    error,
    ApplyError::PathNotFound {
      operation: PatchOperationKind::Set,
      path,
      ..
    } if path == vec![PathSegment::Field("missing")]
  ));
  assert_eq!(pair.count, 1);
  assert_eq!(pair.label, Text("before"));
}

#[test]
fn patch_operation_kind_display_names_are_consistent() {
  assert_eq!(PatchOperationKind::Set.to_string(), "set");
  assert_eq!(PatchOperationKind::InsertKey.to_string(), "insert key");
  assert_eq!(PatchOperationKind::RemoveKey.to_string(), "remove key");
  assert_eq!(PatchOperationKind::InsertItem.to_string(), "insert item");
  assert_eq!(PatchOperationKind::PushItem.to_string(), "push item");
  assert_eq!(PatchOperationKind::RemoveItem.to_string(), "remove item");
  assert_eq!(PatchOperationKind::MoveItem.to_string(), "move item");
}

#[test]
fn move_item_out_of_bounds_leaves_sequence_unchanged() {
  let mut items = vec![1_u8, 2_u8, 3_u8];

  let result =
    ObjectRefMut::new(&mut items as &mut dyn MetaMut).apply([PatchOperation::move_item([], 0, 99)]);

  assert_eq!(
    result,
    Err(ApplyError::IndexOutOfBounds {
      path: Vec::new(),
      operation: PatchOperationKind::MoveItem,
      index: 99,
      len: 3,
    })
  );
  assert_eq!(items, vec![1, 2, 3]);
}

#[test]
fn move_item_reports_out_of_bounds_source_index() {
  let mut items = vec![1_u8, 2_u8, 3_u8];

  let result = items.apply([PatchOperation::move_item([], 3, 0)]);

  assert_eq!(
    result,
    Err(ApplyError::IndexOutOfBounds {
      path: Vec::new(),
      operation: PatchOperationKind::MoveItem,
      index: 3,
      len: 3,
    })
  );
  assert_eq!(
    result.unwrap_err().to_string(),
    "Reflective patch operation move item received index 3 for a sequence of length 3"
  );
}

#[test]
fn failed_move_restores_the_removed_item() {
  let mut sequence = RejectMiddleInsert {
    values: vec![1, 2, 3],
  };

  let result = sequence.apply([PatchOperation::move_item([], 0, 2)]);

  assert!(matches!(
    result,
    Err(ApplyError::Unsupported {
      operation: PatchOperationKind::MoveItem,
      ..
    })
  ));
  assert_eq!(sequence.values, vec![1, 2, 3]);
}

#[test]
fn failed_move_reports_an_item_that_it_cannot_restore() {
  let mut sequence = RejectInsert {
    values: vec![1, 2, 3],
  };

  let result = sequence.apply([PatchOperation::move_item([], 0, 2)]);

  assert_eq!(
    result,
    Err(ApplyError::ItemLost {
      path: Vec::new(),
      index: 0,
    })
  );
  assert_eq!(sequence.values, vec![2, 3]);
}

#[test]
fn ordered_patch_updates_nested_values_and_preserves_operation_order() {
  let mut config = PatchConfig {
    enabled: true,
    labels: BTreeMap::from([(String::from("primary"), 1)]),
    profile: Pair {
      count: 1,
      label: Text("before"),
    },
    items: vec![1, 2, 3],
  };
  let patch = vec![
    PatchOperation::set(
      vec![PathSegment::Field("profile"), PathSegment::Field("count")],
      7_u16,
    ),
    PatchOperation::insert_item(vec![PathSegment::Field("items")], 1, 8_u8),
    PatchOperation::remove_item(vec![PathSegment::Field("items")], 0),
  ];

  ObjectRefMut::new(&mut config).apply(patch).unwrap();

  assert_eq!(config.profile.count, 7);
  assert_eq!(config.items, vec![8, 2, 3]);
}

#[test]
fn patches_reject_non_string_map_keys() {
  let mut map = BTreeMap::from([(1_u8, 2_u8)]);
  let patch = vec![PatchOperation::remove_key(Vec::new(), "1")];

  assert!(ObjectRefMut::new(&mut map).apply(patch).is_err());
  assert_eq!(map.get(&1), Some(&2));
}

#[derive(Debug)]
struct NonClonePatchValue(u8);

impl Meta for NonClonePatchValue {
  fn kind(&self) -> ValueKind {
    ValueKind::Scalar
  }

  fn eq_dyn(&self, other: &dyn Meta) -> bool {
    other.as_any().downcast_ref::<Self>().map(|other| other.0) == Some(self.0)
  }

  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

impl MetaMut for NonClonePatchValue {
  set_body!();

  fn as_any_mut(&mut self) -> &mut dyn Any {
    self
  }
}

#[test]
fn patch_values_do_not_require_clone() {
  let mut value = NonClonePatchValue(1);
  let patch = vec![PatchOperation::set(Vec::new(), NonClonePatchValue(2))];

  ObjectRefMut::new(&mut value).apply(patch).unwrap();

  assert_eq!(value.0, 2);
}

#[test]
fn built_in_containers_do_not_require_clone_for_meta_access() {
  let values = Object::new(vec![NonClonePatchValue(1)]);
  assert_eq!(
    values
      .item(0)
      .unwrap()
      .to_ref::<NonClonePatchValue>()
      .unwrap()
      .0,
    1
  );

  let map = Object::new(BTreeMap::from([(
    String::from("value"),
    NonClonePatchValue(2),
  )]));
  assert_eq!(
    map
      .key("value")
      .unwrap()
      .to_ref::<NonClonePatchValue>()
      .unwrap()
      .0,
    2
  );
}
