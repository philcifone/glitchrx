//! Error type shared by the whole core.

use thiserror::Error;

/// Everything that can go wrong inside the core.
///
/// Front ends map these onto exit codes (CLI) or messages (GUI), so the
/// variants are grouped by how a user should react to them.
#[derive(Debug, Error)]
pub enum Error {
    /// The input does not start with a JPEG SOI marker.
    #[error("not a JPEG file (no SOI marker at the start)")]
    NotJpeg,

    /// A valid JPEG that uses a coding mode we do not handle.
    #[error("unsupported JPEG: {0}")]
    Unsupported(String),

    /// The file claims to be a JPEG but its structure is broken before any
    /// image data was found.
    #[error("malformed JPEG: {0}")]
    Malformed(String),

    /// An effect name that is not in the registry.
    #[error("unknown effect `{0}` (see `glitchrx --list`)")]
    UnknownEffect(String),

    /// A bad, missing or out-of-range effect parameter.
    #[error("{effect}: {msg}")]
    Param { effect: String, msg: String },

    /// `invert` was requested for an effect (or configuration) that
    /// destroys information.
    #[error("{0} is not reversible")]
    NotReversible(String),

    /// A recipe that cannot be read or does not match this build.
    #[error("recipe: {0}")]
    Recipe(String),
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
