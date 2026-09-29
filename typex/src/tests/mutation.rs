use super::support::*;
use super::*;
use core::any::Any;

#[test]
fn set_overwrites_matching_type_in_place() {
  let mut number = Number(1);

  let result = ObjectRefMut::new(&mut number).set(Object::new(Number(2)));

  assert!(result.is_ok());
  assert_eq!(number, Number(2));
}

#[test]
fn set_rejects_mismatched_type_and_returns_it_back() {
  let mut number = Number(1);

  let object = number.set_dyn(Object::new(Text("hello"))).unwrap_err();

  assert_eq!(number, Number(1));
  assert_eq!(object.to_ref::<Text>(), Some(&Text("hello")));
}

#[test]
fn set_replaces_a_struct_wholesale_through_object_ref_mut() {
  let mut pair = Pair {
    count: 1,
    label: Text("a"),
  };

  let replacement = Pair {
    count: 2,
    label: Text("b"),
  };

  let result = ObjectRefMut::new(&mut pair).set(Object::new(replacement.clone()));

  assert!(result.is_ok());
  assert_eq!(pair, replacement);
}

#[test]
fn set_returns_error_when_replacement_is_rejected() {
  #[derive(Clone, Debug)]
  struct Opaque;

  impl Meta for Opaque {
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

  impl MetaMut for Opaque {
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

  let mut opaque = Opaque;

  let object = opaque.set_dyn(Object::new(Opaque)).unwrap_err();

  assert!(object.is::<Opaque>());
}

#[test]
fn object_ref_mut_replace_returns_previous_value() {
  let mut number = Number(1);
  let mut number_ref = ObjectRefMut::new(&mut number);

  let previous = number_ref.replace(Object::new(Number(2))).unwrap();
  assert_eq!(previous.to::<Number>().unwrap(), Number(1));
  assert_eq!(
    number_ref.replace(Object::new(Text("hello"))).unwrap_err(),
    ReflectiveError::MutationTypeMismatch
  );
  assert_eq!(number, Number(2));
}

#[test]
fn field_mut_and_to_mut_mutate_the_original_value() {
  let mut pair = Pair {
    count: 7,
    label: Text("before"),
  };

  *ObjectRefMut::new(&mut pair)
    .field_mut("count")
    .unwrap()
    .to_mut::<u16>()
    .unwrap() = 9;

  assert_eq!(pair.count, 9);
}

#[test]
fn field_path_mut_traverses_and_mutates_a_nested_value() {
  let mut pair = Pair {
    count: 7,
    label: Text("before"),
  };

  let mut object = ObjectRefMut::new(&mut pair);
  let value = object
    .field_path_mut(&[PathSegment::Field("label")])
    .unwrap()
    .to_mut::<Text>()
    .unwrap();
  value.0 = "after";

  assert_eq!(pair.label, Text("after"));
}

#[test]
fn typed_field_paths_read_and_mutate_terminal_values() {
  let mut pair = Pair {
    count: 7,
    label: Text("before"),
  };
  let count_path = TypedField::<Pair, u16>::new("count").path();

  assert_eq!(ObjectRef::new(&pair).field_path(count_path), Some(&7));

  *ObjectRefMut::new(&mut pair)
    .field_path_mut(TypedField::<Pair, u16>::new("count").path())
    .unwrap() = 9;
  assert_eq!(pair.count, 9);

  assert_eq!(
    ObjectRef::new(&pair).field_path(TypedField::<Pair, u16>::new("count").path()),
    Some(&9)
  );

  *ObjectRefMut::new(&mut pair)
    .field_path_mut(TypedField::<Pair, u16>::new("count").path())
    .unwrap() = 11;
  assert_eq!(pair.count, 11);
}

#[test]
fn vec_item_mut_mutates_element_in_place() {
  let mut numbers = vec![Number(1), Number(2), Number(3)];

  *ObjectRefMut::new(&mut numbers)
    .item_mut(1)
    .unwrap()
    .to_mut::<Number>()
    .unwrap() = Number(20);

  assert_eq!(numbers, vec![Number(1), Number(20), Number(3)]);
  assert!(ObjectRefMut::new(&mut numbers).item_mut(3).is_err());
}

#[test]
fn option_meta_mut_forwards_mutation_and_absence() {
  let mut value = Some(Number(7));
  let mut absent: Option<Number> = None;

  ObjectRefMut::new(&mut value)
    .item_mut(0)
    .unwrap()
    .to_mut::<Number>()
    .unwrap()
    .0 = 70;
  assert_eq!(value, Some(Number(70)));

  assert!(ObjectRefMut::new(&mut absent).item_mut(0).is_err());
}

#[test]
fn result_meta_mut_exposes_active_variant() {
  let mut ok: Result<Number, Text> = Ok(Number(7));
  let mut err: Result<Number, Text> = Err(Text("boom"));

  ObjectRefMut::new(&mut ok)
    .field_mut("Ok")
    .unwrap()
    .to_mut::<Number>()
    .unwrap()
    .0 = 70;
  assert_eq!(ok, Ok(Number(70)));
  assert!(ObjectRefMut::new(&mut ok).field_mut("Err").is_err());

  ObjectRefMut::new(&mut err)
    .item_mut(0)
    .unwrap()
    .to_mut::<Text>()
    .unwrap()
    .0 = "fixed";
  assert_eq!(err, Err(Text("fixed")));
}

#[test]
fn tuple_meta_mut_mutates_by_field_or_index() {
  let mut pair = (7u8, true);

  *ObjectRefMut::new(&mut pair)
    .field_mut("0")
    .unwrap()
    .to_mut::<u8>()
    .unwrap() = 9;
  *ObjectRefMut::new(&mut pair)
    .item_mut(1)
    .unwrap()
    .to_mut::<bool>()
    .unwrap() = false;

  assert_eq!(pair, (9u8, false));
}

#[test]
fn map_meta_mut_supports_string_and_typed_key_lookup() {
  let mut by_name = BTreeMap::from([(String::from("primary"), Number(1))]);
  ObjectRefMut::new(&mut by_name)
    .key_mut("primary")
    .unwrap()
    .to_mut::<Number>()
    .unwrap()
    .0 = 11;
  assert_eq!(by_name.get("primary"), Some(&Number(11)));
  assert!(ObjectRefMut::new(&mut by_name).key_mut("missing").is_err());

  let mut by_id = BTreeMap::from([(7usize, Number(1))]);
  by_id
    .key_typed_mut(&7)
    .unwrap()
    .to_mut::<Number>()
    .unwrap()
    .0 = 12;
  assert_eq!(by_id.get(&7), Some(&Number(12)));
  assert!(by_id.key_typed_mut(&8).is_none());
}

#[test]
fn rc_and_arc_meta_mut_forward_only_when_uniquely_owned() {
  let mut rc = Rc::new(Pair {
    count: 1,
    label: Text("x"),
  });
  ObjectRefMut::new(&mut rc)
    .field_mut("count")
    .unwrap()
    .to_mut::<u16>()
    .unwrap();

  let _clone = Rc::clone(&rc);
  assert!(ObjectRefMut::new(&mut rc).field_mut("count").is_err());
  drop(_clone);
  assert!(ObjectRefMut::new(&mut rc).field_mut("count").is_ok());

  let mut arc = Arc::new(Pair {
    count: 1,
    label: Text("x"),
  });
  let _clone = Arc::clone(&arc);
  assert!(ObjectRefMut::new(&mut arc).field_mut("count").is_err());
  drop(_clone);
  assert!(ObjectRefMut::new(&mut arc).field_mut("count").is_ok());
}

#[test]
fn box_meta_mut_forwards_to_inner_value() {
  let mut boxed = Box::new(Pair {
    count: 1,
    label: Text("x"),
  });

  *ObjectRefMut::new(&mut boxed)
    .field_mut("count")
    .unwrap()
    .to_mut::<u16>()
    .unwrap() = 5;

  assert_eq!(boxed.count, 5);
}

#[test]
fn set_and_heap_meta_mut_have_no_structural_mutation_but_support_to_mut() {
  let mut set = BTreeSet::from([Number(1), Number(2)]);
  assert!(ObjectRefMut::new(&mut set).item_mut(0).is_err());
  assert!(
    ObjectRefMut::new(&mut set)
      .to_mut::<BTreeSet<Number>>()
      .is_ok()
  );
  set.insert(Number(3));
  assert_eq!(set.len(), 3);

  let mut heap = BinaryHeap::from([Number(1), Number(2)]);
  assert!(ObjectRefMut::new(&mut heap).item_mut(0).is_err());
  heap.push(Number(3));
  assert_eq!(heap.len(), 3);
}

#[test]
fn linked_list_and_vec_deque_meta_mut_mutate_by_index() {
  let mut list = LinkedList::new();
  list.push_back(Number(1));
  list.push_back(Number(2));
  ObjectRefMut::new(&mut list)
    .item_mut(1)
    .unwrap()
    .to_mut::<Number>()
    .unwrap()
    .0 = 20;
  assert_eq!(list.iter().nth(1), Some(&Number(20)));

  assert_eq!(
    ObjectRefMut::new(&mut list)
      .remove_item(0)
      .unwrap()
      .to_ref::<Number>(),
    Some(&Number(1))
  );
  list.push_back(Number(30));
  ObjectRefMut::new(&mut list).move_item(1, 0).unwrap();
  assert_eq!(
    list.into_iter().collect::<Vec<_>>(),
    vec![Number(30), Number(20)]
  );

  let mut deque = VecDeque::from([Number(1), Number(2)]);
  ObjectRefMut::new(&mut deque)
    .item_mut(0)
    .unwrap()
    .to_mut::<Number>()
    .unwrap()
    .0 = 10;
  assert_eq!(deque[0], Number(10));
}

#[test]
fn vec_deque_and_linked_list_push_append_and_return_the_new_value() {
  let mut vec: Vec<Number> = vec![Number(1)];
  let pushed = Object::new(Number(2));
  ObjectRefMut::new(&mut vec)
    .push_item(pushed)
    .unwrap()
    .to_mut::<Number>()
    .unwrap()
    .0 = 20;
  assert_eq!(vec, vec![Number(1), Number(20)]);

  let wrong_type = Object::new(Text("nope"));
  assert!(ObjectRefMut::new(&mut vec).push_item(wrong_type).is_err());
  assert_eq!(vec.len(), 2);

  let mut deque: VecDeque<Number> = VecDeque::from([Number(1)]);
  let pushed = Object::new(Number(2));
  ObjectRefMut::new(&mut deque).push_item(pushed).unwrap();
  assert_eq!(deque.back(), Some(&Number(2)));

  let mut list: LinkedList<Number> = LinkedList::new();
  list.push_back(Number(1));
  let pushed = Object::new(Number(2));
  ObjectRefMut::new(&mut list).push_item(pushed).unwrap();
  assert_eq!(list.back(), Some(&Number(2)));
}

#[test]
fn object_ref_mut_forwards_map_insert_and_sequence_push() {
  let mut map: BTreeMap<String, Number> = BTreeMap::new();
  ObjectRefMut::new(&mut map)
    .insert_key("answer", Object::new(Number(42)))
    .unwrap()
    .to_mut::<Number>()
    .unwrap()
    .0 = 43;
  assert_eq!(map.get("answer"), Some(&Number(43)));

  let mut values = vec![Number(1)];
  ObjectRefMut::new(&mut values)
    .push_item(Object::new(Number(2)))
    .unwrap()
    .to_mut::<Number>()
    .unwrap()
    .0 = 20;
  assert_eq!(values, vec![Number(1), Number(20)]);
}

#[test]
fn object_ref_mut_option_helpers_support_reflective_access() {
  let mut pair = Pair {
    count: 1,
    label: Text("x"),
  };

  {
    let mut root = ObjectRefMut::new(&mut pair);
    root
      .field_mut("count")
      .unwrap()
      .set(Object::new(7_u16))
      .unwrap();
    root.field_mut("label").unwrap().to_mut::<Text>().unwrap().0 = "y";
  }

  assert_eq!(pair.count, 7);
  assert_eq!(pair.label, Text("y"));

  let mut pair = Pair {
    count: 1,
    label: Text("x"),
  };
  let mut root = ObjectRefMut::new(&mut pair);
  assert!(root.field_mut("missing").ok().is_none());
  assert!(
    root
      .field_mut("count")
      .unwrap()
      .set(Object::new(Text("wrong")))
      .is_err()
  );
}

#[test]
fn object_ref_mut_set_field_path_typed_updates_without_boxing() {
  let mut pair = Pair {
    count: 1,
    label: Text("x"),
  };
  let count = TypedField::<Pair, u16>::new("count").path();
  let label = TypedField::<Pair, Text>::new("label").path();

  let mut root = ObjectRefMut::new(&mut pair);
  root.set_field_path(count, 7_u16).unwrap();
  root.set_field_path(label, Text("y")).unwrap();

  assert_eq!(
    pair,
    Pair {
      count: 7,
      label: Text("y"),
    }
  );
}

#[test]
fn object_ref_mut_set_field_path_traverses_and_overwrites() {
  let mut pair = Pair {
    count: 1,
    label: Text("x"),
  };

  let mut root = ObjectRefMut::new(&mut pair);
  root
    .set_field_path(&[PathSegment::Field("count")], Object::new(7_u16))
    .unwrap();

  assert_eq!(pair.count, 7);
  assert_eq!(pair.label, Text("x"));
}

#[test]
fn object_ref_mut_set_field_path_reports_path_and_type_errors() {
  let mut pair = Pair {
    count: 1,
    label: Text("x"),
  };

  let mut root = ObjectRefMut::new(&mut pair);
  assert_eq!(
    root
      .set_field_path(&[PathSegment::Field("missing")], Object::new(7_u16))
      .unwrap_err(),
    ReflectiveError::PathNotFound
  );
  assert_eq!(
    root
      .set_field_path(&[PathSegment::Field("count")], Object::new(Text("wrong")))
      .unwrap_err(),
    ReflectiveError::MutationTypeMismatch
  );
  assert_eq!(pair.count, 1);
}

#[test]
fn set_and_heap_do_not_support_push() {
  let mut set: BTreeSet<Number> = BTreeSet::from([Number(1)]);
  let value = Object::new(Number(2));
  assert!(ObjectRefMut::new(&mut set).push_item(value).is_err());

  let mut heap: BinaryHeap<Number> = BinaryHeap::from([Number(1)]);
  let value = Object::new(Number(2));
  assert!(ObjectRefMut::new(&mut heap).push_item(value).is_err());
}

#[test]
fn map_insert_grows_string_keyed_maps_but_not_typed_or_wrong_value_type() {
  let mut by_name: BTreeMap<String, Number> = BTreeMap::new();
  let value = Object::new(Number(7));
  ObjectRefMut::new(&mut by_name)
    .insert_key("primary", value)
    .unwrap()
    .to_mut::<Number>()
    .unwrap()
    .0 = 70;
  assert_eq!(by_name.get("primary"), Some(&Number(70)));

  let wrong_value_type = Object::new(Text("nope"));
  assert!(
    ObjectRefMut::new(&mut by_name)
      .insert_key("other", wrong_value_type)
      .is_err()
  );
  assert!(!by_name.contains_key("other"));

  // Non-`String` keys can't be built from a `&str`, so `insert_key` declines.
  let mut by_id: BTreeMap<usize, Number> = BTreeMap::new();
  let value = Object::new(Number(7));
  assert!(
    ObjectRefMut::new(&mut by_id)
      .insert_key("7", value)
      .is_err()
  );
  assert!(by_id.is_empty());
}

#[test]
fn insert_declines_for_static_str_keyed_maps() {
  // `&'static str` keys can't be manufactured from a borrowed `&str` either,
  // so `insert_key` declines the same way it does for other non-`String` keys.
  let mut by_static: BTreeMap<&'static str, Number> = BTreeMap::new();
  let value = Object::new(Number(7));

  assert!(
    ObjectRefMut::new(&mut by_static)
      .insert_key("primary", value)
      .is_err()
  );
  assert!(by_static.is_empty());
}

#[test]
fn remove_key_removes_string_and_static_str_keyed_entries() {
  let mut by_name = BTreeMap::from([(String::from("primary"), Number(1))]);
  let removed = ObjectRefMut::new(&mut by_name)
    .remove_key("primary")
    .unwrap();
  assert_eq!(removed.to_ref::<Number>(), Some(&Number(1)));
  assert!(by_name.is_empty());
  assert!(
    ObjectRefMut::new(&mut by_name)
      .remove_key("primary")
      .is_err()
  );

  let mut by_static = BTreeMap::from([("primary", Number(2))]);
  let removed = ObjectRefMut::new(&mut by_static)
    .remove_key("primary")
    .unwrap();
  assert_eq!(removed.to_ref::<Number>(), Some(&Number(2)));
  assert!(by_static.is_empty());

  let mut by_id = BTreeMap::from([(7_usize, Number(3))]);
  assert!(ObjectRefMut::new(&mut by_id).remove_key("7").is_err());
  assert_eq!(by_id.get(&7), Some(&Number(3)));
}

#[cfg(feature = "std")]
#[test]
fn remove_key_removes_static_str_keyed_hash_map_entries() {
  use std::collections::HashMap;

  let mut by_static = HashMap::from([("primary", Number(2))]);
  let removed = ObjectRefMut::new(&mut by_static)
    .remove_key("primary")
    .unwrap();
  assert_eq!(removed.to_ref::<Number>(), Some(&Number(2)));
  assert!(by_static.is_empty());
}

#[test]
fn move_item_places_item_before_destination_index() {
  let mut items = vec![1_u8, 2, 3];

  ObjectRefMut::new(&mut items).move_item(0, 2).unwrap();
  assert_eq!(items, vec![2, 1, 3]);

  ObjectRefMut::new(&mut items).move_item(0, 3).unwrap();
  assert_eq!(items, vec![1, 3, 2]);

  assert_eq!(
    ObjectRefMut::new(&mut items).move_item(0, 4),
    Err(MoveItemError::IndexOutOfBounds)
  );
  assert_eq!(
    ObjectRefMut::new(&mut items).move_item(3, 0),
    Err(MoveItemError::IndexOutOfBounds)
  );
}

#[test]
fn native_move_item_overrides_preserve_sequence_semantics() {
  let expected = vec![2_u8, 3, 1, 4];

  let mut vec = vec![1_u8, 2, 3, 4];
  ObjectRefMut::new(&mut vec).move_item(0, 3).unwrap();
  assert_eq!(vec, expected);

  let mut deque = VecDeque::from([1_u8, 2, 3, 4]);
  ObjectRefMut::new(&mut deque).move_item(0, 3).unwrap();
  assert_eq!(deque.into_iter().collect::<Vec<_>>(), expected);

  let mut list = LinkedList::from([1_u8, 2, 3, 4]);
  ObjectRefMut::new(&mut list).move_item(0, 3).unwrap();
  assert_eq!(list.into_iter().collect::<Vec<_>>(), expected);
}

#[test]
fn move_item_reports_an_item_that_it_cannot_restore() {
  let mut sequence = RejectInsert {
    values: vec![1, 2, 3],
  };

  assert_eq!(
    ObjectRefMut::new(&mut sequence).move_item(0, 2),
    Err(MoveItemError::RestoreFailed)
  );
  assert_eq!(sequence.values, vec![2, 3]);
}

#[test]
fn apply_moves_item_to_end() {
  let mut items = vec![1_u8, 2, 3];

  ObjectRefMut::new(&mut items)
    .apply([PatchOperation::move_item([], 0, 3)])
    .unwrap();

  assert_eq!(items, vec![2, 3, 1]);
}
