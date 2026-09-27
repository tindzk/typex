use super::support::*;
use super::*;
use core::any::Any;

#[derive(Debug)]
struct Rejecting;

impl Meta for Rejecting {
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

impl MetaMut for Rejecting {
  fn reflect_mut(&mut self) -> ReflectMut<'_> {
    ReflectMut::Opaque
  }

  fn set_dyn(&mut self, value: Object) -> Result<(), Object> {
    Err(value)
  }

  fn replace_dyn(&mut self, value: Object) -> Result<Object, Object> {
    Err(value)
  }

  fn as_meta(&self) -> &dyn Meta {
    self
  }

  fn as_any_mut(&mut self) -> &mut dyn Any {
    self
  }
}

#[test]
fn mutation_batch_defers_owned_operations_until_commit() {
  let mut pair = Pair {
    count: 1,
    label: Text("before"),
  };
  let name = String::from("count");
  let batch = MutationBatch::from([PatchOperation::set(
    [PathSegment::Field(name.as_str())],
    7_u16,
  )]);
  drop(name);

  assert_eq!(pair.count, 1);
  let _rollback = batch.commit(&mut pair).unwrap();

  assert_eq!(pair.count, 7);
}

#[test]
fn mutation_batch_can_be_collected_from_patch_operations() {
  let batch: MutationBatch = vec![PatchOperation::set([PathSegment::Field("count")], 7_u16)]
    .into_iter()
    .collect();

  assert_eq!(batch.len(), 1);
}

#[test]
fn mutation_batch_push_and_extend_stage_operations_in_order() {
  let mut values = vec![1_u8];
  let mut batch = MutationBatch::new();
  batch
    .push(PatchOperation::push_item([], 2_u8))
    .push(OwnedPatchOperation::push_item([], Object::new(3_u8)))
    .extend([
      PatchOperation::push_item([], 4_u8),
      PatchOperation::remove_item([], 0),
    ]);

  assert_eq!(batch.len(), 4);
  let rollback = batch.commit(&mut values).unwrap();
  assert_eq!(values, vec![2, 3, 4]);

  rollback.undo(&mut values).unwrap();
  assert_eq!(values, vec![1]);
}

#[test]
fn mutation_batch_reports_a_move_that_loses_its_item() {
  let mut sequence = RejectInsert {
    values: vec![1, 2, 3],
  };
  let batch = MutationBatch::from([PatchOperation::move_item([], 0, 2)]);

  let error = batch.commit(&mut sequence).unwrap_err();

  assert!(matches!(
    error,
    CommitError::OperationFailed {
      index: 0,
      error: CommitOperationError::ItemLost { index: 0, .. },
    }
  ));
  assert_eq!(sequence.values, vec![2, 3]);
}

#[test]
fn mutation_batch_preserves_order_and_rolls_back_structural_edits() {
  let mut values = vec![1_u8, 2_u8];
  let batch = MutationBatch::from([
    PatchOperation::remove_item([], 0),
    PatchOperation::remove_item([], 1),
  ]);

  let error = batch.commit(&mut values).unwrap_err();
  assert!(matches!(
    error,
    CommitError::OperationFailed {
      index: 1,
      error: CommitOperationError::Unsupported { .. },
      ..
    }
  ));
  assert_eq!(values, vec![1, 2]);
}

#[test]
fn mutation_batch_rolls_back_type_mismatch_to_the_current_state() {
  let mut pair = Pair {
    count: 1,
    label: Text("before"),
  };
  let baseline = MutationBatch::from([PatchOperation::set([PathSegment::Field("count")], 2_u16)]);
  baseline.commit(&mut pair).unwrap();

  let batch = MutationBatch::from([
    PatchOperation::set([PathSegment::Field("count")], 7_u16),
    PatchOperation::set([PathSegment::Field("label")], 8_u16),
  ]);

  let error = batch.commit(&mut pair).unwrap_err();

  assert!(matches!(
    error,
    CommitError::OperationFailed {
      index: 1,
      error: CommitOperationError::TypeMismatch { .. },
      ..
    }
  ));
  assert_eq!(pair.count, 2);
  assert_eq!(pair.label, Text("before"));
}

