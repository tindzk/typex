use crate::path::OwnedPath;
use crate::{
  Meta, MetaMut, MoveItemError, Object, ObjectRefMut, PathSegment, ReflectiveError, TypeInfo,
  ValueKind,
};
// Keep public API names in scope for short intra-doc links.
#[allow(unused_imports)]
use crate::{MutationBatch, TypedMapAccessMut};
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// Operation in a patch, applied with [`ObjectRefMut::apply`] or staged in a
/// [`MutationBatch`].
///
/// Path segments borrow field and key names for `'a`. Map operations address
/// string-keyed entries; use [`TypedMapAccessMut::key_typed_mut`] for maps
/// with other key types.
///
/// # Example
///
/// Apply a patch directly to a value:
///
/// ```
/// # use typex::{ObjectRefMut, PatchOperation, PathSegment};
/// let mut values = vec![1_u16, 2, 3];
/// let patch = [PatchOperation::set([PathSegment::Item(1)], 7_u16)];
///
/// ObjectRefMut::new(&mut values).apply(patch).unwrap();
/// assert_eq!(values, [1, 7, 3]);
/// ```
///
/// Stage the same patch to undo it later:
///
/// ```
/// # use typex::{MutationBatch, PatchOperation, PathSegment};
/// let mut values = vec![1_u16, 2, 3];
/// let patch = [PatchOperation::set([PathSegment::Item(1)], 7_u16)];
/// let rollback = MutationBatch::from(patch).commit(&mut values).unwrap();
///
/// assert_eq!(values, [1, 7, 3]);
/// rollback.undo(&mut values).unwrap();
/// assert_eq!(values, [1, 2, 3]);
/// ```
#[derive(Debug)]
pub enum PatchOperation<'a> {
  /// Replaces the value at `path`.
  Set {
    /// Path to the value being replaced.
    path: Vec<PathSegment<'a>>,
    /// Replacement value.
    value: Object,
  },
  /// Inserts or replaces a string-keyed map entry.
  InsertKey {
    /// Path to the map.
    path: Vec<PathSegment<'a>>,
    /// Entry key.
    key: String,
    /// Entry value.
    value: Object,
  },
  /// Removes a string-keyed map entry.
  RemoveKey {
    /// Path to the map.
    path: Vec<PathSegment<'a>>,
    /// Entry key.
    key: String,
  },
  /// Inserts an item before an index.
  InsertItem {
    /// Path to the sequence.
    path: Vec<PathSegment<'a>>,
    /// Insertion index.
    index: usize,
    /// Item value.
    value: Object,
  },
  /// Appends an item to a sequence.
  PushItem {
    /// Path to the sequence.
    path: Vec<PathSegment<'a>>,
    /// Item value.
    value: Object,
  },
  /// Removes an item at an index.
  RemoveItem {
    /// Path to the sequence.
    path: Vec<PathSegment<'a>>,
    /// Item index.
    index: usize,
  },
  /// Moves an item within a sequence.
  MoveItem {
    /// Path to the sequence.
    path: Vec<PathSegment<'a>>,
    /// Source index.
    from: usize,
    /// Index of the item to move before, or the length to move to the end.
    to: usize,
  },
}

/// One owned operation waiting for a staged commit.
#[derive(Debug)]
pub enum OwnedPatchOperation {
  /// Replaces the value at `path`.
  Set {
    /// Path to the value being replaced.
    path: OwnedPath,
    /// Replacement value.
    value: Object,
  },
  /// Inserts or replaces a string-keyed map entry.
  InsertKey {
    /// Path to the map.
    path: OwnedPath,
    /// Entry key.
    key: String,
    /// Entry value.
    value: Object,
  },
  /// Removes a string-keyed map entry.
  RemoveKey {
    /// Path to the map.
    path: OwnedPath,
    /// Entry key.
    key: String,
  },
  /// Inserts an item before an index.
  InsertItem {
    /// Path to the sequence.
    path: OwnedPath,
    /// Insertion index.
    index: usize,
    /// Item value.
    value: Object,
  },
  /// Appends an item to a sequence.
  PushItem {
    /// Path to the sequence.
    path: OwnedPath,
    /// Item value.
    value: Object,
  },
  /// Removes an item at an index.
  RemoveItem {
    /// Path to the sequence.
    path: OwnedPath,
    /// Item index.
    index: usize,
  },
  /// Moves an item within a sequence.
  MoveItem {
    /// Path to the sequence.
    path: OwnedPath,
    /// Source index.
    from: usize,
    /// Index of the item to move before, or the length to move to the end.
    to: usize,
  },
}

