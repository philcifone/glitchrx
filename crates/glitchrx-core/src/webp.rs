//! WebP export through libwebp (built from the source bundled with
//! `libwebp-sys`, so every build encodes the same bytes).
//!
//! Lossy WebP is VP8 with a quality setting; lossless is VP8L. The colour
//! profile goes in an `ICCP` chunk, which libwebp's mux API adds around the
//! encoded image.

use std::ptr;

use libwebp_sys as webp;

use crate::error::{Error, Result};
use crate::raster::Raster;

/// The WebP format's limit on each side.
pub const MAX_SIDE: usize = 16383;

/// Encode `r` as WebP: lossless, or lossy at `quality` (1..=100).
pub fn encode(r: &Raster, quality: u8, lossless: bool, icc: Option<&[u8]>) -> Result<Vec<u8>> {
    if r.width > MAX_SIDE || r.height > MAX_SIDE {
        return Err(Error::Unsupported(format!(
            "WebP is limited to {MAX_SIDE} pixels a side; this picture is {}x{}",
            r.width, r.height
        )));
    }
    let (w, h, stride) = (r.width as i32, r.height as i32, (r.width * 3) as i32);
    let mut out: *mut u8 = ptr::null_mut();
    // SAFETY: the buffer holds `h` rows of `stride` bytes; libwebp allocates
    // `out`, which is copied and then freed with its own allocator.
    let bitstream = unsafe {
        let n = if lossless {
            webp::WebPEncodeLosslessRGB(r.bytes.as_ptr(), w, h, stride, &mut out)
        } else {
            webp::WebPEncodeRGB(
                r.bytes.as_ptr(),
                w,
                h,
                stride,
                quality.clamp(1, 100) as f32,
                &mut out,
            )
        };
        if n == 0 || out.is_null() {
            return Err(Error::Malformed("webp encoding failed".into()));
        }
        let v = std::slice::from_raw_parts(out, n).to_vec();
        webp::WebPFree(out.cast());
        v
    };
    match icc {
        Some(icc) if !icc.is_empty() => with_profile(&bitstream, icc),
        _ => Ok(bitstream),
    }
}

/// Wrap an encoded image with an ICC profile chunk.
fn with_profile(bitstream: &[u8], icc: &[u8]) -> Result<Vec<u8>> {
    let fail = || Error::Malformed("webp: could not add the colour profile".into());
    // SAFETY: every libwebp object made here is released before returning,
    // and the data handed in outlives the calls (copy_data = 1 copies it).
    unsafe {
        let mux = webp::WebPMuxNew();
        if mux.is_null() {
            return Err(fail());
        }
        let image = webp::WebPData {
            bytes: bitstream.as_ptr(),
            size: bitstream.len(),
        };
        let profile = webp::WebPData {
            bytes: icc.as_ptr(),
            size: icc.len(),
        };
        let mut out = webp::WebPData {
            bytes: ptr::null(),
            size: 0,
        };
        let ok = webp::WebPMuxSetImage(mux, &image, 1) == webp::WebPMuxError::WEBP_MUX_OK
            && webp::WebPMuxSetChunk(mux, c"ICCP".as_ptr(), &profile, 1)
                == webp::WebPMuxError::WEBP_MUX_OK
            && webp::WebPMuxAssemble(mux, &mut out) == webp::WebPMuxError::WEBP_MUX_OK;
        webp::WebPMuxDelete(mux);
        let result = (ok && !out.bytes.is_null())
            .then(|| std::slice::from_raw_parts(out.bytes, out.size).to_vec());
        webp::WebPDataClear(&mut out);
        result.ok_or_else(fail)
    }
}