#[test]
fn mutation_batch_supports_all_patch_operations() {
  let mut config = PatchConfig {
    enabled: true,
    labels: BTreeMap::from([(String::from("primary"), 1)]),
    profile: Pair {
      count: 1,
      label: Text("before"),
    },
    items: vec![1, 2, 3],
  };
  let batch = MutationBatch::from([
    PatchOperation::set(
      [PathSegment::Field("profile"), PathSegment::Field("count")],
      7_u16,
    ),
    PatchOperation::insert_key([PathSegment::Field("labels")], "secondary", 2_u8),
    PatchOperation::remove_key([PathSegment::Field("labels")], "primary"),
    PatchOperation::insert_item([PathSegment::Field("items")], 1, 8_u8),
    PatchOperation::push_item([PathSegment::Field("items")], 9_u8),
    PatchOperation::remove_item([PathSegment::Field("items")], 0),
    PatchOperation::move_item([PathSegment::Field("items")], 0, 3),
  ]);

  let rollback = batch.commit(&mut config).unwrap();

  assert_eq!(config.profile.count, 7);
  assert_eq!(
    config.labels,
    BTreeMap::from([(String::from("secondary"), 2)])
  );
  assert_eq!(config.items, vec![2, 3, 8, 9]);
  rollback.undo(&mut config).unwrap();
  assert_eq!(
    config,
    PatchConfig {
      enabled: true,
      labels: BTreeMap::from([(String::from("primary"), 1)]),
      profile: Pair {
        count: 1,
        label: Text("before"),
      },
      items: vec![1, 2, 3],
    }
  );
}

#[test]
fn mutation_batch_reports_inverse_rollback_failure() {
  let mut value = PushOnly { values: vec![1] };
  let batch = MutationBatch::from([
    PatchOperation::push_item([], 2_u8),
    PatchOperation::set([], Text("wrong")),
  ]);

  let error = batch.commit(&mut value).unwrap_err();

  assert!(matches!(
    error,
    CommitError::RollbackFailed {
      failed_operation,
      operation_index: 0,
      error: ref error @ CommitOperationError::Unsupported {
        operation: PatchOperationKind::RemoveItem,
        ..
      },
    } if error.path().is_empty()
      && matches!(
        *failed_operation,
        CommitError::OperationFailed {
          index: 1,
          error: CommitOperationError::TypeMismatch { .. },
          ..
        }
      )
  ));
  assert_eq!(value, PushOnly { values: vec![1, 2] });
}

#[test]
fn mutation_batch_returns_a_consumable_rollback() {
  let mut pair = Pair {
    count: 1,
    label: Text("before"),
  };
  let batch = MutationBatch::from([PatchOperation::set([PathSegment::Field("count")], 7_u16)]);

  let rollback = batch.commit(&mut pair).unwrap();
  assert_eq!(
    rollback.previous_value(0).unwrap().to_ref::<u16>(),
    Some(&1)
  );
  rollback.undo(&mut pair).unwrap();

  assert_eq!(pair.count, 1);

  let batch = MutationBatch::from([PatchOperation::set([PathSegment::Field("count")], 9_u16)]);
  let rollback = batch.commit(&mut pair).unwrap();
  rollback.undo(&mut pair).unwrap();

  assert_eq!(pair.count, 1);
}

#[test]
fn mutation_batch_rolls_back_map_replacements() {
  let mut map = BTreeMap::from([(String::from("primary"), 1_u8)]);
  let batch = MutationBatch::from([
    PatchOperation::insert_key([], "primary", 2_u8),
    PatchOperation::remove_item([], 0),
  ]);

  assert!(batch.commit(&mut map).is_err());
  assert_eq!(map.get("primary"), Some(&1));
}

#[test]
fn mutation_batch_reports_unsupported_same_typed_set_value() {
  let mut value = Rejecting;
  let batch = MutationBatch::from([PatchOperation::set([], Rejecting)]);

  let error = batch.commit(&mut value).unwrap_err();

  assert!(matches!(
    error,
    CommitError::OperationFailed {
      index: 0,
      error: CommitOperationError::Unsupported {
        operation: PatchOperationKind::Set,
        ..
      },
      ..
    }
  ));
}

#[test]
fn mutation_batch_reports_existing_map_value_type_mismatch() {
  let mut map = BTreeMap::from([(String::from("primary"), Rejecting)]);
  let batch = MutationBatch::from([PatchOperation::insert_key([], "primary", 7_u8)]);

  let error = batch.commit(&mut map).unwrap_err();

  assert!(matches!(
    error,
    CommitError::OperationFailed {
      index: 0,
      error: CommitOperationError::TypeMismatch { .. },
      ..
    }
  ));
}

