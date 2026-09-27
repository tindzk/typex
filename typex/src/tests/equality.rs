use super::support::*;
use super::*;
use core::any::Any;
use core::cell::Cell;

#[test]
fn leaf_eq_dyn_compares_by_value_and_type() {
  let a = Object::new(Number(7));
  let b = Object::new(Number(7));
  let c = Object::new(Number(8));
  let d = Object::new(Text("7"));

  assert!(a.as_ref() as &dyn Meta == b.as_ref() as &dyn Meta);
  assert!(a.as_ref() as &dyn Meta != c.as_ref() as &dyn Meta);
  // Different concrete types are never equal, even with "matching" data.
  assert!(a.as_ref() as &dyn Meta != d.as_ref() as &dyn Meta);
}

#[test]
fn object_ref_mut_eq_compares_structurally() {
  let mut a = Number(7);
  let mut b = Number(7);
  let left = ObjectRefMut::new(&mut a);
  let right = ObjectRefMut::new(&mut b);

  assert!(left == right);
}

#[test]
fn nan_is_never_equal_to_itself() {
  let nan = f64::NAN;
  assert!(!(&nan as &dyn Meta == &nan as &dyn Meta));
}

#[test]
fn struct_eq_dyn_compares_fields_structurally() {
  let a = Pair {
    count: 1,
    label: Text("x"),
  };
  let b = Pair {
    count: 1,
    label: Text("x"),
  };
  let different_count = Pair {
    count: 2,
    label: Text("x"),
  };
  let different_label = Pair {
    count: 1,
    label: Text("y"),
  };

  assert!(&a as &dyn Meta == &b as &dyn Meta);
  assert!(&a as &dyn Meta != &different_count as &dyn Meta);
  assert!(&a as &dyn Meta != &different_label as &dyn Meta);
}

#[test]
fn vec_eq_dyn_compares_items_pairwise() {
  let a = vec![Number(1), Number(2)];
  let b = vec![Number(1), Number(2)];
  let shorter = vec![Number(1)];
  let different_element = vec![Number(1), Number(3)];

  assert!(&a as &dyn Meta == &b as &dyn Meta);
  assert!(&a as &dyn Meta != &shorter as &dyn Meta);
  assert!(&a as &dyn Meta != &different_element as &dyn Meta);
}

#[test]
fn option_eq_dyn_compares_inner_value_or_absence() {
  let some_a = Some(Number(7));
  let some_b = Some(Number(7));
  let some_other = Some(Number(8));
  let none: Option<Number> = None;
  let none_too: Option<Number> = None;

  assert!(&some_a as &dyn Meta == &some_b as &dyn Meta);
  assert!(&some_a as &dyn Meta != &some_other as &dyn Meta);
  assert!(&none as &dyn Meta == &none_too as &dyn Meta);
  assert!(&some_a as &dyn Meta != &none as &dyn Meta);
}

#[test]
fn result_eq_dyn_distinguishes_variants() {
  let ok_a: Result<Number, Text> = Ok(Number(7));
  let ok_b: Result<Number, Text> = Ok(Number(7));
  let err: Result<Number, Text> = Err(Text("boom"));

  assert!(&ok_a as &dyn Meta == &ok_b as &dyn Meta);
  assert!(&ok_a as &dyn Meta != &err as &dyn Meta);
}

#[test]
fn map_eq_dyn_compares_string_keyed_entries_as_a_set() {
  let a = BTreeMap::from([
    (String::from("a"), Number(1)),
    (String::from("b"), Number(2)),
  ]);
  // Same entries, different insertion/iteration order.
  let b = BTreeMap::from([
    (String::from("b"), Number(2)),
    (String::from("a"), Number(1)),
  ]);
  let different_value = BTreeMap::from([
    (String::from("a"), Number(1)),
    (String::from("b"), Number(9)),
  ]);

  assert!(&a as &dyn Meta == &b as &dyn Meta);
  assert!(&a as &dyn Meta != &different_value as &dyn Meta);
}

