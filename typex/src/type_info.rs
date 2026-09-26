use core::any::TypeId;
use core::fmt::Debug;
use core::hash::Hash;

/// Shallow metadata about a type, containing its qualified Rust name and type
/// ID. Equality and hashing are based only on the type ID.
///
/// # Example
/// ```
/// use std::collections::HashSet;
/// use typex::TypeInfo;
///
/// struct UserId(u64);
///
/// let info = TypeInfo::of::<UserId>();
///
/// assert_eq!(info.type_name(), core::any::type_name::<UserId>());
/// assert_eq!(info.id(), TypeInfo::of::<UserId>().id());
/// assert_eq!(info, TypeInfo::of::<UserId>());
/// assert!(HashSet::from([info]).contains(&TypeInfo::of::<UserId>()));
/// ```
#[derive(Copy, Clone)]
pub struct TypeInfo {
  name: &'static str,
  id: TypeId,
}

impl TypeInfo {
  /// Returns shallow type metadata for `T`.
  pub fn of<T: ?Sized + 'static>() -> Self {
    TypeInfo {
      name: core::any::type_name::<T>(),
      id: TypeId::of::<T>(),
    }
  }

  /// Returns the Rust type name, equivalent to [`core::any::type_name`].
  pub fn type_name(&self) -> &'static str {
    self.name
  }

  /// Returns the type's [`TypeId`].
  pub fn id(&self) -> TypeId {
    self.id
  }
}

/// Hashes the type ID of a [`TypeInfo`] value.
///
/// [`TypeInfo`] values can only be created through [`TypeInfo::of`], so hashing
/// the type ID is sufficient.
impl Hash for TypeInfo {
  fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
    self.id.hash(state);
  }
}

/// Compares the type IDs of [`TypeInfo`] values.
///
/// [`TypeInfo`] values can only be created through [`TypeInfo::of`], so matching
/// type IDs is sufficient.
impl PartialEq for TypeInfo {
  fn eq(&self, other: &Self) -> bool {
    self.id == other.id
  }
}

impl Eq for TypeInfo {}

/// Formats [`TypeInfo`] values as their qualified Rust type names.
impl Debug for TypeInfo {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    f.write_str(self.name)
  }
}
