use crate::patch::{OwnedPatchOperation, move_index_out_of_bounds};
use crate::path::OwnedPath;
use crate::{
  MetaMut, MoveItemError, Object, ObjectRef, PatchOperation, PatchOperationKind, PathSegment,
  ReflectMut, TypeInfo, ValueKind,
};
// Keep public API names in scope for short intra-doc links.
#[allow(unused_imports)]
use crate::{Meta, ObjectRefMut};
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::iter::FromIterator;
use core::{fmt, mem};

/// Holds inverse operations used by [`undo`](Self::undo) to revert a committed
/// [`MutationBatch`].
///
/// [`MutationBatch::commit`] returns a [`MutationRollback`] on success. Each
/// applied operation adds one inverse. A `set` inverse holds the previous
/// value, while a `push_item` inverse holds the index of the inserted item.
///
/// Use [`previous_value`](Self::previous_value) to inspect values overwritten
/// or removed by the batch. Use [`undo`](Self::undo) to revert the batch, or
/// drop the rollback to retain the changes.
///
/// Each inverse targets the path and index recorded at commit. If the target
/// changes between commit and [`undo`](Self::undo), an inverse may no longer
/// resolve, in which case [`undo`](Self::undo) returns a [`RollbackError`], or
/// may restore a value at a different position.
///
/// # Example
///
/// ```
/// # use typex::{MutationBatch, PatchOperation, PathSegment};
/// let mut items = vec![23_u8, 42];
/// let rollback = MutationBatch::from([
///   PatchOperation::set([PathSegment::Item(0)], 7_u8),
///   PatchOperation::push_item([], 9_u8),
/// ])
/// .commit(&mut items)
/// .unwrap();
/// assert_eq!(items, vec![7, 42, 9]);
///
/// let previous = rollback.previous_value(0).unwrap();
/// assert_eq!(previous.to_ref::<u8>(), Some(&23));
/// assert!(rollback.previous_value(1).is_none());
///
/// rollback.undo(&mut items).unwrap();
/// assert_eq!(items, vec![23, 42]);
/// ```
pub struct MutationRollback {
  inverses: Vec<OwnedPatchOperation>,
}

impl fmt::Debug for MutationRollback {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("MutationRollback")
      .field("inverse_count", &self.inverses.len())
      .finish()
  }
}

impl MutationRollback {
  /// Returns the value that the operation at `index` in the batch replaced or
  /// removed.
  ///
  /// Insertions, appends, moves and out-of-range indices return `None`.
  pub fn previous_value(&self, index: usize) -> Option<ObjectRef<'_>> {
    match self.inverses.get(index)? {
      OwnedPatchOperation::Set { value, .. }
      | OwnedPatchOperation::InsertKey { value, .. }
      | OwnedPatchOperation::InsertItem { value, .. }
      | OwnedPatchOperation::PushItem { value, .. } => Some(ObjectRef::new(value.as_ref())),
      OwnedPatchOperation::RemoveKey { .. }
      | OwnedPatchOperation::RemoveItem { .. }
      | OwnedPatchOperation::MoveItem { .. } => None,
    }
  }

  /// Reverts the batch by applying each inverse operation in reverse order.
  ///
  /// Stops at the first inverse that fails and returns a [`RollbackError`].
  /// Operations with an index above [`RollbackError::inverse_index`] stay
  /// reverted, while the operation at that index and those below it stay
  /// applied.
  pub fn undo(mut self, target: &mut dyn MetaMut) -> Result<(), RollbackError> {
    while let Some(inverse) = self.inverses.pop() {
      let inverse_index = self.inverses.len();
      apply_staged_operation(target, inverse).map_err(|error| RollbackError {
        inverse_index,
        error,
      })?;
    }
    Ok(())
  }
}

/// Failure returned by [`MutationRollback::undo`].
#[derive(Debug)]
pub struct RollbackError {
  /// Zero-based position in the batch of the operation whose inverse failed.
  pub inverse_index: usize,
  /// Failure reported by the inverse operation.
  pub error: CommitOperationError,
}

