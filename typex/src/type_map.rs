use crate::TypeInfo;
use alloc::vec::Vec;

/// Maps type identities to values of type `T`. Useful for registries and dispatch tables.
///
/// # Example
///
/// ```
/// use typex::TypeMap;
///
/// struct GetUser;
/// struct GetInvoice;
///
/// let mut routes = TypeMap::<&'static str>::new();
/// routes.insert::<GetUser>("/users/:id");
/// routes.insert::<GetInvoice>("/invoices/:id");
///
/// assert_eq!(routes.get::<GetUser>(), Some(&"/users/:id"));
/// assert_eq!(routes.get::<GetInvoice>(), Some(&"/invoices/:id"));
/// assert!(routes.contains::<GetInvoice>());
/// assert_eq!(routes.remove::<GetInvoice>(), Some("/invoices/:id"));
/// assert!(!routes.contains::<GetInvoice>());
/// assert_eq!(routes.len(), 1);
/// ```
///
/// Implementation note: [`TypeMap`] is backed by a `Vec`, so it works under `no_std`.
/// Scans are O(n), which is suitable for static type registries.
#[derive(Clone, Debug)]
pub struct TypeMap<T> {
  entries: Vec<(TypeInfo, T)>,
}

impl<T> TypeMap<T> {
  /// Creates an empty type map.
  pub fn new() -> Self {
    TypeMap {
      entries: Vec::new(),
    }
  }

  /// Returns the number of bound types.
  pub fn len(&self) -> usize {
    self.entries.len()
  }

  /// Checks whether no types are bound.
  pub fn is_empty(&self) -> bool {
    self.entries.is_empty()
  }

  fn position(&self, id: core::any::TypeId) -> Option<usize> {
    self.entries.iter().position(|(info, _)| info.id() == id)
  }

  /// Binds `value` to `U`, returning the previous value if the type was already bound.
  pub fn insert<U: 'static>(&mut self, value: T) -> Option<T> {
    let info = TypeInfo::of::<U>();
    match self.position(info.id()) {
      Some(pos) => Some(core::mem::replace(&mut self.entries[pos].1, value)),
      None => {
        self.entries.push((info, value));
        None
      }
    }
  }

  /// Removes the value bound to `U`.
  pub fn remove<U: 'static>(&mut self) -> Option<T> {
    let info = TypeInfo::of::<U>();
    let pos = self.position(info.id())?;
    Some(self.entries.remove(pos).1)
  }

  /// Checks whether a value is bound to `U`.
  pub fn contains<U: 'static>(&self) -> bool {
    self.position(TypeInfo::of::<U>().id()).is_some()
  }

  /// Returns the value bound to `U`.
  pub fn get<U: 'static>(&self) -> Option<&T> {
    self
      .position(TypeInfo::of::<U>().id())
      .map(|pos| &self.entries[pos].1)
  }

  /// Returns a mutable reference to the value bound to `U`.
  pub fn get_mut<U: 'static>(&mut self) -> Option<&mut T> {
    self
      .position(TypeInfo::of::<U>().id())
      .map(|pos| &mut self.entries[pos].1)
  }

  /// Iterates over the bound Rust type names, in insertion order.
  pub fn type_names(&self) -> impl Iterator<Item = &'static str> + '_ {
    self.entries.iter().map(|(info, _)| info.type_name())
  }

  /// Iterates over the bound `(TypeInfo, value)` pairs, in insertion order.
  pub fn iter(&self) -> impl Iterator<Item = (&TypeInfo, &T)> {
    self.entries.iter().map(|(info, value)| (info, value))
  }
}

impl<T> Default for TypeMap<T> {
  fn default() -> Self {
    Self::new()
  }
}
