use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, LinkedList, VecDeque};
use std::rc::Rc;
use std::sync::Arc;

use typex::{
  AccessKind, Meta, MetaMut, Object, ObjectOps, ObjectRef, ObjectRefMut, PatchOperation,
  PathSegment, TypeInfo, TypedField, TypedMapAccess, TypedMapAccessMut,
};

#[derive(Debug, PartialEq, Meta)]
struct Number(u16);

#[derive(Debug, PartialEq, Meta)]
struct Flag(bool);

#[derive(Meta, MetaMut)]
struct Query;

impl Query {
  fn id(&self) -> &'static str {
    "query"
  }
}

#[derive(Debug, PartialEq, Eq, Meta)]
struct NonCloneMarker;

#[derive(Debug, PartialEq, Meta, MetaMut)]
struct NonCloneRecord {
  count: u8,
}

#[derive(Meta, MetaMut)]
#[typex(opaque)]
struct NonDebugValue(u8);

#[test]
fn derives_do_not_require_debug() {
  let mut value = NonDebugValue(7);
  let object = Object::new(value);

  assert!(object.is::<NonDebugValue>());
  assert!(format!("{object:?}").contains("NonDebugValue"));

  value = object.to::<NonDebugValue>().unwrap();
  ObjectRefMut::new(&mut value)
    .set(Object::new(NonDebugValue(8)))
    .unwrap();
  assert_eq!(value.0, 8);
}

// The closure receives `&&str`; these calls must resolve to inherent `str`
// methods even though the derive traits are in scope.
#[allow(clippy::len_zero)]
#[test]
fn str_methods_resolve_through_references() {
  let query = "a b";

  let non_empty: usize = query
    .split_whitespace()
    .filter(|part| !part.is_empty())
    .count();
  let non_zero: usize = query
    .split_whitespace()
    .filter(|part| part.len() > 0)
    .count();

  assert_eq!(non_empty, 2);
  assert_eq!(non_zero, 2);
}

#[test]
fn smart_pointer_methods_resolve_to_inner_values() {
  let boxed: Box<Vec<u8>> = Box::new(vec![1, 2]);
  let shared: Rc<String> = Rc::new(String::new());
  let atomic: Arc<Vec<u8>> = Arc::new(Vec::new());
  let map: Box<BTreeMap<u8, u8>> = Box::new(BTreeMap::from([(1, 2)]));
  let mut text: Box<String> = Box::new(String::from("a-b"));
  let query: Box<Query> = Box::new(Query);

  let boxed_len: usize = boxed.len();
  let shared_empty: bool = shared.is_empty();
  let atomic_empty: bool = atomic.is_empty();
  let keys: Vec<&u8> = map.keys().collect();
  let replaced: String = text.replace('-', "+");
  text.push('!');
  let id: &str = query.id();

  assert_eq!(boxed_len, 2);
  assert!(shared_empty);
  assert!(atomic_empty);
  assert_eq!(keys, vec![&1]);
  assert_eq!(replaced, "a+b");
  assert_eq!(*text, "a-b!");
  assert_eq!(id, "query");
}

#[test]
fn derive_meta_mut_applies_a_patch_without_a_clone_bound() {
  let mut value = NonCloneRecord { count: 1 };
  let patch = vec![PatchOperation::set(vec![PathSegment::Field("count")], 2_u8)];

  ObjectRefMut::new(&mut value).apply(patch).unwrap();

  assert_eq!(value.count, 2);
}

#[derive(Clone, Debug, PartialEq, Meta, MetaMut)]
struct GenericRecord<T> {
  value: T,
}

#[derive(Clone, Debug, PartialEq, Meta, MetaMut)]
#[typex(opaque)]
struct OpaqueRecord<T>(T);

#[derive(Clone, Debug, PartialEq, Meta, MetaMut)]
#[typex(opaque, partial_eq)]
struct OpaquePartialEqRecord(u8);

#[derive(Debug, Meta)]
#[typex(partial_eq)]
struct StructuralPartialEqRecord {
  value: u8,
}

impl PartialEq for StructuralPartialEqRecord {
  fn eq(&self, other: &Self) -> bool {
    self.value % 2 == other.value % 2
  }
}

#[test]
fn derives_preserve_generics_and_opaque_mode() {
  let mut generic = GenericRecord { value: 3_u8 };
  assert_eq!(
    ObjectRef::new(&generic).field_path(GenericRecord::<u8>::FIELD_VALUE.path()),
    Some(&3)
  );

  *ObjectRefMut::new(&mut generic)
    .field_path_mut(GenericRecord::<u8>::FIELD_VALUE.path())
    .unwrap() = 4;
  assert_eq!(generic.value, 4);

  let opaque = OpaqueRecord(7_u8);
  assert_eq!(ObjectRef::new(&opaque).kind(), typex::ValueKind::Scalar);
  assert!(ObjectRef::new(&opaque).field_names().is_empty());
  assert!(ObjectRef::new(&opaque).item(0).is_none());
}

#[test]
fn opaque_without_eq_is_never_equal_even_to_itself() {
  let opaque = OpaqueRecord(7_u8);
  assert!(!opaque.eq_dyn(&opaque));
}