impl OwnedPatchOperation {
  /// Creates a whole-value replacement operation.
  pub fn set<'a>(path: impl IntoIterator<Item = PathSegment<'a>>, value: Object) -> Self {
    Self::Set {
      path: path.into_iter().collect(),
      value,
    }
  }

  /// Creates a map insertion operation.
  pub fn insert_key<'a>(
    path: impl IntoIterator<Item = PathSegment<'a>>,
    key: impl Into<String>,
    value: Object,
  ) -> Self {
    Self::InsertKey {
      path: path.into_iter().collect(),
      key: key.into(),
      value,
    }
  }

  /// Creates a map removal operation.
  pub fn remove_key<'a>(
    path: impl IntoIterator<Item = PathSegment<'a>>,
    key: impl Into<String>,
  ) -> Self {
    Self::RemoveKey {
      path: path.into_iter().collect(),
      key: key.into(),
    }
  }

  /// Creates a sequence insertion operation.
  pub fn insert_item<'a>(
    path: impl IntoIterator<Item = PathSegment<'a>>,
    index: usize,
    value: Object,
  ) -> Self {
    Self::InsertItem {
      path: path.into_iter().collect(),
      index,
      value,
    }
  }

  /// Creates a sequence append operation.
  pub fn push_item<'a>(path: impl IntoIterator<Item = PathSegment<'a>>, value: Object) -> Self {
    Self::PushItem {
      path: path.into_iter().collect(),
      value,
    }
  }

  /// Creates a sequence removal operation.
  pub fn remove_item<'a>(path: impl IntoIterator<Item = PathSegment<'a>>, index: usize) -> Self {
    Self::RemoveItem {
      path: path.into_iter().collect(),
      index,
    }
  }

  /// Creates a sequence move operation.
  pub fn move_item<'a>(
    path: impl IntoIterator<Item = PathSegment<'a>>,
    from: usize,
    to: usize,
  ) -> Self {
    Self::MoveItem {
      path: path.into_iter().collect(),
      from,
      to,
    }
  }
}

impl<'a> From<PatchOperation<'a>> for OwnedPatchOperation {
  fn from(operation: PatchOperation<'a>) -> Self {
    match operation {
      PatchOperation::Set { path, value } => Self::Set {
        path: OwnedPath::from(path.as_slice()),
        value,
      },
      PatchOperation::InsertKey { path, key, value } => Self::InsertKey {
        path: OwnedPath::from(path.as_slice()),
        key,
        value,
      },
      PatchOperation::RemoveKey { path, key } => Self::RemoveKey {
        path: OwnedPath::from(path.as_slice()),
        key,
      },
      PatchOperation::InsertItem { path, index, value } => Self::InsertItem {
        path: OwnedPath::from(path.as_slice()),
        index,
        value,
      },
      PatchOperation::PushItem { path, value } => Self::PushItem {
        path: OwnedPath::from(path.as_slice()),
        value,
      },
      PatchOperation::RemoveItem { path, index } => Self::RemoveItem {
        path: OwnedPath::from(path.as_slice()),
        index,
      },
      PatchOperation::MoveItem { path, from, to } => Self::MoveItem {
        path: OwnedPath::from(path.as_slice()),
        from,
        to,
      },
    }
  }
}

impl<'a> PatchOperation<'a> {
  /// Creates a whole-value replacement operation.
  pub fn set(path: impl IntoIterator<Item = PathSegment<'a>>, value: impl Meta + 'static) -> Self {
    Self::Set {
      path: path.into_iter().collect(),
      value: Object::new(value),
    }
  }

  /// Creates a map insertion operation.
  pub fn insert_key(
    path: impl IntoIterator<Item = PathSegment<'a>>,
    key: impl Into<String>,
    value: impl Meta + 'static,
  ) -> Self {
    Self::InsertKey {
      path: path.into_iter().collect(),
      key: key.into(),
      value: Object::new(value),
    }
  }

  /// Creates a map removal operation.
  pub fn remove_key(
    path: impl IntoIterator<Item = PathSegment<'a>>,
    key: impl Into<String>,
  ) -> Self {
    Self::RemoveKey {
      path: path.into_iter().collect(),
      key: key.into(),
    }
  }

