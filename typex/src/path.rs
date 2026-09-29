use crate::{Object, ObjectRef, ObjectRefMut};
// Keep public API names in scope for short intra-doc links.
#[allow(unused_imports)]
use crate::PatchOperation;
use alloc::collections::{BTreeMap, LinkedList, VecDeque};
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use core::marker::PhantomData;

/// Borrowed path segment for nested reflective traversal.
///
/// A slice of segments can traverse named fields, indexed items and keyed
/// entries in sequence. The borrowed names remain valid for the path's
/// lifetime.
///
/// # Example
///
/// ```
/// use std::collections::BTreeMap;
/// use typex::{Meta, ObjectRef, PathSegment};
///
/// #[derive(Meta)]
/// struct Document {
///   sections: Vec<BTreeMap<&'static str, u8>>,
/// }
///
/// let document = Document {
///   sections: vec![BTreeMap::from([("title", 42)])],
/// };
///
/// let path = [
///   PathSegment::Field("sections"),
///   PathSegment::Item(0),
///   PathSegment::Key("title"),
/// ];
///
/// let title = ObjectRef::new(&document).field_path(&path).unwrap();
/// assert_eq!(title.to_ref::<u8>(), Some(&42));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathSegment<'a> {
  /// Traverses a named field.
  Field(&'a str),
  /// Traverses an indexed item.
  Item(usize),
  /// Traverses a keyed entry.
  Key(&'a str),
}

/// Owned path used by staged operations and rollback records.
///
/// Stores every segment in one string buffer. Converting a slice reserves
/// space for all its segments before encoding them. [`iter`](Self::iter) yields
/// borrowed [`PathSegment`] values.
///
/// # Example
///
/// ```
/// # use typex::{OwnedPath, PathSegment};
/// let path = OwnedPath::from([PathSegment::Field("items"), PathSegment::Item(2)].as_slice());
///
/// assert_eq!(
///   path.iter().collect::<Vec<_>>(),
///   [PathSegment::Field("items"), PathSegment::Item(2)]
/// );
/// ```
// Each segment is a tag (`f`, `i` or `k`) followed by a number. For fields and
// keys, the number is the name's length in bytes and the name follows. For
// items, the number is the index. Numbers are stored in 6-bit groups, least
// significant first, one group per byte. Bit 6 marks that another group
// follows and bit 7 stays clear, so every byte is ASCII and the buffer remains
// a valid `String` whose names can be borrowed as `&str` without validation.
#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub struct OwnedPath {
  encoded: String,
}

impl OwnedPath {
  /// Creates an empty path.
  #[inline]
  pub fn new() -> Self {
    Self::default()
  }

  /// Returns whether the path has no segments.
  #[inline]
  pub fn is_empty(&self) -> bool {
    self.encoded.is_empty()
  }

  /// Returns the number of segments.
  #[inline]
  pub fn len(&self) -> usize {
    self.iter().count()
  }

  /// Appends a segment.
  #[inline]
  pub fn push(&mut self, segment: PathSegment<'_>) {
    let (tag, number, name) = match segment {
      PathSegment::Field(name) => ('f', name.len(), name),
      PathSegment::Item(index) => ('i', index, ""),
      PathSegment::Key(key) => ('k', key.len(), key),
    };
    self.encoded.push(tag);
    push_number(&mut self.encoded, number);
    self.encoded.push_str(name);
  }

  /// Returns an iterator over the segments in order.
  #[inline]
  pub fn iter(&self) -> OwnedPathIter<'_> {
    OwnedPathIter {
      rest: &self.encoded,
    }
  }
}

fn encoded_len(segment: &PathSegment<'_>) -> usize {
  let (number, name_len) = match segment {
    PathSegment::Field(name) | PathSegment::Key(name) => (name.len(), name.len()),
    PathSegment::Item(index) => (*index, 0),
  };
  1 + number_len(number) + name_len
}

const NUMBER_BITS: u32 = 6;
const NUMBER_MASK: u8 = 0x3F;
const NUMBER_CONTINUE: u8 = 0x40;

