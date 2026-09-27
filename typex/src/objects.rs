// Keep public API names in scope for short intra-doc links.
#[allow(unused_imports)]
use crate::{
  AccessKind, ApplyError, EnumAccess, FieldPathQuery, FieldPathQueryMut, MapAccessMut,
  MapEntryVisitor, Meta, MetaMut, MoveItemError, MutationBatch, PatchOperation, PathSegment,
  Reflect, ReflectMut, ReflectiveError, SequenceAccessMut, TypeInfo, TypedPath, ValueKind,
  VariantFields, apply_patch,
};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
#[cfg(not(typex_trait_upcasting))]
use core::any::Any;
use core::fmt;

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

/// Returns `current` when it is an enum whose active variant is `name`,
/// forwarding through options.
// Kept out of line so that mutation batch traversal stays small enough to
// inline for paths without variant segments.
#[inline(never)]
pub(crate) fn select_variant<'b>(
  current: &'b mut dyn MetaMut,
  name: &str,
) -> Option<&'b mut dyn MetaMut> {
  match current.is_variant_dyn(name) {
    Some(true) => Some(current),
    Some(false) => None,
    None => select_option_variant(current, name),
  }
}

/// Forwards [`select_variant`] to the value contained in an option, and
/// returns `None` for any other value.
// Mutable path traversal handles a matching enum inline and calls this for
// every other value, so the inlined part stays small.
#[inline(never)]
fn select_option_variant<'b>(
  current: &'b mut dyn MetaMut,
  name: &str,
) -> Option<&'b mut dyn MetaMut> {
  match current.reflect_mut() {
    ReflectMut::Option(value) => select_variant(value.value_mut()?.inner, name),
    _ => None,
  }
}

impl<'a> ObjectRef<'a> {
  /// Creates a borrowed object reference.
  #[inline]
  pub fn new(inner: &'a dyn Meta) -> Self {
    ObjectRef { inner }
  }

  /// Returns the underlying [`Meta`] trait object.
  #[inline]
  pub fn as_meta(&self) -> &'a dyn Meta {
    self.inner
  }

  /// Returns runtime type metadata for the referenced value.
  #[inline]
  pub fn type_info(&self) -> TypeInfo {
    self.inner.type_info()
  }

  /// Returns the Rust type name of the referenced value.
  #[inline]
  pub fn type_name(&self) -> &'static str {
    self.inner.type_info().name()
  }

  /// Returns the concrete type ID of the referenced value.
  #[inline]
  pub fn type_id(&self) -> core::any::TypeId {
    self.inner.type_info().id()
  }

  /// Determines the structural shape of the referenced value.
  #[inline]
  pub fn kind(&self) -> ValueKind {
    self.inner.kind_dyn()
  }

  /// Returns the access kind of the referenced value, if any.
  ///
  /// An option reports the access of its contained value.
  #[inline]
  pub fn access_kind(&self) -> Option<AccessKind> {
    self.inner.access_kind_dyn()
  }

  /// Returns the contained value when this reference points to `Some(value)`.
  #[inline]
  pub fn option_value(&self) -> Option<ObjectRef<'a>> {
    match self.inner.reflect() {
      Reflect::Option(value) => value,
      _ => None,
    }
  }

  /// Returns the value when it is an enum whose active variant is `name`,
  /// forwarding through options.
  #[inline]
  pub fn variant(&self, name: &str) -> Option<ObjectRef<'a>> {
    match self.inner.is_variant_dyn(name) {
      Some(true) => Some(*self),
      Some(false) => None,
      None => self.option_variant(name),
    }
  }

  /// Forwards [`Self::variant`] to the value contained in an option.
  // Kept out of line so that path traversal stays small enough to inline.
  #[inline(never)]
  fn option_variant(&self, name: &str) -> Option<ObjectRef<'a>> {
    self.option_value()?.variant(name)
  }

  /// Returns the name of the active enum variant, forwarding through options.
  #[inline]
  pub fn variant_name(&self) -> Option<&'static str> {
    match self.inner.reflect() {
      Reflect::Enum(value) => Some(value.variant_name()),
      Reflect::Option(value) => value?.variant_name(),
      _ => None,
    }
  }

  /// Returns an exposed field by name; see [`Meta::field_dyn`].
  #[inline]
  pub fn field(&self, name: &str) -> Option<ObjectRef<'a>> {
    self.inner.field_dyn(name)
  }

  /// Returns the exposed field names in declaration order.
  ///
  /// An enum exposes the fields of its active variant.
  #[inline]
  pub fn field_names(&self) -> &'static [&'static str] {
    self.inner.field_names_dyn()
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
  #[inline]
  pub fn len(&self) -> Option<usize> {
    self.inner.len_dyn()
  }

  /// Checks whether a value has no exposed structural items, when available.
  #[inline]
  pub fn is_empty(&self) -> Option<bool> {
    self.len().map(|len| len == 0)
  }

  /// Returns a value for a string-like `key`; see [`Meta::key_dyn`].
  #[inline]
  pub fn key(&self, key: &str) -> Option<ObjectRef<'a>> {
    self.inner.key_dyn(key)
  }

  /// Returns the keys as strings for map-like access or a keyed sequence.
  #[inline]
  pub fn keys(&self) -> Option<Vec<String>> {
    match self.inner.reflect() {
      Reflect::Map(value) => value.keys(),
      Reflect::KeyedSequence(value) => Some(value.keys()),
      Reflect::Option(value) => value?.keys(),
      _ => None,
    }
  }

  /// Passes each map entry to `visitor` until it returns `false`.
  ///
  /// Returns `false` when the value has no map-like access. Stopping early
  /// still returns `true`.
  #[inline]
  pub fn visit_map_entries(&self, visitor: &mut MapEntryVisitor<'_>) -> bool {
    self.inner.visit_map_entries_dyn(visitor)
  }

  /// Downcasts the reference to `T`.
  pub fn to_ref<T: 'static>(&self) -> Option<&'a T> {
    self.inner.as_any().downcast_ref::<T>()
  }

  /// Checks whether the referenced value has type `T`.
  pub fn is<T: 'static>(&self) -> bool {
    self.inner.as_any().is::<T>()
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
  #[inline]
  fn eq(&self, other: &Self) -> bool {
    self.inner.eq_dyn(other.inner)
  }
}

