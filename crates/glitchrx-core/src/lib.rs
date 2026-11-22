//! glitchrx-core: the engine behind the `glitchrx` CLI and (later) GUI.
//!
//! The pipeline in one paragraph: a JPEG is parsed into structure plus a
//! *payload* (its de-stuffed scan data, see [`formats::jpeg`]). A [`Recipe`]
//! is an ordered list of [`effects`], each applied in seeded *hits* (see
//! [`placement`]) to that payload. The file is then reassembled with every
//! header byte untouched. With the `turbo` feature, [`turbo::decode`] renders
//! the result the way browsers will, tolerating the damage.

pub mod effects;
pub mod error;
pub mod formats;
pub mod params;
pub mod placement;
pub mod recipe;
pub mod restart;
pub mod rng;

#[cfg(feature = "fs")]
pub mod io;
#[cfg(feature = "turbo")]
pub mod turbo;

pub use error::{Error, Result};
pub use formats::jpeg::{Jpeg, Mode};
pub use recipe::{Recipe, Step};
pub use restart::Restart;