fn number_len(mut number: usize) -> usize {
  let mut len = 1;
  while number > usize::from(NUMBER_MASK) {
    number >>= NUMBER_BITS;
    len += 1;
  }
  len
}

fn push_number(encoded: &mut String, mut number: usize) {
  loop {
    let group = number as u8 & NUMBER_MASK;
    number >>= NUMBER_BITS;
    if number == 0 {
      encoded.push(char::from(group));
      return;
    }
    encoded.push(char::from(group | NUMBER_CONTINUE));
  }
}

impl fmt::Debug for OwnedPath {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_list().entries(self.iter()).finish()
  }
}

impl<'a> From<&[PathSegment<'a>]> for OwnedPath {
  fn from(segments: &[PathSegment<'a>]) -> Self {
    let capacity = segments.iter().map(encoded_len).sum();
    let mut path = Self {
      encoded: String::with_capacity(capacity),
    };
    for &segment in segments {
      path.push(segment);
    }
    path
  }
}

impl<'a> FromIterator<PathSegment<'a>> for OwnedPath {
  fn from_iter<I: IntoIterator<Item = PathSegment<'a>>>(segments: I) -> Self {
    let mut path = Self::new();
    path.extend(segments);
    path
  }
}

impl<'a> Extend<PathSegment<'a>> for OwnedPath {
  fn extend<I: IntoIterator<Item = PathSegment<'a>>>(&mut self, segments: I) {
    for segment in segments {
      self.push(segment);
    }
  }
}

impl<'a> IntoIterator for &'a OwnedPath {
  type Item = PathSegment<'a>;
  type IntoIter = OwnedPathIter<'a>;

  fn into_iter(self) -> OwnedPathIter<'a> {
    self.iter()
  }
}

/// Iterator over the segments of an [`OwnedPath`].
#[derive(Clone, Debug)]
pub struct OwnedPathIter<'a> {
  rest: &'a str,
}

impl<'a> Iterator for OwnedPathIter<'a> {
  type Item = PathSegment<'a>;

  #[inline]
  fn next(&mut self) -> Option<PathSegment<'a>> {
    let bytes = self.rest.as_bytes();
    let tag = *bytes.first()?;
    let mut number = 0_usize;
    let mut shift = 0;
    let mut position = 1;
    loop {
      let byte = bytes[position];
      position += 1;
      number |= usize::from(byte & NUMBER_MASK) << shift;
      if byte & NUMBER_CONTINUE == 0 {
        break;
      }
      shift += NUMBER_BITS;
    }
    let start = position;
    if tag == b'i' {
      self.rest = &self.rest[start..];
      return Some(PathSegment::Item(number));
    }
    let end = start + number;
    let name = &self.rest[start..end];
    self.rest = &self.rest[end..];
    Some(if tag == b'f' {
      PathSegment::Field(name)
    } else {
      PathSegment::Key(name)
    })
  }
}

