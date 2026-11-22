//! Error type shared by the whole core.

use thiserror::Error;

/// Everything that can go wrong inside the core.
///
/// Front ends map these onto exit codes (CLI) or messages (GUI), so the
/// variants are grouped by how a user should react to them.
#[derive(Debug, Error)]
pub enum Error {
    /// The input is not a JPEG, PNG or TIFF file.
    #[error("not a JPEG, PNG or TIFF file")]
    UnknownFormat,

    /// A valid file that uses a feature we do not handle.
    #[error("unsupported image: {0}")]
    Unsupported(String),

    /// A file that cannot be decoded (or encoded).
    #[error("cannot read image: {0}")]
    Malformed(String),

    /// An effect name that is not in the registry.
    #[error("unknown effect `{0}` (see `glitchrx --list`)")]
    UnknownEffect(String),

    /// A bad, missing or out-of-range effect parameter.
    #[error("{effect}: {msg}")]
    Param { effect: String, msg: String },

    /// A recipe that cannot be read or does not match this build.
    #[error("recipe: {0}")]
    Recipe(String),

    /// A newer request superseded this render (see [`crate::recipe::Cancel`]).
    #[error("cancelled")]
    Cancelled,
}

/// Convenience alias used throughout the core.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Shorthand for building a parameter error.
    pub(crate) fn param(effect: &str, msg: impl Into<String>) -> Self {
        Error::Param {
            effect: effect.to_string(),
            msg: msg.into(),
        }
    }
}
