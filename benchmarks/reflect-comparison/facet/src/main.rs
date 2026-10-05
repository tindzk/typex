use facet::{Facet, Peek, Poke};
use facet_path::{Path, PathStep};
use std::hint::black_box;

include!("../../measure.rs");

#[derive(Facet)]
#[facet(pod)]
struct Profile {
  count: u64,
  enabled: bool,
  label: String,
}

#[derive(Facet)]
#[facet(pod)]
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
  let root = black_box(Peek::new(&state));
  let peer = black_box(Peek::new(&other));
  let profile = black_box(Peek::new(&state.profile));
  let cached = profile.into_struct().unwrap();
  let scalar = black_box(Peek::new(&42_u64));
  let field_name = black_box("count");
  let mut nested = Path::new(root.shape());
  nested.push(PathStep::Field(0));
  nested.push(PathStep::Field(0));
  let nested = black_box(nested);
  let mut indexed = Path::new(root.shape());
  indexed.push(PathStep::Field(1));
  indexed.push(PathStep::Index(2));
  let indexed = black_box(indexed);

  assert_eq!(*root.at_path(&nested).unwrap().get::<u64>().unwrap(), 42);
  assert_eq!(*root.at_path(&indexed).unwrap().get::<u64>().unwrap(), 30);
  assert!(root.partial_eq(&peer).is_err());
  measure("baseline", || *black_box(&42_u64));
  measure("downcast", || *scalar.get::<u64>().unwrap());
  measure("field", || {
    *profile
      .into_struct()
      .unwrap()
      .field_by_name(field_name)
      .unwrap()
      .get::<u64>()
      .unwrap()
  });
  measure("cached_field", || {
    *cached
      .field_by_name(field_name)
      .unwrap()
      .get::<u64>()
      .unwrap()
  });
  measure("nested_path", || {
    *root.at_path(&nested).unwrap().get::<u64>().unwrap()
  });
  measure("indexed_path", || {
    *root.at_path(&indexed).unwrap().get::<u64>().unwrap()
  });

  let mut view = black_box(Poke::new(&mut state.profile));
  measure("field_mutation", || {
    let mut structure = view.try_reborrow().unwrap().into_struct().unwrap();
    let mut field = structure.field_by_name(field_name).unwrap();
    let value = field.get_mut::<u64>().unwrap();
    *value = value.wrapping_add(1);
    *value
  });
  let mut structure = view.try_reborrow().unwrap().into_struct().unwrap();
  measure("cached_field_mutation", || {
    let mut field = structure.field_by_name(field_name).unwrap();
    let value = field.get_mut::<u64>().unwrap();
    *value = value.wrapping_add(1);
    *value
  });
}
