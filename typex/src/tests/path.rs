use super::*;

#[test]
fn owned_path_round_trips_segments() {
  let long_name = "n".repeat(64);
  let segments = [
    PathSegment::Field("profile"),
    PathSegment::Variant("Ready"),
    PathSegment::Item(0),
    PathSegment::Item(63),
    PathSegment::Item(64),
    PathSegment::Field(&long_name),
    PathSegment::Key("a:1"),
    PathSegment::Field(""),
    PathSegment::Key("12:k"),
    PathSegment::Item(usize::MAX),
    PathSegment::Key("größe"),
  ];

  let from_slice = OwnedPath::from(segments.as_slice());
  let collected = segments.into_iter().collect::<OwnedPath>();

  assert_eq!(from_slice, collected);
  assert_eq!(from_slice.len(), segments.len());
  assert_eq!(from_slice.iter().collect::<Vec<_>>(), segments);
}

#[test]
fn owned_path_is_empty_without_segments() {
  let path = OwnedPath::new();

  assert!(path.is_empty());
  assert_eq!(path.len(), 0);
  assert_eq!(path.iter().next(), None);
}

#[test]
fn owned_path_debug_lists_segments() {
  let mut path = OwnedPath::new();
  path.push(PathSegment::Field("items"));
  path.push(PathSegment::Item(3));

  assert_eq!(alloc::format!("{path:?}"), r#"[Field("items"), Item(3)]"#);
}
