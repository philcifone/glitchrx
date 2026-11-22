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

# The web GUI is checked when its dependencies are installed (`just web`).
# A Rust-only checkout still passes the gate: the server then embeds a
# placeholder page instead of the GUI.
if [ -d web/node_modules ]; then
    echo "==> web: svelte-check"
    (cd web && npm run --silent check)
    echo "==> web: unit tests"
    (cd web && npm test --silent)
else
    echo "==> web: skipped (run \`just web\` to install and build the GUI)"
fi

echo "==> gate passed"
