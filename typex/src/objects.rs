// Keep public API names in scope for short intra-doc links.
#[allow(unused_imports)]
use crate::{
  AccessKind, ApplyError, FieldPathQuery, FieldPathQueryMut, MapAccessMut, MapEntryVisitor, Meta,
  MetaMut, MoveItemError, MutationBatch, PatchOperation, PathSegment, Reflect, ReflectMut,
  ReflectiveError, SequenceAccessMut, TypeInfo, TypedPath, ValueKind, apply_patch,
};
use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::any::Any;
use core::fmt;
use core::ops::{Deref, DerefMut};

fn fmt_meta(f: &mut fmt::Formatter<'_>, name: &str, meta: &dyn Meta) -> fmt::Result {
  let view = ObjectRef::new(meta);
  f.debug_struct(name)
    .field("type_name", &view.type_name())
    .field("kind", &view.kind())
    .finish()
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
    self.inner.type_info().type_name()
  }

  /// Returns the type ID of the referenced value.
  pub fn id(&self) -> core::any::TypeId {
    self.inner.type_info().id()
  }

  /// Determines the structural shape of the referenced value.
  pub fn kind(&self) -> ValueKind {
    self.inner.reflect().kind()
  }

  /// Returns the named or keyed access exposed by the referenced value, if any.
  ///
  /// An option reports the access of its contained value.
  pub fn access_kind(&self) -> Option<AccessKind> {
    match self.inner.reflect() {
      Reflect::Scalar => None,
      Reflect::Struct(value) => (!value.field_names().is_empty()).then_some(AccessKind::Field),
      Reflect::Sequence(_) => Some(AccessKind::Item),
      Reflect::Map(value) => Some(value.access_kind()),
      Reflect::Option(value) => value?.access_kind(),
    }
  }

  /// Returns the contained value when this reference points to `Some(value)`.
  pub fn option_value(&self) -> Option<ObjectRef<'a>> {
    match self.inner.reflect() {
      Reflect::Option(value) => value,
      _ => None,
    }
  }

  /// Returns an exposed field by name; see [`Meta::field_dyn`].
  #[inline]
  pub fn field(&self, name: &str) -> Option<ObjectRef<'a>> {
    self.inner.field_dyn(name)
  }

  /// Returns the exposed field names in declaration order.
  pub fn field_names(&self) -> &'static [&'static str] {
    match self.inner.reflect() {
      Reflect::Struct(value) => value.field_names(),
      Reflect::Option(Some(value)) => value.field_names(),
      _ => &[],
    }
  }

  /// Returns an item at `index`; see [`Meta::item_dyn`].
  #[inline]
  pub fn item(&self, index: usize) -> Option<ObjectRef<'a>> {
    self.inner.item_dyn(index)
  }

  /// Returns the number of exposed structural items, when available.
  ///
  /// For maps, this is the number of keyed entries. An option has one item
  /// when it holds a value.
  pub fn len(&self) -> Option<usize> {
    match self.inner.reflect() {
      Reflect::Struct(value) => value.len(),
      Reflect::Sequence(value) => Some(value.len()),
      Reflect::Map(value) => Some(value.len()),
      Reflect::Option(value) => Some(usize::from(value.is_some())),
      Reflect::Scalar => None,
    }
  }

  /// Checks whether a value has no exposed structural items, when available.
  pub fn is_empty(&self) -> Option<bool> {
    self.len().map(|len| len == 0)
  }

  /// Returns a value for a string-like `key`; see [`Meta::key_dyn`].
  #[inline]
  pub fn key(&self, key: &str) -> Option<ObjectRef<'a>> {
    self.inner.key_dyn(key)
  }

  /// Returns the keys as strings for map-like access.
  pub fn keys(&self) -> Option<Vec<String>> {
    match self.inner.reflect() {
      Reflect::Map(value) => value.keys(),
      Reflect::Option(value) => value?.keys(),
      _ => None,
    }
  }

  /// Passes each map entry to `visitor` until it returns `false`.
  ///
  /// Returns `false` when the value has no map-like access. Stopping early
  /// still returns `true`.
  pub fn visit_map_entries(&self, visitor: &mut MapEntryVisitor<'_>) -> bool {
    match self.inner.reflect() {
      Reflect::Map(value) => {
        value.visit_entries(visitor);
        true
      }
      Reflect::Option(Some(value)) => value.visit_map_entries(visitor),
      _ => false,
    }
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

/// Generates read accessors that forward to [`ObjectRef`] through a private
/// `view` method, so that every wrapper shares the implementation above.
macro_rules! forward_object_reads {
  ($ty:ty) => {
    impl $ty {
      /// Returns the Rust type name; see [`ObjectRef::type_name`].
      pub fn type_name(&self) -> &'static str {
        self.view().type_name()
      }

      /// Returns the type ID; see [`ObjectRef::id`].
      pub fn id(&self) -> core::any::TypeId {
        self.view().id()
      }

      /// Returns the structural kind; see [`ObjectRef::kind`].
      pub fn kind(&self) -> ValueKind {
        self.view().kind()
      }

      /// Returns the named or indexed access; see [`ObjectRef::access_kind`].
      pub fn access_kind(&self) -> Option<AccessKind> {
        self.view().access_kind()
      }

      /// Returns the contained value of an option; see
      /// [`ObjectRef::option_value`].
      pub fn option_value(&self) -> Option<ObjectRef<'_>> {
        self.view().option_value()
      }

      /// Returns an exposed field by name; see [`ObjectRef::field`].
      pub fn field(&self, name: &str) -> Option<ObjectRef<'_>> {
        self.view().field(name)
      }

      /// Returns the exposed field names; see [`ObjectRef::field_names`].
      pub fn field_names(&self) -> &'static [&'static str] {
        self.view().field_names()
      }

      /// Returns an item at `index`; see [`ObjectRef::item`].
      pub fn item(&self, index: usize) -> Option<ObjectRef<'_>> {
        self.view().item(index)
      }

      /// Returns the number of exposed items; see [`ObjectRef::len`].
      pub fn len(&self) -> Option<usize> {
        self.view().len()
      }

      /// Checks whether there are no exposed items; see [`ObjectRef::is_empty`].
      pub fn is_empty(&self) -> Option<bool> {
        self.view().is_empty()
      }

      /// Returns a value for `key`; see [`ObjectRef::key`].
      pub fn key(&self, key: &str) -> Option<ObjectRef<'_>> {
        self.view().key(key)
      }

      /// Returns the keys as strings; see [`ObjectRef::keys`].
      pub fn keys(&self) -> Option<Vec<String>> {
        self.view().keys()
      }

      /// Passes each map entry to `visitor`; see
      /// [`ObjectRef::visit_map_entries`].
      pub fn visit_map_entries(&self, visitor: &mut MapEntryVisitor<'_>) -> bool {
        self.view().visit_map_entries(visitor)
      }

      /// Traverses a nested field, item or key path; see
      /// [`ObjectRef::field_path`].
      pub fn field_path<'s, Q: FieldPathQuery<'s>>(&'s self, query: Q) -> Option<Q::Output> {
        self.view().field_path(query)
      }
    }
  };
}