/// An operation failure reported by [`MutationBatch::commit`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitOperationError {
  /// No value exists at the requested path.
  PathNotFound {
    /// Path that could not be resolved.
    path: OwnedPath,
    /// Operation that was requested at the missing path.
    operation: PatchOperationKind,
  },
  /// The patch shape cannot be applied to the target shape.
  ShapeMismatch {
    /// Path of the incompatible value.
    path: OwnedPath,
    /// Shape required by the patch.
    expected: ValueKind,
    /// Shape found on the target.
    actual: ValueKind,
  },
  /// The patch value and target value have different concrete types.
  TypeMismatch {
    /// Path of the incompatible value.
    path: OwnedPath,
    /// Concrete type required by the patch.
    expected: TypeInfo,
    /// Concrete type found on the target.
    actual: TypeInfo,
  },
  /// An index lies outside the target sequence.
  IndexOutOfBounds {
    /// Path to the sequence.
    path: OwnedPath,
    /// Operation that received the index.
    operation: PatchOperationKind,
    /// Index that lies outside the sequence.
    index: usize,
    /// Length of the sequence.
    len: usize,
  },
  /// A move failed and could not restore the item at its source index. The
  /// sequence no longer contains the item.
  ItemLost {
    /// Path to the sequence.
    path: OwnedPath,
    /// Source index of the lost item.
    index: usize,
  },
  /// The target does not support the requested operation.
  Unsupported {
    /// Path at which the operation was requested.
    path: OwnedPath,
    /// Operation that was not supported.
    operation: PatchOperationKind,
  },
}

impl fmt::Display for CommitOperationError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::PathNotFound { operation, .. } => write!(
        f,
        "Staged mutation operation {operation} targets a path that does not exist"
      ),
      Self::ShapeMismatch {
        expected, actual, ..
      } => {
        write!(f, "Staged mutation expected {expected:?}, found {actual:?}")
      }
      Self::TypeMismatch {
        expected, actual, ..
      } => write!(
        f,
        "Staged mutation expected {}, found {}",
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
        "Staged mutation operation {operation} received index {index} for a sequence of length {len}"
      ),
      Self::ItemLost { index, .. } => write!(
        f,
        "Staged mutation could not restore the item moved from index {index}"
      ),
      Self::Unsupported { operation, .. } => {
        write!(f, "Staged mutation operation {operation} is unsupported")
      }
    }
  }
}

impl CommitOperationError {
  /// Returns the path at which the operation failed.
  pub fn path(&self) -> &OwnedPath {
    match self {
      Self::PathNotFound { path, .. }
      | Self::ShapeMismatch { path, .. }
      | Self::TypeMismatch { path, .. }
      | Self::IndexOutOfBounds { path, .. }
      | Self::ItemLost { path, .. }
      | Self::Unsupported { path, .. } => path,
    }
  }
}

#[cfg(feature = "std")]
impl std::error::Error for CommitOperationError {}

/// Failure returned by a staged commit.
#[derive(Debug)]
pub enum CommitError {
  /// An operation failed and all earlier successful operations were rolled
  /// back. The target retains its state from before the commit unless `error`
  /// is [`CommitOperationError::ItemLost`].
  OperationFailed {
    /// Zero-based index of the failed operation.
    index: usize,
    /// Failure reported while validating or applying the operation.
    error: CommitOperationError,
  },
  /// Rollback failed after an operation failure. This can occur when a custom
  /// [`MetaMut`] implementation cannot apply an inverse. Rollback stops at the
  /// failing inverse, so the target may be partially restored.
  RollbackFailed {
    /// The operation failure that triggered rollback.
    failed_operation: Box<CommitError>,
    /// Zero-based index of the successful operation whose rollback failed.
    operation_index: usize,
    /// Failure reported by the inverse operation.
    error: CommitOperationError,
  },
}

impl fmt::Display for CommitError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::OperationFailed { index, error, .. } => {
        write!(f, "Staged mutation operation {index} failed: {error}")
      }
      Self::RollbackFailed {
        operation_index,
        error,
        ..
      } => write!(
        f,
        "Staged mutation rollback for operation {operation_index} failed: {error}"
      ),
    }
  }
}

