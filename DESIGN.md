# Design decisions

This is the maintainer-facing record of architectural decisions. Read it
before making an architectural change and treat its current decisions as
constraints unless the change explicitly revises them.

## Capability model

### Decision

- `Meta` provides runtime type information, downcasting and read-only
  structural access through `reflect`.
- `MetaMut: Meta` provides structural mutation through `reflect_mut` and
  type-checked whole-value replacement through `replace_dyn`. `set_dyn` is the
  convenience operation that discards the previous value.
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

## Structural shapes

### Decision

- `Meta::reflect` returns a `Reflect` shape: `Scalar`, `Struct`, `Sequence`,
  `Map` or `Option`. Each structural variant carries a trait object for its
  access trait, `StructAccess`, `SequenceAccess` or `MapAccess`.
- `MetaMut::reflect_mut` returns a `ReflectMut` shape over
  `StructAccessMut`, `SequenceAccessMut`, `MapAccessMut` and
  `OptionAccessMut`, or `Opaque` when the value exposes no mutable structure.
- `Meta` keeps only `type_info`, `reflect`, `field_dyn`, `item_dyn`,
  `key_dyn`, `eq_dyn`, `into_any` and `as_any`. `MetaMut` keeps only
  `reflect_mut`, `field_mut_dyn`, `item_mut_dyn`, `key_mut_dyn`,
  `insert_key_dyn`, `insert_item_dyn`, `push_item_dyn`, `remove_key_dyn`,
  `remove_item_dyn`, `move_item_dyn`, `set_dyn`, `replace_dyn`, `as_any_mut`
  and the hidden `as_meta`.
- `field_dyn`, `item_dyn`, `key_dyn`, `field_mut_dyn`, `item_mut_dyn` and
  `key_mut_dyn` default to going through the shape. The derives and the
  pointer, sequence and map implementations keep the defaults. `Option`
  overrides them to reach the contained value directly.
- `SequenceAccessMut` extends `SequenceAccess`, so a mutable sequence shape
  also reports its length.
- Callers use inherent methods with the familiar names, such as `len`, `key`,
  `keys` and `field_path`, on `Object`, `ObjectMut`, `SendObject`, `ObjectRef`
  and `ObjectRefMut`. These methods dispatch on the shape.
- Trait objects have no inherent methods and expose each trait operation under
  its `_dyn` name only. A concrete value or a bare trait object is wrapped
  first, as in `ObjectRef::new(&value).field_path(path)`.
- `ObjectRef` holds the read methods. The other wrappers forward to it through
  a macro. `ObjectRefMut` and `ObjectMut` call the insertion and removal
  methods of `MetaMut`, which default to going through the shape.
- The owned wrappers do not implement `Deref`. `AsRef` returns the inner trait
  object when a caller needs it. Wrappers compare with `==` rather than
  `eq_dyn`.

### Rationale

Method resolution tries each auto-dereferencing step in order and stops at the
first step with a match. A `Meta` implementation for a type that dereferences
further, such as `&'static str`, `Box`, `Rc` or `Arc`, matches before the inner
type's inherent method. Users import `Meta` and `MetaMut` for their derives, so
trait methods named `len`, `is_empty`, `keys` or `replace` would make
`part.is_empty()` on a `&&str` return `Option<bool>` and
`boxed_text.replace('a', "b")` fail to compile. The access traits carry those
names instead, and callers never need to import them. Inherent methods on the
wrappers apply only to those receiver types, so they cannot shadow methods of
concrete types.

The wrappers are the only home of these methods. With inherent methods on
trait objects as well, `ObjectRef` would forward to them and `dyn MetaMut` and
`dyn SendMeta` would need copies, so the same API would exist in two forms.
`Deref` from a wrapper to its trait object would do the same in reverse,
placing the `_dyn` hooks next to the plain methods. Keeping one form means a
trait object is always wrapped before use, as a concrete value is.

Path traversal follows the same rule. A blanket extension trait would put
`field_path` on every `Meta` type, including pointer types that dereference to
types with their own methods. Wrapping a value in
`ObjectRef` or `ObjectRefMut` before navigating keeps one entry point for
single and multiple hops.

The remaining trait methods keep a `self` receiver so that `Meta` and `MetaMut`
stay dyn-compatible. `set_dyn` and `replace_dyn` stay on `MetaMut` because
whole-value replacement needs a per-type implementation for every shape.

Deriving the kind from the shape also means that a value cannot report a map
without providing map access, or a sequence without a length.

Reaching a field through the shape of a trait object costs two dynamic calls,
one for `reflect` and one for the access method, and the shape is too large to
return in registers. Path resolution performs one lookup per segment, so every
navigation method used by paths stays on the traits as a single dynamic call.
Structural insertion and removal on `MetaMut` follow the same rule.
The `_dyn` suffix, shared with `eq_dyn` and `set_dyn`, keeps them from
shadowing inherent methods.

A default trait method is compiled for each implementing type with a concrete
`Self`, so the default navigation methods inline `reflect` or `reflect_mut`
and the access method. The instruction count benchmarks show:

- A derived or built-in override that performs a direct lookup matches the
  default exactly, so the derives and the `Box`, `Rc`, `Arc`, sequence and map
  implementations do not override the navigation methods.
- A free function over `&dyn Meta` cannot inline the shape
  and costs about 14% more on a two-segment field path. Moving `item` and
  `key` lookups onto the trait as `item_dyn` and `key_dyn` saves about 6% on
  a two-segment path.