  /// Creates a sequence insertion operation.
  pub fn insert_item(
    path: impl IntoIterator<Item = PathSegment<'a>>,
    index: usize,
    value: impl Meta + 'static,
  ) -> Self {
    Self::InsertItem {
      path: path.into_iter().collect(),
      index,
      value: Object::new(value),
    }
  }

  /// Creates a sequence append operation.
  pub fn push_item(
    path: impl IntoIterator<Item = PathSegment<'a>>,
    value: impl Meta + 'static,
  ) -> Self {
    Self::PushItem {
      path: path.into_iter().collect(),
      value: Object::new(value),
    }
  }

  /// Creates a sequence removal operation.
  pub fn remove_item(path: impl IntoIterator<Item = PathSegment<'a>>, index: usize) -> Self {
    Self::RemoveItem {
      path: path.into_iter().collect(),
      index,
    }
  }

  /// Creates a sequence move operation.
  pub fn move_item(
    path: impl IntoIterator<Item = PathSegment<'a>>,
    from: usize,
    to: usize,
  ) -> Self {
    Self::MoveItem {
      path: path.into_iter().collect(),
      from,
      to,
    }
  }
}

/// Operation kinds used to identify unsupported patch requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchOperationKind {
  /// Replaces a value at a path.
  Set,
  /// Inserts or replaces a map entry.
  InsertKey,
  /// Removes a map entry.
  RemoveKey,
  /// Inserts a sequence item.
  InsertItem,
  /// Appends an item to a sequence.
  PushItem,
  /// Removes a sequence item.
  RemoveItem,
  /// Moves an item within a sequence.
  MoveItem,
}

impl fmt::Display for PatchOperationKind {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let name = match self {
      Self::Set => "set",
      Self::InsertKey => "insert key",
      Self::RemoveKey => "remove key",
      Self::InsertItem => "insert item",
      Self::PushItem => "push item",
      Self::RemoveItem => "remove item",
      Self::MoveItem => "move item",
    };
    f.write_str(name)
  }
}

/// Failure returned when applying a reflective patch.
/// Paths in failure variants borrow names from the patch operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyError<'a> {
  /// No value exists at the requested path.
  PathNotFound {
    /// Path that could not be resolved.
    path: Vec<PathSegment<'a>>,
    /// Operation that was requested at the missing path.
    operation: PatchOperationKind,
  },
  /// The patch shape cannot be applied to the target shape.
  ShapeMismatch {
    /// Path of the incompatible value.
    path: Vec<PathSegment<'a>>,
    /// Shape required by the patch.
    expected: ValueKind,
    /// Shape found on the target.
    actual: ValueKind,
  },
  /// The patch value and target value have different concrete types.
  TypeMismatch {
    /// Path of the incompatible value.
    path: Vec<PathSegment<'a>>,
    /// Concrete type required by the patch.
    expected: TypeInfo,
    /// Concrete type found on the target.
    actual: TypeInfo,
  },
  /// An index lies outside the target sequence.
  IndexOutOfBounds {
    /// Path to the sequence.
    path: Vec<PathSegment<'a>>,
    /// Operation that received the index.
    operation: PatchOperationKind,
    /// Index that lies outside the sequence.
    index: usize,
    /// Length of the sequence.
    len: usize,
  },
  /// The target does not support the requested operation.
  Unsupported {
    /// Path at which the operation was requested.
    path: Vec<PathSegment<'a>>,
    /// Operation that was not supported.
    operation: PatchOperationKind,
  },
  /// A move failed and could not restore the item at its source index. The
  /// sequence no longer contains the item.
  ItemLost {
    /// Path to the sequence.
    path: Vec<PathSegment<'a>>,
    /// Source index of the lost item.
    index: usize,
  },
}

impl fmt::Display for ApplyError<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::PathNotFound { operation, .. } => {
        write!(
          f,
          "Reflective patch operation {operation} targets a path that does not exist"
        )
      }
      Self::ShapeMismatch {
        expected, actual, ..
      } => {
        write!(
          f,
          "Reflective patch expected {expected:?}, found {actual:?}"
        )
      }
      Self::TypeMismatch {
        expected, actual, ..
      } => write!(
        f,
        "Reflective patch expected {}, found {}",
        expected.type_name(),
        actual.type_name()
      ),
      Self::IndexOutOfBounds {
        operation,
        index,
        len,
        ..
      } => write!(
        f,
        "Reflective patch operation {operation} received index {index} for a sequence of length {len}"
      ),
      Self::Unsupported { operation, .. } => {
        write!(f, "Reflective patch operation {operation} is unsupported")
      }
      Self::ItemLost { index, .. } => write!(
        f,
        "Reflective patch could not restore the item moved from index {index}"
      ),
    }
  }
}

