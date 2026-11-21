# Command runner recipes. Install `just` with your package manager,
# or call scripts/check.sh directly.

# Format check, clippy (warnings denied), tests.
check:
    scripts/check.sh

# Release build of the glitchrx binary (target/release/glitchrx).
build:
    cargo build --release -p glitchrx-cli

# Regenerate the committed test fixtures. Only needed when the fixture set changes.
fixtures:
    cargo run -p gen-fixtures -- tests/fixtures

# Parameter-sweep grids for visual tuning, written to out/sheets.
sheets input="tests/fixtures/baseline-420.jpg":
    cargo run --release -p contact-sheet -- {{input}} out/sheets