- The default for `Option` reaches the contained value through an erased
  `ObjectRef` or `ObjectRefMut` and costs an additional dynamic call, about 6%
  on a two-segment path, so `Option` overrides the navigation methods that
  forward to the contained value.
- Moving insertion and removal from private functions over `&mut dyn MetaMut`
  onto `MetaMut` saves about 4% to 8% per direct call, and about 10% for a map
  inside an `Option`. Patch and mutation batch operations match on
  `reflect_mut` themselves and are unaffected. Each `MetaMut` vtable grows by
  six entries, 48 bytes on a 64-bit target, and each type compiles its own
  copy of the defaults. In a binary using about 30 types, vtable data grows by
  about 6% and code by under 1%.
- `#[inline]` on the small non-generic methods of the wrappers, `TypeInfo`,
  `Reflect::kind`, `OwnedPath` and the raw path resolvers lets other crates
  inline them. It saves about 5% to 8% on paths, 12% to 20% on owned paths
  and 3% to 10% on direct insertion and removal. When the caller knows the
  concrete type, inlining also removes the dynamic calls, as in a direct
  `move_item` on a `Vec`, which drops by about 70%. Code size grows by about
  0.2%.

Patch and mutation batch operations that need a sequence length read it from
the matched `SequenceAccessMut` instead of calling `reflect` again.

### Consequences

- `Reflect::kind` determines `ValueKind`. The variant depends on the concrete
  type, not on whether a value happens to expose any fields, keys or items.
  Empty structs, empty maps and opaque scalar values remain distinguishable.
- `ValueKind` remains alongside `Reflect`. `Reflect` borrows the value and
  carries trait objects, so errors such as `ApplyError::ShapeMismatch` store
  the owned, comparable `ValueKind` instead.
- `AccessKind` follows from the shape: `Field` for structs with field names,
  `Item` for sequences and `MapAccess::access_kind` for maps. Options report
  the access of their contained value.
- Hand-written implementations implement one access trait per structural
  shape in addition to `Meta` or `MetaMut`. A module that imports an access
  trait to implement it sees that trait's methods on concrete types.

## Structural equality

Enum and result field names may depend on the active variant. Structural
equality therefore compares the exposed name lists before recursing. A derived
enum's variant name resolves to the enum itself so that typed variant paths
can return it. Structural equality treats such a field as equal instead of
recursing into the same value.

The derives generate `eq_dyn` for structural types. It downcasts the other
value and compares the fields directly, matching variants by pattern for
enums. The result equals the default comparison, which reaches each field
through `Reflect` and the access traits, but avoids two dynamic calls per
field.

Built-in `BTreeMap` and `HashMap` implementations compare their native keys
and values directly. The generic structural map access remains string-keyed
for reflective lookup, but equality must also work for maps whose key type is
not string-like. `MapAccess::visit_entries` is the canonical entry stream for
generic equality. Generic equality supports string-like keys through that
stream and `MapAccess::key`; maps with other key types must override
`Meta::eq_dyn`.

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

### `MetaMut` implementations

`MetaMut` is not blanket-implemented for every `T: Meta`. It needs a genuine
`as_any_mut`, and its mutable shape differs between indexed, keyed and named
values. Every type with a `Meta` implementation in this crate also has
a `MetaMut` implementation, so fields in derived structural types remain
usable without extra bounds.

Some shapes cannot safely expose a structural mutable reference. For example,
`BTreeSet` and `BinaryHeap` use their elements as ordering keys. Their mutable
shape is therefore `ReflectMut::Opaque`, while `to_mut::<T>()` still exposes the
whole collection for mutation through its own API.

`Box` forwards both shapes to its contents. `Rc` and `Arc` forward the mutable
shape through `get_mut` and report `ReflectMut::Opaque` when the value is not
uniquely owned.

`SequenceAccessMut::move_item` defaults to `MoveItemError::Unsupported`, while
sequential implementations provide a native move operation.

`key_mut` and `item_mut` traverse existing structure only. `insert_key`,
`insert_item` and `push_item` grow a structure by accepting an already-built
`Object`, so they do not need a `Default` bound or fabricate a value. They check
the concrete type with `Meta::type_info` and recover it with `Meta::into_any`,
mirroring `ObjectOps::to` for owned values. A failed type check or unsupported shape
returns the original `Object` unchanged.

### Whole-value replacement

`MetaMut::replace_dyn` checks the concrete type, downcasts and swaps the value,
returning the previous value as an `Object`. `MutationBatch` uses it to record
rollback state. `MetaMut::set_dyn` overwrites the value and drops the previous
one in place, so it avoids boxing a value the caller discards. Patch
application and `ObjectRefMut::set` use `set_dyn`.

Overwriting `*self` requires `Self: Sized`, so a generic default body cannot
serve trait-object callers. Each implementation provides both methods, which
`set_body!` and the derive generate.

## Owned wrappers

`Object`, `ObjectMut` and `SendObject` hold reflective values but are not
reflective values themselves, so they do not implement `Meta`. Convert between
them with `into_object`, and inspect a wrapper through its own methods.

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

`MetaMut` and `SendMeta` declare a hidden `as_meta(&self) -> &dyn Meta`
bridge. Upcasting `&dyn MetaMut` or `&dyn SendMeta` to `&dyn Meta` only
stabilised in Rust 1.86, while this crate's MSRV is Rust 1.85. The bridge
avoids raising the MSRV. `set_body!` and the derives generate it for
`MetaMut`, and the blanket implementation provides it for `SendMeta`.
`ObjectMut`, `ObjectRefMut` and `SendObject` build their `ObjectRef` through
it.

Remove the bridges when the MSRV has moved beyond Rust 1.86 and the relevant
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