/// Generates read accessors that forward to [`ObjectRef`] through each
/// wrapper's `as_object_ref` method, so that every wrapper shares the
/// implementation above.
macro_rules! forward_object_reads {
  ($ty:ty) => {
    impl $ty {
      /// Returns runtime type metadata; see [`ObjectRef::type_info`].
      #[inline]
      pub fn type_info(&self) -> TypeInfo {
        self.as_object_ref().type_info()
      }

      /// Returns the Rust type name; see [`ObjectRef::type_name`].
      #[inline]
      pub fn type_name(&self) -> &'static str {
        self.as_object_ref().type_name()
      }

      /// Returns the concrete type ID; see [`ObjectRef::type_id`].
      #[inline]
      pub fn type_id(&self) -> core::any::TypeId {
        self.as_object_ref().type_id()
      }

      /// Returns the structural kind; see [`ObjectRef::kind`].
      #[inline]
      pub fn kind(&self) -> ValueKind {
        self.as_object_ref().kind()
      }

      /// Returns the named or indexed access; see [`ObjectRef::access_kind`].
      #[inline]
      pub fn access_kind(&self) -> Option<AccessKind> {
        self.as_object_ref().access_kind()
      }

      /// Returns the contained value of an option; see
      /// [`ObjectRef::option_value`].
      #[inline]
      pub fn option_value(&self) -> Option<ObjectRef<'_>> {
        self.as_object_ref().option_value()
      }

      /// Returns the enum when its active variant is `name`; see [`ObjectRef::variant`].
      #[inline]
      pub fn variant(&self, name: &str) -> Option<ObjectRef<'_>> {
        self.as_object_ref().variant(name)
      }

      /// Returns the active enum variant name; see [`ObjectRef::variant_name`].
      #[inline]
      pub fn variant_name(&self) -> Option<&'static str> {
        self.as_object_ref().variant_name()
      }

      /// Returns an exposed field by name; see [`ObjectRef::field`].
      #[inline]
      pub fn field(&self, name: &str) -> Option<ObjectRef<'_>> {
        self.as_object_ref().field(name)
      }

      /// Returns the exposed field names; see [`ObjectRef::field_names`].
      #[inline]
      pub fn field_names(&self) -> &'static [&'static str] {
        self.as_object_ref().field_names()
      }

      /// Returns an item at `index`; see [`ObjectRef::item`].
      #[inline]
      pub fn item(&self, index: usize) -> Option<ObjectRef<'_>> {
        self.as_object_ref().item(index)
      }

      /// Returns the number of exposed items; see [`ObjectRef::len`].
      #[inline]
      pub fn len(&self) -> Option<usize> {
        self.as_object_ref().len()
      }

      /// Checks whether there are no exposed items; see [`ObjectRef::is_empty`].
      #[inline]
      pub fn is_empty(&self) -> Option<bool> {
        self.as_object_ref().is_empty()
      }

      /// Returns a value for `key`; see [`ObjectRef::key`].
      #[inline]
      pub fn key(&self, key: &str) -> Option<ObjectRef<'_>> {
        self.as_object_ref().key(key)
      }

      /// Returns the keys as strings; see [`ObjectRef::keys`].
      #[inline]
      pub fn keys(&self) -> Option<Vec<String>> {
        self.as_object_ref().keys()
      }

      /// Passes each map entry to `visitor`; see
      /// [`ObjectRef::visit_map_entries`].
      #[inline]
      pub fn visit_map_entries(&self, visitor: &mut MapEntryVisitor<'_>) -> bool {
        self.as_object_ref().visit_map_entries(visitor)
      }

      /// Traverses a nested field, item or key path; see
      /// [`ObjectRef::field_path`].
      pub fn field_path<'s, Q: FieldPathQuery<'s>>(&'s self, query: Q) -> Option<Q::Output> {
        self.as_object_ref().field_path(query)
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
  #[inline]
  pub fn into_inner(self) -> Box<dyn Meta> {
    self.0
  }

  /// Returns a borrowed [`ObjectRef`] view of the value.
  #[inline]
  pub fn as_object_ref(&self) -> ObjectRef<'_> {
    ObjectRef::new(self.0.as_ref())
  }
}

