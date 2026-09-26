// Keep public API names in scope for short intra-doc links.
#[allow(unused_imports)]
use crate::{
  AccessKind, ApplyError, FieldPathQuery, FieldPathQueryMut, MapEntryVisitor, Meta, MetaMut,
  MutationBatch, PatchOperation, PathSegment, ReflectiveError, TypeInfo, TypedPath, ValueKind,
  apply_patch,
};
use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::any::Any;
use core::fmt;
use core::ops::{Deref, DerefMut};

fn fmt_meta<T: Meta + ?Sized>(f: &mut fmt::Formatter<'_>, name: &str, meta: &T) -> fmt::Result {
  f.debug_struct(name)
    .field("type_name", &meta.type_name())
    .field("kind", &meta.kind())
    .finish()
}

impl dyn Meta + '_ {
  /// Downcasts the reference to `T`.
  pub fn to_ref<T: 'static>(&self) -> Option<&T> {
    self.as_any().downcast_ref::<T>()
  }

  /// Checks whether the value has type `T`.
  pub fn is<T: 'static>(&self) -> bool {
    self.as_any().is::<T>()
  }

  /// Passes each map entry to `visitor` until it returns `false`.
  ///
  /// Returns `false` when the value has no map-like access. Stopping early
  /// still returns `true`.
  pub fn visit_map_entries(&self, visitor: &mut MapEntryVisitor<'_>) -> bool {
    Meta::visit_map_entries(self, visitor)
  }

  /// Traverses a nested field, item or key path; see [`FieldPathQuery`].
  pub fn field_path<'a, Q: FieldPathQuery<'a>>(&'a self, query: Q) -> Option<Q::Output> {
    ObjectRef::new(self).field_path(query)
  }
}

/// Structural equality via [`Meta::eq_dyn`].
impl PartialEq for dyn Meta + '_ {
  fn eq(&self, other: &Self) -> bool {
    self.eq_dyn(other)
  }
}

/// Borrowed reflective view of a [`Meta`] value.
///
/// # Example
///
/// ```
/// # use typex::ObjectRef;
/// let values = vec![23_u8, 23, 42];
/// let view = ObjectRef::new(&values);
///
/// assert_eq!(view.item(1).and_then(|item| item.to_ref::<u8>()), Some(&23));
/// assert_eq!(view.to_ref::<Vec<u8>>(), Some(&values));
/// ```
#[derive(Clone, Copy)]
pub struct ObjectRef<'a> {
  inner: &'a dyn Meta,
}

impl<'a> ObjectRef<'a> {
  /// Creates a borrowed object reference.
  pub fn new(inner: &'a dyn Meta) -> Self {
    ObjectRef { inner }
  }