#[test]
fn opaque_partial_eq_delegates_eq_dyn() {
  let a = OpaquePartialEqRecord(7);
  let b = OpaquePartialEqRecord(7);
  let c = OpaquePartialEqRecord(8);

  assert!(a.eq_dyn(&a));
  assert!(a.eq_dyn(&b));
  assert!(!a.eq_dyn(&c));
  assert!(!a.eq_dyn(&Number(7)));
}

#[test]
fn structural_partial_eq_can_override_structural_equality() {
  let odd = StructuralPartialEqRecord { value: 3 };
  let same_parity = StructuralPartialEqRecord { value: 5 };
  let different_parity = StructuralPartialEqRecord { value: 4 };

  assert_eq!(ObjectRef::new(&odd).field_names(), vec!["value"]);
  assert!(odd.eq_dyn(&same_parity));
  assert!(!odd.eq_dyn(&different_parity));
}

#[derive(Clone, Debug, Meta, MetaMut)]
struct Label {
  name: &'static str,
}

#[derive(Clone, Debug, Meta, MetaMut)]
struct TupleRecord(u8);

#[derive(Clone, Debug, Meta, MetaMut)]
struct UnitRecord;

#[derive(Clone, Debug, PartialEq, Meta, MetaMut)]
enum Status {
  Ready,
  Tuple(u8, bool),
  Struct { count: u8 },
}

#[derive(Clone, Debug, Meta, MetaMut)]
struct Payload {
  count: u8,
  label: &'static str,
  boxed_label: Box<Label>,
  shared_label: Rc<Label>,
  atomic_label: Arc<Label>,
  labels: Vec<Label>,
  queued_labels: VecDeque<Label>,
  listed_labels: LinkedList<Label>,
  set_flags: BTreeSet<u8>,
  heap_scores: BinaryHeap<u8>,
  optional_label: Option<Label>,
  absent_label: Option<Label>,
  result_label: Result<Label, &'static str>,
  pair: (u8, bool),
  bytes: [u8; 2],
  status: Status,
  labels_by_id: HashMap<u8, Label>,
  labels_by_name: HashMap<&'static str, Label>,
}

#[test]
fn derive_meta_supports_object_conversions() {
  let object = Object::new(Number(7));

  assert!(object.is::<Number>());
  assert_eq!(object.type_info(), TypeInfo::of::<Number>());
  assert_eq!(object.to_ref::<Number>(), Some(&Number(7)));

  let converted = object.to::<Number>().unwrap();
  assert_eq!(converted, Number(7));
}

#[test]
fn derive_meta_is_structural_for_tuple_structs_by_default() {
  let value = Number(7);

  assert_eq!(ObjectRef::new(&value).kind(), typex::ValueKind::Struct);
  assert_eq!(ObjectRef::new(&value).len(), Some(1));
  assert_eq!(
    ObjectRef::new(&value).item(0).unwrap().to_ref::<u16>(),
    Some(&7)
  );
}

#[test]
fn derive_meta_preserves_original_object_on_failed_conversion() {
  let object = Object::new(Flag(true));

  let object = object.to::<Number>().unwrap_err();

  assert!(object.is::<Flag>());
  assert_eq!(object.to_ref::<Flag>(), Some(&Flag(true)));
}

#[test]
fn derive_meta_does_not_require_clone_for_basic_usage() {
  let object = Object::new(NonCloneMarker);

  assert!(object.is::<NonCloneMarker>());
  assert_eq!(object.to_ref::<NonCloneMarker>(), Some(&NonCloneMarker));
  assert_eq!(object.to::<NonCloneMarker>().unwrap(), NonCloneMarker);
}

#[test]
fn derive_meta_mut_does_not_require_clone_for_basic_usage() {
  let record = NonCloneRecord { count: 4 };

  assert_eq!(ObjectRef::new(&record).field_names(), &["count"]);
  assert_eq!(
    ObjectRef::new(&record)
      .field("count")
      .unwrap()
      .to_ref::<u8>(),
    Some(&4)
  );
}

