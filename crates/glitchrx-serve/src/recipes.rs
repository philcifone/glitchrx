//! Recipes saved on the server (SPEC 5e, D-51): found in the library and
//! export folders so the GUI can load one without a file picker, and
//! matched to a photo when it is opened, to offer "continue from".
//!
//! A recipe matches a photo when it names it (`source`, written by every
//! save since D-51), or, for older recipes, when its file name is the
//! export name the GUI and CLI give: `photo-glitch.recipe.toml`,
//! `photo-glitch-2.recipe.toml`, or `photo.recipe.toml`.
//!
//! The search is shallow and bounded (three folders deep, a few thousand
//! entries), so a big photo library cannot stall opening a photo.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::browse;

/// The ending every recipe file has.
pub const SUFFIX: &str = ".recipe.toml";
/// Folders below a root that are searched.
const DEPTH: usize = 3;
/// Directory entries looked at, at most, per root.
const MAX_ENTRIES: usize = 5000;
/// Recipes returned, at most (newest first).
const MAX_FOUND: usize = 300;
/// Larger files are not recipes anyone wrote by hand or saved here.
pub const MAX_BYTES: u64 = 1 << 20;

/// Which folder a recipe was found in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Place {
    Library,
    Export,
}

#[derive(Debug, Clone, Serialize)]
pub struct Found {
    pub place: Place,
    /// Relative to that folder, with `/`.
    pub path: String,
    pub name: String,
    /// Seconds since 1970, for "newest first" and for showing the age.
    pub modified: u64,
    /// The photo it was made on, when the recipe says.
    pub source: Option<String>,
}

/// Every recipe under the given (canonical) folders, newest first. A
/// folder given twice (the export folder inside the library, say) is only
/// listed once, under the first name.
pub fn scan(roots: &[(Place, PathBuf)]) -> Vec<Found> {
    let mut found = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (place, root) in roots {
        let mut budget = MAX_ENTRIES;
        walk(*place, root, root, 0, &mut budget, &mut found, &mut seen);
    }
    found.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.path.cmp(&b.path)));
    found.truncate(MAX_FOUND);
    found
}

fn walk(
    place: Place,
    root: &Path,
    dir: &Path,
    depth: usize,
    budget: &mut usize,
    found: &mut Vec<Found>,
    seen: &mut std::collections::HashSet<PathBuf>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        // Symlinks are followed only to places inside the root.
        let Ok(real) = entry.path().canonicalize() else {
            continue;
        };
        if !real.starts_with(root) {
            continue;
        }
        let Ok(meta) = fs::metadata(&real) else {
            continue;
        };
        if meta.is_dir() {
            subdirs.push(real);
        } else if meta.is_file()
            && name.ends_with(SUFFIX)
            && meta.len() <= MAX_BYTES
            && seen.insert(real.clone())
        {
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            found.push(Found {
                place,
                path: browse::relative(root, &dir.join(&name)),
                source: source_of(&real),
                name,
                modified,
            });
        }
    }
    if depth < DEPTH {
        subdirs.sort();
        for d in subdirs {
            walk(place, root, &d, depth + 1, budget, found, seen);
        }
    }
}

/// The `source` a recipe file names, read without checking the rest.
fn source_of(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let table: toml::Table = toml::from_str(&text).ok()?;
    table.get("source")?.as_str().map(str::to_string)
}

/// Whether a recipe was made on the photo called `photo` (a file name).
pub fn matches(found: &Found, photo: &str) -> bool {
    if found.source.as_deref() == Some(photo) {
        return true;
    }
    if found.source.is_some() {
        // It names another photo; a similar file name is a coincidence.
        return false;
    }
    let stem = Path::new(photo)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let Some(base) = found.name.strip_suffix(SUFFIX) else {
        return false;
    };
    if stem.is_empty() {
        return false;
    }
    if base == stem || base == format!("{stem}-glitch") {
        return true;
    }
    // photo-glitch-2, photo-glitch-17: the numbered exports.
    base.strip_prefix(&format!("{stem}-glitch-"))
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(name: &str, source: Option<&str>) -> Found {
        Found {
            place: Place::Export,
            path: name.into(),
            name: name.into(),
            modified: 0,
            source: source.map(str::to_string),
        }
    }

    #[test]
    fn matching_by_source_or_export_name() {
        assert!(matches(
            &found("anything.recipe.toml", Some("IMG_1.jpg")),
            "IMG_1.jpg"
        ));
        assert!(!matches(
            &found("IMG_1-glitch.recipe.toml", Some("IMG_2.jpg")),
            "IMG_1.jpg"
        ));
        for name in [
            "IMG_1-glitch.recipe.toml",
            "IMG_1-glitch-12.recipe.toml",
            "IMG_1.recipe.toml",
        ] {
            assert!(matches(&found(name, None), "IMG_1.jpg"), "{name}");
        }
        for name in [
            "IMG_10-glitch.recipe.toml",
            "IMG_1-glitch-x.recipe.toml",
            "IMG_1-glitch.toml",
        ] {
            assert!(!matches(&found(name, None), "IMG_1.jpg"), "{name}");
        }
    }

    #[test]
    fn scans_both_folders_newest_first_and_skips_the_rest() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap();
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::create_dir_all(root.join(".hidden")).unwrap();
        fs::write(root.join("one.recipe.toml"), "source = \"x.jpg\"\n").unwrap();
        fs::write(root.join("a/b/two.recipe.toml"), "").unwrap();
        fs::write(root.join(".hidden/three.recipe.toml"), "").unwrap();
        fs::write(root.join("photo.jpg"), "").unwrap();
        let list = scan(&[
            (Place::Library, root.clone()),
            (Place::Export, root.join("a")),
        ]);
        let names: Vec<_> = list.iter().map(|f| (f.place, f.path.as_str())).collect();
        assert_eq!(list.len(), 2, "{names:?}");
        assert!(names.contains(&(Place::Library, "a/b/two.recipe.toml")));
        let one = list.iter().find(|f| f.name == "one.recipe.toml").unwrap();
        assert_eq!(one.source.as_deref(), Some("x.jpg"));
    }
}