  /// Returns the underlying [`Meta`] trait object.
  pub fn as_meta(&self) -> &'a dyn Meta {
    self.inner
  }

  /// Returns runtime type metadata for the referenced value.
  pub fn type_info(&self) -> TypeInfo {
    self.inner.type_info()
  }

  /// Returns the Rust type name of the referenced value.
  pub fn type_name(&self) -> &'static str {
    self.inner.type_name()
  }

  /// Returns the type ID of the referenced value.
  pub fn id(&self) -> core::any::TypeId {
    self.inner.id()
  }

  /// Determines the structural shape of the referenced value.
  pub fn kind(&self) -> ValueKind {
    self.inner.kind()
  }

  /// Returns the named or keyed access exposed by the referenced value, if any.
  pub fn access_kind(&self) -> Option<AccessKind> {
    self.inner.access_kind()
  }

  /// Returns the contained value when this reference points to `Some(value)`.
  pub fn option_value(&self) -> Option<ObjectRef<'a>> {
    self.inner.option_value()
  }

  /// Returns an exposed field by name.
  pub fn field(&self, name: &str) -> Option<ObjectRef<'a>> {
    self.inner.field(name)
  }

  /// Returns the exposed field names in declaration order.
  pub fn field_names(&self) -> &'static [&'static str] {
    self.inner.field_names()
  }

  /// Returns an item at `index` for sequential access.
  pub fn item(&self, index: usize) -> Option<ObjectRef<'a>> {
    self.inner.item(index)
  }

  /// Returns the number of exposed structural items, when available.
  pub fn len(&self) -> Option<usize> {
    self.inner.len()
  }

  /// Checks whether a value has no exposed structural items, when available.
  pub fn is_empty(&self) -> Option<bool> {
    self.inner.is_empty()
  }

  /// Returns a value for `key` during map-like access.
  pub fn key(&self, key: &str) -> Option<ObjectRef<'a>> {
    self.inner.key(key)
  }

  /// Returns the available keys for map-like access.
  pub fn keys(&self) -> Option<Vec<String>> {
    self.inner.keys()
  }

  /// Passes each map entry to `visitor` until it returns `false`.
  ///
  /// Returns `false` when the value has no map-like access. Stopping early
  /// still returns `true`.
  pub fn visit_map_entries(&self, visitor: &mut MapEntryVisitor<'_>) -> bool {
    Meta::visit_map_entries(self.inner, visitor)
  }

  /// Downcasts the reference to `T`.
  pub fn to_ref<T: 'static>(&self) -> Option<&'a T> {
    self.inner.as_any().downcast_ref::<T>()
  }

  /// Checks whether the referenced value has type `T`.
  pub fn is<T: 'static>(&self) -> bool {
    self.inner.as_any().is::<T>()
  }

  /// Compares the referenced value against `other` structurally; see
  /// [`Meta::eq_dyn`].
  pub fn eq_dyn(&self, other: ObjectRef<'_>) -> bool {
    self.inner.eq_dyn(other.inner)
  }

  /// Traverses a nested field, item or key path. Accepts a raw
  /// `&[PathSegment]` (returns [`ObjectRef`]) or a typed [`TypedPath`]
  /// (returns `&'a Value`); see [`FieldPathQuery`].
  pub fn field_path<Q: FieldPathQuery<'a>>(&self, query: Q) -> Option<Q::Output> {
    query.resolve(*self)
  }
}

impl fmt::Debug for ObjectRef<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt_meta(f, "ObjectRef", self.inner)
  }
}

/// Structural equality via [`Meta::eq_dyn`].
impl PartialEq for ObjectRef<'_> {
  fn eq(&self, other: &Self) -> bool {
    self.inner.eq_dyn(other.inner)
  }
}

/// Owned reflective value stored as a boxed [`Meta`] trait object.
///
/// Access is read-only. Use [`ObjectMut`] for values that need mutation.
///
/// # Example
///
/// ```
/// # use typex::{Object, ObjectOps};
/// let object = Object::new(23_u64);
///
/// assert!(object.is::<u64>());
/// assert_eq!(object.to_ref::<u64>(), Some(&23));
/// assert_eq!(object.to::<u64>().unwrap(), 23);
/// ```
#[repr(transparent)]
pub struct Object(Box<dyn Meta>);

impl Object {
  /// Boxes a value as an [`Object`].
  pub fn new<T>(value: T) -> Self
  where
    T: Meta + 'static,
  {
    Self(Box::new(value))
  }

  /// Returns the underlying boxed [`Meta`] trait object.
  pub fn into_inner(self) -> Box<dyn Meta> {
    self.0
  }

  /// Converts into a reference-counted [`Meta`] trait object.
  pub fn into_rc(self) -> Rc<dyn Meta> {
    Rc::from(self.0)
  }
}

impl From<Box<dyn Meta>> for Object {
  fn from(value: Box<dyn Meta>) -> Self {
    Self(value)
  }
}

impl Deref for Object {
  type Target = dyn Meta;

  fn deref(&self) -> &Self::Target {
    self.0.as_ref()
  }
}

impl DerefMut for Object {
  fn deref_mut(&mut self) -> &mut Self::Target {
    self.0.as_mut()
  }
}

impl AsRef<dyn Meta> for Object {
  fn as_ref(&self) -> &(dyn Meta + 'static) {
    self.0.as_ref()
  }
}

impl AsMut<dyn Meta> for Object {
  fn as_mut(&mut self) -> &mut (dyn Meta + 'static) {
    self.0.as_mut()
  }
}

impl fmt::Debug for Object {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt_meta(f, "Object", self.0.as_ref())
  }
}

