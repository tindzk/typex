//! Checks that importing `Meta` and `MetaMut` for the derives leaves inherent
//! methods reachable through auto-dereferencing.

use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use typex::{Meta, MetaMut, Object, ObjectRef, ObjectRefMut};

#[derive(Meta, MetaMut)]
struct Query {
  terms: Vec<String>,
}

impl Query {
  fn id(&self) -> &'static str {
    "query"
  }
}

// `part.len() > 0` is the call shape that resolved to the reflective `len`
// while the trait exposed it under that name.
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
  let query: Box<Query> = Box::new(Query { terms: Vec::new() });

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
fn reflective_methods_remain_available_through_objects() {
  let mut query = Query {
    terms: vec![String::from("a")],
  };

  let view = ObjectRef::new(&query);
  assert_eq!(view.field("terms").unwrap().len(), Some(1));

  let object = Object::new(Query {
    terms: vec![String::from("a"), String::from("b")],
  });
  assert_eq!(object.field("terms").unwrap().len(), Some(2));
  let terms: &dyn Meta = &query.terms;
  assert_eq!(terms.is_empty(), Some(false));

  ObjectRefMut::new(&mut query)
    .field_mut("terms")
    .unwrap()
    .push_item(Object::new(String::from("b")))
    .unwrap();
  assert_eq!(query.terms, vec!["a", "b"]);
}
