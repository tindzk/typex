use crate::path::OwnedPath;
use crate::{
  Meta, MetaMut, MoveItemError, Object, ObjectRefMut, PathSegment, ReflectMut, TypeInfo, ValueKind,
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

fn resolve_target<'a, 'p>(
  root: &'a mut dyn MetaMut,
  path: Vec<PathSegment<'p>>,
  operation: PatchOperationKind,
) -> Result<(&'a mut dyn MetaMut, Vec<PathSegment<'p>>), ApplyError<'p>> {
  match ObjectRefMut::path_from(root, &path) {
    Some(target) => Ok((target.inner, path)),
    None => Err(ApplyError::PathNotFound { path, operation }),
  }
}

/// Reports why `target` exposes no mutable shape for an operation that needs
/// `expected`.
///
/// Callers match on [`MetaMut::reflect_mut`] first and compute the kind only on
/// this error path. A target of the expected kind without matching mutable
/// structure, such as a `BTreeSet`, does not support the operation.
fn container_error<'p>(
  target: &dyn MetaMut,
  path: Vec<PathSegment<'p>>,
  operation: PatchOperationKind,
  expected: ValueKind,
) -> ApplyError<'p> {
  let actual = target.kind();
  if actual == expected {
    ApplyError::Unsupported { path, operation }
  } else {
    ApplyError::ShapeMismatch {
      path,
      expected,
      actual,
    }
  }
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

fn apply_operation<'p>(
  root: &mut dyn MetaMut,
  operation: PatchOperation<'p>,
) -> Result<(), ApplyError<'p>> {
  match operation {
    PatchOperation::Set { path, value } => {
      let operation = PatchOperationKind::Set;
      let (target, path) = resolve_target(root, path, operation)?;
      let expected = value.type_info();
      let actual = target.type_info();
      if expected != actual {
        return Err(ApplyError::TypeMismatch {
          path,
          expected,
          actual,
        });
      }
      unsupported(target.set_dyn(value), path, operation)?;
    }
    PatchOperation::InsertKey { path, key, value } => {
      let operation = PatchOperationKind::InsertKey;
      let (target, path) = resolve_target(root, path, operation)?;
      let ReflectMut::Map(map) = target.reflect_mut() else {
        return Err(container_error(target, path, operation, ValueKind::Map));
      };
      unsupported(map.insert_key(&key, value), path, operation)?;
    }
    PatchOperation::RemoveKey { path, key } => {
      let operation = PatchOperationKind::RemoveKey;
      let (target, path) = resolve_target(root, path, operation)?;
      let ReflectMut::Map(map) = target.reflect_mut() else {
        return Err(container_error(target, path, operation, ValueKind::Map));
      };
      unsupported(map.remove_key(&key).ok_or(()), path, operation)?;
    }
    PatchOperation::InsertItem { path, index, value } => {
      let operation = PatchOperationKind::InsertItem;
      let (target, path) = resolve_target(root, path, operation)?;
      let ReflectMut::Sequence(sequence) = target.reflect_mut() else {
        return Err(container_error(
          target,
          path,
          operation,
          ValueKind::Sequence,
        ));
      };
      unsupported(sequence.insert_item(index, value), path, operation)?;
    }
    PatchOperation::PushItem { path, value } => {
      let operation = PatchOperationKind::PushItem;
      let (target, path) = resolve_target(root, path, operation)?;
      let ReflectMut::Sequence(sequence) = target.reflect_mut() else {
        return Err(container_error(
          target,
          path,
          operation,
          ValueKind::Sequence,
        ));
      };
      unsupported(sequence.push_item(value), path, operation)?;
    }
    PatchOperation::RemoveItem { path, index } => {
      let operation = PatchOperationKind::RemoveItem;
      let (target, path) = resolve_target(root, path, operation)?;
      let ReflectMut::Sequence(sequence) = target.reflect_mut() else {
        return Err(container_error(
          target,
          path,
          operation,
          ValueKind::Sequence,
        ));
      };
      unsupported(sequence.remove_item(index).ok_or(()), path, operation)?;
    }
    PatchOperation::MoveItem { path, from, to } => {
      let operation = PatchOperationKind::MoveItem;
      let (target, path) = resolve_target(root, path, operation)?;
      let ReflectMut::Sequence(sequence) = target.reflect_mut() else {
        return Err(container_error(
          target,
          path,
          operation,
          ValueKind::Sequence,
        ));
      };
      // Check the indices here so the error names the offending index.
      let len = sequence.len();
      if let Some(index) = move_index_out_of_bounds(from, to, len) {
        return Err(ApplyError::IndexOutOfBounds {
          path,
          operation,
          index,
          len,
        });
      }
      sequence.move_item(from, to).map_err(|error| match error {
        MoveItemError::RestoreFailed => ApplyError::ItemLost { path, index: from },
        _ => ApplyError::Unsupported { path, operation },
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

pub(crate) fn apply_patch<'p, I>(
  root: &mut dyn MetaMut,
  operations: I,
) -> Result<(), ApplyError<'p>>
where
  I: IntoIterator<Item = PatchOperation<'p>>,
{
  for operation in operations {
    apply_operation(root, operation)?;
  }
  Ok(())
}