#[test]
fn derive_meta_exposes_struct_fields() {
  let payload = Payload {
    count: 3,
    label: "ready",
    boxed_label: Box::new(Label { name: "boxed" }),
    shared_label: Rc::new(Label { name: "shared" }),
    atomic_label: Arc::new(Label { name: "atomic" }),
    labels: vec![Label { name: "primary" }],
    queued_labels: VecDeque::from([Label { name: "queued" }]),
    listed_labels: LinkedList::from([Label { name: "listed" }]),
    set_flags: BTreeSet::from([2]),
    heap_scores: BinaryHeap::from([4, 9]),
    optional_label: Some(Label { name: "optional" }),
    absent_label: None,
    result_label: Ok(Label { name: "result" }),
    pair: (8, false),
    bytes: [0x0a, 0xff],
    status: Status::Struct { count: 5 },
    labels_by_id: HashMap::from([(7, Label { name: "indexed" })]),
    labels_by_name: HashMap::from([("primary", Label { name: "mapped" })]),
  };

  assert_eq!(
    ObjectRef::new(&payload).field_names(),
    &[
      "count",
      "label",
      "boxed_label",
      "shared_label",
      "atomic_label",
      "labels",
      "queued_labels",
      "listed_labels",
      "set_flags",
      "heap_scores",
      "optional_label",
      "absent_label",
      "result_label",
      "pair",
      "bytes",
      "status",
      "labels_by_id",
      "labels_by_name",
    ]
  );
  assert_eq!(
    ObjectRef::new(&payload).access_kind(),
    Some(AccessKind::Field)
  );
  assert_eq!(
    ObjectRef::new(&payload)
      .field("count")
      .unwrap()
      .to_ref::<u8>(),
    Some(&3)
  );
  assert_eq!(
    ObjectRef::new(&payload)
      .field("label")
      .unwrap()
      .to_ref::<&'static str>(),
    Some(&"ready")
  );
  assert_eq!(
    ObjectRef::new(&payload)
      .field("boxed_label")
      .unwrap()
      .field("name")
      .unwrap()
      .to_ref::<&'static str>(),
    Some(&"boxed")
  );
  assert_eq!(
    ObjectRef::new(&payload)
      .field("shared_label")
      .unwrap()
      .field("name")
      .unwrap()
      .to_ref::<&'static str>(),
    Some(&"shared")
  );
  assert_eq!(
    ObjectRef::new(&payload)
      .field("atomic_label")
      .unwrap()
      .field("name")
      .unwrap()
      .to_ref::<&'static str>(),
    Some(&"atomic")
  );
  let labels = ObjectRef::new(&payload).field("labels").unwrap();
  assert_eq!(labels.len(), Some(1));
  assert_eq!(labels.item(0).unwrap().field_names(), &["name"]);
  let labels: &dyn Meta = &payload.labels;
  assert_eq!(labels.is_empty(), Some(false));
  assert_eq!(
    ObjectRef::new(&payload)
      .field("queued_labels")
      .unwrap()
      .item(0)
      .unwrap()
      .field("name")
      .unwrap()
      .to_ref::<&'static str>(),
    Some(&"queued")
  );
  assert_eq!(
    ObjectRef::new(&payload)
      .field("listed_labels")
      .unwrap()
      .item(0)
      .unwrap()
      .field("name")
      .unwrap()
      .to_ref::<&'static str>(),
    Some(&"listed")
  );
  assert_eq!(
    ObjectRef::new(&payload)
      .field("set_flags")
      .unwrap()
      .item(0)
      .unwrap()
      .to_ref::<u8>(),
    Some(&2)
  );
  let heap_scores = ObjectRef::new(&payload).field("heap_scores").unwrap();
  assert_eq!(heap_scores.len(), Some(2));
  assert!(matches!(
    heap_scores.item(0).unwrap().to_ref::<u8>(),
    Some(4 | 9)
  ));
  assert_eq!(
    ObjectRef::new(&payload)
      .field("optional_label")
      .unwrap()
      .field("name")
      .unwrap()
      .to_ref::<&'static str>(),
    Some(&"optional")
  );
  assert_eq!(
    ObjectRef::new(&payload)
      .field("optional_label")
      .unwrap()
      .len(),
    Some(1)
  );
  assert_eq!(
    ObjectRef::new(&payload)
      .field("absent_label")
      .unwrap()
      .len(),
    Some(0)
  );
  assert!(
    ObjectRef::new(&payload)
      .field("absent_label")
      .unwrap()
      .field("name")
      .is_none()
  );
  assert_eq!(
    ObjectRef::new(&payload)
      .field("result_label")
      .unwrap()
      .field("Ok")
      .unwrap()
      .field("name")
      .unwrap()
      .to_ref::<&'static str>(),
    Some(&"result")
  );
  assert_eq!(
    ObjectRef::new(&payload).field("pair").unwrap().len(),
    Some(2)
  );
  assert_eq!(
    ObjectRef::new(&payload)
      .field("pair")
      .unwrap()
      .item(0)
      .unwrap()
      .to_ref::<u8>(),
    Some(&8)
  );
  assert_eq!(
    ObjectRef::new(&payload)
      .field("bytes")
      .unwrap()
      .item(1)
      .unwrap()
      .to_ref::<u8>(),
    Some(&0xff)
  );

  let labels_by_name = ObjectRef::new(&payload).field("labels_by_name").unwrap();
  assert_eq!(labels_by_name.keys(), Some(vec!["primary".to_string()]));
  assert_eq!(
    labels_by_name
      .key("primary")
      .unwrap()
      .field("name")
      .unwrap()
      .to_ref::<&'static str>(),
    Some(&"mapped")
  );
  assert_eq!(
    payload
      .labels_by_id
      .key_typed(&7)
      .unwrap()
      .field("name")
      .unwrap()
      .to_ref::<&'static str>(),
    Some(&"indexed")
  );
  assert!(
    ObjectRef::new(&payload)
      .field_path(&[
        PathSegment::Field("labels_by_id"),
        PathSegment::Key("7"),
        PathSegment::Field("name"),
      ])
      .is_none()
  );
  let status = ObjectRef::new(&payload).field("status").unwrap();
  assert_eq!(status.field_names(), &["Struct", "count"]);
  assert_eq!(
    status
      .field("Struct")
      .unwrap()
      .field("count")
      .unwrap()
      .to_ref::<u8>(),
    Some(&5)
  );
  assert_eq!(
    ObjectRef::new(&payload)
      .field_path(&[
        PathSegment::Field("labels"),
        PathSegment::Item(0),
        PathSegment::Field("name"),
      ])
      .unwrap()
      .to_ref::<&'static str>(),
    Some(&"primary")
  );
  assert_eq!(
    ObjectRef::new(&payload)
      .field_path(&[
        PathSegment::Field("labels_by_name"),
        PathSegment::Key("primary"),
        PathSegment::Field("name"),
      ])
      .unwrap()
      .to_ref::<&'static str>(),
    Some(&"mapped")
  );
  assert_eq!(
    ObjectRef::new(&payload)
      .field_path(&[
        PathSegment::Field("optional_label"),
        PathSegment::Field("name"),
      ])
      .unwrap()
      .to_ref::<&'static str>(),
    Some(&"optional")
  );
  assert_eq!(
    ObjectRef::new(&payload)
      .field_path(&[PathSegment::Field("pair"), PathSegment::Item(1)])
      .unwrap()
      .to_ref::<bool>(),
    Some(&false)
  );
  assert_eq!(
    ObjectRef::new(&payload)
      .field_path(&[
        PathSegment::Field("status"),
        PathSegment::Field("Struct"),
        PathSegment::Field("count"),
      ])
      .unwrap()
      .to_ref::<u8>(),
    Some(&5)
  );
  assert!(ObjectRef::new(&payload).field("missing").is_none());
}