#[cfg(feature = "std")]
impl std::error::Error for CommitError {}

/// Ordered list of patch operations that can be applied to a mutable reflective
/// value.
///
/// Build a batch with [`MutationBatch::new`] and [`MutationBatch::push`],
/// collect it from [`PatchOperation`] values or convert an array with
/// [`MutationBatch::from`]. The batch owns its operations, so it can outlive
/// the values they borrowed.
///
/// Committing with [`MutationBatch::commit`] applies the operations in order.
/// If one fails, the batch attempts to revert the earlier ones before returning
/// the error. On success, it returns a [`MutationRollback`] that undoes the
/// whole batch.
///
/// By contrast, [`ObjectRefMut::apply`] stops at the first failure and keeps
/// the changes made before it. It records no undo information, so it is
/// cheaper when a partial update is acceptable.
///
/// # Examples
///
/// Commit a batch and undo it later:
///
/// ```
/// # use typex::{MutationBatch, PatchOperation, PathSegment};
/// let mut values = vec![1_u8, 2];
/// let rollback = MutationBatch::from([
///   PatchOperation::set([PathSegment::Item(0)], 10_u8),
///   PatchOperation::push_item([], 3_u8),
/// ])
/// .commit(&mut values)
/// .unwrap();
/// assert_eq!(values, [10, 2, 3]);
///
/// rollback.undo(&mut values).unwrap();
/// assert_eq!(values, [1, 2]);
/// ```
///
/// A failed operation attempts to revert the operations before it:
///
/// ```
/// # use typex::{MutationBatch, PatchOperation, PathSegment};
/// let mut values = vec![1_u8, 2];
/// let result = MutationBatch::from([
///   PatchOperation::set([PathSegment::Item(0)], 10_u8),
///   PatchOperation::remove_item([], 5),
/// ])
/// .commit(&mut values);
///
/// assert!(result.is_err());
/// assert_eq!(values, [1, 2]);
/// ```
#[derive(Debug, Default)]
pub struct MutationBatch {
  operations: Vec<OwnedPatchOperation>,
}

impl<'a> FromIterator<PatchOperation<'a>> for MutationBatch {
  fn from_iter<I: IntoIterator<Item = PatchOperation<'a>>>(operations: I) -> Self {
    let mut batch = Self::new();
    batch.extend(operations);
    batch
  }
}

impl<'a, const N: usize> From<[PatchOperation<'a>; N]> for MutationBatch {
  fn from(operations: [PatchOperation<'a>; N]) -> Self {
    operations.into_iter().collect()
  }
}

impl MutationBatch {
  /// Creates an empty mutation batch.
  pub fn new() -> Self {
    Self::default()
  }

  /// Returns whether no operations are staged.
  pub fn is_empty(&self) -> bool {
    self.operations.is_empty()
  }

  /// Returns the number of staged operations.
  pub fn len(&self) -> usize {
    self.operations.len()
  }

  /// Appends an operation to the batch.
  pub fn push(&mut self, operation: impl Into<OwnedPatchOperation>) -> &mut Self {
    self.operations.push(operation.into());
    self
  }

  /// Appends patch operations in iteration order.
  pub fn extend<'a, I>(&mut self, operations: I) -> &mut Self
  where
    I: IntoIterator<Item = PatchOperation<'a>>,
  {
    self
      .operations
      .extend(operations.into_iter().map(OwnedPatchOperation::from));
    self
  }