/// Typed path from a `Root` value to a nested `Value`.
///
/// Start a path with [`TypedField::path`] on a `FIELD_*` constant that
/// `#[derive(Meta)]` generates, then extend it with [`then`](Self::then) for
/// fields, [`item`](Self::item) for sequence items and [`key`](Self::key) for
/// map entries. Lookups return `&Value` or `&mut Value` without a downcast.
///
/// [`ObjectRef::field_path`], [`ObjectRefMut::field_path_mut`] and
/// [`ObjectRefMut::set_field_path`] accept typed paths. Typed paths iterate as
/// [`PathSegment`] values, so they can be passed to [`PatchOperation::set`].
/// The lifetime `'k` bounds the keys that [`key`](Self::key) borrows. Paths
/// without key steps are `TypedPath<'static, Root, Value>`.
///
/// The compiler checks `Value` at each step but not `Root`. A lookup fails at
/// runtime when:
///
/// - the path is applied to a type other than `Root`
/// - an enum holds a variant other than the one the path names
/// - an index or key is absent
/// - a hand-written [`TypedField::new`] constant names a missing field or
///   declares the wrong type
///
/// # Example
///
/// ```
/// # use typex::{Meta, MetaMut, ObjectRef, ObjectRefMut};
/// #[derive(Meta, MetaMut)]
/// struct Scope {
///   name: &'static str,
/// }
///
/// #[derive(Meta, MetaMut)]
/// struct User {
///   id: u16,
///   scopes: Vec<Scope>,
/// }
///
/// let mut user = User { id: 42, scopes: vec![Scope { name: "read" }] };
///
/// // `TypedPath<'static, User, &'static str>`
/// let name = User::FIELD_SCOPES.item(0).then(Scope::FIELD_NAME);
/// assert_eq!(ObjectRef::new(&user).field_path(name), Some(&"read"));
///
/// *ObjectRefMut::new(&mut user)
///   .field_path_mut(User::FIELD_ID.path())
///   .unwrap() = 7;
/// assert_eq!(user.id, 7);
/// ```
///
/// Map entries with `String` or `&'static str` keys use [`key`](Self::key):
///
/// ```
/// # use std::collections::BTreeMap;
/// # use typex::{Meta, ObjectRef};
/// #[derive(Meta)]
/// struct Config {
///   limits: BTreeMap<String, u32>,
/// }
///
/// let config = Config {
///   limits: BTreeMap::from([(String::from("requests"), 100)]),
/// };
///
/// let key = String::from("requests");
/// assert_eq!(
///   ObjectRef::new(&config).field_path(Config::FIELD_LIMITS.key(&key)),
///   Some(&100)
/// );
/// ```
pub struct TypedPath<'k, Root, Value> {
  segments: Vec<PathSegment<'k>>,
  marker: PhantomData<fn() -> (Root, Value)>,
}

/// A single typed field segment that can be composed into a [`TypedPath`].
///
/// The parent and value type parameters let `then` and `item` carry the
/// expected types through a path without requiring a second downcast at the
/// call site.
///
/// The derive macro generates associated `FIELD_*` constants for named struct
/// fields and enum variants, including constants for enum variant fields and
/// tuple items.
///
/// # Example
///
/// ```
/// # use typex::{Meta, ObjectRef};
/// #[derive(Meta)]
/// struct Address {
///   city: &'static str,
/// }
///
/// #[derive(Meta)]
/// struct User {
///   addresses: Vec<Address>,
/// }
///
/// let user = User {
///   addresses: vec![Address { city: "London" }],
/// };
/// let city = User::FIELD_ADDRESSES.item(0).then(Address::FIELD_CITY);
///
/// assert_eq!(ObjectRef::new(&user).field_path(city), Some(&"London"));
/// ```
#[derive(Clone, Copy)]
pub struct TypedField<Parent, Value> {
  name: &'static str,
  segments: Option<&'static [PathSegment<'static>]>,
  marker: PhantomData<fn() -> (Parent, Value)>,
}

/// Associates a sequential value with the type of each indexed item.
///
/// This is implemented for the built-in sequence collections that expose
/// uniform indexed items. The trait only carries type information; traversal
/// can still fail when an index is absent or the collection cannot provide a
/// mutable item.
pub trait TypedSequence {
  /// Type of each indexed item in the sequence.
  type Item;
}

impl<Item> TypedSequence for Vec<Item> {
  type Item = Item;
}

impl<Item, const LENGTH: usize> TypedSequence for [Item; LENGTH] {
  type Item = Item;
}

impl<Item> TypedSequence for VecDeque<Item> {
  type Item = Item;
}

impl<Item> TypedSequence for LinkedList<Item> {
  type Item = Item;
}

/// Associates a map with the type of its values.
///
/// Implemented for `BTreeMap` and `HashMap` with `String` or `&'static str`
/// keys, which are the key types that [`PathSegment::Key`] can look up. The
/// trait only carries type information; traversal fails when the key is absent.
pub trait TypedMap {
  /// Type of each value in the map.
  type Value;
}

impl<Value> TypedMap for BTreeMap<String, Value> {
  type Value = Value;
}

impl<Value> TypedMap for BTreeMap<&'static str, Value> {
  type Value = Value;
}

