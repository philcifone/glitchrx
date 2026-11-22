//! Turning rasters into something a browser can show or save.

use glitchrx_core::Raster;
use glitchrx_core::codec;
use glitchrx_core::turbo::{self, EncodeOptions, PixelFormat, Subsampling};

/// Longest side of any preview, whatever the client asks for. Keeps phone
/// memory in check (iOS caps image and canvas memory at roughly 16 MP).
pub const MAX_PREVIEW_SIDE: u32 = 4096;

/// Encode a preview raster as a JPEG for transfer. Full chroma: glitch
/// colour fringes are the point, keep them.
pub fn preview_jpeg(r: &Raster, quality: u8) -> Result<Vec<u8>, String> {
    turbo::encode(
        r.width as u32,
        r.height as u32,
        &r.bytes,
        PixelFormat::Rgb,
        EncodeOptions {
            quality,
            subsampling: Subsampling::S444,
            progressive: false,
            restart_rows: 0,
        },
        None,
    )
    .ok_or_else(|| "could not encode the preview".to_string())
}

/// Export formats offered by the GUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    /// Uncompressed TIFF, for Photoshop (the default).
    Tiff,
    Png,
    /// `jpeg-baked` is the 2a name, still accepted from old clients.
    #[serde(alias = "jpeg-baked")]
    Jpeg,
}

impl From<Format> for codec::Format {
    fn from(f: Format) -> codec::Format {
        match f {
            Format::Tiff => codec::Format::Tiff,
            Format::Png => codec::Format::Png,
            Format::Jpeg => codec::Format::Jpeg,
        }
    }
}
