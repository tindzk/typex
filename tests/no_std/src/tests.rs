extern crate std;

use alloc::collections::BTreeMap;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use typex::{
  FieldPath, Meta, Object, ObjectOps, ObjectRefMut, PathSegment, SendObject, TypedMapAccess,
};
use typex_derive::{Meta, MetaMut};

/// Reflective trait-object view of a concrete test value.
trait DynMeta {
  fn dyn_meta(&self) -> &dyn Meta;
}

impl<T: Meta> DynMeta for T {
  fn dyn_meta(&self) -> &dyn Meta {
    self
  }
}

#[derive(Debug, PartialEq, Meta)]
#[typex(opaque)]
struct Number(u16);

#[derive(Clone, Debug, PartialEq, Meta)]
#[typex(opaque)]
struct Flag(bool);

#[derive(Clone, Debug, Meta, MetaMut)]
struct Label {
  name: &'static str,
}

#[derive(Clone, Debug, Meta, MetaMut)]
enum Status {
  Ready,
  Tuple(u8, bool),
  Struct { count: usize },
}

#[derive(Clone, Debug, Meta, MetaMut)]
struct Payload {
  values: usize,
  label: &'static str,
  labels: Vec<Label>,
  optional_label: Option<Label>,
  absent_label: Option<Label>,
  pair: (usize, bool),
  bytes: [u8; 2],
  status: Status,
  labels_by_id: BTreeMap<usize, Label>,
  labels_by_name: BTreeMap<&'static str, Label>,
}

#[test]
fn meta_smoke_test() {
  let object = Object::new(Number(7));
  assert_eq!(object.to_ref::<Number>().unwrap().0, 7);

  let payload = Payload {
    values: 2,
    label: "ok",
    labels: vec![Label { name: "primary" }],
    optional_label: Some(Label { name: "optional" }),
    absent_label: None,
    pair: (4, false),
    bytes: [0x0a, 0xff],
    status: Status::Struct { count: 6 },
    labels_by_id: BTreeMap::from([(7, Label { name: "indexed" })]),
    labels_by_name: BTreeMap::from([("primary", Label { name: "mapped" })]),
  };
  assert_eq!(
    payload.dyn_meta().field_names(),
    &[
      "values",
      "label",
      "labels",
      "optional_label",
      "absent_label",
      "pair",
      "bytes",
      "status",
      "labels_by_id",
      "labels_by_name",
    ]
  );
  assert_eq!(
    payload
      .dyn_meta()
      .field("values")
      .unwrap()
      .to_ref::<usize>(),
    Some(&2)
  );
  assert_eq!(
    payload
      .dyn_meta()
      .field("label")
      .unwrap()
      .to_ref::<&'static str>(),
    Some(&"ok")
  );
  let labels = payload.dyn_meta().field("labels").unwrap();
  assert_eq!(labels.len(), Some(1));
  assert_eq!(labels.item(0).unwrap().field_names(), &["name"]);
  assert_eq!(
    payload
      .dyn_meta()
      .field("optional_label")
      .unwrap()
      .field("name")
      .unwrap()
      .to_ref::<&'static str>(),
    Some(&"optional")
  );
  assert_eq!(
    payload.dyn_meta().field("optional_label").unwrap().len(),
    Some(1)
  );
  assert_eq!(
    payload.dyn_meta().field("absent_label").unwrap().len(),
    Some(0)
  );
  assert!(
    payload
      .dyn_meta()
      .field("absent_label")
      .unwrap()
      .field("name")
      .is_none()
  );
  assert!(
    payload
      .dyn_meta()
      .field("absent_label")
      .unwrap()
      .item(0)
      .is_none()
  );
  assert_eq!(
    payload
      .dyn_meta()
      .field("pair")
      .unwrap()
      .item(0)
      .unwrap()
      .to_ref::<usize>(),
    Some(&4)
  );
  assert_eq!(
    payload
      .dyn_meta()
      .field("bytes")
      .unwrap()
      .item(1)
      .unwrap()
      .to_ref::<u8>(),
    Some(&0xff)
  );

  let labels_by_name = payload.dyn_meta().field("labels_by_name").unwrap();
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
  let status = payload.dyn_meta().field("status").unwrap();
  assert_eq!(status.field_names(), &["Struct", "count"]);
  assert_eq!(
    status
      .field("Struct")
      .unwrap()
      .field("count")
      .unwrap()
      .to_ref::<usize>(),
    Some(&6)
  );
  assert_eq!(
    payload
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
    payload
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
    payload
      .field_path(&[
        PathSegment::Field("optional_label"),
        PathSegment::Field("name"),
      ])
      .unwrap()
      .to_ref::<&'static str>(),
    Some(&"optional")
  );
  assert_eq!(
    payload
      .field_path(&[PathSegment::Field("pair"), PathSegment::Item(1)])
      .unwrap()
      .to_ref::<bool>(),
    Some(&false)
  );
  assert_eq!(
    payload
      .field_path(&[
        PathSegment::Field("status"),
        PathSegment::Field("Struct"),
        PathSegment::Field("count"),
      ])
      .unwrap()
      .to_ref::<usize>(),
    Some(&6)
  );
}

