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
