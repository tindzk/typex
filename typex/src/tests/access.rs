use super::support::*;
use super::*;

#[test]
fn option_meta_forwards_nested_access_and_presence() {
  let value = Some(Number(7));
  let absent: Option<Number> = None;

  assert_eq!(
    ObjectRef::new(&value)
      .option_value()
      .unwrap()
      .to_ref::<Number>(),
    Some(&Number(7))
  );
  assert!(ObjectRef::new(&absent).option_value().is_none());
  assert_eq!(ObjectRef::new(&value).len(), Some(1));
  assert_eq!(
    ObjectRef::new(&value).item(0).unwrap().to_ref::<Number>(),
    Some(&Number(7))
  );
  assert_eq!(ObjectRef::new(&absent).len(), Some(0));
  assert!(ObjectRef::new(&absent).item(0).is_none());
}

#[test]
fn object_ref_determines_kind_without_type_name_matching() {
  let value = Some(Number(7));
  let list = vec![Number(7)];
  let boxed = Box::new(Some(Number(7)));
  let rc = Rc::new(Some(Number(7)));
  let arc = Arc::new(Some(Number(7)));
  let scalar = Number(7);
  let pair = Pair {
    count: 7,
    label: Text("seven"),
  };
  let mut map = BTreeMap::new();
  map.insert("key".to_owned(), Number(7));

  assert_eq!(ObjectRef::new(&value).kind(), ValueKind::Option);
  assert_eq!(ObjectRef::new(&list).kind(), ValueKind::Sequence);
  assert_eq!(ObjectRef::new(&boxed).kind(), ValueKind::Option);
  assert_eq!(ObjectRef::new(&rc).kind(), ValueKind::Option);
  assert_eq!(ObjectRef::new(&arc).kind(), ValueKind::Option);
  assert_eq!(ObjectRef::new(&scalar).kind(), ValueKind::Scalar);
  assert_eq!(ObjectRef::new(&pair).kind(), ValueKind::Struct);
  assert_eq!(ObjectRef::new(&map).kind(), ValueKind::Map);
  assert_eq!(ObjectRef::new(&value).access_kind(), None);
  assert_eq!(ObjectRef::new(&list).access_kind(), Some(AccessKind::Item));
  assert_eq!(ObjectRef::new(&scalar).access_kind(), None);
  assert_eq!(ObjectRef::new(&pair).access_kind(), Some(AccessKind::Field));
  assert_eq!(ObjectRef::new(&map).access_kind(), Some(AccessKind::Key));

  let optional_pair = Some(pair);
  let absent_pair: Option<Pair> = None;
  assert_eq!(
    ObjectRef::new(&optional_pair).access_kind(),
    Some(AccessKind::Field)
  );
  assert_eq!(ObjectRef::new(&absent_pair).access_kind(), None);
}

#[test]
fn tuple_meta_exposes_indexed_access() {
  let pair = (7u8, true);

  assert_eq!(ObjectRef::new(&pair).field_names(), &["0", "1"]);
  assert_eq!(ObjectRef::new(&pair).len(), Some(2));
  assert_eq!(
    ObjectRef::new(&pair).item(0).unwrap().to_ref::<u8>(),
    Some(&7)
  );
  assert_eq!(
    ObjectRef::new(&pair).field("1").unwrap().to_ref::<bool>(),
    Some(&true)
  );
}

#[test]
fn map_meta_supports_typed_lookup_for_non_string_keys() {
  let map = BTreeMap::from([(7usize, Number(11))]);

  assert_eq!(
    map.key_typed(&7).unwrap().to_ref::<Number>(),
    Some(&Number(11))
  );
  assert!(ObjectRef::new(&map).key("7").is_none());
  assert!(ObjectRef::new(&map).keys().is_none());
}

#[test]
fn map_meta_supports_string_lookup_for_string_keys() {
  let map = BTreeMap::from([(String::from("primary"), Number(11))]);

  assert_eq!(
    ObjectRef::new(&map).keys(),
    Some(vec!["primary".to_string()])
  );
  assert_eq!(
    ObjectRef::new(&map)
      .key("primary")
      .unwrap()
      .to_ref::<Number>(),
    Some(&Number(11))
  );
}

#[test]
fn map_meta_reports_keyed_entry_count_through_len() {
  let map = BTreeMap::from([
    (String::from("primary"), Number(11)),
    (String::from("secondary"), Number(13)),
  ]);

  assert_eq!(ObjectRef::new(&map).len(), Some(2));
}

#[test]
fn map_meta_does_not_expose_string_lookup_for_non_string_keys() {
  let numbers = BTreeMap::from([(7usize, Number(11))]);
  let chars = BTreeMap::from([('x', Number(13))]);

  assert!(ObjectRef::new(&numbers).keys().is_none());
  assert!(ObjectRef::new(&numbers).key("7").is_none());
  assert!(ObjectRef::new(&chars).keys().is_none());
  assert!(ObjectRef::new(&chars).key("x").is_none());
  assert_eq!(
    numbers.key_typed(&7).unwrap().to_ref::<Number>(),
    Some(&Number(11))
  );
  assert_eq!(
    chars.key_typed(&'x').unwrap().to_ref::<Number>(),
    Some(&Number(13))
  );
}