#[test]
fn mutation_batch_reports_unsupported_existing_map_replacement() {
  let mut map = BTreeMap::from([(String::from("primary"), Rejecting)]);
  let batch = MutationBatch::from([PatchOperation::insert_key([], "primary", Rejecting)]);

  let error = batch.commit(&mut map).unwrap_err();

  assert!(matches!(
    error,
    CommitError::OperationFailed {
      index: 0,
      error: CommitOperationError::Unsupported {
        operation: PatchOperationKind::InsertKey,
        ..
      },
      ..
    }
  ));
}

#[test]
fn mutation_batch_reports_unsupported_new_key_when_key_cannot_be_built() {
  let mut map = BTreeMap::from([(1_usize, 2_u8)]);
  let batch = MutationBatch::from([PatchOperation::insert_key([], "3", 4_u8)]);

  let error = batch.commit(&mut map).unwrap_err();

  assert!(matches!(
    error,
    CommitError::OperationFailed {
      index: 0,
      error: CommitOperationError::Unsupported {
        operation: PatchOperationKind::InsertKey,
        ..
      },
      ..
    }
  ));
  assert_eq!(map, BTreeMap::from([(1_usize, 2_u8)]));
}

#[test]
fn mutation_batch_rolls_back_item_moves() {
  for (from, to) in [(0, 0), (0, 1), (0, 2), (0, 3), (2, 0), (1, 0), (2, 1)] {
    let mut items = vec![1_u8, 2, 3];
    let batch = MutationBatch::from([PatchOperation::move_item([], from, to)]);

    let rollback = batch.commit(&mut items).unwrap();
    rollback.undo(&mut items).unwrap();

    assert_eq!(items, vec![1, 2, 3], "move {from} -> {to}");
  }
}

#[test]
fn mutation_batch_reports_out_of_bounds_move() {
  let mut items = vec![1_u8, 2, 3];
  let batch = MutationBatch::from([PatchOperation::move_item([], 0, 4)]);

  let error = batch.commit(&mut items).unwrap_err();

  assert!(matches!(
    error,
    CommitError::OperationFailed {
      index: 0,
      error: CommitOperationError::IndexOutOfBounds {
        operation: PatchOperationKind::MoveItem,
        index: 4,
        len: 3,
        ..
      },
    }
  ));
  assert_eq!(items, vec![1, 2, 3]);
}

#[test]
fn mutation_batch_reports_shape_mismatch_for_a_container_of_another_kind() {
  let mut map = Some(BTreeMap::from([(String::from("keep"), 1_u8)]));
  let batch = MutationBatch::from([PatchOperation::insert_key([], "new", 2_u8)]);

  let error = batch.commit(&mut map).unwrap_err();

  assert!(matches!(
    error,
    CommitError::OperationFailed {
      index: 0,
      error: CommitOperationError::ShapeMismatch {
        expected: ValueKind::Map,
        actual: ValueKind::Option,
        ..
      },
    }
  ));
  assert_eq!(map, Some(BTreeMap::from([(String::from("keep"), 1_u8)])));
}

#[test]
fn mutation_batch_reports_unsupported_edits_of_a_sequence_without_mutable_structure() {
  let mut set = BTreeSet::from([1_u8, 2]);
  for (operation, kind) in [
    (
      PatchOperation::push_item([], 3_u8),
      PatchOperationKind::PushItem,
    ),
    (
      PatchOperation::insert_item([], 0, 3_u8),
      PatchOperationKind::InsertItem,
    ),
    (
      PatchOperation::remove_item([], 0),
      PatchOperationKind::RemoveItem,
    ),
    (
      PatchOperation::move_item([], 0, 2),
      PatchOperationKind::MoveItem,
    ),
  ] {
    let error = MutationBatch::from([operation])
      .commit(&mut set)
      .unwrap_err();

    assert!(matches!(
      error,
      CommitError::OperationFailed {
        index: 0,
        error: CommitOperationError::Unsupported { operation, .. },
      } if operation == kind
    ));
  }
  assert_eq!(set, BTreeSet::from([1, 2]));
}