  /// Applies the staged operations to `target` in order and returns a
  /// [`MutationRollback`] that undoes them.
  ///
  /// If an operation fails, reverts the earlier operations and returns
  /// [`CommitError::OperationFailed`]. If reverting fails, returns
  /// [`CommitError::RollbackFailed`] and leaves `target` partially restored.
  #[allow(clippy::result_large_err)]
  pub fn commit(self, target: &mut dyn MetaMut) -> Result<MutationRollback, CommitError> {
    // Each inverse replaces its operation, so the rollback reuses the batch
    // allocation.
    let mut inverses = self.operations;

    for index in 0..inverses.len() {
      let placeholder = OwnedPatchOperation::MoveItem {
        path: OwnedPath::new(),
        from: 0,
        to: 0,
      };
      let operation = mem::replace(&mut inverses[index], placeholder);
      match apply_staged_operation(target, operation) {
        Ok(inverse) => inverses[index] = inverse,
        Err(error) => {
          inverses.truncate(index);
          let failed_operation = CommitError::OperationFailed { index, error };
          let rollback = MutationRollback { inverses };
          return Err(match rollback.undo(target) {
            Ok(()) => failed_operation,
            Err(RollbackError {
              inverse_index,
              error,
            }) => CommitError::RollbackFailed {
              failed_operation: Box::new(failed_operation),
              operation_index: inverse_index,
              error,
            },
          });
        }
      }
    }

    Ok(MutationRollback { inverses })
  }
}

fn resolve<'a>(
  target: &'a mut dyn MetaMut,
  path: &OwnedPath,
  operation: PatchOperationKind,
) -> Result<&'a mut dyn MetaMut, CommitOperationError> {
  resolve_path(target, path).ok_or_else(|| CommitOperationError::PathNotFound {
    path: path.clone(),
    operation,
  })
}

fn resolve_path<'a>(target: &'a mut dyn MetaMut, path: &OwnedPath) -> Option<&'a mut dyn MetaMut> {
  let mut current = target;
  for segment in path {
    current = match segment {
      PathSegment::Field(name) => current.field_mut_dyn(name)?,
      PathSegment::Item(index) => current.item_mut_dyn(index)?,
      PathSegment::Key(key) => current.key_mut_dyn(key)?,
    }
    .inner;
  }
  Some(current)
}

/// Reports why a value exposes no mutable shape for an operation that needs
/// `expected`.
///
/// Callers match on [`MetaMut::reflect_mut`] first and compute the kind only on
/// this error path. A target of the expected kind without matching mutable
/// structure, such as a `BTreeSet`, does not support the operation.
fn container_error(
  actual: ValueKind,
  path: &OwnedPath,
  operation: PatchOperationKind,
  expected: ValueKind,
) -> CommitOperationError {
  if actual == expected {
    unsupported_error(path, operation)
  } else {
    CommitOperationError::ShapeMismatch {
      path: path.clone(),
      expected,
      actual,
    }
  }
}

fn unsupported_error(path: &OwnedPath, operation: PatchOperationKind) -> CommitOperationError {
  CommitOperationError::Unsupported {
    path: path.clone(),
    operation,
  }
}

fn replacement_error(
  path: &OwnedPath,
  operation: PatchOperationKind,
  value: &Object,
  actual: TypeInfo,
) -> CommitOperationError {
  let expected = value.type_info();
  if expected != actual {
    CommitOperationError::TypeMismatch {
      path: path.clone(),
      expected,
      actual,
    }
  } else {
    unsupported_error(path, operation)
  }
}

