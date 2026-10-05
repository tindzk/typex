use std::hint::black_box;
use typex::{Meta, MetaMut, ObjectRef, ObjectRefMut, PathSegment, Reflect, ReflectMut};

include!("../../measure.rs");

#[derive(Meta, MetaMut)]
struct Profile {
  count: u64,
  enabled: bool,
  label: String,
}

#[derive(Meta, MetaMut)]
struct State {
  profile: Profile,
  items: Vec<u64>,
}

fn create_state() -> State {
  State {
    profile: Profile {
      count: 42,
      enabled: true,
      label: "example".into(),
    },
    items: vec![10, 20, 30, 40],
  }
}

fn main() {
  let mut state = create_state();
  let other = create_state();
  let root = ObjectRef::new(black_box(&state as &dyn Meta));
  let peer = ObjectRef::new(black_box(&other as &dyn Meta));
  let profile = root.field("profile").unwrap();
  let cached = match profile.as_meta().reflect() {
    Reflect::Struct(value) => value,
    _ => unreachable!(),
  };
  let scalar = ObjectRef::new(black_box(&42_u64 as &dyn Meta));
  let field_name = black_box("count");
  let nested = black_box([PathSegment::Field("profile"), PathSegment::Field("count")]);
  let indexed = black_box([PathSegment::Field("items"), PathSegment::Item(2)]);

  assert_eq!(
    *root.field_path(&nested).unwrap().to_ref::<u64>().unwrap(),
    42
  );
  assert_eq!(
    *root.field_path(&indexed).unwrap().to_ref::<u64>().unwrap(),
    30
  );
  assert_eq!(root, peer);
  measure("baseline", || *black_box(&42_u64));
  measure("downcast", || *scalar.to_ref::<u64>().unwrap());
  measure("field", || {
    *profile.field(field_name).unwrap().to_ref::<u64>().unwrap()
  });
  measure("cached_field", || {
    *cached.field(field_name).unwrap().to_ref::<u64>().unwrap()
  });
  measure("nested_path", || {
    *root.field_path(&nested).unwrap().to_ref::<u64>().unwrap()
  });
  measure("indexed_path", || {
    *root.field_path(&indexed).unwrap().to_ref::<u64>().unwrap()
  });
  measure("equality", || root == peer);

  let mut view = ObjectRefMut::new(black_box(&mut state.profile as &mut dyn MetaMut));
  measure("field_mutation", || {
    let field = view.field_mut(field_name).unwrap();
    let value = field.to_mut::<u64>().unwrap();
    *value = value.wrapping_add(1);
    *value
  });
  let erased = black_box(&mut state.profile as &mut dyn MetaMut);
  let structure = match erased.reflect_mut() {
    ReflectMut::Struct(value) => value,
    _ => unreachable!(),
  };
  measure("cached_field_mutation", || {
    let field = structure.field_mut(field_name).unwrap();
    let value = field.to_mut::<u64>().unwrap();
    *value = value.wrapping_add(1);
    *value
  });
}
