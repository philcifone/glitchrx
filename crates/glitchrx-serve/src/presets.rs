//! Presets: the user's own saved recipes, kept by the server.
//!
//! Each preset is an ordinary recipe file, `<name>.toml`, in one directory
//! (by default `~/.config/glitchrx/presets`), so the phone and the desktop
//! share them through the server, and the CLI can replay any of them with
//! `glitchrx --recipe ~/.config/glitchrx/presets/<name>.toml photo.jpg`.
//!
//! Names become file names, so they are checked rather than cleaned up:
//! two different names never land on the same file, and nothing can reach
//! outside the directory. A preset is only replaced when asked to.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use glitchrx_core::Recipe;
use serde::Serialize;

/// Longest preset name, in characters.
pub const MAX_NAME: usize = 60;

/// The preset directory and the operations on it.
pub struct Presets {
    dir: PathBuf,
}

/// A preset as listed: enough to pick one without loading them all.
#[derive(Debug, Serialize, PartialEq)]
pub struct PresetInfo {
    pub name: String,
    /// Number of steps.
    pub steps: usize,
    /// Effect of each step, in order.
    pub effects: Vec<String>,
    /// Last saved, in seconds since 1970.
    pub modified: u64,
    /// Why the preset cannot be used with this build (say, an effect whose
    /// version has moved on since it was saved).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// What can go wrong, mapped to HTTP statuses by the API.
#[derive(Debug)]
pub enum PresetError {
    BadName(String),
    NotFound(String),
    Exists(String),
    Invalid(String),
    Io(io::Error),
}

impl std::fmt::Display for PresetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PresetError::BadName(m) | PresetError::Invalid(m) => f.write_str(m),
            PresetError::NotFound(n) => write!(f, "no preset named `{n}`"),
            PresetError::Exists(n) => write!(f, "a preset named `{n}` already exists"),
            PresetError::Io(e) => write!(f, "presets: {e}"),
        }
    }
}

impl From<io::Error> for PresetError {
    fn from(e: io::Error) -> Self {
        PresetError::Io(e)
    }
}

type Result<T> = std::result::Result<T, PresetError>;

/// Check a preset name and return it trimmed. Letters (any script),
/// digits, spaces and a little punctuation; no leading dot, no slashes.
pub fn check_name(name: &str) -> Result<&str> {
    let n = name.trim();
    let ok_char = |c: char| c.is_alphanumeric() || " -_.,()'&+!#".contains(c);
    if n.is_empty() {
        return Err(PresetError::BadName("a preset needs a name".into()));
    }
    if n.chars().count() > MAX_NAME {
        return Err(PresetError::BadName(format!(
            "preset names are at most {MAX_NAME} characters"
        )));
    }
    if n.starts_with('.') || !n.chars().all(ok_char) {
        return Err(PresetError::BadName(format!(
            "`{n}`: use letters, digits, spaces and - _ . , ( ) ' & + ! # (not a leading dot)"
        )));
    }
    Ok(n)
}

/// The default directory: `$XDG_CONFIG_HOME/glitchrx/presets`, or
/// `~/.config/glitchrx/presets`.
pub fn default_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("glitchrx").join("presets"))
}

