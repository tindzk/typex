use super::support::*;
use super::*;

#[test]
fn type_map_binds_and_looks_up_values_by_type() {
  let mut map = TypeMap::<&'static str>::new();

  assert!(map.is_empty());
  assert_eq!(map.insert::<Number>("number"), None);
  assert_eq!(map.insert::<Text>("text"), None);
  assert_eq!(map.len(), 2);

  assert_eq!(map.get::<Number>(), Some(&"number"));
  assert_eq!(map.get::<Text>(), Some(&"text"));
  assert!(map.contains::<Number>());
}

#[test]
fn type_map_insert_overwrites_existing_binding_for_same_type() {
  let mut map = TypeMap::<u32>::new();

  assert_eq!(map.insert::<Number>(1), None);
  assert_eq!(map.insert::<Number>(2), Some(1));
  assert_eq!(map.get::<Number>(), Some(&2));
  assert_eq!(map.len(), 1);
}

#[test]
fn type_map_remove_returns_and_drops_binding() {
  let mut map = TypeMap::<u32>::new();
  map.insert::<Number>(7);

  assert_eq!(map.remove::<Number>(), Some(7));
  assert_eq!(map.remove::<Number>(), None);
  assert!(!map.contains::<Number>());
}

#[test]
fn type_map_type_names_iterate_bound_type_names_in_insertion_order() {
  let mut map = TypeMap::<()>::new();
  map.insert::<Number>(());
  map.insert::<Text>(());

  let names: Vec<_> = map.type_names().collect();

  assert_eq!(
    names,
    vec![
      core::any::type_name::<Number>(),
      core::any::type_name::<Text>()
    ]
  );
}
