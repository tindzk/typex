# Design decisions

This is the maintainer-facing record of architectural decisions. Read it
before making an architectural change and treat its current decisions as
constraints unless the change explicitly revises them.

## Capability model

### Decision

- `Meta` provides runtime type information, downcasting and read-only
  structural access.
- `MetaMut: Meta` provides structural mutation and type-checked whole-value
  replacement through `replace`. `set` is the convenience operation that
  discards the previous value.
- Reflection and mutation remain separate capabilities. Read-only reflection
  must not impose mutation or clone bounds.

### Rationale

Callers should be able to inspect values without granting mutation access.
Mutation must also work for values that cannot or should not be cloned.

### Consequences

Implementations expose only the structural operations they can support.
Unsupported access returns `None` or rejects the supplied value rather than
fabricating a value or imposing unrelated bounds.

## Mutation model

### Immediate application

`ObjectRefMut::apply` is the immediate patch entry point. It applies one
ordered operation list, stops at the first failure and leaves earlier
successful operations applied.

This is deliberately non-transactional. Callers that need a transaction use
`MutationBatch`, even when they have only one change to make.

### Staged commits

`MutationBatch` owns its operations and does not borrow or change the target
while operations are staged. `commit` consumes the batch and applies its
operations against the target's current state.

For every successful operation, the commit records an owned inverse. If a
later operation fails, it replays those inverses in reverse order. A successful
commit returns the inverses as a `MutationRollback`, which `undo` consumes to
restore the earlier state.

Staging does not provide read-your-writes and does not take a whole-value
snapshot. Structural operations therefore affect the paths and indexes used
by later operations in the same batch.

### Failure guarantees

- A failed operation returns `CommitError::OperationFailed` after earlier
  operations have been rolled back where possible.
- A failed move that cannot restore its item reports
  `CommitOperationError::ItemLost`. Rollback still replays the earlier
  inverses, but the sequence no longer contains the item, so the target must
  be treated as invalid for transactional purposes.
- If inverse replay fails, the commit returns
  `CommitError::RollbackFailed`. Replay stops at that inverse, so the target
  may be partially restored and must be treated as invalid for transactional
  purposes.
- `MutationRollback::undo` reports the same condition as `RollbackError`.
- Custom `MetaMut` implementations must implement `replace` correctly so
  whole-value inverses can be retained. Structural implementations should
  provide inverse-compatible operations when they support staged commits.

Immediate patch errors borrow path data. Staged operations and rollback records
own their path segments so they can outlive the staging inputs. `OwnedPath`
encodes all segments in one string buffer, so staging an operation allocates
at most once for its path. Owning each name separately would allocate once
per field or key segment plus once for the segment list, which dominated the
cost of staging.

`TypedPath<'k, Root, Value>` stores `Vec<PathSegment<'k>>` and borrows the
keys of its key steps for `'k`. Typed paths are usually built for a single
lookup, where encoding segments into an `OwnedPath` and decoding them again
cost more than the allocation that copying keys would require. Paths without
key steps are `'static`. Typed key steps exist only for maps with `String` or
`&'static str` keys, because reflective key lookup takes a `&str`.

## Structural shape and equality

`Meta::kind()` returns a `ValueKind` determined by the concrete type, not by
whether a value happens to expose any fields, keys or items. Empty structs,
empty maps and opaque scalar values must remain distinguishable by shape.

Enum and result field names may depend on the active variant. Structural
equality therefore compares the exposed name lists before recursing. A derived
enum's variant name resolves to the enum itself so that typed variant paths
can return it. Structural equality treats such a field as equal instead of
recursing into the same value.

Built-in `BTreeMap` and `HashMap` implementations compare their native keys
and values directly. The generic structural map access remains string-keyed
for reflective lookup, but equality must also work for maps whose key type is
not string-like. `Meta::len` reports the number of exposed structural items,
including keyed entries for maps. Map implementations must provide that count,
and `Meta::visit_map_entries` is the canonical entry stream for generic equality.
Generic equality supports string-like keys through that stream and `Meta::key`;
maps with other key types must override `Meta::eq_dyn`.

## Type-erased references

### Decision

- `ObjectRef` represents a borrowed value that implements `Meta` and supports
  structural reflection.
- `AnyRef` represents a borrowed type-erased value that only needs runtime
  type information and downcasting.
- `MapEntryVisitor<'visitor>` is a higher-ranked `FnMut` trait-object alias for
  visiting map entries. It uses `AnyRef` for keys because map keys do not need
  to implement `Meta`.

### Rationale

The erased-reference storage and operations are useful beyond map keys. The
general name keeps the abstraction reusable while the callback still gives the
value its map-key context. A trait-object alias keeps `Meta` object-safe while
allowing callers to pass closures directly.

## Trait implementation constraints

### `FieldPath` and `Meta`