#[cfg(feature = "std")]
impl std::error::Error for ApplyError<'_> {}

/// A mutation target that immediate patch operations can act on.
pub(crate) trait PatchTarget: Sized {
  fn type_info(&self) -> TypeInfo;
  fn kind(&self) -> ValueKind;
  fn len(&self) -> Option<usize>;
  fn set(self, value: Object) -> Result<(), ReflectiveError>;
  fn insert_key(&mut self, key: &str, value: Object) -> Result<(), ReflectiveError>;
  fn insert_item(&mut self, index: usize, value: Object) -> Result<(), ReflectiveError>;
  fn push_item(&mut self, value: Object) -> Result<(), ReflectiveError>;
  fn remove_key(&mut self, key: &str) -> Result<Object, ReflectiveError>;
  fn remove_item(&mut self, index: usize) -> Result<Object, ReflectiveError>;
  fn move_item(&mut self, from: usize, to: usize) -> Result<(), MoveItemError>;
}

impl PatchTarget for ObjectRefMut<'_> {
  fn type_info(&self) -> TypeInfo {
    Self::type_info(self)
  }
  fn kind(&self) -> ValueKind {
    Self::kind(self)
  }
  fn len(&self) -> Option<usize> {
    Self::len(self)
  }
  fn set(self, value: Object) -> Result<(), ReflectiveError> {
    Self::set(self, value)
  }
  fn insert_key(&mut self, key: &str, value: Object) -> Result<(), ReflectiveError> {
    Self::insert_key(self, key, value).map(|_| ())
  }
  fn insert_item(&mut self, index: usize, value: Object) -> Result<(), ReflectiveError> {
    Self::insert_item(self, index, value).map(|_| ())
  }
  fn push_item(&mut self, value: Object) -> Result<(), ReflectiveError> {
    Self::push_item(self, value).map(|_| ())
  }
  fn remove_key(&mut self, key: &str) -> Result<Object, ReflectiveError> {
    Self::remove_key(self, key)
  }
  fn remove_item(&mut self, index: usize) -> Result<Object, ReflectiveError> {
    Self::remove_item(self, index)
  }
  fn move_item(&mut self, from: usize, to: usize) -> Result<(), MoveItemError> {
    self.inner.move_item(from, to)
  }
}

/// Resolves patch paths into [`PatchTarget`] values.
pub(crate) trait PatchResolver {
  type Target<'b>: PatchTarget
  where
    Self: 'b;

  fn resolve<'b>(&'b mut self, path: &[PathSegment<'_>]) -> Option<Self::Target<'b>>;
}

impl PatchResolver for dyn MetaMut + '_ {
  type Target<'b>
    = ObjectRefMut<'b>
  where
    Self: 'b;

  fn resolve<'b>(&'b mut self, path: &[PathSegment<'_>]) -> Option<ObjectRefMut<'b>> {
    ObjectRefMut::path_from(self, path)
  }
}

fn resolve_target<'a, 'p, R>(
  root: &'a mut R,
  path: Vec<PathSegment<'p>>,
  operation: PatchOperationKind,
) -> Result<(R::Target<'a>, Vec<PathSegment<'p>>), ApplyError<'p>>
where
  R: PatchResolver + ?Sized + 'a,
{
  let target = match root.resolve(&path) {
    Some(target) => target,
    None => {
      return Err(ApplyError::PathNotFound { path, operation });
    }
  };
  Ok((target, path))
}

fn resolve_container<'a, 'p, R>(
  root: &'a mut R,
  path: Vec<PathSegment<'p>>,
  operation: PatchOperationKind,
  expected: ValueKind,
) -> Result<(R::Target<'a>, Vec<PathSegment<'p>>), ApplyError<'p>>
where
  R: PatchResolver + ?Sized + 'a,
{
  let (target, path) = resolve_target(root, path, operation)?;
  let actual = target.kind();
  if actual != expected {
    return Err(ApplyError::ShapeMismatch {
      path,
      expected,
      actual,
    });
  }
  Ok((target, path))
}

/// Maps a failed reflective operation onto [`ApplyError::Unsupported`],
/// discarding whatever error payload the operation returned.
fn unsupported<'p, T, E>(
  result: Result<T, E>,
  path: Vec<PathSegment<'p>>,
  operation: PatchOperationKind,
) -> Result<T, ApplyError<'p>> {
  result.map_err(|_| ApplyError::Unsupported { path, operation })
}

