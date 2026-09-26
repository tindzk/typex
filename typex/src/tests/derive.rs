use super::*;
use typex_derive::Meta;

#[derive(Debug, PartialEq, Meta)]
#[typex(opaque)]
struct OpaqueNumber(u16);

#[test]
fn derived_opaque_supports_borrowed_and_owned_conversions() {
  let object = Object::new(OpaqueNumber(7));

  assert!(object.is::<OpaqueNumber>());
  assert_eq!(object.to_ref::<OpaqueNumber>(), Some(&OpaqueNumber(7)));

  let object = Object::new(OpaqueNumber(9));
  assert_eq!(object.to::<OpaqueNumber>().unwrap(), OpaqueNumber(9));
}

#[test]
fn derived_opaque_object_to_handles_owned_values() {
  let object = Object::new(OpaqueNumber(7));

  assert_eq!(object.to::<OpaqueNumber>(), Ok(OpaqueNumber(7)));
}
