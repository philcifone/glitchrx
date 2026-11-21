//! Keeps docs/effects.md in sync with the effect registry.
//!
//! Regenerate with: UPDATE_DOCS=1 cargo test -p glitchrx-core --test docs

use std::path::PathBuf;

#[test]
fn effects_reference_is_current() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/effects.md");
    let expected = glitchrx_core::effects::markdown_reference();
    if std::env::var_os("UPDATE_DOCS").is_some() {
        std::fs::write(&path, &expected).unwrap();
        return;
    }
    let actual = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        actual == expected,
        "docs/effects.md is out of date; run: UPDATE_DOCS=1 cargo test -p glitchrx-core --test docs"
    );
}