fn apply_operation<'p, R: PatchResolver + ?Sized>(
  root: &mut R,
  operation: PatchOperation<'p>,
) -> Result<(), ApplyError<'p>> {
  match operation {
    PatchOperation::Set { path, value } => {
      let (child, path) = resolve_target(root, path, PatchOperationKind::Set)?;
      let expected = value.type_info();
      let actual = child.type_info();
      if expected != actual {
        return Err(ApplyError::TypeMismatch {
          path,
          expected,
          actual,
        });
      }
      unsupported(child.set(value), path, PatchOperationKind::Set)?;
    }
    PatchOperation::InsertKey { path, key, value } => {
      let (mut map, path) =
        resolve_container(root, path, PatchOperationKind::InsertKey, ValueKind::Map)?;
      unsupported(
        map.insert_key(&key, value),
        path,
        PatchOperationKind::InsertKey,
      )?;
    }
    PatchOperation::RemoveKey { path, key } => {
      let (mut map, path) =
        resolve_container(root, path, PatchOperationKind::RemoveKey, ValueKind::Map)?;
      unsupported(map.remove_key(&key), path, PatchOperationKind::RemoveKey)?;
    }
    PatchOperation::InsertItem { path, index, value } => {
      let (mut sequence, path) = resolve_container(
        root,
        path,
        PatchOperationKind::InsertItem,
        ValueKind::Sequence,
      )?;
      unsupported(
        sequence.insert_item(index, value),
        path,
        PatchOperationKind::InsertItem,
      )?;
    }
    PatchOperation::PushItem { path, value } => {
      let (mut sequence, path) = resolve_container(
        root,
        path,
        PatchOperationKind::PushItem,
        ValueKind::Sequence,
      )?;
      unsupported(
        sequence.push_item(value),
        path,
        PatchOperationKind::PushItem,
      )?;
    }
    PatchOperation::RemoveItem { path, index } => {
      let (mut sequence, path) = resolve_container(
        root,
        path,
        PatchOperationKind::RemoveItem,
        ValueKind::Sequence,
      )?;
      unsupported(
        sequence.remove_item(index),
        path,
        PatchOperationKind::RemoveItem,
      )?;
    }
    PatchOperation::MoveItem { path, from, to } => {
      let (mut sequence, path) = resolve_container(
        root,
        path,
        PatchOperationKind::MoveItem,
        ValueKind::Sequence,
      )?;
      // Check the indices here so the error names the offending index.
      let Some(len) = sequence.len() else {
        return Err(ApplyError::Unsupported {
          path,
          operation: PatchOperationKind::MoveItem,
        });
      };
      if let Some(index) = move_index_out_of_bounds(from, to, len) {
        return Err(ApplyError::IndexOutOfBounds {
          path,
          operation: PatchOperationKind::MoveItem,
          index,
          len,
        });
      }
      sequence.move_item(from, to).map_err(|error| match error {
        MoveItemError::RestoreFailed => ApplyError::ItemLost { path, index: from },
        _ => ApplyError::Unsupported {
          path,
          operation: PatchOperationKind::MoveItem,
        },
      })?;
    }
  }
  Ok(())
}

/// Returns the first index of a move from `from` to `to` that lies outside a
/// sequence of length `len`.
pub(crate) fn move_index_out_of_bounds(from: usize, to: usize, len: usize) -> Option<usize> {
  if from >= len {
    Some(from)
  } else if to > len {
    Some(to)
  } else {
    None
  }
}

pub(crate) fn apply_patch<'p, R, I>(root: &mut R, operations: I) -> Result<(), ApplyError<'p>>
where
  R: PatchResolver + ?Sized,
  I: IntoIterator<Item = PatchOperation<'p>>,
{
  for operation in operations {
    apply_operation(root, operation)?;
  }
  Ok(())
}