forward_object_reads!(Object);
forward_object_reads!(ObjectRefMut<'_>);
forward_object_reads!(ObjectMut);
forward_object_reads!(SendObject);

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

  fn view(&self) -> ObjectRef<'_> {
    ObjectRef::new(self.0.as_ref())
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
      PathSegment::Field(name) => current.field_mut_dyn(name)?,
      PathSegment::Item(index) => current.item_mut_dyn(*index)?,
      PathSegment::Key(key) => current.key_mut_dyn(key)?,
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

  fn view(&self) -> ObjectRef<'_> {
    ObjectRef::new(self.inner.as_meta())
  }

  /// Checks whether the referenced value has type `T`.
  pub fn is<T: 'static>(&self) -> bool {
    self.inner.as_any().is::<T>()
  }

  /// Compares the referenced value against `other` structurally; see
  /// [`Meta::eq_dyn`].
  pub fn eq_dyn(&self, other: &ObjectRefMut<'_>) -> bool {
    self.inner.eq_dyn(other.inner.as_meta())
  }

  /// Returns a mutable field by name.
  ///
  /// Returns [`ReflectiveError::PathNotFound`] when no field named `name` is
  /// exposed.
  pub fn field_mut(&mut self, name: &str) -> Result<ObjectRefMut<'_>, ReflectiveError> {
    self
      .inner
      .field_mut_dyn(name)
      .ok_or(ReflectiveError::PathNotFound)
  }

  /// Returns a mutable item at `index`.
  ///
  /// Returns [`ReflectiveError::PathNotFound`] when sequential access is
  /// unsupported or `index` is out of bounds.
  pub fn item_mut(&mut self, index: usize) -> Result<ObjectRefMut<'_>, ReflectiveError> {
    self
      .inner
      .item_mut_dyn(index)
      .ok_or(ReflectiveError::PathNotFound)
  }

  /// Returns a mutable value for `key`.
  ///
  /// Returns [`ReflectiveError::PathNotFound`] when map-like access is
  /// unsupported or `key` is not present.
  pub fn key_mut(&mut self, key: &str) -> Result<ObjectRefMut<'_>, ReflectiveError> {
    self
      .inner
      .key_mut_dyn(key)
      .ok_or(ReflectiveError::PathNotFound)
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
    self
      .inner
      .insert_key_dyn(key, value)
      .map_err(|_| ReflectiveError::MutationTypeMismatch)
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
    self
      .inner
      .insert_item_dyn(index, value)
      .map_err(|_| ReflectiveError::MutationTypeMismatch)
  }

  /// Appends a value when the referenced value exposes sequential access,
  /// returning a mutable view of the appended value.
  ///
  /// Returns [`ReflectiveError::MutationTypeMismatch`] when appending is
  /// unsupported or `value` has an incompatible concrete type.
  pub fn push_item(&mut self, value: Object) -> Result<ObjectRefMut<'_>, ReflectiveError> {
    self
      .inner
      .push_item_dyn(value)
      .map_err(|_| ReflectiveError::MutationTypeMismatch)
  }

  /// Removes the value for `key`.
  ///
  /// Returns [`ReflectiveError::PathNotFound`] when keyed removal is
  /// unsupported or `key` is not present.
  pub fn remove_key(&mut self, key: &str) -> Result<Object, ReflectiveError> {
    self
      .inner
      .remove_key_dyn(key)
      .ok_or(ReflectiveError::PathNotFound)
  }

  /// Removes the item at `index`.
  ///
  /// Returns [`ReflectiveError::PathNotFound`] when indexed removal is
  /// unsupported or `index` is out of bounds.
  pub fn remove_item(&mut self, index: usize) -> Result<Object, ReflectiveError> {
    self
      .inner
      .remove_item_dyn(index)
      .ok_or(ReflectiveError::PathNotFound)
  }

  /// Moves an item; see [`SequenceAccessMut::move_item`].
  pub fn move_item(&mut self, from: usize, to: usize) -> Result<(), MoveItemError> {
    self.inner.move_item_dyn(from, to)
  }

  /// Overwrites the whole referenced value in place with `value`, consuming
  /// this handle. See [`MetaMut::set_dyn`].
  ///
  /// Returns [`ReflectiveError::MutationTypeMismatch`] when replacement is
  /// unsupported or `value` has an incompatible concrete type.
  pub fn set(self, value: Object) -> Result<(), ReflectiveError> {
    self
      .inner
      .set_dyn(value)
      .map_err(|_| ReflectiveError::MutationTypeMismatch)
  }

  /// Replaces the whole referenced value and returns the previous value. See
  /// [`MetaMut::replace_dyn`].
  ///
  /// Returns [`ReflectiveError::MutationTypeMismatch`] when replacement is
  /// unsupported or `value` has an incompatible concrete type.
  pub fn replace(&mut self, value: Object) -> Result<Object, ReflectiveError> {
    self
      .inner
      .replace_dyn(value)
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
    self
      .inner
      .as_any_mut()
      .downcast_mut::<T>()
      .ok_or(ReflectiveError::MutationTypeMismatch)
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

  fn reflect(&self) -> Reflect<'_> {
    self.0.reflect()
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

  fn view(&self) -> ObjectRef<'_> {
    ObjectRef::new(self.0.as_meta())
  }

  /// Returns a mutable field by name; see [`MetaMut::field_mut_dyn`].
  pub fn field_mut(&mut self, name: &str) -> Option<ObjectRefMut<'_>> {
    self.0.field_mut_dyn(name)
  }

  /// Returns a mutable item at `index`; see [`MetaMut::item_mut_dyn`].
  pub fn item_mut(&mut self, index: usize) -> Option<ObjectRefMut<'_>> {
    self.0.item_mut_dyn(index)
  }

  /// Returns a mutable value for `key`; see [`MetaMut::key_mut_dyn`].
  pub fn key_mut(&mut self, key: &str) -> Option<ObjectRefMut<'_>> {
    self.0.key_mut_dyn(key)
  }

  /// Overwrites the whole value; see [`MetaMut::set_dyn`].
  pub fn set(&mut self, value: Object) -> Result<(), Object> {
    self.0.set_dyn(value)
  }

  /// Replaces the whole value and returns the previous value; see
  /// [`MetaMut::replace_dyn`].
  pub fn replace(&mut self, value: Object) -> Result<Object, Object> {
    self.0.replace_dyn(value)
  }

  /// Downcasts the value to `T`.
  pub fn to_mut<T: 'static>(&mut self) -> Option<&mut T> {
    self.0.as_any_mut().downcast_mut::<T>()
  }

  /// Traverses a nested field, item or key path; see [`FieldPathQueryMut`].
  pub fn field_path_mut<'r, Q: FieldPathQueryMut<'r>>(
    &'r mut self,
    query: Q,
  ) -> Result<Q::Output, ReflectiveError> {
    query.resolve(ObjectRefMut::new(self.0.as_mut()))
  }

  /// Inserts `value` under `key`; see [`MapAccessMut::insert_key`].
  ///
  /// Options forward the insertion to their contained value.
  pub fn insert_key(&mut self, key: &str, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    self.0.insert_key_dyn(key, value)
  }

  /// Inserts `value` at `index`; see [`SequenceAccessMut::insert_item`].
  ///
  /// An option without a value accepts an insertion at index 0.
  pub fn insert_item(&mut self, index: usize, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    self.0.insert_item_dyn(index, value)
  }

  /// Appends `value`; see [`SequenceAccessMut::push_item`].
  pub fn push_item(&mut self, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    self.0.push_item_dyn(value)
  }

  /// Removes and returns the value stored under `key`.
  ///
  /// Options forward the removal to their contained value.
  pub fn remove_key(&mut self, key: &str) -> Option<Object> {
    self.0.remove_key_dyn(key)
  }

  /// Removes and returns the item at `index`.
  ///
  /// An option gives up its contained value at index 0.
  pub fn remove_item(&mut self, index: usize) -> Option<Object> {
    self.0.remove_item_dyn(index)
  }

  /// Moves an item; see [`SequenceAccessMut::move_item`].
  pub fn move_item(&mut self, from: usize, to: usize) -> Result<(), MoveItemError> {
    self.0.move_item_dyn(from, to)
  }

  /// Applies an ordered list of [`PatchOperation`] values. A failed operation
  /// does not undo earlier ones. Use [`MutationBatch`] for transactional
  /// application.
  pub fn apply<'p, I>(&mut self, operations: I) -> Result<(), ApplyError<'p>>
  where
    I: IntoIterator<Item = PatchOperation<'p>>,
  {
    apply_patch(self.0.as_mut(), operations)
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

  /// Returns this value as a [`Meta`] trait object.
  ///
  /// Converting `&dyn SendMeta` into `&dyn Meta` through trait upcasting
  /// requires Rust 1.86, but the crate's MSRV is 1.85.
  #[doc(hidden)]
  fn as_meta(&self) -> &dyn Meta;
}

impl<T> SendMeta for T
where
  T: Meta + Send + Sync + 'static,
{
  fn into_object(self: Box<Self>) -> Object {
    Object::new(*self)
  }

  fn as_meta(&self) -> &dyn Meta {
    self
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

  fn view(&self) -> ObjectRef<'_> {
    ObjectRef::new(self.0.as_meta())
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
    fmt_meta(f, "SendObject", self.0.as_meta())
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
