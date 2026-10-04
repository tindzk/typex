# Contributing

Add regression tests for public behaviour changes.

For architectural changes, read `DESIGN.md` first, then update it alongside
the implementation, relevant tests and public documentation. Document API
details in the README and Rust API documentation.

## Verification

Run the checks relevant to the change:

- `cargo fmt --all -- --check`
- `cargo test --workspace --all-features`
- `cargo doc --workspace --no-deps`

Before a release, run `./scripts/release-check.sh`.

## Benchmarks

`typex/benches/instruction_counts.rs` counts instructions with
[iai-callgrind](https://github.com/iai-callgrind/iai-callgrind), which needs
Valgrind and `iai-callgrind-runner`. Counts are deterministic, so a single run
is enough to compare two versions:

- `cargo bench -p typex --bench instruction_counts -- --save-baseline=before`
  records the counts before a change
- `cargo bench -p typex --bench instruction_counts -- --baseline=before`
  compares the changed code against them

A benchmark fails when its instruction count rises by more than 5%.

Add a benchmark that covers any change made for performance, and record the
outcome and its reason in `DESIGN.md`. Describe results as approximate
percentages rather than exact counts, because counts shift with compiler
releases.