impl fmt::Debug for Object {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt_meta(f, "Object", self.0.as_ref())
  }
}

/// Structural equality via [`Meta::eq_dyn`].
impl PartialEq for Object {
  #[inline]
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
  #[inline]
  pub(crate) fn path_from<'b>(
    mut current: &'b mut dyn MetaMut,
    mut path: &[PathSegment<'_>],
  ) -> Option<ObjectRefMut<'b>> {
    while let Some((segment, rest)) = path.split_first() {
      current = match segment {
        PathSegment::Field(name) => current.field_mut_dyn(name)?.inner,
        PathSegment::Item(index) => current.item_mut_dyn(*index)?.inner,
        PathSegment::Key(key) => current.key_mut_dyn(key)?.inner,
        // A tail call keeps no value alive across the variant check, so paths
        // without variant segments save no extra registers.
        PathSegment::Variant(name) => return Self::path_from_variant(current, name, rest),
      };
      path = rest;
    }
    Some(ObjectRefMut::new(current))
  }

  /// Selects the variant `name` of `current`, then traverses `rest`.
  #[inline(never)]
  fn path_from_variant<'b>(
    current: &'b mut dyn MetaMut,
    name: &str,
    rest: &[PathSegment<'_>],
  ) -> Option<ObjectRefMut<'b>> {
    let current = match current.is_variant_dyn(name) {
      Some(true) => current,
      _ => select_option_variant(current, name)?,
    };
    Self::path_from(current, rest)
  }

  /// Creates a borrowed mutable object reference.
  #[inline]
  pub fn new(inner: &'a mut dyn MetaMut) -> Self {
    Self { inner }
  }

  /// Returns a borrowed [`ObjectRef`] view of the value.
  #[inline]
  pub fn as_object_ref(&self) -> ObjectRef<'_> {
    #[cfg(typex_trait_upcasting)]
    {
      ObjectRef::new(self.inner)
    }
    #[cfg(not(typex_trait_upcasting))]
    {
      ObjectRef::new(self.inner.as_meta())
    }
  }

  /// Checks whether the referenced value has type `T`.
  pub fn is<T: 'static>(&self) -> bool {
    self.inner.as_any().is::<T>()
  }

  /// Returns a mutable field by name.
  ///
  /// Returns [`ReflectiveError::PathNotFound`] when no field named `name` is
  /// exposed.
  #[inline]
  pub fn field_mut(&mut self, name: &str) -> Result<ObjectRefMut<'_>, ReflectiveError> {
    self
      .inner
      .field_mut_dyn(name)
      .ok_or(ReflectiveError::PathNotFound)
  }

  /// Returns a mutable enum view when its active variant is `name`, forwarding
  /// through options. Returns [`ReflectiveError::PathNotFound`] when no
  /// matching enum is present.
  #[inline]
  pub fn variant_mut(&mut self, name: &str) -> Result<ObjectRefMut<'_>, ReflectiveError> {
    select_variant(self.inner, name)
      .map(ObjectRefMut::new)
      .ok_or(ReflectiveError::PathNotFound)
  }

  /// Returns a mutable item at `index`.
  ///
  /// Returns [`ReflectiveError::PathNotFound`] when sequential access is
  /// unsupported or `index` is out of bounds.
  #[inline]
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
  #[inline]
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
  #[inline]
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
  #[inline]
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
  #[inline]
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
  #[inline]
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
  #[inline]
  pub fn remove_item(&mut self, index: usize) -> Result<Object, ReflectiveError> {
    self
      .inner
      .remove_item_dyn(index)
      .ok_or(ReflectiveError::PathNotFound)
  }

  /// Moves an item; see [`SequenceAccessMut::move_item`].
  #[inline]
  pub fn move_item(&mut self, from: usize, to: usize) -> Result<(), MoveItemError> {
    self.inner.move_item_dyn(from, to)
  }

  /// Overwrites the whole referenced value in place with `value`, consuming
  /// this handle. See [`MetaMut::set_dyn`].
  ///
  /// Returns [`ReflectiveError::MutationTypeMismatch`] when replacement is
  /// unsupported or `value` has an incompatible concrete type.
  #[inline]
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
  #[inline]
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

