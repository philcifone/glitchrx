//! Browsing photos on the server, confined to the `--root` directory.
//!
//! Paths from the browser are always relative to the root and use `/`. They
//! are resolved with `canonicalize`, which follows symlinks and removes `..`,
//! and the result must still lie inside the (canonical) root. That one check
//! covers `../../etc/passwd`, absolute paths and symlinks pointing outside.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// File extensions offered in the browser (case-insensitive).
const PHOTO_EXTENSIONS: &[&str] = &["jpg", "jpeg", "jpe", "jfif", "png", "tif", "tiff"];

#[derive(Debug, Serialize)]
pub struct Listing {
    /// This directory, relative to the root (`""` for the root itself).
    pub path: String,
    /// The parent directory, or `None` at the root.
    pub parent: Option<String>,
    pub dirs: Vec<Entry>,
    pub files: Vec<Entry>,
}

#[derive(Debug, Serialize)]
pub struct Entry {
    pub name: String,
    /// Relative to the root.
    pub path: String,
    pub size: u64,
}

/// Why a path was refused.
#[derive(Debug)]
pub enum BrowseError {
    /// Missing, unreadable, or outside the root (the browser is not told
    /// which, so it cannot probe for files outside the root).
    NotFound,
    Io(String),
}

/// Resolve a root-relative path to a canonical absolute path inside `root`.
/// `root` must itself be canonical.
pub fn resolve(root: &Path, rel: &str) -> Result<PathBuf, BrowseError> {
    let rel = rel.trim_start_matches('/');
    let joined = if rel.is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel)
    };
    let real = joined.canonicalize().map_err(|_| BrowseError::NotFound)?;
    if real.starts_with(root) {
        Ok(real)
    } else {
        Err(BrowseError::NotFound)
    }
}

/// The root-relative form of a canonical path inside `root`, with `/`.
pub fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

pub fn is_photo(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|e| PHOTO_EXTENSIONS.contains(&e.as_str()))
}

/// List the subdirectories and photos of a root-relative directory.
/// Hidden entries (dot files) are skipped.
pub fn list(root: &Path, rel: &str) -> Result<Listing, BrowseError> {
    let dir = resolve(root, rel)?;
    if !dir.is_dir() {
        return Err(BrowseError::NotFound);
    }
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| BrowseError::Io(e.to_string()))? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        // Follow symlinks for the type, but only list targets inside root.
        let Ok(real) = entry.path().canonicalize() else {
            continue;
        };
        if !real.starts_with(root) {
            continue;
        }
        let Ok(meta) = fs::metadata(&real) else {
            continue;
        };
        let item = Entry {
            path: relative(root, &dir.join(&name)),
            name,
            size: meta.len(),
        };
        if meta.is_dir() {
            dirs.push(item);
        } else if meta.is_file() && is_photo(&real) {
            files.push(item);
        }
    }
    let by_name = |a: &Entry, b: &Entry| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.name.cmp(&b.name))
    };
    dirs.sort_by(by_name);
    files.sort_by(by_name);
    let path = relative(root, &dir);
    let parent = (dir != root).then(|| relative(root, dir.parent().unwrap_or(root)));
    Ok(Listing {
        path,
        parent,
        dirs,
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confined_to_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("sub/a.JPG"), b"x").unwrap();
        fs::write(root.join("notes.txt"), b"x").unwrap();
        fs::write(root.join(".hidden.jpg"), b"x").unwrap();
        fs::write(tmp.path().join("outside.jpg"), b"x").unwrap();
        let root = root.canonicalize().unwrap();

        let top = list(&root, "").unwrap();
        assert_eq!(top.parent, None);
        assert_eq!(top.dirs.len(), 1);
        assert!(top.files.is_empty(), "only photos, no dot files");

        let sub = list(&root, "sub").unwrap();
        assert_eq!(sub.parent.as_deref(), Some(""));
        assert_eq!(sub.files[0].path, "sub/a.JPG");

        assert!(resolve(&root, "../outside.jpg").is_err());
        assert!(resolve(&root, "sub/../../outside.jpg").is_err());
        assert!(
            resolve(&root, "/sub/a.JPG").is_ok(),
            "leading slash is relative"
        );
        assert!(resolve(&root, "missing.jpg").is_err());

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(tmp.path().join("outside.jpg"), root.join("link.jpg"))
                .unwrap();
            assert!(resolve(&root, "link.jpg").is_err());
            assert!(list(&root, "").unwrap().files.is_empty());
        }
    }
}
