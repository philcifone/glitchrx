//! Filesystem rules: output naming and safe writes.
//!
//! Two promises are kept here:
//!
//! 1. **The input is never written.** Even with `force`, an output path that
//!    resolves to the input file (directly, through a symlink, or as a hard
//!    link) is refused.
//! 2. **Nothing is overwritten by accident.** The output is written to a
//!    temporary file in the target directory and then moved into place with a
//!    no-clobber rename, which fails atomically if the target exists, even
//!    if another process created it a moment ago.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use thiserror::Error;

/// Why a write was refused or failed.
#[derive(Debug, Error)]
pub enum WriteError {
    #[error("refusing to write to the input file {0}")]
    SameAsInput(PathBuf),
    #[error("{0} already exists (use --force to overwrite)")]
    Exists(PathBuf),
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: io::Error },
}

/// Strip trailing `-glitch`, `-glitch-N` (and the `-restored` names the
/// archived JPEG engine wrote), any number of times, so that re-glitching `photo-glitch-1.jpg` continues the
/// `photo-glitch-*` sequence instead of starting `photo-glitch-1-glitch`.
pub fn base_stem(stem: &str) -> &str {
    let mut s = stem;
    loop {
        let before = s;
        for suffix in ["glitch", "restored"] {
            let tag = format!("-{suffix}");
            if let Some(rest) = s.strip_suffix(tag.as_str()) {
                s = rest;
            } else if let Some((head, n)) = s.rsplit_once('-')
                && !n.is_empty()
                && n.bytes().all(|b| b.is_ascii_digit())
                && let Some(rest) = head.strip_suffix(tag.as_str())
            {
                s = rest;
            }
        }
        if s == before || s.is_empty() {
            return if s.is_empty() { before } else { s };
        }
    }
}

/// If `stem` is `{base}-{suffix}` or `{base}-{suffix}-N`, return N (0 for
/// the bare form).
fn sequence_number(stem: &str, base: &str, suffix: &str) -> Option<u64> {
    let rest = stem
        .strip_prefix(base)?
        .strip_prefix('-')?
        .strip_prefix(suffix)?;
    if rest.is_empty() {
        return Some(0);
    }
    let n = rest.strip_prefix('-')?;
    if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    n.parse().ok()
}

/// Pick the next free automatic output name for `input`.
///
/// `photo.jpg` with extension `tif` gives `photo-glitch.tif`, then
/// `photo-glitch-1.tif`, `photo-glitch-2.tif`, ... The number is one past
/// the highest existing one (any extension counts, so TIFF, PNG and JPEG
/// outputs of the same photo never share a number).
pub fn auto_output(input: &Path, extension: &str) -> io::Result<PathBuf> {
    let dir = match input.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let base = base_stem(&stem).to_string();
    let tag = "glitch";

    let mut highest: Option<u64> = None;
    for entry in fs::read_dir(&dir)? {
        let name = entry?.file_name();
        let name = name.to_string_lossy();
        // Compare against the stem: everything before the last dot.
        let entry_stem = name.rsplit_once('.').map_or(name.as_ref(), |(s, _)| s);
        if let Some(n) = sequence_number(entry_stem, &base, tag) {
            highest = Some(highest.map_or(n, |h| h.max(n)));
        }
    }
    let file_stem = match highest {
        None => format!("{base}-{tag}"),
        Some(n) => format!("{base}-{tag}-{}", n + 1),
    };
    let name = format!("{file_stem}.{extension}");
    // with_file_name keeps the input's own directory spelling (no "./").
    Ok(input.with_file_name(name))
}

/// True if `a` and `b` are the same file on disk (symlinks and hard links
/// included). A path that does not exist is never the same as anything.
pub fn same_file(a: &Path, b: &Path) -> bool {
    same_file::is_same_file(a, b).unwrap_or(false)
}

/// Write `bytes` to `output`, never touching `input`.
///
/// Without `force`, an existing `output` is an error. The new file gets the
/// input's permission bits (plus owner-write), rather than the 0600 that
/// temporary files are created with.
pub fn write_output(
    input: &Path,
    output: &Path,
    bytes: &[u8],
    force: bool,
) -> Result<(), WriteError> {
    if same_file(input, output) {
        return Err(WriteError::SameAsInput(output.to_path_buf()));
    }
    if !force && fs::symlink_metadata(output).is_ok() {
        return Err(WriteError::Exists(output.to_path_buf()));
    }
    let io_err = |source| WriteError::Io {
        path: output.to_path_buf(),
        source,
    };
    let dir = match output.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let mut tmp = tempfile::Builder::new()
        .prefix(".glitchrx-")
        .tempfile_in(dir)
        .map_err(io_err)?;
    tmp.write_all(bytes).map_err(io_err)?;
    tmp.as_file().sync_all().map_err(io_err)?;
    if let Ok(meta) = fs::metadata(input) {
        let mut perms = meta.permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(perms.mode() | 0o200);
        }
        #[cfg(not(unix))]
        perms.set_readonly(false);
        fs::set_permissions(tmp.path(), perms).map_err(io_err)?;
    }
    let result = if force {
        tmp.persist(output).map(|_| ())
    } else {
        tmp.persist_noclobber(output).map(|_| ())
    };
    result.map_err(|e| {
        if e.error.kind() == io::ErrorKind::AlreadyExists {
            WriteError::Exists(output.to_path_buf())
        } else {
            io_err(e.error)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_stems() {
        assert_eq!(base_stem("photo"), "photo");
        assert_eq!(base_stem("photo-glitch"), "photo");
        assert_eq!(base_stem("photo-glitch-12"), "photo");
        assert_eq!(base_stem("photo-glitch-restored"), "photo");
        assert_eq!(base_stem("photo-restored-2"), "photo");
        assert_eq!(base_stem("my-photo-2"), "my-photo-2");
        assert_eq!(base_stem("glitch"), "glitch");
        assert_eq!(base_stem("-glitch"), "-glitch");
    }

    #[test]
    fn sequence_numbers() {
        assert_eq!(sequence_number("a-glitch", "a", "glitch"), Some(0));
        assert_eq!(sequence_number("a-glitch-3", "a", "glitch"), Some(3));
        assert_eq!(sequence_number("a-glitch-x", "a", "glitch"), None);
        assert_eq!(sequence_number("ab-glitch", "a", "glitch"), None);
        assert_eq!(sequence_number("a-glitchy", "a", "glitch"), None);
    }
}