impl Presets {
    /// Presets kept in `dir`, which is created on the first save.
    pub fn new(dir: PathBuf) -> Presets {
        Presets { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path(&self, name: &str) -> Result<PathBuf> {
        Ok(self.dir.join(format!("{}.toml", check_name(name)?)))
    }

    /// Every preset, by name (case-insensitive). A missing directory is an
    /// empty list.
    pub fn list(&self) -> Result<Vec<PresetInfo>> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_suffix(".toml"))
            else {
                continue;
            };
            // Skip files that could not have been saved here (temporary
            // files, hand-made names); they stay untouched.
            if check_name(name).ok() != Some(name) {
                continue;
            }
            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            let info = match fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|t| Recipe::from_toml(&t).map_err(|e| e.to_string()))
            {
                Ok(r) => PresetInfo {
                    name: name.to_string(),
                    steps: r.steps.len(),
                    effects: r.steps.iter().map(|s| s.effect.clone()).collect(),
                    modified,
                    error: None,
                },
                Err(e) => PresetInfo {
                    name: name.to_string(),
                    steps: 0,
                    effects: Vec::new(),
                    modified,
                    error: Some(e),
                },
            };
            out.push(info);
        }
        out.sort_by_key(|p| p.name.to_lowercase());
        Ok(out)
    }

    pub fn get(&self, name: &str) -> Result<Recipe> {
        let path = self.path(name)?;
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(PresetError::NotFound(name.trim().into()));
            }
            Err(e) => return Err(e.into()),
        };
        Recipe::from_toml(&text).map_err(|e| PresetError::Invalid(e.to_string()))
    }

    /// Save a recipe as a preset. An existing preset of that name is only
    /// replaced with `replace`; the new file is written beside it and
    /// renamed over it, so a failed save never leaves half a preset.
    pub fn put(&self, name: &str, recipe: &Recipe, replace: bool) -> Result<String> {
        let path = self.path(name)?;
        let name = check_name(name)?.to_string();
        fs::create_dir_all(&self.dir)?;
        let text = recipe.to_toml();
        if replace {
            let tmp = self.dir.join(format!(".{name}.toml.tmp"));
            fs::write(&tmp, &text)?;
            fs::rename(&tmp, &path)?;
        } else {
            // create_new fails if the file exists, with no window between
            // checking and writing.
            let mut f = match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(f) => f,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    return Err(PresetError::Exists(name));
                }
                Err(e) => return Err(e.into()),
            };
            f.write_all(text.as_bytes())?;
        }
        Ok(name)
    }

    pub fn delete(&self, name: &str) -> Result<()> {
        match fs::remove_file(self.path(name)?) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                Err(PresetError::NotFound(name.trim().into()))
            }
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glitchrx_core::Step;

    fn recipe(effects: &[&str]) -> Recipe {
        Recipe {
            steps: effects
                .iter()
                .enumerate()
                .map(|(i, e)| Step::new(e, "", i as u64).unwrap())
                .collect(),
            ..Recipe::default()
        }
    }

    #[test]
    fn names_are_checked_not_cleaned() {
        assert_eq!(
            check_name("  Moody bands (v2) ").unwrap(),
            "Moody bands (v2)"
        );
        assert!(check_name("Été #3").is_ok());
        for bad in [
            "",
            "   ",
            ".hidden",
            "a/b",
            "..",
            "a\\b",
            "x\ny",
            &"n".repeat(61),
        ] {
            assert!(check_name(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn save_list_load_replace_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let p = Presets::new(tmp.path().join("presets"));
        assert!(p.list().unwrap().is_empty(), "a missing directory is empty");

        p.put("melt", &recipe(&["pixelsort", "echo"]), false)
            .unwrap();
        p.put("Blocky", &recipe(&["bitcrush"]), false).unwrap();
        let names: Vec<_> = p.list().unwrap().into_iter().map(|i| i.name).collect();
        assert_eq!(names, ["Blocky", "melt"]);
        assert_eq!(p.list().unwrap()[1].effects, ["pixelsort", "echo"]);
        assert_eq!(p.get("melt").unwrap(), recipe(&["pixelsort", "echo"]));

        // Never replaced unless asked.
        assert!(matches!(
            p.put("melt", &recipe(&["invert"]), false),
            Err(PresetError::Exists(_))
        ));
        assert_eq!(p.get("melt").unwrap().steps.len(), 2);
        p.put("melt", &recipe(&["invert"]), true).unwrap();
        assert_eq!(p.get("melt").unwrap(), recipe(&["invert"]));

        p.delete("melt").unwrap();
        assert!(matches!(p.get("melt"), Err(PresetError::NotFound(_))));
        assert!(matches!(p.delete("melt"), Err(PresetError::NotFound(_))));
    }

    #[test]
    fn broken_files_are_listed_with_their_error() {
        let tmp = tempfile::tempdir().unwrap();
        let p = Presets::new(tmp.path().to_path_buf());
        fs::write(
            tmp.path().join("old.toml"),
            "format = \"glitcharaxia-recipe\"\nversion = 99\n",
        )
        .unwrap();
        fs::write(tmp.path().join(".partial.toml.tmp"), "junk").unwrap();
        let list = p.list().unwrap();
        assert_eq!(list.len(), 1);
        assert!(list[0].error.as_deref().unwrap().contains("version 99"));
    }
}