#[derive(Clone, Debug, Meta, MetaMut)]
struct TypedPayload {
  label: Label,
  labels: Vec<Label>,
  bytes: [u8; 2],
}

#[derive(Clone, Debug, Meta, MetaMut)]
struct TypedSequencePayload {
  deque: VecDeque<Label>,
  list: LinkedList<Label>,
}

#[test]
fn derive_meta_generates_typed_paths() {
  let mut payload = TypedPayload {
    label: Label { name: "direct" },
    labels: vec![Label { name: "nested" }],
    bytes: [0x0a, 0xff],
  };

  let nested_name = TypedPayload::FIELD_LABELS.item(0).then(Label::FIELD_NAME);
  assert_eq!(
    nested_name.segments(),
    &[
      PathSegment::Field("labels"),
      PathSegment::Item(0),
      PathSegment::Field("name"),
    ]
  );

  assert_eq!(
    ObjectRef::new(&payload).field_path(TypedPayload::FIELD_LABELS.item(0).then(Label::FIELD_NAME)),
    Some(&"nested")
  );

  let mut root = ObjectRefMut::new(&mut payload);
  root
    .set_field_path(TypedPayload::FIELD_LABEL.path(), Label { name: "direct2" })
    .unwrap();
  root.set_field_path(nested_name, "nested2").unwrap();
  root
    .set_field_path(TypedPayload::FIELD_BYTES.item(1), 0x0b_u8)
    .unwrap();
  *root
    .field_path_mut(TypedPayload::FIELD_LABELS.item(0).then(Label::FIELD_NAME))
    .unwrap() = "nested3";

  assert_eq!(payload.label.name, "direct2");
  assert_eq!(payload.labels[0].name, "nested3");
  assert_eq!(payload.bytes[1], 0x0b);
}

#[derive(Clone, Debug, Meta, MetaMut)]
struct TypedMapPayload {
  owned: BTreeMap<String, Label>,
  borrowed: HashMap<&'static str, u8>,
}

#[test]
fn derive_meta_generates_typed_paths_for_supported_maps() {
  let mut payload = TypedMapPayload {
    owned: BTreeMap::from([(String::from("primary"), Label { name: "first" })]),
    borrowed: HashMap::from([("count", 1)]),
  };

  let key = String::from("primary");
  let name = TypedMapPayload::FIELD_OWNED
    .key(&key)
    .then(Label::FIELD_NAME);
  assert_eq!(
    name.segments(),
    &[
      PathSegment::Field("owned"),
      PathSegment::Key("primary"),
      PathSegment::Field("name"),
    ]
  );
  assert_eq!(
    ObjectRef::new(&payload).field_path(
      TypedMapPayload::FIELD_OWNED
        .key("primary")
        .then(Label::FIELD_NAME)
    ),
    Some(&"first")
  );
  assert_eq!(
    ObjectRef::new(&payload).field_path(TypedMapPayload::FIELD_BORROWED.key("count")),
    Some(&1)
  );
  assert_eq!(
    ObjectRef::new(&payload).field_path(TypedMapPayload::FIELD_BORROWED.key("missing")),
    None
  );

  *ObjectRefMut::new(&mut payload)
    .field_path_mut(name)
    .unwrap() = "updated";
  let mut root = ObjectRefMut::new(&mut payload);
  root
    .set_field_path(TypedMapPayload::FIELD_BORROWED.key("count"), 2)
    .unwrap();

  assert_eq!(payload.owned["primary"].name, "updated");
  assert_eq!(payload.borrowed["count"], 2);
}