#[test]
fn tuple_and_unit_enum_variants_are_accessible() {
  let tuple = Status::Tuple(9, true);
  assert_eq!(tuple.dyn_meta().field_names(), &["Tuple", "0", "1"]);
  assert_eq!(tuple.dyn_meta().len(), Some(2));
  assert_eq!(tuple.dyn_meta().item(0).unwrap().to_ref::<u8>(), Some(&9));
  assert_eq!(
    tuple
      .dyn_meta()
      .field("Tuple")
      .unwrap()
      .item(1)
      .unwrap()
      .to_ref::<bool>(),
    Some(&true)
  );

  let ready = Status::Ready;
  assert_eq!(ready.dyn_meta().field_names(), &["Ready"]);
  assert!(
    ready
      .dyn_meta()
      .field("Ready")
      .unwrap()
      .field("missing")
      .is_none()
  );
}

#[test]
fn derived_set_swaps_a_nested_field_wholesale() {
  let mut payload = Payload {
    values: 2,
    label: "ok",
    labels: vec![Label { name: "primary" }],
    optional_label: None,
    absent_label: None,
    pair: (4, false),
    bytes: [0x0a, 0xff],
    status: Status::Ready,
    labels_by_id: BTreeMap::new(),
    labels_by_name: BTreeMap::new(),
  };

  ObjectRefMut::new(&mut payload)
    .field_path_mut(&[PathSegment::Field("optional_label")])
    .unwrap()
    .set(Object::new(Some(Label { name: "swapped" })))
    .unwrap();

  assert_eq!(
    payload
      .dyn_meta()
      .field("optional_label")
      .unwrap()
      .field("name")
      .unwrap()
      .to_ref::<&'static str>(),
    Some(&"swapped")
  );

  {
    let mut root = ObjectRefMut::new(&mut payload);
    root
      .set_field_path(
        Payload::FIELD_LABELS.item(0).then(Label::FIELD_NAME),
        "typed",
      )
      .unwrap();
    root
      .set_field_path(Payload::FIELD_BYTES.item(1), 0x0b_u8)
      .unwrap();
  }
  assert_eq!(payload.labels[0].name, "typed");
  assert_eq!(payload.bytes[1], 0x0b);

  let mismatch = ObjectRefMut::new(&mut payload)
    .field_path_mut(&[PathSegment::Field("values")])
    .unwrap()
    .set(Object::new(Label { name: "wrong type" }))
    .unwrap_err();
  assert_eq!(mismatch, typex::ReflectiveError::MutationTypeMismatch);
  assert_eq!(payload.values, 2);
}

#[test]
fn shared_object_fallback_chain_works_without_default_features() {
  let map = |obj: SendObject| {
    obj
      .to::<Number>()
      .map(|value| value.0.to_string())
      .or_else(|obj| obj.to::<Flag>().map(|value| value.0.to_string()))
      .unwrap()
  };

  assert_eq!(map(SendObject::new(Number(3))), "3");
  assert_eq!(map(SendObject::new(Flag(true))), "true");
}