/// Converts a mutable view into a shared view that keeps the lifetime `'a`.
impl<'a> From<ObjectRefMut<'a>> for ObjectRef<'a> {
  fn from(value: ObjectRefMut<'a>) -> Self {
    let inner: &'a dyn MetaMut = value.inner;
    #[cfg(typex_trait_upcasting)]
    {
      ObjectRef::new(inner)
    }
    #[cfg(not(typex_trait_upcasting))]
    {
      ObjectRef::new(inner.as_meta())
    }
  }
}

impl fmt::Debug for ObjectRefMut<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt_meta(f, "ObjectRefMut", self.as_object_ref().as_meta())
  }
}

/// Structural equality via [`Meta::eq_dyn`].
impl PartialEq for ObjectRefMut<'_> {
  #[inline]
  fn eq(&self, other: &Self) -> bool {
    self.inner.eq_dyn(other.as_object_ref().as_meta())
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

/// Adapts a boxed [`MetaMut`] to [`Meta`] for [`ObjectMut::into_object`] on
/// Rust 1.85, which lacks trait-object upcasting.
#[cfg(not(typex_trait_upcasting))]
struct MetaMutObject(Box<dyn MetaMut>);

#[cfg(not(typex_trait_upcasting))]
impl fmt::Debug for MetaMutObject {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt_meta(f, "MetaMutObject", self.0.as_meta())
  }
}

#[cfg(not(typex_trait_upcasting))]
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
  #[inline]
  pub fn into_inner(self) -> Box<dyn MetaMut> {
    self.0
  }

  /// Converts the owned mutable value into an immutable reflective object.
  /// Uses direct trait-object upcasting on Rust 1.86 and later; Rust 1.85
  /// allocates an adapter around the existing box.
  #[inline]
  pub fn into_object(self) -> Object {
    #[cfg(typex_trait_upcasting)]
    {
      Object(self.0)
    }
    #[cfg(not(typex_trait_upcasting))]
    {
      Object::new(MetaMutObject(self.0))
    }
  }

  /// Returns a borrowed [`ObjectRef`] view of the value.
  #[inline]
  pub fn as_object_ref(&self) -> ObjectRef<'_> {
    #[cfg(typex_trait_upcasting)]
    {
      ObjectRef::new(self.0.as_ref())
    }
    #[cfg(not(typex_trait_upcasting))]
    {
      ObjectRef::new(self.0.as_meta())
    }
  }

  /// Returns a borrowed [`ObjectRefMut`] view of the value.
  #[inline]
  pub fn as_object_ref_mut(&mut self) -> ObjectRefMut<'_> {
    ObjectRefMut::new(self.0.as_mut())
  }

  /// Returns a mutable field by name; see [`MetaMut::field_mut_dyn`].
  #[inline]
  pub fn field_mut(&mut self, name: &str) -> Option<ObjectRefMut<'_>> {
    self.0.field_mut_dyn(name)
  }

  /// Returns a mutable enum view when its active variant is `name`, forwarding
  /// through options. Returns `None` when no matching enum is present.
  #[inline]
  pub fn variant_mut(&mut self, name: &str) -> Option<ObjectRefMut<'_>> {
    select_variant(self.0.as_mut(), name).map(ObjectRefMut::new)
  }

  /// Returns a mutable item at `index`; see [`MetaMut::item_mut_dyn`].
  #[inline]
  pub fn item_mut(&mut self, index: usize) -> Option<ObjectRefMut<'_>> {
    self.0.item_mut_dyn(index)
  }

  /// Returns a mutable value for `key`; see [`MetaMut::key_mut_dyn`].
  #[inline]
  pub fn key_mut(&mut self, key: &str) -> Option<ObjectRefMut<'_>> {
    self.0.key_mut_dyn(key)
  }

  /// Overwrites the whole value; see [`MetaMut::set_dyn`].
  #[inline]
  pub fn set(&mut self, value: Object) -> Result<(), Object> {
    self.0.set_dyn(value)
  }

  /// Replaces the whole value and returns the previous value; see
  /// [`MetaMut::replace_dyn`].
  #[inline]
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
    query.resolve(self.as_object_ref_mut())
  }

  /// Inserts `value` under `key`; see [`MapAccessMut::insert_key`].
  ///
  /// Options forward the insertion to their contained value.
  #[inline]
  pub fn insert_key(&mut self, key: &str, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    self.0.insert_key_dyn(key, value)
  }

  /// Inserts `value` at `index`; see [`SequenceAccessMut::insert_item`].
  ///
  /// An option without a value accepts an insertion at index 0.
  #[inline]
  pub fn insert_item(&mut self, index: usize, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    self.0.insert_item_dyn(index, value)
  }

  /// Appends `value`; see [`SequenceAccessMut::push_item`].
  #[inline]
  pub fn push_item(&mut self, value: Object) -> Result<ObjectRefMut<'_>, Object> {
    self.0.push_item_dyn(value)
  }

  /// Removes and returns the value stored under `key`.
  ///
  /// Options forward the removal to their contained value.
  #[inline]
  pub fn remove_key(&mut self, key: &str) -> Option<Object> {
    self.0.remove_key_dyn(key)
  }

  /// Removes and returns the item at `index`.
  ///
  /// An option gives up its contained value at index 0.
  #[inline]
  pub fn remove_item(&mut self, index: usize) -> Option<Object> {
    self.0.remove_item_dyn(index)
  }

  /// Moves an item; see [`SequenceAccessMut::move_item`].
  #[inline]
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