#[test]
fn derive_meta_generates_typed_paths_for_supported_sequences() {
  let mut payload = TypedSequencePayload {
    deque: VecDeque::from([Label { name: "deque-0" }, Label { name: "deque-1" }]),
    list: LinkedList::from([Label { name: "list-0" }]),
  };

  assert_eq!(
    ObjectRef::new(&payload).field_path(
      TypedSequencePayload::FIELD_DEQUE
        .item(1)
        .then(Label::FIELD_NAME)
    ),
    Some(&"deque-1")
  );
  assert_eq!(
    ObjectRef::new(&payload).field_path(
      TypedSequencePayload::FIELD_LIST
        .item(0)
        .then(Label::FIELD_NAME)
    ),
    Some(&"list-0")
  );

  *ObjectRefMut::new(&mut payload)
    .field_path_mut(
      TypedSequencePayload::FIELD_DEQUE
        .item(0)
        .then(Label::FIELD_NAME),
    )
    .unwrap() = "deque-updated";
  *ObjectRefMut::new(&mut payload)
    .field_path_mut(
      TypedSequencePayload::FIELD_LIST
        .item(0)
        .then(Label::FIELD_NAME),
    )
    .unwrap() = "list-updated";

  assert_eq!(payload.deque[0].name, "deque-updated");
  assert_eq!(
    payload.list.front().map(|label| label.name),
    Some("list-updated")
  );
}

#[test]
fn derive_meta_generates_typed_enum_paths() {
  let mut value = Status::Struct { count: 3 };

  assert_eq!(
    Status::FIELD_STRUCT_COUNT.path().segments(),
    &[PathSegment::Field("Struct"), PathSegment::Field("count")]
  );
  assert_eq!(
    ObjectRef::new(&value).field_path(Status::FIELD_STRUCT_COUNT.path()),
    Some(&3)
  );

  *ObjectRefMut::new(&mut value)
    .field_path_mut(Status::FIELD_STRUCT_COUNT.path())
    .unwrap() = 4;

  assert_eq!(value, Status::Struct { count: 4 });

  let tuple = Status::Tuple(9, true);
  assert_eq!(
    ObjectRef::new(&tuple).field_path(Status::FIELD_TUPLE_0.path()),
    Some(&9)
  );
  assert_eq!(
    ObjectRef::new(&tuple).field_path(Status::FIELD_TUPLE_1.path()),
    Some(&true)
  );
  assert_eq!(
    ObjectRef::new(&Status::Ready).field_path(Status::FIELD_READY.path()),
    Some(&Status::Ready)
  );
}

#[test]
fn set_field_path_reports_path_and_runtime_type_errors() {
  let mut payload = TypedPayload {
    label: Label { name: "direct" },
    labels: vec![Label { name: "nested" }],
    bytes: [0x0a, 0xff],
  };
  let mut root = ObjectRefMut::new(&mut payload);

  assert_eq!(
    root.set_field_path(
      TypedPayload::FIELD_LABELS.item(1),
      Label { name: "missing" }
    ),
    Err(typex::ReflectiveError::PathNotFound)
  );

  let mismatched = TypedField::<TypedPayload, u16>::new("label").path();
  assert_eq!(
    root.set_field_path(mismatched, 7_u16),
    Err(typex::ReflectiveError::MutationTypeMismatch)
  );
}

