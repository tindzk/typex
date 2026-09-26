#!/usr/bin/env bash
set -euo pipefail

cargo fmt --check
cargo test
cargo test --doc
cargo test --manifest-path tests/no_std/Cargo.toml
cargo package --allow-dirty --list -p typex
cargo package --allow-dirty --list -p typex_derive
