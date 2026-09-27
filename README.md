# typex
[![Crates.io](https://img.shields.io/crates/v/typex.svg)](https://crates.io/crates/typex)
[![docs.rs](https://img.shields.io/docsrs/typex)](https://docs.rs/typex)
[![CI](https://github.com/tindzk/typex/actions/workflows/ci.yml/badge.svg)](https://github.com/tindzk/typex/actions/workflows/ci.yml)
[![no_std](https://img.shields.io/badge/no__std-%E2%9C%94-blue)](#installation)
[![Minimum Rust Version](https://img.shields.io/badge/rustc-1.85+-orange.svg)](#features)
[![Licence](https://img.shields.io/badge/licence-Apache--2.0-blue.svg)](LICENCE)

typex is a lightweight Rust library for inspecting, traversing, comparing and
mutating values at runtime without knowing their concrete types. It supports
common Rust types, such as `u8` and `Vec<T>`, as well as custom structs and
enums using `#[derive(Meta)]`. Values can be wrapped in an `Object`, which
provides reflection operations. typex supports `no_std`.

## Why typex?

typex builds on `dyn Any` and provides an ergonomic API on top of it.

[`bevy_reflect`](https://crates.io/crates/bevy_reflect) is a broader reflection
framework used by Bevy. It covers structural type information, dynamic
reflected values, instantiation and serialisation. typex has a smaller surface
and focuses on structural access, mutation and equality of runtime objects.
Instantiation, deserialisation and instance-independent type descriptors are
out of scope.

## Contents

- [Why typex?](#why-typex)
- [Features](#features)
- [Installation](#installation)
- [Supported types](#supported-types)
- [Quick start](#quick-start)
- [Core concepts](#core-concepts)
- [Derive macros](#derive-macros)
- [Mutation](#mutation)
- [Reflective patching](#reflective-patching)
- [Staged mutation batches](#staged-mutation-batches)
- [Thread-safe objects](#thread-safe-objects)
- [Hand-written implementations](#hand-written-implementations)
- [Workspace structure](#workspace-structure)
- [Licence](#licence)

## Features

- Runtime type introspection of objects
- Structural access, traversal, mutation and equality (fields, keys, items)
- Typed paths for nested access
- Patch operations with batching and rollback
- Type-keyed maps for registries and dispatch tables
- Support for common scalar, container and collection types
- Derive macros for structs and enums
- MSRV: Rust 1.85 (edition 2024)
- Compatible with `no_std` + `alloc`
- No runtime dependencies
- No `unsafe` code
- Small implementation: ~4k lines of Rust code
<!-- check: tokei=typex/src,typex_derive/src exclude=tests min=3500 max=4300 -->

## Installation

```toml
[dependencies]
typex = { version = "0.1.0", features = ["derive"] }
```

The `derive` feature re-exports the `Meta` and `MetaMut` derive macros from
`typex_derive`. Leave it out to use the reflection traits without the
derives.

The `std` feature is enabled by default. It adds `HashMap` support and
[`Error`](https://doc.rust-lang.org/std/error/trait.Error.html) implementations
for the crate's error types. Disable default features to use typex in
`no_std` environments with `alloc`:

```toml
[dependencies]
typex = { version = "0.1.0", default-features = false }
```

You can read the full API docs on [docs.rs](https://docs.rs/typex).

## Supported types

- Scalars: `bool`, `char`, `i8`/`i16`/`i32`/`i64`/`i128`/`isize`, `u8`/`u16`/`u32`/`u64`/`u128`/`usize`, `f32`/`f64`, `&'static str`, `String`
- Containers: `Option`, `Result`, tuples (up to 8 elements), arrays of any length, `Box`, `Rc`, `Arc`
- Collections: `Vec`, `VecDeque`, `LinkedList`, `BTreeSet`, `BinaryHeap`, `BTreeMap`, `HashMap` (requires `std`)
- Custom structs and enums via `#[derive(Meta)]`. Mutable reflection is opt-in via the separate `MetaMut` derive.

## Quick start

Enable the `derive` feature when using the derive macros.

<!-- check: name=example -->
```rust
#[derive(Debug, Meta)]
struct Scope {
  name: &'static str,
}

#[derive(Debug, Meta)]
struct User {
  id: u16,
  scopes: Vec<Scope>,
}

let object = Object::new(User {
  id: 42,
  scopes: vec![Scope { name: "read" }],
});

assert!(object.is::<User>());
assert_eq!(object.type_name(), core::any::type_name::<User>());
assert_eq!(object.field_names(), &["id", "scopes"]);

assert_eq!(object.field("id").unwrap().to_ref::<u16>(), Some(&42));

let scope_name = |object: &Object| {
  object
    .field("scopes")?
    .item(0)?
    .field("name")?
    .to_ref::<&'static str>()
    .copied()
};
assert_eq!(scope_name(&object), Some("read"));

// Use typed paths to traverse fields without an explicit downcast
assert_eq!(object.field_path(User::FIELD_ID.path()), Some(&42));
assert_eq!(
  object.field_path(User::FIELD_SCOPES.item(0).then(Scope::FIELD_NAME)),
  Some(&"read")
);
```

## Core concepts

### Borrowed object views

`Object` owns a boxed value with reflective access. `ObjectRef` is the
corresponding read-only view, which borrows the value without allocating. Use it
when the value already exists elsewhere, or when passing a borrowed view of the
value to another function:

<!-- check: name=object-ref -->
```rust
use typex::ObjectRef;

let values = vec![10_u8, 20, 30];
let view = ObjectRef::new(&values);

assert!(view.is::<Vec<u8>>());
assert_eq!(view.item(1).unwrap().to_ref::<u8>(), Some(&20));
assert_eq!(view.to_ref::<Vec<u8>>(), Some(&values));
assert_eq!(values, vec![10, 20, 30]);
```

You do not need to construct an `ObjectRef` explicitly when starting with an
owned `Object`. The read-only navigation methods on `Object` return borrowed
`ObjectRef` values:

<!-- check: name=object-ref-from-object extends=object-ref -->
```rust
let object = Object::new(values);
let item = object.item(1).unwrap();

assert_eq!(item.to_ref::<u8>(), Some(&20));
```

`ObjectRef` is returned by `field()`, `item()`, `key()` and raw path traversal.
Use `ObjectRefMut` when the underlying value must be changed.

### Container access

Objects expose indexed, named and keyed access through `item()`, `field()` and
`key()`:

<!-- check: name=container-access -->
```rust
let option = Object::new(Some(42_u8));
let list = Object::new(vec![1_u8, 2, 3]);
let record = Object::new((42_u8, true));
let map = Object::new(std::collections::BTreeMap::from([("read", true)]));

assert_eq!(option.item(0).unwrap().to_ref::<u8>(), Some(&42));
assert_eq!(list.item(1).unwrap().to_ref::<u8>(), Some(&2));
// Tuple fields use string names, so "0" refers to the first field.
assert_eq!(record.field("0").unwrap().to_ref::<u8>(), Some(&42));
assert_eq!(map.key("read").unwrap().to_ref::<bool>(), Some(&true));
```

### Map access

`key()` looks up string-like keys. `key_typed()` looks up keys using their
concrete key type. `visit_map_entries()` visits arbitrary keys through a
`MapEntryVisitor` callback without converting them to strings.

`len()` reports the number of exposed structural items, including keyed entries
for maps. Hand-written maps implement `MapAccess`, which provides that count,
string-key lookup and the entry stream used for structural equality. Maps with
non-string keys must override `Meta::eq_dyn`.

<!-- check: name=map-access -->
```rust
use typex::{AnyRef, ObjectRef, TypedMapAccess, TypedMapAccessMut};

let numbers = std::collections::BTreeMap::from([(7_u32, 11_u8), (9, 13)]);
assert_eq!(
  numbers.key_typed(&7).unwrap().to_ref::<u8>(),
  Some(&11)
);

let mut mutable_numbers = numbers.clone();
mutable_numbers
  .key_typed_mut(&7)
  .unwrap()
  .set(Object::new(12_u8))
  .unwrap();
assert_eq!(mutable_numbers.get(&7), Some(&12));

let object = Object::new(numbers);
let mut seen = Vec::new();
assert!(object.visit_map_entries(
  &mut |key: AnyRef<'_>, value: ObjectRef<'_>| {
    seen.push((*key.to_ref::<u32>().unwrap(), *value.to_ref::<u8>().unwrap()));
    true
  },
));
assert_eq!(seen, vec![(7, 11), (9, 13)]);
```

### Type shapes

`kind()` reports the structural shape of an object. `access_kind()` reports how
the value can be traversed.

<!-- check: name=type-shapes extends=container-access -->
```rust
let scalar = Object::new(42_u8);
let shape = |object: &Object| (object.kind(), object.access_kind());

assert_eq!(shape(&scalar), (ValueKind::Scalar, None));
assert_eq!(shape(&option), (ValueKind::Option, None));
assert_eq!(shape(&list), (ValueKind::Sequence, Some(AccessKind::Item)));
assert_eq!(shape(&record), (ValueKind::Struct, Some(AccessKind::Field)));
assert_eq!(shape(&map), (ValueKind::Map, Some(AccessKind::Key)));
```

`AccessKind::ItemKey` is reserved for types that combine keyed and indexed
access, such as [`indexmap`](https://crates.io/crates/indexmap), using
operations such as `key()` and `item()` together.

`Option<T>` always reports `ValueKind::Option` as its own shape. For
`Some`, `field()` and `key()` forward to the inner value, while `item(0)` and
`option_value()` expose that value as an `ObjectRef`. `access_kind()` follows
the inner value's access mode. For `None`, these operations return no inner
value or forwarded access.

### Type information

`is()`, `type_info()` and `type_name()` expose runtime type information. Owned
values should be created explicitly with `Object::new` or
`ObjectMut::from_clone` when an owned mutable clone is required. An
`ObjectMut` keeps the value mutable while an `Object` is immutable:

<!-- check: name=type-information -->
```rust
use typex::ObjectMut;

let object = Object::new(42_u8);

assert!(object.is::<u8>());
assert_eq!(object.type_info(), TypeInfo::of::<u8>());
assert_eq!(object.type_name(), core::any::type_name::<u8>());

let mutable = ObjectMut::from_clone(&42_u8);
assert_eq!(
  mutable.as_ref().as_any().downcast_ref::<u8>(),
  Some(&42)
);
```

### Object conversions

`ObjectOps` provides conversions to borrowed or owned values. `to_ref()`
borrows the value. `to()` consumes an owned value and performs an exact-type
downcast without requiring `Clone`. Use Rust's `From` or `TryFrom` traits for
conversions between known concrete types.

<!-- check: name=object-conversions extends=type-information -->
```rust
assert_eq!(object.to_ref::<u8>(), Some(&42));
assert_eq!(object.to::<u8>(), Ok(42));
```

### Equality

`Meta::eq_dyn()` compares two `Meta` values and is also available as `==` on
`ObjectRef`, `ObjectRefMut` and `&dyn Meta`. Mismatched concrete types are
never equal. Structural values compare recursively through exposed fields, map
entries or indexed items. Derived types can opt into their own `PartialEq`
implementation with `#[typex(partial_eq)]`; see
[Equality options](#equality-options).

Hand-written `Meta` implementations can implement `eq_dyn()` using `PartialEq`,
as `#[typex(partial_eq)]` does:

<!-- check: name=manual-equality -->
```rust
use typex::Reflect;

#[derive(Debug, PartialEq)]
struct ManualId(u64);

impl Meta for ManualId {
  fn reflect(&self) -> Reflect<'_> {
    Reflect::Scalar
  }

  fn eq_dyn(&self, other: &dyn Meta) -> bool {
    match other.as_any().downcast_ref::<Self>() {
      Some(other) => self == other,
      None => false,
    }
  }

  fn into_any(self: Box<Self>) -> Box<dyn core::any::Any> {
    self
  }

  fn as_any(&self) -> &dyn core::any::Any {
    self
  }
}

let a = ManualId(7);
let b = ManualId(7);
assert!(a.eq_dyn(&b));
```

Equality dispatches on the shape from `Meta::reflect()`, so a struct or map with
zero exposed fields compares equal to itself:

<!-- check: name=empty-struct-equality -->
```rust
#[derive(Debug, Meta)]
struct EmptyState;

let a = EmptyState;
let b = EmptyState;
assert!(&a as &dyn Meta == &b as &dyn Meta);
```

### Paths

`FieldPath` traverses several hops in one call using either raw
`&[PathSegment]` values or typed paths. Typed item steps support `Vec`, arrays,
`VecDeque` and `LinkedList`. Typed key steps support `BTreeMap` and `HashMap`
with `String` or `&'static str` keys. `BTreeSet` and `BinaryHeap` support
read-only indexed reflection through raw paths, but do not support typed item
steps or mutable item access.

Raw paths return an `ObjectRef` to downcast explicitly, while typed paths
return a reference to the target:

<!-- check: name=paths -->
```rust
use std::collections::BTreeMap;

#[derive(Debug, Meta, MetaMut)]
struct Label {
  name: &'static str,
}

#[derive(Debug, Meta, MetaMut)]
struct Payload {
  count: u8,
  labels: Vec<Label>,
  bytes: [u8; 2],
  limits: BTreeMap<String, u16>,
}

let mut payload = Payload {
  count: 3,
  labels: vec![Label { name: "primary" }],
  bytes: [0x0a, 0xff],
  limits: BTreeMap::from([(String::from("requests"), 100)]),
};

assert_eq!(
  payload
    .field_path(&[
      PathSegment::Field("labels"),
      PathSegment::Item(0),
      PathSegment::Field("name"),
    ])
    .unwrap()
    .to_ref::<&'static str>(),
  Some(&"primary")
);

assert_eq!(
  payload.field_path(Payload::FIELD_LABELS.item(0).then(Label::FIELD_NAME)),
  Some(&"primary")
);

assert_eq!(
  payload.field_path(Payload::FIELD_LIMITS.key("requests")),
  Some(&100)
);
```

### Dispatch tables

`TypeMap<T>` maps type identities to values of type `T`. It is useful for
registries and dispatch tables. For example, a router could be represented as
follows:

<!-- check: name=registry -->
```rust
struct GetUser;
struct GetInvoice;

let mut routes = TypeMap::<&'static str>::new();
routes.insert::<GetUser>("/users/:id");
routes.insert::<GetInvoice>("/invoices/:id");

assert_eq!(routes.get::<GetUser>(), Some(&"/users/:id"));
assert!(routes.contains::<GetInvoice>());
```

Similarly, a dispatch table can bind marker types to handlers:

<!-- check: name=dispatch-table -->
```rust
struct GetUser;
struct GetInvoice;

fn get_user() -> &'static str {
  "user"
}

fn get_invoice() -> &'static str {
  "invoice"
}

let mut handlers = TypeMap::<fn() -> &'static str>::new();
handlers.insert::<GetUser>(get_user);
handlers.insert::<GetInvoice>(get_invoice);

let handler = handlers.get::<GetUser>().unwrap();
assert_eq!(handler(), "user");
```

## Derive macros

With the `derive` feature, typex re-exports these macros from `typex_derive`:

- `#[derive(Meta)]`: provides structural read-only access for structs and
  enums. Add `#[typex(opaque)]` to hide fields and items.
- `#[derive(MetaMut)]`: adds structural mutation and whole-value replacement;
  `Meta` must be derived separately.

### Opaque types

By default, derived structs expose their fields and enums expose their active
variant and payload fields. Add `#[typex(opaque)]` when the representation
should remain hidden or child types do not implement the required reflection
trait:

<!-- check: name=opaque -->
```rust
#[derive(Debug, Meta, MetaMut)]
#[typex(opaque)]
struct UserId(u64);
```

For enums, `opaque` hides both the active variant and its payload fields:

<!-- check: name=opaque-enum -->
```rust
#[derive(Debug, Meta, MetaMut)]
#[typex(opaque)]
enum Command {
  GetUser { id: u64 },
  Quit,
}
```

Opaque values still support replacement through
`ObjectRefMut::set`; see [Mutation](#mutation).

### Equality options

`#[typex(opaque)]` and `#[typex(partial_eq)]` control different aspects of a
derived type:

- `#[typex(opaque)]` hides fields and items. Without `partial_eq`, `eq_dyn()`
  has no fields or items to compare and returns `false`, even for the same
  value.
- `#[typex(partial_eq)]` uses the type's `PartialEq` implementation instead of
  structural equality. It can be used with or without `opaque`.
- `#[typex(opaque, partial_eq)]` hides the structure and uses `PartialEq`.

An opaque type without `partial_eq` is unequal even to itself:

<!-- check: name=opaque-equality-disabled -->
```rust
#[derive(Debug, Meta)]
#[typex(opaque)]
struct OpaqueId(u64);

let id = OpaqueId(7);
assert!(!id.eq_dyn(&id));
```

Combining `opaque` with `partial_eq` restores equality via `PartialEq`:

<!-- check: name=opaque-eq -->
```rust
#[derive(Debug, PartialEq, Meta)]
#[typex(opaque, partial_eq)]
struct UserId(u64);

let id = UserId(7);
assert!(id.eq_dyn(&UserId(7)));
```

### Capabilities and bounds

Field types must implement `Meta`. The crate provides implementations for the
[supported types](#supported-types).

For generic types, the derives preserve the declared generics. They add a
`Meta` or `MetaMut` bound for each field type that uses a type parameter.

Bounding a field type that contains the derived type would overflow trait
resolution. For such a field, the derives bound only the parts that use a type
parameter without the derived type. Here, `Tree<T>` requires `T: Meta` for
`value`, but no bound on `Vec<Tree<T>>`:

<!-- check: name=recursive-bounds -->
```rust
#[derive(Meta)]
struct Tree<T> {
  value: T,
  children: Vec<Tree<T>>,
}

let tree = Tree {
  value: "root",
  children: vec![Tree { value: "leaf", children: Vec::new() }],
};

assert_eq!(
  tree.field_path(Tree::<&str>::FIELD_CHILDREN.item(0).then(Tree::FIELD_VALUE)),
  Some(&"leaf")
);
```

Opaque types expose no child paths, so they add no child capability bounds.

`Meta` and `MetaMut` do not require `Clone` or `Debug` for ordinary reflective
access. Use `ObjectMut::from_clone` when an owned clone is needed. Wrapper
`Debug` implementations report type metadata and shape, so they do not require
the underlying value to implement `Debug`.

## Mutation

`ObjectRefMut::set` replaces the whole value with an owned `Object` when
the concrete types match. This also works for opaque values. `ObjectRefMut` also
provides structural mutation through fields, items and keys:

<!-- check: name=mutation extends=paths -->
```rust
#[derive(Clone, Debug, Meta, MetaMut)]
#[typex(opaque)]
struct UserId(u64);

let mut id = UserId(7);
ObjectRefMut::new(&mut id)
  .set(Object::new(UserId(8)))
  .unwrap();
assert_eq!(id.0, 8);

*ObjectRefMut::new(&mut payload)
  .field_mut("count")
  .unwrap()
  .to_mut::<u8>()
  .unwrap() = 4;
assert_eq!(payload.count, 4);
```

The derive macro defines constants for type-safe access. Compose them with
`then` for nested fields, `item` for sequence items and `key` for map entries:

<!-- check: name=mutation-paths extends=mutation -->
```rust
let mut root = ObjectRefMut::new(&mut payload);
*root
  .field_path_mut(Payload::FIELD_LABELS.item(0).then(Label::FIELD_NAME))
  .unwrap() = "secondary";
root
  .set_field_path(Payload::FIELD_BYTES.item(1), 0x0b_u8)
  .unwrap();
drop(root);

assert_eq!(payload.labels[0].name, "secondary");
assert_eq!(payload.bytes[1], 0x0b);
```

`ObjectRefMut` also provides collection operations (`item_mut`,
`key_mut`, `push_item`, `insert_key`). Each returns a mutable view that supports
`set` to replace the value or `to_mut` to access the underlying value.

<!-- check: name=mutation-operations -->
```rust
use std::collections::BTreeMap;
use typex::ReflectiveError;

let mut values = vec![1_u8];
let mut values_ref = ObjectRefMut::new(&mut values);
values_ref
  .item_mut(0)
  .unwrap()
  .set(Object::new(2_u8))
  .unwrap();
*values_ref
  .push_item(Object::new(3_u8))
  .unwrap()
  .to_mut::<u8>()
  .unwrap() += 1;
drop(values_ref);

assert_eq!(values, vec![2, 4]);

let mut map = BTreeMap::from([(String::from("primary"), 1_u8)]);
let mut map_ref = ObjectRefMut::new(&mut map);
map_ref.key_mut("primary").unwrap().set(Object::new(2_u8)).unwrap();
map_ref.insert_key("secondary", Object::new(4_u8)).unwrap();
drop(map_ref);

assert_eq!(map, BTreeMap::from([
  (String::from("primary"), 2_u8),
  (String::from("secondary"), 4_u8),
]));

assert!(matches!(
  ObjectRefMut::new(&mut values).item_mut(42),
  Err(ReflectiveError::PathNotFound)
));
```

## Reflective patching

A patch groups mutating operations that target one or more paths and applies
them in order. Use it to batch fine-grained updates.

<!-- check: name=reflective-patching -->
```rust
use std::collections::BTreeMap;
use typex::PatchOperation;

#[derive(Debug, Meta, MetaMut)]
struct ServiceConfig {
  enabled: bool,
  retries: u8,
  labels: BTreeMap<String, u8>,
}

let mut config = ServiceConfig {
  enabled: true,
  retries: 1,
  labels: BTreeMap::from([(String::from("primary"), 1)]),
};

let patch = vec![
  PatchOperation::set([PathSegment::Field("enabled")], false),
  PatchOperation::insert_key(
    [PathSegment::Field("labels")],
    "primary",
    2_u8,
  ),
  PatchOperation::insert_key(
    [PathSegment::Field("labels")],
    "secondary",
    3_u8,
  ),
];

ObjectRefMut::new(&mut config).apply(patch).unwrap();

assert!(!config.enabled);
assert_eq!(config.retries, 1);
assert_eq!(config.labels.get("primary"), Some(&2));
assert_eq!(config.labels.get("secondary"), Some(&3));

let path_patch = vec![
  PatchOperation::set([PathSegment::Field("enabled")], true),
  PatchOperation::set(
    [
      PathSegment::Field("labels"),
      PathSegment::Key("primary"),
    ],
    4_u8,
  ),
];

ObjectRefMut::new(&mut config).apply(path_patch).unwrap();

assert!(config.enabled);
assert_eq!(config.labels.get("primary"), Some(&4));
```

`PatchOperation` also provides `remove_key`, `insert_item`, `push_item`,
`remove_item` and `move_item`. `ObjectRefMut::apply` applies operations in
order, stops at the first failure and leaves earlier successful operations
applied. Use a `MutationBatch` when earlier changes must be undone after a
later operation fails.

`SequenceAccessMut::move_item` defaults to `MoveItemError::Unsupported`.
Sequential access implementations override it with a native move operation.

## Staged mutation batches

Use a `MutationBatch` when a group of changes must be committed
transactionally. A batch owns its operations without borrowing or changing the
target. `commit` consumes the batch, applies operations in order against the
target's current state and records owned inverses for successful operations. If
a later operation fails, it replays those inverses in reverse order to roll
back the earlier changes.

<!-- check: name=staged-mutation -->
```rust
use typex::{MutationBatch, PatchOperation};

let mut values = vec![1_u8];
let batch = MutationBatch::from([
  PatchOperation::set([PathSegment::Item(0)], 2_u8),
  PatchOperation::push_item([], 3_u8),
]);

let rollback = batch.commit(&mut values).unwrap();
assert_eq!(values, vec![2, 3]);
rollback.undo(&mut values).unwrap();
assert_eq!(values, vec![1]);
```

Call `MutationRollback::undo` immediately or retain the rollback to undo the
changes later. A later batch can then stage replacement and structural
operations in its execution order:

<!-- check: name=staged-mutation-rollback -->
```rust
use typex::{MutationBatch, PatchOperation};

#[derive(Clone, Debug, Meta, MetaMut)]
struct Profile {
  name: &'static str,
}

#[derive(Clone, Debug, Meta, MetaMut)]
struct Settings {
  profiles: Vec<Profile>,
}

let mut settings = Settings {
  profiles: vec![Profile { name: "primary" }],
};
let name_path = [
  PathSegment::Field("profiles"),
  PathSegment::Item(0),
  PathSegment::Field("name"),
];

let batch = MutationBatch::from([PatchOperation::set(name_path, "discarded")]);
batch
  .commit(&mut settings)
  .unwrap()
  .undo(&mut settings)
  .unwrap();
assert_eq!(settings.profiles[0].name, "primary");

let batch = MutationBatch::from([
  PatchOperation::set(name_path, "secondary"),
  PatchOperation::push_item(
    [PathSegment::Field("profiles")],
    Profile { name: "quaternary" },
  ),
  PatchOperation::insert_item(
    [PathSegment::Field("profiles")],
    1,
    Profile { name: "tertiary" },
  ),
  PatchOperation::remove_item([PathSegment::Field("profiles")], 0),
]);

let rollback = batch.commit(&mut settings).unwrap();
assert_eq!(
  settings.profiles.iter().map(|profile| profile.name).collect::<Vec<_>>(),
  vec!["tertiary", "quaternary"]
);
rollback.undo(&mut settings).unwrap();
assert_eq!(settings.profiles[0].name, "primary");
```

Structural operations affect the paths and indexes used by later operations.
Rollback replays an inverse for each operation instead of restoring a
whole-value snapshot, and staged values become visible only after `commit`.
Custom `MetaMut` implementations must implement `MetaMut::replace_dyn` correctly
when whole-value inverse operations are required.

A custom `MetaMut` implementation can also make a commit fail in a way that
leaves the target inconsistent. `CommitError::RollbackFailed` reports an
inverse that could not be applied; `CommitOperationError::ItemLost` reports a
move that could not restore its item. In both cases, treat the target as
invalid for transactional purposes.

## Thread-safe objects

`Object` is not `Send`. Use `SendObject` to move a boxed object across thread
boundaries. `SendMeta` is the backing trait for `SendObject`; normal callers
should use the owned wrapper:

<!-- check: name=send-object -->
```rust
#[derive(Debug, Meta)]
struct Job(u32);

let job = SendObject::new(Job(42));

std::thread::spawn(move || {
  assert!(job.is::<Job>());
})
.join()
.unwrap();
```

## Hand-written implementations

`Meta::reflect()` returns the value's structural shape as a `Reflect`. Each
structural shape has an access trait: `StructAccess`, `SequenceAccess` or
`MapAccess`. `MetaMut::reflect_mut()` does the same for mutation through
`StructAccessMut`, `SequenceAccessMut`, `MapAccessMut` and `OptionAccessMut`.
The shape determines `kind()` and `access_kind()`, so a value cannot report a
shape without providing its access.

Callers do not import the access traits. `Object`, `ObjectRef`, `ObjectRefMut`
and the trait objects dispatch to them. Importing `Meta` or `MetaMut` for the
derives therefore brings only `type_info`, `reflect`, `reflect_mut`, `eq_dyn`,
`set_dyn`, `replace_dyn` and the `Any` conversions into scope, and a call such
as `part.len()` on a `&&str` still reaches `str::len`.

<!-- check: name=manual-sequence -->
```rust
use typex::{ObjectRef, Reflect, SequenceAccess};

struct Ring {
  values: Vec<u8>,
  start: usize,
}

impl Meta for Ring {
  fn reflect(&self) -> Reflect<'_> {
    Reflect::Sequence(self)
  }

  fn into_any(self: Box<Self>) -> Box<dyn core::any::Any> {
    self
  }

  fn as_any(&self) -> &dyn core::any::Any {
    self
  }
}

impl SequenceAccess for Ring {
  fn len(&self) -> usize {
    self.values.len()
  }

  fn item(&self, index: usize) -> Option<ObjectRef<'_>> {
    let len = self.values.len();
    (index < len).then(|| ObjectRef::new(&self.values[(self.start + index) % len] as &dyn Meta))
  }
}

let ring = Ring {
  values: vec![1, 2, 3],
  start: 1,
};
let view = ObjectRef::new(&ring);

assert_eq!(view.kind(), ValueKind::Sequence);
assert_eq!(view.len(), Some(3));
assert_eq!(view.item(0).unwrap().to_ref::<u8>(), Some(&2));
```

## Workspace structure

The workspace contains two crates:

- `typex`: runtime reflection, mutation, object APIs
- `typex_derive`: derive macros for `Meta` and `MetaMut`, re-exported by the
  `derive` feature of `typex`

## Licence

Licensed under the [Apache License, Version 2.0](LICENCE).
