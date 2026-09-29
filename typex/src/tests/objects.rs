use super::support::*;
use super::*;
use core::any::TypeId;

#[test]
fn object_helper_boxes_meta_values() {
  let object = Object::new(Number(7));

  assert_eq!(object.to_ref::<Number>(), Some(&Number(7)));
}

#[test]
fn object_into_inner_returns_the_boxed_meta_value() {
  let object = Object::new(Number(7));

  let number = *object.into_inner().into_any().downcast::<Number>().unwrap();

  assert_eq!(number, Number(7));
}

#[test]
fn object_mut_constructor_and_into_inner_work() {
  let object = ObjectMut::new(Number(7));

  let number = *object.into_inner().into_any().downcast::<Number>().unwrap();

  assert_eq!(number, Number(7));
}

#[test]
fn object_mut_from_clone_owns_a_copy() {
  let source = Number(7);
  let object = ObjectMut::from_clone(&source);

  assert_eq!(object.to_ref::<Number>(), Some(&Number(7)));
}

#[test]
fn object_mut_converts_to_object_without_losing_type_information() {
  let object = ObjectMut::new(Number(7)).into_object();

  assert_eq!(object.to_ref::<Number>(), Some(&Number(7)));
  assert_eq!(object.to::<Number>().unwrap(), Number(7));
}

#[test]
fn send_object_constructor_and_into_inner_work() {
  let object = SendObject::new(Number(7));

  let number = *object.into_inner().into_any().downcast::<Number>().unwrap();

  assert_eq!(number, Number(7));
}

#[test]
fn send_object_preserves_inner_type_information() {
  let object = SendObject::new(Number(7));

  assert_eq!(object.type_info(), TypeInfo::of::<Number>());
  assert_eq!(object.type_name(), core::any::type_name::<Number>());
  assert_eq!(object.id(), TypeId::of::<Number>());
}

#[test]
fn send_object_forwards_option_value() {
  let present = SendObject::new(Some(Number(7)));
  let absent = SendObject::new(None::<Number>);

  assert_eq!(
    present.option_value().unwrap().to_ref::<Number>(),
    Some(&Number(7))
  );
  assert!(absent.option_value().is_none());
}

#[test]
fn object_to_returns_owned_value_on_success() {
  let object = Object::new(Number(7));

  let number = object.to::<Number>().unwrap();

  assert_eq!(number, Number(7));
}

#[test]
fn object_to_returns_original_object_on_failure() {
  let object = Object::new(Text("hello"));

  let object = object.to::<Number>().unwrap_err();

  assert!(object.is::<Text>());
  assert_eq!(object.to_ref::<Text>(), Some(&Text("hello")));
}

#[test]
fn object_type_checks_use_the_concrete_any_type() {
  let object = Object::new(LyingType);

  assert!(!object.is::<Number>());
  assert!(object.to::<Number>().is_err());
}

#[test]
fn to_ref_returns_some_for_matching_type_and_none_otherwise() {
  let object = Object::new(Number(9));

  assert_eq!(object.to_ref::<Number>(), Some(&Number(9)));
  assert_eq!(object.to_ref::<Text>(), None);
}

#[test]
fn is_distinguishes_concrete_types() {
  let number = Object::new(Number(1));
  let text = Object::new(Text("a"));

  assert!(number.is::<Number>());
  assert!(!number.is::<Text>());
  assert!(text.is::<Text>());
  assert!(!text.is::<Number>());
}

#[test]
fn type_info_matches_meta_accessors() {
  let object = Object::new(Number(5));

  assert_eq!(object.type_info(), TypeInfo::of::<Number>());
  assert_eq!(object.type_name(), core::any::type_name::<Number>());
  assert_eq!(object.id(), TypeId::of::<Number>());
}

#[test]
fn type_info_accessors_expose_runtime_metadata() {
  let info = TypeInfo::of::<Number>();

  assert_eq!(info.type_name(), core::any::type_name::<Number>());
  assert_eq!(info.id(), TypeId::of::<Number>());
}