#[test]
fn map_entries_visit_non_string_keys_without_stringifying_lookup() {
  let numbers = BTreeMap::from([(7usize, Number(11)), (9usize, Number(13))]);
  let mut seen = Vec::new();

  assert!(ObjectRef::new(&numbers).visit_map_entries(
    &mut |key: AnyRef<'_>, value: ObjectRef<'_>| {
      seen.push((
        *key.to_ref::<usize>().unwrap(),
        value.to_ref::<Number>().unwrap().0,
      ));
      true
    }
  ));

  assert_eq!(seen, vec![(7, 11), (9, 13)]);
  assert!(ObjectRef::new(&numbers).keys().is_none());
}

#[test]
fn map_entries_can_stop_early() {
  let numbers = BTreeMap::from([(7usize, Number(11)), (9usize, Number(13))]);
  let mut seen = Vec::new();

  assert!(ObjectRef::new(&numbers).visit_map_entries(
    &mut |key: AnyRef<'_>, value: ObjectRef<'_>| {
      seen.push((
        *key.to_ref::<usize>().unwrap(),
        value.to_ref::<Number>().unwrap().0,
      ));
      false
    }
  ));

  assert_eq!(seen, vec![(7, 11)]);
}

#[test]
fn wrapper_and_collection_meta_forward_access() {
  let boxed = Box::new((7u8, true));
  let shared = Rc::new(vec![Number(8), Number(9)]);
  let atomic = Arc::new(Some(Number(10)));
  let deque = VecDeque::from([Number(10), Number(11)]);
  let mut list = LinkedList::new();
  list.push_back(Number(12));
  list.push_back(Number(13));
  let set = BTreeSet::from([Number(14)]);
  let heap = BinaryHeap::from([15u8, 16u8]);

  assert_eq!(ObjectRef::new(&boxed).field_names(), &["0", "1"]);
  assert_eq!(
    ObjectRef::new(&boxed).item(0).unwrap().to_ref::<u8>(),
    Some(&7)
  );
  assert_eq!(ObjectRef::new(&shared).len(), Some(2));
  assert_eq!(
    ObjectRef::new(&shared).item(1).unwrap().to_ref::<Number>(),
    Some(&Number(9))
  );
  assert_eq!(ObjectRef::new(&atomic).len(), Some(1));
  assert_eq!(
    ObjectRef::new(&atomic).item(0).unwrap().to_ref::<Number>(),
    Some(&Number(10))
  );
  assert_eq!(ObjectRef::new(&deque).len(), Some(2));
  assert_eq!(
    ObjectRef::new(&deque).item(1).unwrap().to_ref::<Number>(),
    Some(&Number(11))
  );
  assert_eq!(ObjectRef::new(&list).len(), Some(2));
  assert_eq!(
    ObjectRef::new(&list).item(0).unwrap().to_ref::<Number>(),
    Some(&Number(12))
  );
  assert_eq!(ObjectRef::new(&set).len(), Some(1));
  assert_eq!(
    ObjectRef::new(&set).item(0).unwrap().to_ref::<Number>(),
    Some(&Number(14))
  );
  assert_eq!(ObjectRef::new(&heap).len(), Some(2));
  assert!(matches!(
    ObjectRef::new(&heap).item(0).unwrap().to_ref::<u8>(),
    Some(15 | 16)
  ));
}

#[test]
fn indexed_key_meta_exposes_access_by_key_and_index() {
  let items = IndexMap::new(vec![
    ("primary".to_owned(), Number(7)),
    ("secondary".to_owned(), Number(9)),
  ]);

  assert_eq!(
    ObjectRef::new(&items).access_kind(),
    Some(AccessKind::ItemKey)
  );
  assert_eq!(ObjectRef::new(&items).len(), Some(2));
  assert_eq!(
    ObjectRef::new(&items).item(1).unwrap().to_ref::<Number>(),
    Some(&Number(9))
  );
  assert_eq!(
    ObjectRef::new(&items)
      .key("primary")
      .unwrap()
      .to_ref::<Number>(),
    Some(&Number(7))
  );
  assert_eq!(
    ObjectRef::new(&items).keys(),
    Some(vec!["primary".to_owned(), "secondary".to_owned()])
  );
  assert!(ObjectRef::new(&items).key("missing").is_none());
  assert!(ObjectRef::new(&items).item(2).is_none());
}

#[test]
fn result_meta_exposes_active_variant() {
  let ok: Result<Number, Text> = Ok(Number(7));
  let err: Result<Number, Text> = Err(Text("boom"));

  assert_eq!(ObjectRef::new(&ok).field_names(), &["Ok"]);
  assert_eq!(ObjectRef::new(&ok).len(), Some(1));
  assert_eq!(
    ObjectRef::new(&ok).item(0).unwrap().to_ref::<Number>(),
    Some(&Number(7))
  );
  assert_eq!(
    ObjectRef::new(&ok).field("Ok").unwrap().to_ref::<Number>(),
    Some(&Number(7))
  );

  assert_eq!(ObjectRef::new(&err).field_names(), &["Err"]);
  assert_eq!(ObjectRef::new(&err).len(), Some(1));
  assert_eq!(
    ObjectRef::new(&err).item(0).unwrap().to_ref::<Text>(),
    Some(&Text("boom"))
  );
  assert_eq!(
    ObjectRef::new(&err).field("Err").unwrap().to_ref::<Text>(),
    Some(&Text("boom"))
  );
}
