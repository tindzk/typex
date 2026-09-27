use super::*;
pub(super) use alloc::borrow::ToOwned;
pub(super) use alloc::boxed::Box;
pub(super) use alloc::collections::{BTreeMap, BTreeSet, BinaryHeap, LinkedList, VecDeque};
pub(super) use alloc::rc::Rc;
pub(super) use alloc::string::{String, ToString};
pub(super) use alloc::sync::Arc;
pub(super) use alloc::vec;
pub(super) use alloc::vec::Vec;

mod support;

/// Reflective trait-object view of a concrete test value.
trait DynMeta {
  fn dyn_meta(&self) -> &dyn Meta;
}

impl<T: Meta> DynMeta for T {
  fn dyn_meta(&self) -> &dyn Meta {
    self
  }
}

/// Mutable reflective trait-object view of a concrete test value.
trait DynMetaMut {
  fn dyn_meta_mut(&mut self) -> &mut dyn MetaMut;
}

impl<T: MetaMut> DynMetaMut for T {
  fn dyn_meta_mut(&mut self) -> &mut dyn MetaMut {
    self
  }
}

mod access;
mod derive;
mod equality;
mod mutation;
mod mutation_batch;
mod objects;
mod patch;
mod path;
mod type_map;