/// Structural equality via [`Meta::eq_dyn`].
impl PartialEq for Object {
  fn eq(&self, other: &Self) -> bool {
    Meta::eq_dyn(self.0.as_ref(), other.0.as_ref())
  }
}

/// Borrowed mutable reflective view of a [`MetaMut`] value.
///
/// # Example
///
/// ```
/// # use typex::{Object, ObjectRefMut};
/// let mut values = vec![23_u8, 23, 42];
/// let mut view = ObjectRefMut::new(&mut values);
///
/// view.item_mut(0).unwrap().set(Object::new(7_u8)).unwrap();
/// view.item_mut(1).unwrap().set(Object::new(42_u8)).unwrap();
///
/// assert_eq!(values, vec![7, 42, 42]);
/// ```
pub struct ObjectRefMut<'a> {
  pub(crate) inner: &'a mut dyn MetaMut,
}

impl<'a> ObjectRefMut<'a> {
  pub(crate) fn path_from<'b>(
    current: &'b mut dyn MetaMut,
    path: &[PathSegment<'_>],
  ) -> Option<ObjectRefMut<'b>> {
    let Some((segment, rest)) = path.split_first() else {
      return Some(ObjectRefMut::new(current));
    };
    let child = match segment {
      PathSegment::Field(name) => current.field_mut(name)?,
      PathSegment::Item(index) => current.item_mut(*index)?,
      PathSegment::Key(key) => current.key_mut(key)?,
    };
    Self::path_from(child.inner, rest)
  }

  /// Creates a borrowed mutable object reference.
  pub fn new(inner: &'a mut dyn MetaMut) -> Self {
    Self { inner }
  }

  /// Returns runtime type metadata for the referenced value.
  pub fn type_info(&self) -> TypeInfo {
    self.inner.type_info()
  }

  /// Returns the Rust type name of the referenced value.
  pub fn type_name(&self) -> &'static str {
    self.inner.type_name()
  }

  /// Returns the type ID of the referenced value.
  pub fn id(&self) -> core::any::TypeId {
    self.inner.id()
  }

  /// Determines the structural shape of the referenced value.
  pub fn kind(&self) -> ValueKind {
    self.inner.kind()
  }

  /// Returns the named or keyed access exposed by the referenced value, if any.
  pub fn access_kind(&self) -> Option<AccessKind> {
    self.inner.access_kind()
  }

  /// Returns the exposed field names in declaration order.
  pub fn field_names(&self) -> &'static [&'static str] {
    self.inner.field_names()
  }

  /// Returns the number of exposed structural items, when available.
  pub fn len(&self) -> Option<usize> {
    self.inner.len()
  }

  /// Checks whether a value has no exposed structural items, when available.
  pub fn is_empty(&self) -> Option<bool> {
    self.inner.is_empty()
  }

  /// Returns the available keys for map-like access.
  pub fn keys(&self) -> Option<Vec<String>> {
    self.inner.keys()
  }

  /// Checks whether the referenced value has type `T`.
  pub fn is<T: 'static>(&self) -> bool {
    self.inner.as_any().is::<T>()
  }

  /// Compares the referenced value against `other` structurally; see
  /// [`Meta::eq_dyn`].
  pub fn eq_dyn(&self, other: &ObjectRefMut<'_>) -> bool {
    self.inner.as_meta().eq_dyn(other.inner.as_meta())
  }

  /// Returns a mutable field by name.
  ///
  /// Returns [`ReflectiveError::PathNotFound`] when no field named `name` is
  /// exposed.
  pub fn field_mut(&mut self, name: &str) -> Result<ObjectRefMut<'_>, ReflectiveError> {
    self
      .inner
      .field_mut(name)
      .ok_or(ReflectiveError::PathNotFound)
  }

  /// Returns a mutable item at `index`.
  ///
  /// Returns [`ReflectiveError::PathNotFound`] when sequential access is
  /// unsupported or `index` is out of bounds.
  pub fn item_mut(&mut self, index: usize) -> Result<ObjectRefMut<'_>, ReflectiveError> {
    self
      .inner
      .item_mut(index)
      .ok_or(ReflectiveError::PathNotFound)
  }

  /// Returns a mutable value for `key`.
  ///
  /// Returns [`ReflectiveError::PathNotFound`] when map-like access is
  /// unsupported or `key` is not present.
  pub fn key_mut(&mut self, key: &str) -> Result<ObjectRefMut<'_>, ReflectiveError> {
    self.inner.key_mut(key).ok_or(ReflectiveError::PathNotFound)
  }

  /// Inserts or replaces a value under a key when the referenced value exposes
  /// map-like access, returning a mutable view of the resulting value.
  ///
  /// Returns [`ReflectiveError::MutationTypeMismatch`] when keyed insertion is
  /// unsupported, the key cannot be built from `key` or `value` has an
  /// incompatible concrete type.
  pub fn insert_key(
    &mut self,
    key: &str,
    value: Object,
  ) -> Result<ObjectRefMut<'_>, ReflectiveError> {
    let child = self
      .inner
      .insert_key(key, value)
      .map_err(|_| ReflectiveError::MutationTypeMismatch)?;
    Ok(child)
  }

  /// Inserts a value at `index` for sequential access, returning a mutable
  /// view of the inserted item.
  ///
  /// Returns [`ReflectiveError::MutationTypeMismatch`] when indexed insertion
  /// is unsupported, `index` is out of bounds or `value` has an incompatible
  /// concrete type.
  pub fn insert_item(
    &mut self,
    index: usize,
    value: Object,
  ) -> Result<ObjectRefMut<'_>, ReflectiveError> {
    let child = self
      .inner
      .insert_item(index, value)
      .map_err(|_| ReflectiveError::MutationTypeMismatch)?;
    Ok(child)
  }

  /// Appends a value when the referenced value exposes sequential access,
  /// returning a mutable view of the appended value.
  ///
  /// Returns [`ReflectiveError::MutationTypeMismatch`] when appending is
  /// unsupported or `value` has an incompatible concrete type.
  pub fn push_item(&mut self, value: Object) -> Result<ObjectRefMut<'_>, ReflectiveError> {
    let child = self
      .inner
      .push_item(value)
      .map_err(|_| ReflectiveError::MutationTypeMismatch)?;
    Ok(child)
  }

  /// Removes the value for `key`.
  ///
  /// Returns [`ReflectiveError::PathNotFound`] when keyed removal is
  /// unsupported or `key` is not present.
  pub fn remove_key(&mut self, key: &str) -> Result<Object, ReflectiveError> {
    let removed = self
      .inner
      .remove_key(key)
      .ok_or(ReflectiveError::PathNotFound)?;
    Ok(removed)
  }

  /// Removes the item at `index`.
  ///
  /// Returns [`ReflectiveError::PathNotFound`] when indexed removal is
  /// unsupported or `index` is out of bounds.
  pub fn remove_item(&mut self, index: usize) -> Result<Object, ReflectiveError> {
    let removed = self
      .inner
      .remove_item(index)
      .ok_or(ReflectiveError::PathNotFound)?;
    Ok(removed)
  }

  /// Overwrites the whole referenced value in place with `value`, consuming
  /// this handle. See [`MetaMut::set`].
  ///
  /// Returns [`ReflectiveError::MutationTypeMismatch`] when replacement is
  /// unsupported or `value` has an incompatible concrete type.
  pub fn set(self, value: Object) -> Result<(), ReflectiveError> {
    self
      .inner
      .set(value)
      .map_err(|_| ReflectiveError::MutationTypeMismatch)?;
    Ok(())
  }

  /// Replaces the whole referenced value and returns the previous value. See
  /// [`MetaMut::replace`].
  ///
  /// Returns [`ReflectiveError::MutationTypeMismatch`] when replacement is
  /// unsupported or `value` has an incompatible concrete type.
  pub fn replace(&mut self, value: Object) -> Result<Object, ReflectiveError> {
    self
      .inner
      .replace(value)
      .map_err(|_| ReflectiveError::MutationTypeMismatch)
  }

  /// Applies an ordered list of [`PatchOperation`] values.
  ///
  /// This method is intentionally fallible and non-transactional. Earlier
  /// successful operations remain applied when a later operation fails. Use
  /// [`MutationBatch`] when the whole ordered change must be staged.
  pub fn apply<'p, I>(&mut self, operations: I) -> Result<(), ApplyError<'p>>
  where
    I: IntoIterator<Item = PatchOperation<'p>>,
  {
    apply_patch(self.inner, operations)
  }

  /// Downcasts the mutable reference to `T`, consuming this handle.
  ///
  /// Returns [`ReflectiveError::MutationTypeMismatch`] when the referenced
  /// value is not a `T`.
  pub fn to_mut<T: 'static>(self) -> Result<&'a mut T, ReflectiveError> {
    let value = self
      .inner
      .as_any_mut()
      .downcast_mut::<T>()
      .ok_or(ReflectiveError::MutationTypeMismatch)?;
    Ok(value)
  }

  /// Traverses a nested field, item or key path; see [`FieldPathQueryMut`].
  pub fn field_path_mut<'r, Q>(&'r mut self, query: Q) -> Result<Q::Output, ReflectiveError>
  where
    Q: FieldPathQueryMut<'r>,
  {
    query.resolve(ObjectRefMut {
      inner: &mut *self.inner,
    })
  }

  /// Traverses a nested field, item or key path and overwrites its target.
  ///
  /// A raw path takes an [`Object`] and a [`TypedPath`] takes its `Value`
  /// directly without boxing; see [`FieldPathQueryMut`] for the errors.
  pub fn set_field_path<'r, Q>(
    &'r mut self,
    query: Q,
    value: Q::Value,
  ) -> Result<(), ReflectiveError>
  where
    Q: FieldPathQueryMut<'r>,
  {
    query.assign(
      ObjectRefMut {
        inner: &mut *self.inner,
      },
      value,
    )
  }
}