#[test]
fn derive_meta_mut_exposes_and_mutates_struct_fields() {
  let mut payload = Payload {
    count: 3,
    label: "ready",
    boxed_label: Box::new(Label { name: "boxed" }),
    shared_label: Rc::new(Label { name: "shared" }),
    atomic_label: Arc::new(Label { name: "atomic" }),
    labels: vec![Label { name: "primary" }],
    queued_labels: VecDeque::from([Label { name: "queued" }]),
    listed_labels: LinkedList::from([Label { name: "listed" }]),
    set_flags: BTreeSet::from([2]),
    heap_scores: BinaryHeap::from([4, 9]),
    optional_label: Some(Label { name: "optional" }),
    absent_label: None,
    result_label: Ok(Label { name: "result" }),
    pair: (8, false),
    bytes: [0x0a, 0xff],
    status: Status::Struct { count: 5 },
    labels_by_id: HashMap::from([(7, Label { name: "indexed" })]),
    labels_by_name: HashMap::from([("primary", Label { name: "mapped" })]),
  };

  // Scalar field
  *ObjectRefMut::new(&mut payload)
    .field_mut("count")
    .unwrap()
    .to_mut::<u8>()
    .unwrap() = 30;
  assert_eq!(payload.count, 30);

  // Uniquely-owned Box/Rc/Arc fields forward field_mut to the inner value
  *ObjectRefMut::new(&mut payload)
    .field_mut("boxed_label")
    .unwrap()
    .field_mut("name")
    .unwrap()
    .to_mut::<&'static str>()
    .unwrap() = "boxed2";
  assert_eq!(payload.boxed_label.name, "boxed2");

  *ObjectRefMut::new(&mut payload)
    .field_mut("shared_label")
    .unwrap()
    .field_mut("name")
    .unwrap()
    .to_mut::<&'static str>()
    .unwrap() = "shared2";
  assert_eq!(payload.shared_label.name, "shared2");

  // List item mutation
  *ObjectRefMut::new(&mut payload)
    .field_mut("labels")
    .unwrap()
    .item_mut(0)
    .unwrap()
    .field_mut("name")
    .unwrap()
    .to_mut::<&'static str>()
    .unwrap() = "primary2";
  assert_eq!(payload.labels[0].name, "primary2");

  // BTreeSet/BinaryHeap fields have no structural item_mut, but their whole
  // value is still reachable and mutable through their own API
  assert!(
    ObjectRefMut::new(&mut payload)
      .field_mut("set_flags")
      .unwrap()
      .item_mut(0)
      .ok()
      .is_none()
  );
  ObjectRefMut::new(&mut payload)
    .field_mut("set_flags")
    .unwrap()
    .to_mut::<BTreeSet<u8>>()
    .unwrap()
    .insert(9);
  assert!(payload.set_flags.contains(&9));

  // Option<T>: item_mut(0) on Some, None on absent
  *ObjectRefMut::new(&mut payload)
    .field_mut("optional_label")
    .unwrap()
    .item_mut(0)
    .unwrap()
    .field_mut("name")
    .unwrap()
    .to_mut::<&'static str>()
    .unwrap() = "optional2";
  assert_eq!(payload.optional_label.as_ref().unwrap().name, "optional2");
  assert!(
    ObjectRefMut::new(&mut payload)
      .field_mut("absent_label")
      .unwrap()
      .item_mut(0)
      .ok()
      .is_none()
  );

  // Result<T, E>: field_mut("Ok") on the active variant, None for "Err"
  assert!(
    ObjectRefMut::new(&mut payload)
      .field_mut("result_label")
      .unwrap()
      .field_mut("Err")
      .ok()
      .is_none()
  );
  *ObjectRefMut::new(&mut payload)
    .field_mut("result_label")
    .unwrap()
    .field_mut("Ok")
    .unwrap()
    .field_mut("name")
    .unwrap()
    .to_mut::<&'static str>()
    .unwrap() = "result2";
  assert_eq!(payload.result_label.as_ref().unwrap().name, "result2");

  // Tuple and array item mutation
  *ObjectRefMut::new(&mut payload)
    .field_mut("pair")
    .unwrap()
    .item_mut(0)
    .unwrap()
    .to_mut::<u8>()
    .unwrap() = 80;
  assert_eq!(payload.pair.0, 80);

  *ObjectRefMut::new(&mut payload)
    .field_mut("bytes")
    .unwrap()
    .item_mut(1)
    .unwrap()
    .to_mut::<u8>()
    .unwrap() = 0x0b;
  assert_eq!(payload.bytes[1], 0x0b);

  // Enum struct-variant field mutation, one level deep
  *ObjectRefMut::new(&mut payload)
    .field_mut("status")
    .unwrap()
    .field_mut("Struct")
    .unwrap()
    .field_mut("count")
    .unwrap()
    .to_mut::<u8>()
    .unwrap() = 50;
  assert_eq!(
    ObjectRef::new(&payload.status)
      .field("Struct")
      .unwrap()
      .field("count")
      .unwrap()
      .to_ref::<u8>(),
    Some(&50)
  );

  // String-keyed and typed-keyed map mutation
  *ObjectRefMut::new(&mut payload)
    .field_mut("labels_by_name")
    .unwrap()
    .key_mut("primary")
    .unwrap()
    .field_mut("name")
    .unwrap()
    .to_mut::<&'static str>()
    .unwrap() = "mapped2";
  assert_eq!(
    payload.labels_by_name.get("primary").unwrap().name,
    "mapped2"
  );

  *TypedMapAccessMut::key_typed_mut(&mut payload.labels_by_id, &7)
    .unwrap()
    .field_mut("name")
    .unwrap()
    .to_mut::<&'static str>()
    .unwrap() = "indexed2";
  assert_eq!(payload.labels_by_id.get(&7).unwrap().name, "indexed2");

  // field_path_mut walks several hops in one call
  *ObjectRefMut::new(&mut payload)
    .field_path_mut(&[
      PathSegment::Field("labels"),
      PathSegment::Item(0),
      PathSegment::Field("name"),
    ])
    .unwrap()
    .to_mut::<&'static str>()
    .unwrap() = "primary3";
  assert_eq!(payload.labels[0].name, "primary3");
}

#[test]
fn derive_meta_mut_supports_tuple_and_unit_enum_variants() {
  let mut tuple = Status::Tuple(9, true);
  *ObjectRefMut::new(&mut tuple)
    .item_mut(0)
    .unwrap()
    .to_mut::<u8>()
    .unwrap() = 90;
  assert_eq!(
    ObjectRef::new(&tuple).item(0).unwrap().to_ref::<u8>(),
    Some(&90)
  );
  *ObjectRefMut::new(&mut tuple)
    .field_mut("Tuple")
    .unwrap()
    .item_mut(1)
    .unwrap()
    .to_mut::<bool>()
    .unwrap() = false;
  assert_eq!(
    ObjectRef::new(&tuple).item(1).unwrap().to_ref::<bool>(),
    Some(&false)
  );

  let mut ready = Status::Ready;
  assert!(
    ObjectRefMut::new(&mut ready)
      .field_mut("Ready")
      .unwrap()
      .field_mut("missing")
      .ok()
      .is_none()
  );
}

