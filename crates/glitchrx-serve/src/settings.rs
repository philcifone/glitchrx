//! Server settings the GUI can change (SPEC 4c): the library folder (what
//! Open browses) and the export folder (where Save As starts).
//!
//! They live in `~/.config/glitchrx/config.toml`:
//!
//! ```toml
//! library = "/home/phil/Pictures"
//! export = "/home/phil/Pictures/glitched"
//! ```
//!
//! Folders are picked in the GUI with a folder browser limited to the home
//! directory, and only folders inside it are accepted. The browser sees
//! them as paths relative to home (`Pictures/glitched`), never absolute
//! ones. `--root` overrides the library for that run.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::browse::{self, BrowseError};

/// The values in the file. Unknown keys are kept out of the way (ignored),
/// so a newer build's file still loads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export: Option<PathBuf>,
}

/// The settings file, and the home directory folders must lie in.
pub struct SettingsStore {
    file: PathBuf,
    /// Canonical home directory.
    home: PathBuf,
    values: Mutex<Settings>,
}

/// `~/.config/glitchrx/config.toml` (or under `$XDG_CONFIG_HOME`).
pub fn default_file() -> Option<PathBuf> {
    crate::presets::default_dir().and_then(|d| d.parent().map(|p| p.join("config.toml")))
}

/// The user's home directory, from `$HOME`.
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

impl SettingsStore {
    /// Load `file` (missing is fine: empty settings). Folders that no
    /// longer exist, or lie outside `home`, are dropped with a warning.
    pub fn open(file: PathBuf, home: &Path) -> io::Result<SettingsStore> {
        let home = home.canonicalize()?;
        let mut values = match fs::read_to_string(&file) {
            Ok(text) => toml::from_str::<Settings>(&text).unwrap_or_else(|e| {
                eprintln!("glitchrx: {}: {e}; using defaults", file.display());
                Settings::default()
            }),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Settings::default(),
            Err(e) => return Err(io::Error::new(e.kind(), format!("{}: {e}", file.display()))),
        };
        for (key, slot) in [
            ("library", &mut values.library),
            ("export", &mut values.export),
        ] {
            if let Some(p) = slot.as_ref() {
                match p.canonicalize() {
                    Ok(c) if c.is_dir() && c.starts_with(&home) => *slot = Some(c),
                    _ => {
                        eprintln!(
                            "glitchrx: {}: {key} folder {} is gone or outside your home folder; ignoring it",
                            file.display(),
                            p.display()
                        );
                        *slot = None;
                    }
                }
            }
        }
        Ok(SettingsStore {
            file,
            home,
            values: Mutex::new(values),
        })
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn get(&self) -> Settings {
        self.values
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// A folder from the browser (relative to home) as a canonical path
    /// inside home. Missing, outside or not a folder: `NotFound`.
    pub fn folder(&self, rel: &str) -> Result<PathBuf, BrowseError> {
        let p = browse::resolve(&self.home, rel)?;
        if p.is_dir() {
            Ok(p)
        } else {
            Err(BrowseError::NotFound)
        }
    }

    /// A canonical path inside home as the browser sees it.
    pub fn relative(&self, path: &Path) -> String {
        browse::relative(&self.home, path)
    }

    /// Replace the settings and write the file (atomically: a temporary
    /// file renamed over the old one).
    pub fn set(&self, values: Settings) -> io::Result<()> {
        let text = toml::to_string(&values).map_err(io::Error::other)?;
        let dir = self.file.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(dir)?;
        let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
        tmp.write_all(b"# glitchrx settings, written by the GUI's Settings sheet.\n")?;
        tmp.write_all(text.as_bytes())?;
        tmp.persist(&self.file).map_err(|e| e.error)?;
        *self.values.lock().unwrap_or_else(|e| e.into_inner()) = values;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_saves_and_drops_what_is_gone_or_outside() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(home.join("Pictures/out")).unwrap();
        fs::create_dir_all(tmp.path().join("elsewhere")).unwrap();
        let file = home.join(".config/glitchrx/config.toml");

        // No file yet: empty settings.
        let s = SettingsStore::open(file.clone(), &home).unwrap();
        assert_eq!(s.get(), Settings::default());

        let pics = s.folder("Pictures").unwrap();
        s.set(Settings {
            library: Some(pics.clone()),
            export: Some(s.folder("Pictures/out").unwrap()),
        })
        .unwrap();
        assert_eq!(s.relative(&pics), "Pictures");
        let again = SettingsStore::open(file.clone(), &home).unwrap();
        assert_eq!(again.get().library, Some(pics));

        // Outside home, or not there any more: dropped on load.
        fs::write(
            &file,
            format!(
                "library = {:?}\nexport = {:?}\nfuture = 1\n",
                tmp.path().join("elsewhere"),
                home.join("gone")
            ),
        )
        .unwrap();
        assert_eq!(
            SettingsStore::open(file, &home).unwrap().get(),
            Settings::default()
        );

        // The folder picker never leaves home.
        assert!(s.folder("..").is_err());
        assert!(s.folder("../elsewhere").is_err());
        assert!(s.folder("missing").is_err());
        assert!(s.folder("").is_ok());
    }
}