impl fmt::Debug for ObjectRefMut<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt_meta(f, "ObjectRefMut", self.inner.as_meta())
  }
}

/// Structural equality via [`Meta::eq_dyn`].
impl PartialEq for ObjectRefMut<'_> {
  fn eq(&self, other: &Self) -> bool {
    self.eq_dyn(other)
  }
}

/// Owned mutable reflective value stored as a boxed [`MetaMut`] trait object.
///
/// # Example
///
/// ```
/// # use typex::{ObjectMut, ObjectOps};
/// # use typex::{Meta, MetaMut};
/// #[derive(Meta, MetaMut)]
/// struct Number(u16);
///
/// let mut value = ObjectMut::new(Number(23));
/// *value.item_mut(0).unwrap().to_mut::<u16>().unwrap() = 42;
///
/// let object = value.into_object();
/// assert_eq!(object.to::<Number>().unwrap().0, 42);
/// ```
#[repr(transparent)]
pub struct ObjectMut(Box<dyn MetaMut>);

/// Adapts a boxed [`MetaMut`] to [`Meta`] so that
/// [`ObjectMut::into_object`] can wrap it in an [`Object`].
struct MetaMutObject(Box<dyn MetaMut>);

impl fmt::Debug for MetaMutObject {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt_meta(f, "MetaMutObject", self.0.as_meta())
  }
}

