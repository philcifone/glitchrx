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

/// Encode a mask as an RGBA PNG: `tint` everywhere, with the coverage as
/// alpha, so the GUI can lay it over the preview. Fast compression: a mask
/// is mostly flat and squeezes well anyway.
pub fn mask_png(w: usize, h: usize, coverage: &[f32], tint: [u8; 3]) -> Result<Vec<u8>, String> {
    let mut rgba = Vec::with_capacity(w * h * 4);
    for &c in coverage {
        let a = (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        rgba.extend_from_slice(&[tint[0], tint[1], tint[2], a]);
    }
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Fast);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&rgba).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
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