`FieldPath` remains separate from `Meta` because its default method needs
`Self: Sized` to build an `ObjectRef`. A `Self: Sized` default on `Meta` would
collide with the inherent method of the same name on `impl dyn Meta + '_` for
trait-object receivers. rustc reports the call as ambiguous even though the
trait default is unreachable there. An unrelated trait avoids that ambiguity.

### `MetaMut` implementations

`MetaMut` is not blanket-implemented for every `T: Meta`. It needs a genuine
`as_any_mut`, and its navigation methods differ between indexed, keyed and
named shapes. Every type with a `Meta` implementation in this crate also has
a `MetaMut` implementation, so fields in derived structural types remain
usable without extra bounds.

Some shapes cannot safely expose a structural mutable reference. For example,
`BTreeSet` and `BinaryHeap` use their elements as ordering keys. Their
`field_mut`, `item_mut` and `key_mut` methods therefore return `None`, while
`to_mut::<T>()` still exposes the whole collection for mutation through its
own API.

`Rc` and `Arc` forward mutable access through `get_mut`. They return `None`
when the value is not uniquely owned, as well as when the requested structure
does not exist.

`move_item` defaults to `MoveItemError::Unsupported`, while sequential
implementations provide a native move operation.

`key_mut` and `item_mut` traverse existing structure only. `insert_key`,
`insert_item` and `push_item` grow a structure by accepting an already-built
`Object`, so they do not need a `Default` bound or fabricate a value. They check
the concrete type with `Meta::type_info` and recover it with `Meta::into_any`,
mirroring `ObjectOps::to` for owned values. A failed type check or unsupported shape
returns the original `Object` unchanged.

### Whole-value replacement

`MetaMut::replace` checks the concrete type, downcasts and swaps the value,
returning the previous value as an `Object`. `MutationBatch` uses it to record
rollback state. `MetaMut::set` overwrites the value and drops the previous one
in place, so it avoids boxing a value the caller discards. Patch application
and `ObjectRefMut::set` use `set`.

Overwriting `*self` requires `Self: Sized`, so a generic default body cannot
serve trait-object callers. Each implementation provides `replace`. The
default `set` calls `replace` and discards the result, while `set_body!` and
the derive generate an in-place `set`.

## Owned wrappers

`Object`, `ObjectMut` and `SendObject` hold reflective values but are not
reflective values themselves, so they do not implement `Meta`. Convert between
them with `into_object`, and inspect a wrapper through `Deref`.

A wrapper that implemented `Meta` by forwarding to its inner value would
report the inner type. `Object::new(wrapper)` could then not be downcast back
to the wrapper, and containers of wrappers would expose items of the wrong
type. Keeping wrappers out of `Meta` preserves `Object::new(x).is::<X>()` for
every `Meta` type and avoids a second box.

## Bounds, cloneability and debugging

Mutation does not imply cloneability. `Meta` and `MetaMut` do not require
`Clone` or `Debug`. `ObjectMut::from_clone` is an explicit convenience for
callers that need an owned clone. Reflective wrapper `Debug` implementations
report runtime type metadata and structural shape instead of imposing a
formatting bound on the underlying value.

`to_mut::<T>()` provides caller-driven mutation of an opaque value. Such
mutations are intentionally outside staged patch journals and rollback
records because they are not reflective operations that can be inverted.

## Trait objects and MSRV

`MetaMut` declares a hidden `as_meta(&self) -> &dyn Meta` bridge. Upcasting
`&dyn MetaMut` to `&dyn Meta` only stabilised in Rust 1.86, while this crate's
MSRV is Rust 1.85. The bridge avoids raising the MSRV and is generated by
`set_body!` and the derives.

Remove the bridge when the MSRV has moved beyond Rust 1.86 and the relevant
trait-object coercions are available.

## Derive implementation constraints

### Generic bounds

The derives add a `Meta` or `MetaMut` bound for each field type that mentions a
declared type parameter. When a field type contains the derived type, the
derive skips a bound on the whole field and descends through its type
expression. It bounds each part that mentions a declared type parameter but
not the derived type. Duplicate bounds are emitted once. This allows recursive
types such as `struct Tree<T> { children: Vec<Tree<T>> }` to derive without
overflowing trait resolution while still bounding the `T` in
`Option<(Box<Tree<T>>, T)>`. Qualified paths to unrelated types are not treated
as the derived type merely because their final segment has the same name.

Opaque types do not impose child capability bounds because they expose no
child paths.

### Enum variant-name lookup

A derived enum's `field_mut` resolves the active variant's own name to the
enum itself. It reads the active variant name in a separate match whose
patterns bind no fields, then returns early on a match. Binding fields in the
same match would keep them mutably borrowed for the rest of the arm, so
returning `self` there could alias those bindings. A second match binds the
fields and handles per-field lookups.

## Updating decisions

When changing a capability boundary, mutation guarantee, cloneability rule,
structural shape rule, trait-object constraint or derive policy:

1. Record the decision and its rationale here.
2. Update the implementation and regression tests.
3. Update the README and Rust API documentation for observable behaviour.
