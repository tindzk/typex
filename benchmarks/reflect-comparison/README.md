# Reflection comparison

Standalone examples compare this checkout of typex with bevy_reflect 0.19.1
and Facet 0.46.5. The examples sit outside the main workspace and use
independent lockfiles.

Run from the repository root:

```sh
python3 benchmarks/reflect-comparison/run.py --build-runs 3 --jobs 4
```

On Linux, add `--cpu N` to pin runtime measurements to one CPU. The runner
disables Rust compiler wrappers, fetches dependencies before timing and builds
each trial offline in a fresh temporary target directory. Temporary build
artifacts are removed when the runner exits.

The variants are typex with `derive`, Bevy with default features, Bevy with
default features disabled and `std` enabled and Facet with default features
and `reflect` enabled. Facet needs `reflect` to expose `Peek` and `Poke`.
The Facet example also imports `facet-path`, which is already a transitive
dependency of `facet-reflect`. All release profiles use one code generation
unit and no LTO. Debug builds use Cargo's default development profile. Each
example derives inspection and mutation for the same two structs and stores
the same scalar, string and vector values.

Dependency counts include resolved normal and build dependencies recursively,
excluding the example binary and the compared library. Distinct versions of
one crate count separately. Counts come from `cargo tree`, rather than the
lockfile, which also contains dependencies for disabled features and other
targets.

Each runtime operation runs two million times per sample after a warm-up.
There are nine samples per operation and three executions per binary. Output
reports nanoseconds per operation, including loop and measurement overhead.
The `baseline` operation helps identify that overhead. Inputs cross
`black_box` as erased trait objects for typex and Bevy, or as erased `Peek`
and `Poke` views for Facet. Results also cross `black_box`.

The shared operations are successful scalar downcasting, named field access,
named field access with a cached structural view, prepared nested and indexed
paths and mutation of one named scalar field. The mutation cases include
starting from an erased mutable view and starting from a cached structural
view. Facet's first case uses `Poke::try_reborrow` inside the timed operation;
its cached case prepares the `PokeStruct` before timing. The Facet structs use
`#[facet(pod)]` to allow field mutation.

Paths are prepared before timing. Typex uses a borrowed segment array, Bevy
uses `ParsedPath` and Facet uses `facet_path::Path`. Facet's paths use field
indices, while typex's and Bevy's paths use field names. Bevy also measures a
string path parsed during each lookup; that operation has no equivalent in
the supplied typex and Facet APIs.

Basic correctness assertions run before measurement. Typex and Bevy measure
their default structural equality implementations, with no delegation to
derived `PartialEq`. Facet's `Peek::partial_eq` requires the reflected type
to implement `PartialEq`. The Facet example asserts that this operation
returns an error for these structs, so its equality result is unavailable.
These examples measure a small data model on one machine;
compilation of hundreds of derives, incremental compilation, allocations,
serialisation and transactional mutation require separate workloads.

## Measured results

The recorded run uses an Apple M2 Pro with aarch64 Linux and Rust 1.97.0.
Builds use four jobs and runtime measurements use CPU 4. The three clean
builds per profile include dependency compilation and linking, with downloads
and compiler caches excluded. [results.json](results.json) contains the
resolved packages, individual build timings and runtime samples.

| Measurement | typex with derives | Bevy default | Bevy with `std` only | Facet with `reflect` |
| --- | ---: | ---: | ---: | ---: |
| Resolved dependencies | 5 | 36 | 34 | 25 |
| Clean debug build | 1.87 s | 4.88 s | 4.75 s | 3.08 s |
| Clean release build | 1.97 s | 9.32 s | 9.05 s | 5.12 s |

Runtime values are medians of the three executions' sample medians, in
nanoseconds per operation. Lower values mean less elapsed time.

| Operation | typex | Bevy default | Bevy with `std` only | Facet |
| --- | ---: | ---: | ---: | ---: |
| Baseline | 0.83 | 0.81 | 0.82 | 0.83 |
| Scalar downcast | 2.27 | 2.27 | 2.27 | 1.82 |
| Named field read | 2.64 | 3.86 | 3.86 | 4.93 |
| Named field read with cached structure | 2.61 | 3.13 | 3.16 | 4.01 |
| Prepared nested field path | 4.31 | 14.92 | 14.80 | 10.01 |
| Prepared indexed path | 4.63 | 15.45 | 15.42 | 11.03 |
| Structural equality | 22.20 | 85.82 | 92.31 | unavailable |
| Named field mutation | 2.47 | 3.88 | 3.88 | 37.55 |
| Named field mutation with cached structure | 2.47 | 3.08 | 3.10 | 5.24 |
| String path lookup | unavailable | 29.47 | 27.78 | unavailable |

In this example, typex has the fewest dependencies and shortest clean builds.
Facet's build times sit between typex and Bevy. Facet's scalar downcast takes
less time, while its named field access takes more time. Facet's prepared
paths take less time than Bevy's and more time than typex's.

Facet's mutable-view reborrow accounts for most of the difference between
its two mutation measurements. Preparing a `PokeStruct` outside the timed
operation reduces its mutation cost from about 38 ns to about 5 ns. The
mutation result therefore depends on how callers retain their mutable view.

The path results include the differences between typex's segment array,
Bevy's dynamic `ParsedPath` representation and Facet's paths with field
indices. The compiler can specialise typex's traversal for the array length.
Equality uses typex's and Bevy's structural defaults, while Facet requires
`PartialEq` on these structs. These measurements describe the supplied
examples and their API choices.