impl Meta for MetaMutObject {
  fn type_info(&self) -> TypeInfo {
    self.0.type_info()
  }

  fn kind(&self) -> ValueKind {
    self.0.kind()
  }

  fn access_kind(&self) -> Option<AccessKind> {
    self.0.access_kind()
  }

  fn option_value(&self) -> Option<ObjectRef<'_>> {
    self.0.option_value()
  }

  fn field(&self, name: &str) -> Option<ObjectRef<'_>> {
    self.0.field(name)
  }

  fn field_names(&self) -> &'static [&'static str] {
    self.0.field_names()
  }

  fn item(&self, index: usize) -> Option<ObjectRef<'_>> {
    self.0.item(index)
  }

  fn len(&self) -> Option<usize> {
    self.0.len()
  }

  fn is_empty(&self) -> Option<bool> {
    self.0.is_empty()
  }

  fn key(&self, key: &str) -> Option<ObjectRef<'_>> {
    self.0.key(key)
  }

  fn keys(&self) -> Option<Vec<String>> {
    self.0.keys()
  }

  fn visit_map_entries(&self, visitor: &mut MapEntryVisitor<'_>) -> bool {
    self.0.visit_map_entries(visitor)
  }

  fn eq_dyn(&self, other: &dyn Meta) -> bool {
    self.0.eq_dyn(other)
  }

  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    let MetaMutObject(inner) = *self;
    Meta::into_any(inner)
  }

  fn as_any(&self) -> &dyn Any {
    Meta::as_any(self.0.as_ref())
  }
}

