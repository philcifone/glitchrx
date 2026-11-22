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

# Build the web GUI (web/dist), which the next cargo build embeds.
web:
    cd web && npm ci && npm run build

# Type-check and unit-test the web GUI.
web-check:
    cd web && npm run check && npm test

# Release binary with the web GUI embedded.
build-all: web build

# Run the GUI for use from other machines (LAN/VPN), with an access token.
serve root="samples/jpeg": build-all
    target/release/glitchrx serve --host 0.0.0.0 --token --root {{root}}

# Frontend hot-reload server on :5173 (0.0.0.0); needs `glitchrx serve` on :7878.
web-dev:
    cd web && npm run dev
