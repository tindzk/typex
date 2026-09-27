use super::support::*;
use super::*;
use core::any::TypeId;

#[test]
fn variant_selection_checks_names_and_preserves_the_enum_view() {
  let value: Result<u8, bool> = Ok(42);
  let object = Object::new(value);
  let mutable = ObjectMut::new(value);
  let send = SendObject::new(value);
  for view in [
    ObjectRef::new(&value),
    object.as_object_ref(),
    mutable.as_object_ref(),
    send.as_object_ref(),
  ] {
    let selected = view.variant("Ok").unwrap();
    assert!(core::ptr::eq(
      view.to_ref::<Result<u8, bool>>().unwrap(),
      selected.to_ref::<Result<u8, bool>>().unwrap(),
    ));
    assert_eq!(selected.item(0).unwrap().to_ref::<u8>(), Some(&42));
    assert!(view.variant("Err").is_none());
    assert!(view.variant("ok").is_none());
  }
  assert!(object.variant("Ok").is_some());
  assert!(mutable.variant("Ok").is_some());
  assert!(send.variant("Ok").is_some());

  let nested = Some(Some(value));
  assert_eq!(
    ObjectRef::new(&nested)
      .variant("Ok")
      .unwrap()
      .to_ref::<Result<u8, bool>>(),
    Some(&value),
  );
  let absent: Option<Result<u8, bool>> = None;
  assert!(ObjectRef::new(&absent).variant("Ok").is_none());
  assert!(ObjectRef::new(&42_u8).variant("Ok").is_none());
}

#[test]
fn mutable_variant_selection_forwards_through_options() {
  let mut value: Option<Option<Result<u8, bool>>> = Some(Some(Ok(42)));
  {
    let mut view = ObjectRefMut::new(&mut value);
    assert!(view.variant("Ok").is_some());
    assert!(matches!(
      view.variant_mut("Err"),
      Err(ReflectiveError::PathNotFound)
    ));
    *view
      .variant_mut("Ok")
      .unwrap()
      .item_mut(0)
      .unwrap()
      .to_mut::<u8>()
      .unwrap() = 7;
  }
  assert_eq!(value, Some(Some(Ok(7))));

  let mut object = ObjectMut::new(value);
  assert!(object.variant_mut("Err").is_none());
  *object
    .variant_mut("Ok")
    .unwrap()
    .to_mut::<Result<u8, bool>>()
    .unwrap() = Err(true);
  assert!(object.variant_mut("Ok").is_none());
  assert_eq!(
    object
      .variant("Err")
      .unwrap()
      .item(0)
      .unwrap()
      .to_ref::<bool>(),
    Some(&true),
  );

  let mut absent: Option<Result<u8, bool>> = None;
  assert!(matches!(
    ObjectRefMut::new(&mut absent).variant_mut("Ok"),
    Err(ReflectiveError::PathNotFound)
  ));
  assert!(ObjectMut::new(absent).variant_mut("Ok").is_none());
  assert!(matches!(
    ObjectRefMut::new(&mut 42_u8).variant_mut("Ok"),
    Err(ReflectiveError::PathNotFound)
  ));
  assert!(ObjectMut::new(42_u8).variant_mut("Ok").is_none());
}

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
fn object_mut_lends_an_object_ref_mut() {
  let mut object = ObjectMut::new(Pair {
    count: 1,
    label: Text("one"),
  });

  object
    .as_object_ref_mut()
    .set_field_path(TypedField::<Pair, u16>::new("count").path(), 2)
    .unwrap();

  assert_eq!(object.to_ref::<Pair>().unwrap().count, 2);
  assert_eq!(object.as_object_ref().field_names(), &["count", "label"]);
}

#[test]
fn object_mut_converts_to_object_without_losing_type_information() {
  let object = ObjectMut::new(Number(7)).into_object();

  assert_eq!(object.to_ref::<Number>(), Some(&Number(7)));
  assert_eq!(object.to::<Number>().unwrap(), Number(7));
}

#[test]
fn object_mut_conversion_preserves_the_value_and_structural_access() {
  let mutable = ObjectMut::new(Pair {
    count: 7,
    label: Text("seven"),
  });
  let address = mutable.to_ref::<Pair>().unwrap() as *const Pair;
  let object = mutable.into_object();

  assert_eq!(object.to_ref::<Pair>().unwrap() as *const Pair, address);
  assert_eq!(object.type_info(), TypeInfo::of::<Pair>());
  assert_eq!(object.field("count").unwrap().to_ref::<u16>(), Some(&7));
  assert_eq!(
    object,
    Object::new(Pair {
      count: 7,
      label: Text("seven"),
    })
  );
}

#[test]
fn send_object_constructor_and_into_inner_work() {
  let object = SendObject::new(Number(7));

  let number = *object.into_inner().into_any().downcast::<Number>().unwrap();

  assert_eq!(number, Number(7));
}

#[test]
fn send_object_conversion_preserves_the_allocation() {
  for failed_downcast in [false, true] {
    let shared = SendObject::new(Pair {
      count: 7,
      label: Text("seven"),
    });
    let address = shared.to_ref::<Pair>().unwrap() as *const Pair;
    let object = if failed_downcast {
      shared.to::<u8>().unwrap_err()
    } else {
      shared.into_object()
    };

    assert_eq!(object.to_ref::<Pair>().unwrap() as *const Pair, address);
    assert_eq!(object.field("count").unwrap().to_ref::<u16>(), Some(&7));
    assert_eq!(object.to::<Pair>().unwrap().count, 7);
  }
}

#[test]
fn send_object_converts_into_object() {
  let shared = SendObject::new(Number(7));
  let address = shared.to_ref::<Number>().unwrap() as *const Number;
  let object = Object::from(shared);

  assert_eq!(object.to_ref::<Number>().unwrap() as *const Number, address);
  assert_eq!(object.to::<Number>().unwrap(), Number(7));
}

#[test]
fn object_ref_mut_converts_into_object_ref_with_the_same_lifetime() {
  fn shared_field<'a>(object: ObjectRefMut<'a>, name: &str) -> Option<ObjectRef<'a>> {
    ObjectRef::from(object).field(name)
  }

  let mut pair = Pair {
    count: 7,
    label: Text("seven"),
  };
  let count = shared_field(ObjectRefMut::new(&mut pair), "count").unwrap();

  assert_eq!(count.to_ref::<u16>(), Some(&7));
}

#[test]
fn send_object_preserves_inner_type_information() {
  let object = SendObject::new(Number(7));

  assert_eq!(object.type_info(), TypeInfo::of::<Number>());
  assert_eq!(object.type_name(), core::any::type_name::<Number>());
  assert_eq!(object.type_id(), TypeId::of::<Number>());
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
  assert_eq!(object.type_id(), TypeId::of::<Number>());
}

#[test]
fn type_info_accessors_expose_runtime_metadata() {
  let info = TypeInfo::of::<Number>();

  assert_eq!(info.name(), core::any::type_name::<Number>());
  assert_eq!(info.id(), TypeId::of::<Number>());
}