#[cfg(feature = "std")]
impl<Value, State> TypedMap for std::collections::HashMap<String, Value, State> {
  type Value = Value;
}

#[cfg(feature = "std")]
impl<Value, State> TypedMap for std::collections::HashMap<&'static str, Value, State> {
  type Value = Value;
}

impl<Parent, Value> TypedField<Parent, Value> {
  /// Creates a typed field segment.
  pub const fn new(name: &'static str) -> Self {
    TypedField {
      name,
      segments: None,
      marker: PhantomData,
    }
  }

  /// Creates a typed field from a static reflective path.
  #[doc(hidden)]
  pub const fn from_segments(segments: &'static [PathSegment<'static>]) -> Self {
    TypedField {
      name: "",
      segments: Some(segments),
      marker: PhantomData,
    }
  }

  /// Starts a path at this field and appends another typed field.
  pub fn then<Next>(self, next: TypedField<Value, Next>) -> TypedPath<'static, Parent, Next> {
    TypedPath::root().then(self).then(next)
  }

  /// Converts this field into a one-segment typed path.
  pub fn path(self) -> TypedPath<'static, Parent, Value> {
    TypedPath::root().then(self)
  }
}

impl<Parent, Sequence> TypedField<Parent, Sequence>
where
  Sequence: TypedSequence,
{
  /// Starts an indexed path into this sequential field.
  pub fn item(self, index: usize) -> TypedPath<'static, Parent, Sequence::Item> {
    self.path().item(index)
  }
}

impl<Parent, Map> TypedField<Parent, Map>
where
  Map: TypedMap,
{
  /// Starts a keyed path into this map field. The path borrows `key`.
  pub fn key(self, key: &str) -> TypedPath<'_, Parent, Map::Value> {
    self.path().key(key)
  }
}

impl<'k, Root, Value> TypedPath<'k, Root, Value> {
  /// Appends a field of the current `Value`. The path's `Value` becomes the
  /// field's type.
  pub fn then<Next>(mut self, field: TypedField<Value, Next>) -> TypedPath<'k, Root, Next> {
    if let Some(segments) = field.segments {
      self.segments.extend_from_slice(segments);
    } else {
      self.segments.push(PathSegment::Field(field.name));
    }
    TypedPath {
      segments: self.segments,
      marker: PhantomData,
    }
  }

  /// Returns the reflective representation of this path for APIs that accept
  /// raw [`PathSegment`] values.
  pub fn segments(&self) -> &[PathSegment<'k>] {
    &self.segments
  }
}

impl<'k, Root, Sequence> TypedPath<'k, Root, Sequence>
where
  Sequence: TypedSequence,
{
  /// Appends an index into the current sequence. The path's `Value` becomes
  /// the sequence's item type.
  pub fn item(mut self, index: usize) -> TypedPath<'k, Root, Sequence::Item> {
    self.segments.push(PathSegment::Item(index));
    TypedPath {
      segments: self.segments,
      marker: PhantomData,
    }
  }
}

impl<'k, Root, Map> TypedPath<'k, Root, Map>
where
  Map: TypedMap,
{
  /// Appends a key into the current map. The path borrows `key` and its
  /// `Value` becomes the map's value type.
  pub fn key(mut self, key: &'k str) -> TypedPath<'k, Root, Map::Value> {
    self.segments.push(PathSegment::Key(key));
    TypedPath {
      segments: self.segments,
      marker: PhantomData,
    }
  }
}

impl<Root> TypedPath<'_, Root, Root> {
  /// Creates an empty path rooted at `Root`.
  pub fn root() -> Self {
    TypedPath {
      segments: Vec::new(),
      marker: PhantomData,
    }
  }
}

impl<'k, Root, Value> IntoIterator for TypedPath<'k, Root, Value> {
  type Item = PathSegment<'k>;
  type IntoIter = alloc::vec::IntoIter<Self::Item>;

  fn into_iter(self) -> Self::IntoIter {
    self.segments.into_iter()
  }
}