#[test]
fn derive_meta_supports_tuple_and_unit_enum_variants() {
  let tuple = Status::Tuple(9, true);
  assert_eq!(
    ObjectRef::new(&tuple).access_kind(),
    Some(AccessKind::Field)
  );
  assert_eq!(ObjectRef::new(&tuple).field_names(), &["Tuple", "0", "1"]);
  assert_eq!(ObjectRef::new(&tuple).len(), Some(2));
  assert_eq!(
    ObjectRef::new(&tuple).item(0).unwrap().to_ref::<u8>(),
    Some(&9)
  );
  assert_eq!(
    ObjectRef::new(&tuple)
      .field("Tuple")
      .unwrap()
      .item(1)
      .unwrap()
      .to_ref::<bool>(),
    Some(&true)
  );

  let ready = Status::Ready;
  assert_eq!(
    ObjectRef::new(&ready).access_kind(),
    Some(AccessKind::Field)
  );
  assert_eq!(ObjectRef::new(&ready).field_names(), &["Ready"]);
  assert!(
    ObjectRef::new(&ready)
      .field("Ready")
      .unwrap()
      .field("missing")
      .is_none()
  );
}

#[test]
fn derive_meta_reports_access_kind_for_struct_shapes() {
  let tuple = TupleRecord(7);
  assert_eq!(
    ObjectRef::new(&tuple).access_kind(),
    Some(AccessKind::Field)
  );

  let unit = UnitRecord;
  assert_eq!(ObjectRef::new(&unit).access_kind(), None);
}

// Deliberately not `Clone`: Owned exact-type conversion still works through
// `ObjectOps::to`, which consumes the value instead of cloning it.
#[derive(Debug, PartialEq, Meta)]
#[typex(opaque)]
struct NonCloneOpaqueId(u32);

#[test]
fn owned_object_to_downcasts_non_clone_opaque_values() {
  // Owned exact-type conversion works through `ObjectOps::to`, which
  // consumes the value instead of cloning it.
  let id = NonCloneOpaqueId(42);

  assert_eq!(
    Object::new(id).to::<NonCloneOpaqueId>(),
    Ok(NonCloneOpaqueId(42))
  );

  let returned = Object::new(Number(42))
    .to::<NonCloneOpaqueId>()
    .unwrap_err();
  assert_eq!(returned.to_ref::<Number>(), Some(&Number(42)));
}

#[derive(Debug, Meta, MetaMut)]
enum Shape {
  Circle(u8),
  Rect { width: u8, height: u8 },
  Empty,
}

#[test]
fn derived_enums_compare_structurally() {
  assert!(Shape::Empty.eq_dyn(&Shape::Empty));
  assert!(Shape::Circle(1).eq_dyn(&Shape::Circle(1)));
  assert!(!Shape::Circle(1).eq_dyn(&Shape::Circle(2)));
  assert!(!Shape::Circle(1).eq_dyn(&Shape::Empty));
  assert!(
    Shape::Rect {
      width: 1,
      height: 2
    }
    .eq_dyn(&Shape::Rect {
      width: 1,
      height: 2
    })
  );
  assert_eq!(Object::new(Shape::Empty), Object::new(Shape::Empty));
}

#[derive(Debug, Meta)]
struct Labelled {
  label: &'static str,
  values: Vec<u8>,
}

#[derive(Debug, Meta)]
struct Point(u8, u8);

#[derive(Debug, Meta)]
struct Marker;

#[derive(Debug, Meta)]
enum Only {
  Value(u8),
}

#[test]
fn derived_structs_compare_fields_structurally() {
  let labelled = Labelled {
    label: "a",
    values: vec![1, 2],
  };
  assert!(labelled.eq_dyn(&Labelled {
    label: "a",
    values: vec![1, 2],
  }));
  assert!(!labelled.eq_dyn(&Labelled {
    label: "b",
    values: vec![1, 2],
  }));
  assert!(!labelled.eq_dyn(&Labelled {
    label: "a",
    values: vec![1],
  }));
  assert!(!labelled.eq_dyn(&Point(1, 2)));

  assert!(Point(1, 2).eq_dyn(&Point(1, 2)));
  assert!(!Point(1, 2).eq_dyn(&Point(2, 1)));
  assert!(Marker.eq_dyn(&Marker));

  assert!(Only::Value(1).eq_dyn(&Only::Value(1)));
  assert!(!Only::Value(1).eq_dyn(&Only::Value(2)));
}

#[derive(Debug, Meta, MetaMut)]
struct Keyword {
  r#type: u8,
}

#[derive(Debug, Meta)]
enum RawVariant {
  r#Loop { r#in: u8 },
}