impl ObjectMut {
  /// Boxes a mutable value with reflective access as an [`ObjectMut`].
  pub fn new<T>(value: T) -> Self
  where
    T: MetaMut + 'static,
  {
    Self(Box::new(value))
  }

  /// Clones a concrete mutable value into an [`ObjectMut`].
  ///
  /// The concrete value must implement [`Clone`].
  pub fn from_clone<T>(value: &T) -> Self
  where
    T: MetaMut + Clone + 'static,
  {
    Self::new(value.clone())
  }

  /// Returns the underlying boxed [`MetaMut`] trait object.
  pub fn into_inner(self) -> Box<dyn MetaMut> {
    self.0
  }

  /// Converts the owned mutable value into an immutable reflective object.
  pub fn into_object(self) -> Object {
    Object::new(MetaMutObject(self.0))
  }
}

impl From<Box<dyn MetaMut>> for ObjectMut {
  fn from(value: Box<dyn MetaMut>) -> Self {
    Self(value)
  }
}

impl From<ObjectMut> for Object {
  fn from(value: ObjectMut) -> Self {
    value.into_object()
  }
}

impl Deref for ObjectMut {
  type Target = dyn MetaMut;

  fn deref(&self) -> &Self::Target {
    self.0.as_ref()
  }
}

impl DerefMut for ObjectMut {
  fn deref_mut(&mut self) -> &mut Self::Target {
    self.0.as_mut()
  }
}

impl AsRef<dyn MetaMut> for ObjectMut {
  fn as_ref(&self) -> &(dyn MetaMut + 'static) {
    self.0.as_ref()
  }
}

impl AsMut<dyn MetaMut> for ObjectMut {
  fn as_mut(&mut self) -> &mut (dyn MetaMut + 'static) {
    self.0.as_mut()
  }
}

impl fmt::Debug for ObjectMut {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt_meta(f, "ObjectMut", self.0.as_meta())
  }
}

/// Thread-safe counterpart of [`Meta`] for boxed values moved across threads.
///
/// Implemented for every [`Meta`] type that is also `Send` and `Sync`. Most
/// callers should use [`SendObject`] rather than `Box<dyn SendMeta>` directly.
///
/// # Example
///
/// ```
/// use typex::{Meta, ObjectOps, SendMeta};
///
/// #[derive(Debug, Meta)]
/// #[typex(opaque)]
/// struct Job(u32);
///
/// let value: Box<dyn SendMeta> = Box::new(Job(42));
/// let value = std::thread::spawn(move || value).join().unwrap();
/// let object = value.into_object();
/// assert!(object.is::<Job>());
/// ```
pub trait SendMeta: Meta + Send + Sync {
  /// Converts the boxed value back into a plain reflective object.
  fn into_object(self: Box<Self>) -> Object;
}

impl<T> SendMeta for T
where
  T: Meta + Send + Sync + 'static,
{
  fn into_object(self: Box<Self>) -> Object {
    Object::new(*self)
  }
}

/// Owned value with reflective access whose concrete value is `Send` and
/// `Sync`.
///
/// Use this wrapper when a boxed value with reflective access must cross a
/// thread boundary. [`into_object`](Self::into_object) converts it into a
/// plain [`Object`], which no longer carries the thread-safety guarantee.
///
/// # Example
///
/// ```
/// use typex::{Meta, ObjectOps, SendObject};
///
/// #[derive(Debug, Meta)]
/// #[typex(opaque)]
/// struct Job(u32);
///
/// let object = SendObject::new(Job(42));
/// std::thread::spawn(move || assert!(object.is::<Job>()))
///   .join()
///   .unwrap();
/// ```
#[repr(transparent)]
pub struct SendObject(Box<dyn SendMeta>);

