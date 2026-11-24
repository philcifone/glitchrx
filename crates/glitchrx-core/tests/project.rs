//! The committed project file (`tests/fixtures/project-v1.glrx`) opens and
//! replays to the same picture in every later build: project files are a
//! promise kept across versions, as recipes are (SPEC 8.1).
//!
//! Regenerate (only when the format itself changes, with a new version):
//! UPDATE_FIXTURES=1 cargo test -p glitchrx-core --test project

use std::path::PathBuf;

use glitchrx_core::Recipe;
use glitchrx_core::codec;
use glitchrx_core::project::{self, Project};

fn path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/project-v1.glrx")
}

/// An upright-turned photo, a crop, two steps, one masked.
const RECIPE: &str = r#"
format = "glitcharaxia-recipe"
version = 4

[crop]
x = 0.1
y = 0.0
w = 0.8
h = 1.0

[[step]]
effect = "echo"
effect_version = 1
seed = 11

[[step]]
effect = "bitcrush"
effect_version = 1
seed = 12

[step.mask]
[[step.mask.op]]
shape = "radial"
from = [0.5, 0.5]
to = [0.8, 0.5]
"#;

fn make() -> Vec<u8> {
    let dir = path().parent().unwrap().to_path_buf();
    let original = std::fs::read(dir.join("orient-6.jpg")).unwrap();
    let recipe = Recipe::from_toml(RECIPE).unwrap();
    let image = codec::decode(&original).unwrap();
    let picture = recipe.render(&image.raster).unwrap();
    let preview = project::preview(&picture, image.icc.as_deref()).unwrap();
    Project::new(recipe, "orient-6.jpg", &original, Some(preview))
        .unwrap()
        .to_bytes()
        .unwrap()
}

#[test]
fn the_version_1_project_replays() {
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(path(), make()).unwrap();
    }
    let bytes = std::fs::read(path()).expect("tests/fixtures/project-v1.glrx");
    assert!(project::is_project(&bytes));
    let p = Project::from_bytes(&bytes).unwrap();
    assert_eq!(p.name, "orient-6.jpg");
    assert_eq!(p.recipe.source.as_deref(), Some("orient-6.jpg"));
    assert_eq!(p.recipe.steps.len(), 2);
    let image = codec::decode(&p.original).unwrap();
    assert_eq!(
        image.orientation, 6,
        "orientation survives location removal"
    );
    let picture = p.recipe.render(&image.raster).unwrap();
    let preview = codec::decode(p.preview.as_deref().unwrap()).unwrap();
    assert!(preview.raster.width.max(preview.raster.height) <= project::PREVIEW_SIDE);
    insta::assert_yaml_snapshot!(
        "project-v1",
        (
            blake3::hash(&p.original).to_hex().to_string(),
            blake3::hash(&picture.bytes).to_hex().to_string()
        )
    );
}

#[test]
fn a_project_made_today_reads_like_the_committed_one() {
    // Catches an accidental change to what goes in. Not byte for byte: the
    // writer's version and the preview's JPEG encoder may change freely.
    let (made, kept) = (
        Project::from_bytes(&make()).unwrap(),
        Project::from_bytes(&std::fs::read(path()).unwrap()).unwrap(),
    );
    assert_eq!(made.original, kept.original);
    assert_eq!(made.recipe, kept.recipe);
    assert_eq!(made.name, kept.name);
}