#[test]
fn raw_identifiers_use_unprefixed_names() {
  let mut keyword = Keyword { r#type: 1 };

  assert_eq!(ObjectRef::new(&keyword).field_names(), &["type"]);
  assert_eq!(
    ObjectRef::new(&keyword).field_path(Keyword::FIELD_TYPE.path()),
    Some(&1)
  );
  *ObjectRefMut::new(&mut keyword)
    .field_mut("type")
    .unwrap()
    .to_mut::<u8>()
    .unwrap() = 2;
  assert_eq!(keyword.r#type, 2);

  let value = RawVariant::r#Loop { r#in: 3 };
  assert_eq!(ObjectRef::new(&value).field_names(), &["Loop", "in"]);
  assert_eq!(
    ObjectRef::new(&value).field_path(RawVariant::FIELD_LOOP_IN.path()),
    Some(&3)
  );
}

#[derive(Debug, Meta, MetaMut)]
struct Node {
  value: u8,
  children: Vec<Node>,
}

#[derive(Debug, Meta, MetaMut)]
struct Tree<T> {
  value: T,
  children: Vec<Tree<T>>,
}

#[derive(Debug, Meta, MetaMut)]
struct RecursiveGeneric<T> {
  next: Option<(Box<RecursiveGeneric<T>>, T)>,
}

struct NotMeta;

#[derive(Meta, MetaMut)]
struct RecursiveOpaqueGeneric<T> {
  next: Option<(Box<RecursiveOpaqueGeneric<T>>, OpaqueRecord<T>)>,
}

#[derive(Debug, Meta, MetaMut)]
struct Linked {
  next: Option<Box<Self>>,
}

#[test]
fn recursive_types_derive() {
  let node = Node {
    value: 1,
    children: vec![Node {
      value: 2,
      children: Vec::new(),
    }],
  };
  assert_eq!(
    ObjectRef::new(&node).field_path(&[
      PathSegment::Field("children"),
      PathSegment::Item(0),
      PathSegment::Field("value"),
    ]),
    Some(ObjectRef::new(&2_u8))
  );

  let tree = Tree {
    value: "root",
    children: vec![Tree {
      value: "leaf",
      children: Vec::new(),
    }],
  };
  assert!(tree.eq_dyn(&tree));
  assert_eq!(
    ObjectRef::new(&tree).field_path(Tree::<&str>::FIELD_CHILDREN.item(0).then(Tree::FIELD_VALUE)),
    Some(&"leaf")
  );

  let linked = Linked {
    next: Some(Box::new(Linked { next: None })),
  };
  assert!(linked.eq_dyn(&linked));
}

fn assert_meta_mut<T: MetaMut>() {}

// Both tests pass by compiling. `RecursiveGeneric` needs a bound on the `T`
// beside its recursive reference, while `RecursiveOpaqueGeneric` must not bound
// the `T` inside an opaque type.
#[test]
fn recursive_fields_bound_neighbouring_generic_values() {
  assert_meta_mut::<RecursiveGeneric<u8>>();
}

#[test]
fn recursive_fields_keep_opaque_generic_children_unbounded() {
  assert_meta_mut::<RecursiveOpaqueGeneric<NotMeta>>();
}

#[derive(Debug, Meta, MetaMut)]
struct GenericMap<K, V> {
  entries: HashMap<K, V>,
}

#[test]
fn generic_field_types_get_their_own_bounds() {
  let value = GenericMap {
    entries: HashMap::from([(String::from("a"), 1_u8)]),
  };

  assert_eq!(
    ObjectRef::new(&value).field_path(&[PathSegment::Field("entries"), PathSegment::Key("a")]),
    Some(ObjectRef::new(&1_u8))
  );
}

mod other_item {
  use typex::{Meta, MetaMut};

  #[derive(Debug, Meta, MetaMut)]
  pub(super) struct Item<T> {
    pub(super) value: T,
  }
}

#[derive(Debug, Meta, MetaMut)]
struct Item<T> {
  child: other_item::Item<T>,
}

#[test]
fn generic_bounds_distinguish_same_named_types() {
  let mut value = Item {
    child: other_item::Item { value: 1_u8 },
  };

  assert_eq!(
    ObjectRef::new(&value).field_path(&[PathSegment::Field("child"), PathSegment::Field("value")]),
    Some(ObjectRef::new(&1_u8))
  );
  *ObjectRefMut::new(&mut value)
    .field_mut("child")
    .unwrap()
    .field_mut("value")
    .unwrap()
    .to_mut::<u8>()
    .unwrap() = 2;
  assert_eq!(value.child.value, 2);
}

mod shadowed_names {
  #![allow(dead_code)]

  use typex::{Meta, MetaMut, ObjectRef, ObjectRefMut};

  type Result<T> = ::core::result::Result<T, ()>;
  type Option = ();
  struct Box;
  mod core {}

  #[derive(Debug, Meta, MetaMut)]
  pub(super) struct Record {
    pub(super) name: u8,
    pub(super) index: u8,
  }

  #[derive(Debug, Meta, MetaMut)]
  pub(super) enum Captures {
    Named { name: u8, index: u8, value: u8 },
    Tuple(u8, u8),
  }

  #[test]
  fn generated_code_ignores_shadowed_names() {
    let record = Record { name: 1, index: 2 };
    assert_eq!(
      ObjectRef::new(&record).field("index"),
      Some(ObjectRef::new(&2_u8))
    );

    let mut captures = Captures::Named {
      name: 3,
      index: 4,
      value: 5,
    };
    assert_eq!(
      ObjectRef::new(&captures).field("name"),
      Some(ObjectRef::new(&3_u8))
    );
    assert_eq!(
      ObjectRef::new(&captures).field("value"),
      Some(ObjectRef::new(&5_u8))
    );
    *ObjectRefMut::new(&mut captures)
      .field_mut("index")
      .unwrap()
      .to_mut::<u8>()
      .unwrap() = 6;
    assert!(matches!(captures, Captures::Named { index: 6, .. }));
    assert_eq!(
      ObjectRef::new(&Captures::Tuple(7, 8)).item(1),
      Some(ObjectRef::new(&8_u8))
    );
  }
}

#[derive(Debug, Meta, MetaMut)]
enum Never {}

#[test]
fn empty_enums_derive() {
  assert_eq!(
    TypeInfo::of::<Never>().type_name(),
    core::any::type_name::<Never>()
  );
}