impl<'a, 'k, Root, Value> IntoIterator for &'a TypedPath<'k, Root, Value> {
  type Item = PathSegment<'k>;
  type IntoIter = core::iter::Copied<core::slice::Iter<'a, Self::Item>>;

  fn into_iter(self) -> Self::IntoIter {
    self.segments.iter().copied()
  }
}

/// Path accepted by `field_path`. Its kind determines the return type.
///
/// This trait is not normally used directly. Pass one of the following to
/// [`ObjectRef::field_path`] or to `field_path` on another wrapper such as
/// [`Object`]:
///
/// | Path | Returns |
/// |---|---|
/// | `&[PathSegment]` or `&[PathSegment; N]` | `Option<ObjectRef>`, a reflective view of the target |
/// | [`TypedPath<'k, Root, Value>`](TypedPath) | `Option<&Value>`, a reference to the target itself |
///
/// Raw segments suit paths that are only known at runtime. A [`TypedPath`]
/// needs no downcast. Both return `None` when a segment cannot be found. A
/// typed path also returns `None` when the target is not a `Value`, such as
/// when the path is applied to a type other than `Root`.
///
/// # Example
///
/// ```
/// # use typex::{Meta, ObjectRef, PathSegment};
/// #[derive(Meta)]
/// struct User {
///   id: u16,
///   tags: Vec<&'static str>,
/// }
///
/// let user = User { id: 42, tags: vec!["admin"] };
///
/// // A raw path returns an `ObjectRef` that still needs a downcast
/// let root = ObjectRef::new(&user);
/// let tag = root
///   .field_path(&[PathSegment::Field("tags"), PathSegment::Item(0)])
///   .unwrap();
/// assert_eq!(tag.to_ref::<&'static str>(), Some(&"admin"));
///
/// // A typed path returns `&u16` directly
/// assert_eq!(root.field_path(User::FIELD_ID.path()), Some(&42));
/// ```
pub trait FieldPathQuery<'a> {
  /// Value that the path resolves to.
  type Output;

  #[doc(hidden)]
  fn resolve(self, root: ObjectRef<'a>) -> Option<Self::Output>;
}

impl<'a, 'b, 'c> FieldPathQuery<'a> for &'b [PathSegment<'c>] {
  type Output = ObjectRef<'a>;

  #[inline]
  fn resolve(self, root: ObjectRef<'a>) -> Option<Self::Output> {
    let mut current = root;

    for segment in self {
      current = match segment {
        PathSegment::Field(name) => current.field(name)?,
        PathSegment::Item(index) => current.item(*index)?,
        PathSegment::Key(key) => current.key(key)?,
      };
    }

    Some(current)
  }
}

// Array literals (`&[PathSegment::Field("x")]`) don't unsize-coerce to `&[_]`
// at a generic call site the way they do for a concretely-typed parameter,
// so this mirrors the slice impl to keep that call shape working.
impl<'a, 'b, 'c, const N: usize> FieldPathQuery<'a> for &'b [PathSegment<'c>; N] {
  type Output = ObjectRef<'a>;

  fn resolve(self, root: ObjectRef<'a>) -> Option<Self::Output> {
    FieldPathQuery::resolve(self.as_slice(), root)
  }
}

impl<'a, Root, Value: 'static> FieldPathQuery<'a> for TypedPath<'_, Root, Value> {
  type Output = &'a Value;

  fn resolve(self, root: ObjectRef<'a>) -> Option<Self::Output> {
    root.field_path(self.segments())?.to_ref::<Value>()
  }
}

/// Failure returned by reflective path traversal or mutable operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReflectiveError {
  /// A field, item or key was not available at some path segment.
  PathNotFound,
  /// A supplied value or requested concrete type did not match the target.
  MutationTypeMismatch,
}

impl fmt::Display for ReflectiveError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::PathNotFound => write!(f, "Reflective path not found"),
      Self::MutationTypeMismatch => {
        write!(f, "The value has an incompatible concrete type")
      }
    }
  }
}

#[cfg(feature = "std")]
impl std::error::Error for ReflectiveError {}