#[test]
fn map_eq_dyn_compares_non_string_keyed_maps_via_native_key_equality() {
  // `Meta::keys()`/`key()` only work for string-like keys, but `BTreeMap`'s
  // `eq_dyn` override compares `K` natively instead of round-tripping
  // through them, so non-string keys still compare correctly.
  let a = BTreeMap::from([(7usize, Number(1))]);
  let b = BTreeMap::from([(7usize, Number(1))]);
  let different_key = BTreeMap::from([(8usize, Number(1))]);
  let different_value = BTreeMap::from([(7usize, Number(2))]);

  assert!(&a as &dyn Meta == &b as &dyn Meta);
  assert!(&a as &dyn Meta != &different_key as &dyn Meta);
  assert!(&a as &dyn Meta != &different_value as &dyn Meta);
}

#[cfg(feature = "std")]
#[test]
fn hash_map_eq_dyn_compares_via_native_key_equality() {
  use std::collections::HashMap;

  let a = HashMap::from([
    (String::from("a"), Number(1)),
    (String::from("b"), Number(2)),
  ]);
  // Same entries, different insertion order (HashMap iteration order isn't
  // guaranteed to match even for equal contents).
  let b = HashMap::from([
    (String::from("b"), Number(2)),
    (String::from("a"), Number(1)),
  ]);
  let different_value = HashMap::from([
    (String::from("a"), Number(1)),
    (String::from("b"), Number(9)),
  ]);
  let fewer_entries = HashMap::from([(String::from("a"), Number(1))]);

  assert!(&a as &dyn Meta == &b as &dyn Meta);
  assert!(&a as &dyn Meta != &different_value as &dyn Meta);
  assert!(&a as &dyn Meta != &fewer_entries as &dyn Meta);
}

#[test]
fn mismatched_wrapper_types_are_not_equal() {
  let boxed: Box<Number> = Box::new(Number(7));
  let plain = Number(7);

  assert!(&boxed as &dyn Meta != &plain as &dyn Meta);
}

/// Hand-written struct with zero exposed fields, standing in for a unit struct
/// with `#[derive(Meta)]`. It reflects as a struct even though
/// `field_names()` is empty, unlike a truly opaque scalar.
#[derive(Debug)]
struct EmptyRecord;

impl Meta for EmptyRecord {
  fn reflect(&self) -> Reflect<'_> {
    Reflect::Struct(self)
  }

  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

impl StructAccess for EmptyRecord {
  fn field(&self, _name: &str) -> Option<ObjectRef<'_>> {
    None
  }

  fn field_names(&self) -> &'static [&'static str] {
    &[]
  }
}

#[test]
fn struct_eq_dyn_treats_zero_exposed_fields_as_equal_not_opaque() {
  let a = EmptyRecord;
  let b = EmptyRecord;

  assert!(&a as &dyn Meta == &b as &dyn Meta);
}

/// Hand-written struct that advertises a field without exposing its value.
#[derive(Debug)]
struct UnresolvedFieldRecord;

impl Meta for UnresolvedFieldRecord {
  fn reflect(&self) -> Reflect<'_> {
    Reflect::Struct(self)
  }

  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

impl StructAccess for UnresolvedFieldRecord {
  fn field(&self, _name: &str) -> Option<ObjectRef<'_>> {
    None
  }

  fn field_names(&self) -> &'static [&'static str] {
    &["value"]
  }
}

#[test]
fn struct_eq_dyn_rejects_unresolved_fields() {
  let a = UnresolvedFieldRecord;
  let b = UnresolvedFieldRecord;

  assert!(&a as &dyn Meta != &b as &dyn Meta);
}

/// Hand-written map without an `eq_dyn` override. Exercises the default
/// `Meta::eq_dyn` map comparison in `traits.rs`, which `BTreeMap` and
/// `HashMap` bypass via their own native-key `eq_dyn`. Counts
/// `visit_entries` visits so tests can assert the comparison short-circuits
/// before visiting the other map.
#[derive(Debug)]
struct EntryMap {
  entries: Vec<(String, Number)>,
  visits: Cell<usize>,
}

