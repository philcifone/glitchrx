//! glitchrx-core: the engine behind the `glitchrx` CLI and web GUI.
//!
//! The pipeline in one paragraph: an image file is decoded to a [`Raster`]
//! of raw RGB bytes ([`codec`]). A [`Recipe`] is an ordered list of
//! [`effects`], each run in seeded windows ([`placement`]) that read the
//! bytes as audio samples in some encoding ([`samples`]), process them the
//! way Audacity would, and clip them back to bytes. The raster is then
//! encoded as TIFF, PNG or JPEG. This is "databending": what Phil used to do
//! by importing TIFFs into Audacity as raw data.

pub mod codec;
pub mod dsp;
pub mod effects;
pub mod error;
pub mod mask;
pub mod params;
pub mod placement;
pub mod raster;
pub mod recipe;
pub mod rng;
pub mod samples;
pub mod turbo;

#[cfg(feature = "fs")]
pub mod io;

pub use codec::{Format, Image};
pub use error::{Error, Result};
pub use params::Units;
pub use raster::Raster;
pub use recipe::{Recipe, Step};
pub use samples::Encoding;
