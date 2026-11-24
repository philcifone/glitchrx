//! AVIF export with rav1e (through `ravif`), pure Rust.
//!
//! Two things keep the output the same from machine to machine:
//!
//! - rav1e splits the picture into one tile per thread, and the tiles shape
//!   the file, so the thread count is fixed rather than the core count;
//! - assembly is off (it would need nasm to build), and the Rust paths are
//!   the same everywhere.
//!
//! This encoder cannot embed an ICC profile. Colour is described instead
//! with code points (an `nclx` colour box): sRGB, or Display P3 when the
//! photo's profile is Display P3 (as iPhone photos are). Other profiles are
//! dropped; [`keeps_profile`] tells callers so they can say so.

use crate::error::{Error, Result};
use crate::raster::Raster;
use rgb::FromSlice;

/// Threads, and so tiles, for every encode.
const THREADS: usize = 8;

/// Encode `r` as AVIF at `quality` (1..=100) and `speed` (1 slowest and
/// smallest to 10 fastest).
pub fn encode(r: &Raster, quality: u8, speed: u8, icc: Option<&[u8]>) -> Result<Vec<u8>> {
    let enc = ravif::Encoder::new()
        .with_quality(quality.clamp(1, 100) as f32)
        .with_speed(speed.clamp(1, 10))
        .with_bit_depth(ravif::BitDepth::Eight)
        .with_num_threads(Some(THREADS));
    let img = ravif::Img::new(r.bytes.as_rgb(), r.width, r.height);
    let out = enc
        .encode_rgb(img)
        .map_err(|e| Error::Malformed(format!("avif encoding: {e}")))?;
    if icc.is_some_and(is_display_p3) {
        return as_display_p3(&out.avif_file, out.color_byte_size, r);
    }
    Ok(out.avif_file)
}

/// Rewrap an encoded picture, marked as Display P3.
///
/// ravif always marks its files as sRGB, which leaves the colour box out
/// (sRGB is what readers assume). The AV1 data inside does not depend on the
/// primaries (they only say how to read the colours), so it is taken out of
/// the file's `mdat` box and wrapped again by the same serializer, with a
/// colour box naming P3 primaries and otherwise what ravif wrote: sRGB
/// transfer, BT.601 matrix, full range. Readers take the colour box over the
/// AV1 header's own description.
fn as_display_p3(file: &[u8], color_len: usize, r: &Raster) -> Result<Vec<u8>> {
    use avif_serialize::constants::{ColorPrimaries, MatrixCoefficients, TransferCharacteristics};
    let bad = || Error::Malformed("avif: unexpected layout from the encoder".into());
    let at = file
        .windows(4)
        .rposition(|w| w == b"mdat")
        .ok_or_else(bad)?;
    let size = u32::from_be_bytes(
        file.get(at - 4..at)
            .ok_or_else(bad)?
            .try_into()
            .map_err(|_| bad())?,
    ) as usize;
    let av1 = file.get(at + 4..at - 4 + size).ok_or_else(bad)?;
    // Without alpha or EXIF, the colour data is all the box holds.
    if av1.len() != color_len {
        return Err(bad());
    }
    let mut out = avif_serialize::Aviffy::new();
    out.set_color_primaries(ColorPrimaries::DisplayP3)
        .set_transfer_characteristics(TransferCharacteristics::Srgb)
        .set_matrix_coefficients(MatrixCoefficients::Bt601)
        .set_full_color_range(true);
    Ok(out.to_vec(av1, None, r.width as u32, r.height as u32, 8))
}

/// Whether an AVIF export keeps the colours this profile describes: no
/// profile, sRGB, or Display P3.
pub fn keeps_profile(icc: Option<&[u8]>) -> bool {
    match icc {
        None => true,
        Some(p) => {
            let name = profile_name(p).unwrap_or_default();
            name.contains("sRGB") || is_display_p3(p)
        }
    }
}

fn is_display_p3(icc: &[u8]) -> bool {
    profile_name(icc).is_some_and(|n| n.contains("Display P3"))
}

/// The profile's description (its `desc` tag), from either ICC version: a
/// v2 `desc` (ASCII) or a v4 `mluc` (UTF-16, first record).
pub fn profile_name(icc: &[u8]) -> Option<String> {
    let be32 = |at: usize| -> Option<usize> {
        icc.get(at..at + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    let count = be32(128)?;
    for i in 0..count.min(256) {
        let e = 132 + i * 12;
        if icc.get(e..e + 4)? != b"desc" {
            continue;
        }
        let (off, len) = (be32(e + 4)?, be32(e + 8)?);
        let tag = icc.get(off..off.checked_add(len)?)?;
        return match tag.get(0..4)? {
            b"desc" => {
                let n = u32::from_be_bytes(tag.get(8..12)?.try_into().ok()?) as usize;
                let text = tag.get(12..12 + n)?;
                Some(
                    String::from_utf8_lossy(text)
                        .trim_end_matches('\0')
                        .to_string(),
                )
            }
            b"mluc" => {
                let (n, rec) = (
                    u32::from_be_bytes(tag.get(8..12)?.try_into().ok()?),
                    tag.get(16..28)?,
                );
                if n == 0 {
                    return None;
                }
                let len = u32::from_be_bytes(rec[4..8].try_into().ok()?) as usize;
                let at = u32::from_be_bytes(rec[8..12].try_into().ok()?) as usize;
                let units: Vec<u16> = tag
                    .get(at..at + len)?
                    .chunks_exact(2)
                    .map(|c| u16::from_be_bytes([c[0], c[1]]))
                    .collect();
                Some(
                    String::from_utf16_lossy(&units)
                        .trim_end_matches('\0')
                        .to_string(),
                )
            }
            _ => None,
        };
    }
    None
}