impl EntryMap {
  fn new(entries: &[(&str, u16)]) -> Self {
    EntryMap {
      entries: entries
        .iter()
        .map(|&(key, value)| (key.to_owned(), Number(value)))
        .collect(),
      visits: Cell::new(0),
    }
  }
}

impl Meta for EntryMap {
  fn reflect(&self) -> Reflect<'_> {
    Reflect::Map(self)
  }

  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

impl MapAccess for EntryMap {
  fn len(&self) -> usize {
    self.entries.len()
  }

  fn key(&self, key: &str) -> Option<ObjectRef<'_>> {
    self
      .entries
      .iter()
      .find(|(k, _)| k == key)
      .map(|(_, value)| ObjectRef::new(value as &dyn Meta))
  }

  fn keys(&self) -> Option<Vec<String>> {
    None
  }

  fn visit_entries(&self, visitor: &mut MapEntryVisitor<'_>) {
    for (key, value) in &self.entries {
      self.visits.set(self.visits.get() + 1);
      if !visitor(AnyRef::new(key), ObjectRef::new(value as &dyn Meta)) {
        break;
      }
    }
  }
}

#[test]
fn map_entries_eq_dyn_fallback_compares_entries_as_a_set() {
  let a = EntryMap::new(&[("a", 1), ("b", 2)]);
  // Same entries, different order.
  let b = EntryMap::new(&[("b", 2), ("a", 1)]);

  assert!(&a as &dyn Meta == &b as &dyn Meta);
}

#[test]
fn map_entries_eq_dyn_fallback_rejects_missing_key() {
  let a = EntryMap::new(&[("a", 1), ("b", 2)]);
  let fewer_entries = EntryMap::new(&[("a", 1)]);

  assert!(&a as &dyn Meta != &fewer_entries as &dyn Meta);
  assert!(&fewer_entries as &dyn Meta != &a as &dyn Meta);
}

#[test]
fn map_entries_eq_dyn_fallback_short_circuits_on_first_mismatch() {
  let a = EntryMap::new(&[("a", 1), ("b", 2), ("c", 3)]);
  let different_first_value = EntryMap::new(&[("a", 9), ("b", 2), ("c", 3)]);

  assert!(&a as &dyn Meta != &different_first_value as &dyn Meta);
  // Only the first entry needed visiting before the mismatch broke the loop.
  assert_eq!(a.visits.get(), 1);
}

#[test]
fn map_entries_eq_dyn_fallback_rejects_different_lengths_without_traversal() {
  let a = EntryMap::new(&[("a", 1), ("b", 2)]);
  let larger = EntryMap::new(&[("a", 1), ("b", 2), ("c", 3), ("d", 4), ("e", 5)]);

  assert!(&a as &dyn Meta != &larger as &dyn Meta);
  assert_eq!(a.visits.get(), 0);
  assert_eq!(larger.visits.get(), 0);
}

#[test]
fn map_entries_eq_dyn_fallback_uses_len_to_skip_other_traversal() {
  let a = EntryMap::new(&[("a", 1), ("b", 2)]);
  let b = EntryMap::new(&[("b", 2), ("a", 1)]);

  assert!(&a as &dyn Meta == &b as &dyn Meta);
  assert_eq!(a.visits.get(), 2);
  assert_eq!(b.visits.get(), 0);
}