impl SendObject {
  /// Boxes a `Send` and `Sync` value with reflective access as a
  /// [`SendObject`].
  pub fn new<T>(value: T) -> Self
  where
    T: Meta + Send + Sync + 'static,
  {
    Self(Box::new(value))
  }

  /// Returns the underlying boxed [`SendMeta`] trait object.
  pub fn into_inner(self) -> Box<dyn SendMeta> {
    self.0
  }

  /// Converts this value into a plain [`Object`].
  pub fn into_object(self) -> Object {
    SendMeta::into_object(self.0)
  }
}

impl From<Box<dyn SendMeta>> for SendObject {
  fn from(value: Box<dyn SendMeta>) -> Self {
    Self(value)
  }
}

impl Deref for SendObject {
  type Target = dyn SendMeta;

  fn deref(&self) -> &Self::Target {
    self.0.as_ref()
  }
}

impl AsRef<dyn SendMeta> for SendObject {
  fn as_ref(&self) -> &(dyn SendMeta + 'static) {
    self.0.as_ref()
  }
}

impl fmt::Debug for SendObject {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt_meta(f, "SendObject", self.0.as_ref())
  }
}

/// Conversion and downcasting helpers for owned reflective objects.
///
/// Implemented for [`Object`] and [`SendObject`]. Owned conversion consumes the
/// wrapper and returns the reflected value as an [`Object`] when the requested
/// concrete type does not match, while borrowed conversion leaves the wrapper
/// in place.
pub trait ObjectOps {
  /// Checks whether the object stores a `T`.
  fn is<T: 'static>(&self) -> bool;

  /// Converts the boxed object into `T`, returning the reflected value as an
  /// [`Object`] when its concrete type does not match.
  ///
  /// To try several target types, use [`Result::or_else`] to call `to` again
  /// on the returned [`Object`], as shown in the example.
  ///
  /// # Example
  ///
  /// ```
  /// use typex::{Meta, ObjectOps, SendObject};
  ///
  /// #[derive(Debug, Meta)]
  /// #[typex(opaque)]
  /// struct Count(u8);
  ///
  /// #[derive(Debug, Meta)]
  /// #[typex(opaque)]
  /// struct Label(&'static str);
  ///
  /// let describe = |object: SendObject| {
  ///   object
  ///     .to::<Count>()
  ///     .map(|count| count.0.to_string())
  ///     .or_else(|remaining| {
  ///       remaining
  ///         .to::<Label>()
  ///         .map(|label| label.0.to_string())
  ///     })
  ///     .unwrap()
  /// };
  ///
  /// assert_eq!(describe(SendObject::new(Count(23))), "23");
  /// assert_eq!(describe(SendObject::new(Label("hello"))), "hello");
  /// ```
  fn to<T: 'static>(self) -> Result<T, Object>;

  /// Borrows the boxed object as `T`, returning `None` when its type does not
  /// match.
  fn to_ref<T: 'static>(&self) -> Option<&T>;
}

impl ObjectOps for Object {
  fn is<T: 'static>(&self) -> bool {
    self.0.as_any().is::<T>()
  }

  fn to<T: 'static>(self) -> Result<T, Object> {
    if self.is::<T>() {
      // The preceding type check guarantees that this downcast succeeds.
      Ok(*self.into_inner().into_any().downcast::<T>().unwrap())
    } else {
      Err(self)
    }
  }

  fn to_ref<T: 'static>(&self) -> Option<&T> {
    self.0.as_any().downcast_ref::<T>()
  }
}

impl ObjectOps for SendObject {
  fn is<T: 'static>(&self) -> bool {
    self.0.as_ref().as_any().is::<T>()
  }

  fn to<T: 'static>(self) -> Result<T, Object> {
    if self.is::<T>() {
      // The preceding type check guarantees that this downcast succeeds.
      Ok(*Meta::into_any(self.0).downcast::<T>().unwrap())
    } else {
      Err(SendMeta::into_object(self.0))
    }
  }

  fn to_ref<T: 'static>(&self) -> Option<&T> {
    self.as_ref().as_any().downcast_ref::<T>()
  }
}
