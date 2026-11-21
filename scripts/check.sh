#!/usr/bin/env bash
# The gate: formatting, lints (warnings are errors), and the full test suite.
# Must pass before any work is called done.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "==> cargo fmt --check"
cargo fmt --all --check

echo "==> cargo clippy"
cargo clippy --workspace --all-targets -- -D warnings

echo "==> cargo test"
cargo test --workspace

echo "==> gate passed"
