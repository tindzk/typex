use bevy_reflect::{GetPath, ParsedPath, Reflect, ReflectMut, ReflectRef};
use std::hint::black_box;

include!("../../measure.rs");

#[derive(Reflect)]
struct Profile {
  count: u64,
  enabled: bool,
  label: String,
}

#[derive(Reflect)]
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
  let root = black_box(&state as &dyn Reflect);
  let peer = black_box(&other as &dyn Reflect);
  let profile = black_box(&state.profile as &dyn Reflect);
  let cached = match profile.reflect_ref() {
    ReflectRef::Struct(value) => value,
    _ => unreachable!(),
  };
  let scalar = black_box(&42_u64 as &dyn Reflect);
  let field_name = black_box("count");
  let nested = black_box(ParsedPath::parse("profile.count").unwrap());
  let indexed = black_box(ParsedPath::parse("items[2]").unwrap());

  assert_eq!(*root.path::<u64>(&nested).unwrap(), 42);
  assert_eq!(*root.path::<u64>(&indexed).unwrap(), 30);
  assert_eq!(root.reflect_partial_eq(peer), Some(true));
  measure("baseline", || *black_box(&42_u64));
  measure("downcast", || *scalar.downcast_ref::<u64>().unwrap());
  measure("field", || {
    let value = match profile.reflect_ref() {
      ReflectRef::Struct(value) => value,
      _ => unreachable!(),
    };
    *value
      .field(field_name)
      .unwrap()
      .try_downcast_ref::<u64>()
      .unwrap()
  });
  measure("cached_field", || {
    *cached
      .field(field_name)
      .unwrap()
      .try_downcast_ref::<u64>()
      .unwrap()
  });
  measure("nested_path", || *root.path::<u64>(&nested).unwrap());
  measure("indexed_path", || *root.path::<u64>(&indexed).unwrap());
  measure("string_path", || {
    *root.path::<u64>(black_box("profile.count")).unwrap()
  });
  measure("equality", || root.reflect_partial_eq(peer).unwrap());

  let view = black_box(&mut state.profile as &mut dyn Reflect);
  measure("field_mutation", || {
    let structure = match view.reflect_mut() {
      ReflectMut::Struct(value) => value,
      _ => unreachable!(),
    };
    let value = structure
      .field_mut(field_name)
      .unwrap()
      .try_downcast_mut::<u64>()
      .unwrap();
    *value = value.wrapping_add(1);
    *value
  });
  let structure = match view.reflect_mut() {
    ReflectMut::Struct(value) => value,
    _ => unreachable!(),
  };
  measure("cached_field_mutation", || {
    let value = structure
      .field_mut(field_name)
      .unwrap()
      .try_downcast_mut::<u64>()
      .unwrap();
    *value = value.wrapping_add(1);
    *value
  });
}