#[derive(Debug)]
struct StaticKeyEntryMap {
  entries: Vec<(&'static str, Number)>,
}

impl StaticKeyEntryMap {
  fn new(entries: &[(&'static str, u16)]) -> Self {
    StaticKeyEntryMap {
      entries: entries
        .iter()
        .map(|&(key, value)| (key, Number(value)))
        .collect(),
    }
  }
}

impl Meta for StaticKeyEntryMap {
  fn reflect(&self) -> Reflect<'_> {
    Reflect::Map(self)
  }

  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

impl MapAccess for StaticKeyEntryMap {
  fn len(&self) -> usize {
    self.entries.len()
  }

  fn key(&self, key: &str) -> Option<ObjectRef<'_>> {
    self
      .entries
      .iter()
      .find(|(entry_key, _)| *entry_key == key)
      .map(|(_, value)| ObjectRef::new(value as &dyn Meta))
  }

  fn keys(&self) -> Option<Vec<String>> {
    None
  }

  fn visit_entries(&self, visitor: &mut MapEntryVisitor<'_>) {
    for (key, value) in &self.entries {
      if !visitor(AnyRef::new(key), ObjectRef::new(value as &dyn Meta)) {
        break;
      }
    }
  }
}

#[test]
fn map_entries_eq_dyn_fallback_accepts_static_str_keys() {
  let a = StaticKeyEntryMap::new(&[("a", 1), ("b", 2)]);
  let b = StaticKeyEntryMap::new(&[("b", 2), ("a", 1)]);

  assert!(&a as &dyn Meta == &b as &dyn Meta);
}

#[derive(Debug)]
struct NumericEntryMap {
  entries: Vec<(usize, Number)>,
}

impl NumericEntryMap {
  fn new(entries: &[(usize, u16)]) -> Self {
    NumericEntryMap {
      entries: entries
        .iter()
        .map(|&(key, value)| (key, Number(value)))
        .collect(),
    }
  }
}

impl Meta for NumericEntryMap {
  fn reflect(&self) -> Reflect<'_> {
    Reflect::Map(self)
  }

  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

impl MapAccess for NumericEntryMap {
  fn len(&self) -> usize {
    self.entries.len()
  }

  fn key(&self, _key: &str) -> Option<ObjectRef<'_>> {
    None
  }

  fn keys(&self) -> Option<Vec<String>> {
    None
  }

  fn visit_entries(&self, visitor: &mut MapEntryVisitor<'_>) {
    for (key, value) in &self.entries {
      if !visitor(AnyRef::new(key), ObjectRef::new(value as &dyn Meta)) {
        break;
      }
    }
  }
}

#[test]
fn map_entries_eq_dyn_fallback_rejects_non_string_keys() {
  let a = NumericEntryMap::new(&[(1, 7)]);
  let b = NumericEntryMap::new(&[(1, 7)]);

  assert!(&a as &dyn Meta != &b as &dyn Meta);
}

#[test]
fn smart_pointers_compare_their_contents() {
  assert!(Box::new(1_u8).eq_dyn(&Box::new(1_u8)));
  assert!(!Box::new(1_u8).eq_dyn(&Box::new(2_u8)));
  assert!(Rc::new(String::from("a")).eq_dyn(&Rc::new(String::from("a"))));
  assert!(Arc::new(vec![1_u8, 2]).eq_dyn(&Arc::new(vec![1_u8, 2])));
  assert!(vec![Box::new(1_u8)].eq_dyn(&vec![Box::new(1_u8)]));
  // A pointer and its target have different concrete types.
  assert!(!Box::new(1_u8).eq_dyn(&1_u8));
}

#[test]
fn binary_heap_eq_dyn_ignores_insertion_order() {
  let a = [1_u8, 2, 3, 4].into_iter().collect::<BinaryHeap<_>>();
  let b = [4_u8, 3, 2, 1].into_iter().collect::<BinaryHeap<_>>();
  let c = [4_u8, 3, 2, 2].into_iter().collect::<BinaryHeap<_>>();

  assert!(a.eq_dyn(&b));
  assert!(!a.eq_dyn(&c));
}

#[test]
fn iterated_sequences_eq_dyn_compares_items_in_order() {
  let list = [1_u8, 2, 3].into_iter().collect::<LinkedList<_>>();
  let reversed = [3_u8, 2, 1].into_iter().collect::<LinkedList<_>>();
  let shorter = [1_u8, 2].into_iter().collect::<LinkedList<_>>();

  assert!(list.eq_dyn(&list.clone()));
  assert!(!list.eq_dyn(&reversed));
  assert!(!list.eq_dyn(&shorter));

  let set = BTreeSet::from([1_u8, 2, 3]);
  assert!(set.eq_dyn(&BTreeSet::from([3_u8, 2, 1])));
  assert!(!set.eq_dyn(&BTreeSet::from([1_u8, 2, 4])));
}
