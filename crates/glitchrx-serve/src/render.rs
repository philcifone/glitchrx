//! Turning glitched bytes into something a browser can show or save.

use glitchrx_core::formats::jpeg::Frame;
use glitchrx_core::turbo::{self, DecodeStatus, EncodeOptions, Scale, Subsampling};

/// Longest side of any preview, whatever the client asks for. Keeps phone
/// memory in check (iOS caps image and canvas memory at roughly 16 MP).
pub const MAX_PREVIEW_SIDE: u32 = 4096;

/// A decoded, re-encoded preview.
pub struct Preview {
    /// A clean JPEG of the glitched render, sized for the screen.
    pub jpeg: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub status: DecodeStatus,
}

/// Decode `bytes` at the smallest libjpeg-turbo scale that still covers a
/// `max_w` x `max_h` box, and re-encode it for transfer.
///
/// The preview is a *clean* JPEG of the decoded pixels: the browser never
/// sees the broken bitstream, so it shows exactly what libjpeg-turbo decoded
/// here, whatever engine the browser uses.
pub fn preview(bytes: &[u8], frame: &Frame, max_w: u32, max_h: u32) -> Result<Preview, String> {
    let max_w = max_w.clamp(16, MAX_PREVIEW_SIDE);
    let max_h = max_h.clamp(16, MAX_PREVIEW_SIDE);
    let scale = Scale::to_fit(frame.width as u32, frame.height as u32, max_w, max_h);
    let (img, status) = turbo::decode(bytes, scale);
    let img = img.ok_or_else(|| status.message().unwrap_or("decode failed").to_string())?;
    let jpeg = turbo::encode(
        &img,
        EncodeOptions {
            quality: 88,
            // Full chroma: glitch colour fringes are the point, keep them.
            subsampling: Subsampling::S444,
            progressive: false,
            restart_rows: 0,
        },
    )
    .ok_or("could not encode the preview")?;
    Ok(Preview {
        jpeg,
        width: img.width,
        height: img.height,
        status,
    })
}

/// Export formats offered by the GUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    /// The glitched file itself: broken bitstream, looks different per viewer.
    Jpeg,
    /// Decoded with libjpeg-turbo and saved losslessly.
    Png,
    /// Decoded, then re-encoded as a clean, high-quality JPEG.
    JpegBaked,
}

impl Format {
    pub fn extension(self) -> Option<&'static str> {
        match self {
            Format::Jpeg => None, // keep the input's
            Format::Png => Some("png"),
            Format::JpegBaked => Some("jpg"),
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Format::Png => "image/png",
            Format::Jpeg | Format::JpegBaked => "image/jpeg",
        }
    }
}

/// Produce the export bytes for `glitched` in `format`.
pub fn export(glitched: Vec<u8>, format: Format) -> Result<Vec<u8>, String> {
    if format == Format::Jpeg {
        return Ok(glitched);
    }
    let (img, status) = turbo::decode(&glitched, Scale::Full);
    let img = img.ok_or_else(|| {
        format!(
            "cannot bake: {}",
            status.message().unwrap_or("decode failed")
        )
    })?;
    match format {
        Format::Png => Ok(turbo::to_png(&img)),
        _ => turbo::encode(
            &img,
            EncodeOptions {
                quality: 95,
                subsampling: Subsampling::S444,
                progressive: false,
                restart_rows: 0,
            },
        )
        .ok_or_else(|| "could not encode the baked JPEG".into()),
    }
}