/// Path accepted by `field_path_mut` and [`ObjectRefMut::set_field_path`]:
/// the mutable counterpart of [`FieldPathQuery`].
///
/// | Path | Returns | Assigned value |
/// |---|---|---|
/// | `&[PathSegment]` or `&[PathSegment; N]` | `Result<ObjectRefMut, ReflectiveError>` | [`Object`] |
/// | [`TypedPath<'k, Root, Value>`](TypedPath) | `Result<&mut Value, ReflectiveError>` | `Value` |
///
/// A lookup fails with [`ReflectiveError::PathNotFound`] when a segment is
/// missing. A typed path also fails with
/// [`ReflectiveError::MutationTypeMismatch`] when the target is not a `Value`.
///
/// # Example
///
/// ```
/// # use typex::{Meta, MetaMut, ObjectRefMut, PathSegment};
/// #[derive(Meta, MetaMut)]
/// struct User {
///   id: u16,
///   tags: Vec<&'static str>,
/// }
///
/// let mut user = User { id: 42, tags: vec!["admin"] };
///
/// // A raw path returns an `ObjectRefMut` that still needs a downcast
/// let mut root = ObjectRefMut::new(&mut user);
/// let tag = root
///   .field_path_mut(&[PathSegment::Field("tags"), PathSegment::Item(0)])
///   .unwrap();
/// *tag.to_mut::<&'static str>().unwrap() = "owner";
///
/// // A typed path returns `&mut u16` directly
/// *root.field_path_mut(User::FIELD_ID.path()).unwrap() = 7;
/// assert_eq!(user.tags, ["owner"]);
/// assert_eq!(user.id, 7);
/// ```
pub trait FieldPathQueryMut<'r> {
  /// Value that the path resolves to.
  type Output;

  /// Value that [`ObjectRefMut::set_field_path`] writes to the path's target.
  type Value;

  #[doc(hidden)]
  fn resolve(self, root: ObjectRefMut<'r>) -> Result<Self::Output, ReflectiveError>;

  #[doc(hidden)]
  fn assign(self, root: ObjectRefMut<'r>, value: Self::Value) -> Result<(), ReflectiveError>;
}

impl<'r, 'b, 'c> FieldPathQueryMut<'r> for &'b [PathSegment<'c>] {
  type Output = ObjectRefMut<'r>;
  type Value = Object;

  #[inline]
  fn resolve(self, root: ObjectRefMut<'r>) -> Result<Self::Output, ReflectiveError> {
    ObjectRefMut::path_from(root.inner, self).ok_or(ReflectiveError::PathNotFound)
  }

  #[inline]
  fn assign(self, root: ObjectRefMut<'r>, value: Object) -> Result<(), ReflectiveError> {
    FieldPathQueryMut::resolve(self, root)?.set(value)
  }
}

// See the array impl of `FieldPathQuery` above for why this is needed
// alongside the slice impl.
impl<'r, 'b, 'c, const N: usize> FieldPathQueryMut<'r> for &'b [PathSegment<'c>; N] {
  type Output = ObjectRefMut<'r>;
  type Value = Object;

  fn resolve(self, root: ObjectRefMut<'r>) -> Result<Self::Output, ReflectiveError> {
    FieldPathQueryMut::resolve(self.as_slice(), root)
  }

  fn assign(self, root: ObjectRefMut<'r>, value: Object) -> Result<(), ReflectiveError> {
    FieldPathQueryMut::assign(self.as_slice(), root, value)
  }
}

impl<'r, Root, Value: 'static> FieldPathQueryMut<'r> for TypedPath<'_, Root, Value> {
  type Output = &'r mut Value;
  type Value = Value;

  fn resolve(self, root: ObjectRefMut<'r>) -> Result<Self::Output, ReflectiveError> {
    ObjectRefMut::path_from(root.inner, self.segments())
      .ok_or(ReflectiveError::PathNotFound)?
      .to_mut::<Value>()
  }

  fn assign(self, root: ObjectRefMut<'r>, value: Value) -> Result<(), ReflectiveError> {
    *FieldPathQueryMut::resolve(self, root)? = value;
    Ok(())
  }
}
