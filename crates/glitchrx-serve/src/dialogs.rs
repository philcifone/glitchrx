//! Native file dialogs, for the desktop app.
//!
//! In a browser, photos come in as uploads and go out as downloads. The
//! desktop app (`glitchrx-desktop`) runs this same server inside a native
//! window, where the system's own Open and Save dialogs are the better fit.
//! It implements [`Dialogs`] and passes it in through
//! [`AppState::dialogs`](crate::AppState); the server then offers
//! `/api/desktop/open` and `/api/docs/{id}/save-as`. A plain
//! `glitchrx serve` has no dialogs and answers those routes with 404.
//!
//! The dialogs run on the server's blocking pool, never on an async worker:
//! they wait for the user.

use std::path::{Path, PathBuf};

/// The system's file dialogs. `None` means the user cancelled.
pub trait Dialogs: Send + Sync {
    /// Ask for a photo to open, starting in `dir` if given.
    fn pick_photo(&self, dir: Option<&Path>) -> Option<PathBuf>;

    /// Ask where to save, suggesting `name` in `dir`. The dialog asks the
    /// user before replacing an existing file, so a path returned here may
    /// be overwritten: that confirmation is the GUI's `--force`. The input
    /// photo is still never written (see `glitchrx_core::io::write_output`).
    fn save_as(&self, dir: Option<&Path>, name: &str) -> Option<PathBuf>;
}

/// File extensions offered in the Open dialog (the decoders' formats).
pub const PHOTO_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "tif", "tiff"];