impl From<ObjectMut> for Object {
  fn from(value: ObjectMut) -> Self {
    value.into_object()
  }
}

impl fmt::Debug for ObjectMut {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt_meta(f, "ObjectMut", self.as_object_ref().as_meta())
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
    Object(self)
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
  #[inline]
  pub fn into_inner(self) -> Box<dyn SendMeta> {
    self.0
  }

  /// Converts this value into a plain [`Object`].
  /// Preserves the existing allocation and concrete type.
  #[inline]
  pub fn into_object(self) -> Object {
    #[cfg(typex_trait_upcasting)]
    {
      Object(self.0)
    }
    #[cfg(not(typex_trait_upcasting))]
    {
      SendMeta::into_object(self.0)
    }
  }

  /// Returns a borrowed [`ObjectRef`] view of the value.
  #[inline]
  pub fn as_object_ref(&self) -> ObjectRef<'_> {
    #[cfg(typex_trait_upcasting)]
    {
      ObjectRef::new(self.0.as_ref())
    }
    #[cfg(not(typex_trait_upcasting))]
    {
      ObjectRef::new(self.0.as_meta())
    }
  }
}

impl From<SendObject> for Object {
  fn from(value: SendObject) -> Self {
    value.into_object()
  }
}

impl fmt::Debug for SendObject {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt_meta(f, "SendObject", self.as_object_ref().as_meta())
  }
}

/// Conversion and downcasting helpers for owned reflective objects.
///
/// Implemented for [`Object`], [`ObjectMut`] and [`SendObject`]. Owned
/// conversion consumes the wrapper and returns the reflected value as an
/// [`Object`] when the requested concrete type does not match, while borrowed
/// conversion leaves the wrapper in place.
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

impl ObjectOps for ObjectMut {
  #[inline]
  fn is<T: 'static>(&self) -> bool {
    self.0.as_any().is::<T>()
  }

  fn to<T: 'static>(self) -> Result<T, Object> {
    if self.is::<T>() {
      // The preceding type check guarantees that this downcast succeeds.
      Ok(*Meta::into_any(self.0).downcast::<T>().unwrap())
    } else {
      Err(self.into_object())
    }
  }

  #[inline]
  fn to_ref<T: 'static>(&self) -> Option<&T> {
    self.0.as_any().downcast_ref::<T>()
  }
}

impl ObjectOps for SendObject {
  fn is<T: 'static>(&self) -> bool {
    self.0.as_any().is::<T>()
  }

  fn to<T: 'static>(self) -> Result<T, Object> {
    if self.is::<T>() {
      // The preceding type check guarantees that this downcast succeeds.
      Ok(*Meta::into_any(self.0).downcast::<T>().unwrap())
    } else {
      Err(self.into_object())
    }
  }

  fn to_ref<T: 'static>(&self) -> Option<&T> {
    self.0.as_any().downcast_ref::<T>()
  }
}