/// Applies `operation` to `target` and returns the operation that reverts it.
fn apply_staged_operation(
  target: &mut dyn MetaMut,
  operation: OwnedPatchOperation,
) -> Result<OwnedPatchOperation, CommitOperationError> {
  match operation {
    OwnedPatchOperation::Set { path, value } => {
      let operation_kind = PatchOperationKind::Set;
      let child = resolve(target, &path, operation_kind)?;
      match child.replace_dyn(value) {
        Ok(previous) => Ok(OwnedPatchOperation::Set {
          path,
          value: previous,
        }),
        Err(value) => Err(replacement_error(
          &path,
          operation_kind,
          &value,
          child.type_info(),
        )),
      }
    }
    OwnedPatchOperation::InsertKey { path, key, value } => {
      let operation_kind = PatchOperationKind::InsertKey;
      let target = resolve(target, &path, operation_kind)?;
      let ReflectMut::Map(map) = target.reflect_mut() else {
        return Err(container_error(
          target.kind(),
          &path,
          operation_kind,
          ValueKind::Map,
        ));
      };
      let Some(child) = map.key_mut(&key) else {
        map
          .insert_key(&key, value)
          .map_err(|_| unsupported_error(&path, operation_kind))?;
        return Ok(OwnedPatchOperation::RemoveKey { path, key });
      };
      match child.inner.replace_dyn(value) {
        Ok(previous) => Ok(OwnedPatchOperation::InsertKey {
          path,
          key,
          value: previous,
        }),
        Err(value) => Err(replacement_error(
          &path,
          operation_kind,
          &value,
          child.type_info(),
        )),
      }
    }
    OwnedPatchOperation::RemoveKey { path, key } => {
      let operation_kind = PatchOperationKind::RemoveKey;
      let target = resolve(target, &path, operation_kind)?;
      let ReflectMut::Map(map) = target.reflect_mut() else {
        return Err(container_error(
          target.kind(),
          &path,
          operation_kind,
          ValueKind::Map,
        ));
      };
      match map.remove_key(&key) {
        Some(previous) => Ok(OwnedPatchOperation::InsertKey {
          path,
          key,
          value: previous,
        }),
        None => Err(CommitOperationError::PathNotFound {
          path,
          operation: operation_kind,
        }),
      }
    }
    OwnedPatchOperation::InsertItem { path, index, value } => {
      let operation_kind = PatchOperationKind::InsertItem;
      let target = resolve(target, &path, operation_kind)?;
      let ReflectMut::Sequence(sequence) = target.reflect_mut() else {
        return Err(container_error(
          target.kind(),
          &path,
          operation_kind,
          ValueKind::Sequence,
        ));
      };
      sequence
        .insert_item(index, value)
        .map_err(|_| unsupported_error(&path, operation_kind))?;
      Ok(OwnedPatchOperation::RemoveItem { path, index })
    }
    OwnedPatchOperation::PushItem { path, value } => {
      let operation_kind = PatchOperationKind::PushItem;
      let target = resolve(target, &path, operation_kind)?;
      let ReflectMut::Sequence(sequence) = target.reflect_mut() else {
        return Err(container_error(
          target.kind(),
          &path,
          operation_kind,
          ValueKind::Sequence,
        ));
      };
      let index = sequence.len();
      sequence
        .push_item(value)
        .map_err(|_| unsupported_error(&path, operation_kind))?;
      Ok(OwnedPatchOperation::RemoveItem { path, index })
    }
    OwnedPatchOperation::RemoveItem { path, index } => {
      let operation_kind = PatchOperationKind::RemoveItem;
      let target = resolve(target, &path, operation_kind)?;
      let ReflectMut::Sequence(sequence) = target.reflect_mut() else {
        return Err(container_error(
          target.kind(),
          &path,
          operation_kind,
          ValueKind::Sequence,
        ));
      };
      match sequence.remove_item(index) {
        Some(previous) => Ok(OwnedPatchOperation::InsertItem {
          path,
          index,
          value: previous,
        }),
        None => Err(unsupported_error(&path, operation_kind)),
      }
    }
    OwnedPatchOperation::MoveItem { path, from, to } => {
      let operation_kind = PatchOperationKind::MoveItem;
      let target = resolve(target, &path, operation_kind)?;
      let ReflectMut::Sequence(sequence) = target.reflect_mut() else {
        return Err(container_error(
          target.kind(),
          &path,
          operation_kind,
          ValueKind::Sequence,
        ));
      };
      let len = sequence.len();
      if let Some(index) = move_index_out_of_bounds(from, to, len) {
        return Err(CommitOperationError::IndexOutOfBounds {
          path,
          operation: operation_kind,
          index,
          len,
        });
      }
      sequence.move_item(from, to).map_err(|error| match error {
        MoveItemError::RestoreFailed => CommitOperationError::ItemLost {
          path: path.clone(),
          index: from,
        },
        _ => unsupported_error(&path, operation_kind),
      })?;
      let (from, to) = if from < to {
        (to - 1, from)
      } else {
        (to, from + 1)
      };
      Ok(OwnedPatchOperation::MoveItem { path, from, to })
    }
  }
}
