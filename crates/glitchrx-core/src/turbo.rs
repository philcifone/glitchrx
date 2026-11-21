//! Tolerant JPEG decoding (and fixture encoding) through libjpeg-turbo.
//!
//! libjpeg-turbo is what Firefox, Chromium and most image viewers use, so a
//! glitched file decoded here looks the way it will look to everyone else.
//! See `docs/decisions/0001-decoder.md` for why it was chosen.
//!
//! Glitched files are broken on purpose. The TurboJPEG API reports problems
//! two ways: *warnings* (corrupt data that the decoder worked around; the
//! image is complete) and *fatal errors* (decoding stopped; rows decoded so far
//! are still in the buffer). Both are normal outcomes here, not failures.

use std::ffi::CStr;
use std::os::raw::c_int;

use turbojpeg_sys as tj;

/// Output size relative to the source. Scaling happens inside the inverse
/// DCT, so a scaled decode is much faster and still shows the real glitch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scale {
    Full,
    Half,
    Quarter,
    Eighth,
}

impl Scale {
    fn factor(self) -> tj::tjscalingfactor {
        let denom = match self {
            Scale::Full => 1,
            Scale::Half => 2,
            Scale::Quarter => 4,
            Scale::Eighth => 8,
        };
        tj::tjscalingfactor { num: 1, denom }
    }

    /// Smallest (fastest) scale whose output is still at least as large as
    /// the image shown fitted into a `max_w` x `max_h` box.
    pub fn to_fit(width: u32, height: u32, max_w: u32, max_h: u32) -> Scale {
        for s in [Scale::Eighth, Scale::Quarter, Scale::Half] {
            let d = s.factor().denom as u32;
            if width.div_ceil(d) >= max_w || height.div_ceil(d) >= max_h {
                return s;
            }
        }
        Scale::Full
    }
}

/// How well a decode went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeStatus {
    /// No complaints.
    Clean,
    /// The decoder worked around corrupt data; the image is complete.
    Recovered(String),
    /// Decoding stopped part way; `rows` rows from the top are valid.
    Partial { rows: u32, message: String },
    /// Nothing usable.
    Failed(String),
}

impl DecodeStatus {
    /// One-word summary for status lines.
    pub fn word(&self) -> &'static str {
        match self {
            DecodeStatus::Clean => "clean",
            DecodeStatus::Recovered(_) => "recovered",
            DecodeStatus::Partial { .. } => "partial",
            DecodeStatus::Failed(_) => "failed",
        }
    }

    /// The decoder's message, if any.
    pub fn message(&self) -> Option<&str> {
        match self {
            DecodeStatus::Clean => None,
            DecodeStatus::Recovered(m) | DecodeStatus::Failed(m) => Some(m),
            DecodeStatus::Partial { message, .. } => Some(message),
        }
    }
}

/// Decoded pixels, 8-bit RGBA, rows top to bottom.
#[derive(Debug, Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Owns a TurboJPEG handle and frees it on drop.
struct Handle(tj::tjhandle);

impl Handle {
    fn new(kind: tj::TJINIT) -> Option<Handle> {
        // SAFETY: tj3Init has no preconditions; it returns null on failure.
        let h = unsafe { tj::tj3Init(kind as c_int) };
        (!h.is_null()).then_some(Handle(h))
    }

    fn set(&self, param: tj::TJPARAM, value: c_int) {
        // SAFETY: valid handle; unknown params only return an error code.
        unsafe { tj::tj3Set(self.0, param as c_int, value) };
    }

    fn get(&self, param: tj::TJPARAM) -> c_int {
        // SAFETY: valid handle.
        unsafe { tj::tj3Get(self.0, param as c_int) }
    }

    fn error(&self) -> (bool, String) {
        // SAFETY: valid handle; the returned string is owned by the handle
        // and valid until the next call on it, so copy it out immediately.
        unsafe {
            let fatal = tj::tj3GetErrorCode(self.0) == tj::TJERR_TJERR_FATAL as c_int;
            let msg = CStr::from_ptr(tj::tj3GetErrorStr(self.0))
                .to_string_lossy()
                .into_owned();
            (fatal, msg)
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle came from tj3Init and is destroyed exactly once.
        unsafe { tj::tj3Destroy(self.0) };
    }
}

/// Decode a (possibly glitched) JPEG. Never panics; an undecodable file
/// yields `(None, Failed)`.
pub fn decode(jpeg: &[u8], scale: Scale) -> (Option<Image>, DecodeStatus) {
    let Some(h) = Handle::new(tj::TJINIT_TJINIT_DECOMPRESS) else {
        return (
            None,
            DecodeStatus::Failed("could not initialise libjpeg-turbo".into()),
        );
    };
    // Keep going past corrupt data instead of stopping at the first warning.
    h.set(tj::TJPARAM_TJPARAM_STOPONWARNING, 0);

    // SAFETY: valid handle, pointer and length describe the caller's slice.
    let rc = unsafe { tj::tj3DecompressHeader(h.0, jpeg.as_ptr(), jpeg.len() as tj::size_t) };
    if rc != 0 {
        let (fatal, msg) = h.error();
        if fatal {
            return (None, DecodeStatus::Failed(msg));
        }
    }
    let (w, ht) = (
        h.get(tj::TJPARAM_TJPARAM_JPEGWIDTH),
        h.get(tj::TJPARAM_TJPARAM_JPEGHEIGHT),
    );
    if w <= 0 || ht <= 0 {
        return (None, DecodeStatus::Failed("image has no dimensions".into()));
    }
    let f = scale.factor();
    // SAFETY: valid handle; 1/2, 1/4 and 1/8 are always supported factors.
    unsafe { tj::tj3SetScalingFactor(h.0, f) };
    // TJSCALED(): scaled dimensions round up.
    let sw = (w as u32).div_ceil(f.denom as u32);
    let sh = (ht as u32).div_ceil(f.denom as u32);

    // Alpha starts at 0 and libjpeg-turbo writes 255 into every pixel it
    // produces, which lets us count the rows a fatal error left behind.
    let mut rgba = vec![0u8; sw as usize * sh as usize * 4];
    // SAFETY: the buffer is exactly width * height * 4 bytes for the scaled
    // size, which is what tj3Decompress8 writes with pitch 0 and TJPF_RGBA.
    let rc = unsafe {
        tj::tj3Decompress8(
            h.0,
            jpeg.as_ptr(),
            jpeg.len() as tj::size_t,
            rgba.as_mut_ptr(),
            0,
            tj::TJPF_TJPF_RGBA,
        )
    };
    let image = |rgba| Image {
        width: sw,
        height: sh,
        rgba,
    };
    if rc == 0 {
        return (Some(image(rgba)), DecodeStatus::Clean);
    }
    let (fatal, message) = h.error();
    if !fatal {
        return (Some(image(rgba)), DecodeStatus::Recovered(message));
    }
    let row_bytes = sw as usize * 4;
    let rows = rgba
        .chunks_exact(row_bytes)
        .take_while(|row| row[row_bytes - 1] == 255)
        .count() as u32;
    if rows == 0 {
        (None, DecodeStatus::Failed(message))
    } else {
        // Undecoded rows: make them opaque mid-gray rather than transparent.
        for px in rgba[rows as usize * row_bytes..].chunks_exact_mut(4) {
            px.copy_from_slice(&[128, 128, 128, 255]);
        }
        (Some(image(rgba)), DecodeStatus::Partial { rows, message })
    }
}

/// Chroma subsampling for [`encode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subsampling {
    S444,
    S422,
    S420,
    Gray,
}

/// Encoder settings (used to generate test fixtures; later, baked exports).
#[derive(Debug, Clone, Copy)]
pub struct EncodeOptions {
    pub quality: u8,
    pub subsampling: Subsampling,
    pub progressive: bool,
    /// Insert a restart marker every N MCU rows (0 = none).
    pub restart_rows: u16,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        EncodeOptions {
            quality: 90,
            subsampling: Subsampling::S420,
            progressive: false,
            restart_rows: 0,
        }
    }
}

/// Encode RGBA pixels as a JPEG. Returns `None` if libjpeg-turbo fails.
pub fn encode(img: &Image, opts: EncodeOptions) -> Option<Vec<u8>> {
    let h = Handle::new(tj::TJINIT_TJINIT_COMPRESS)?;
    h.set(tj::TJPARAM_TJPARAM_QUALITY, opts.quality as c_int);
    let samp = match opts.subsampling {
        Subsampling::S444 => tj::TJSAMP_TJSAMP_444,
        Subsampling::S422 => tj::TJSAMP_TJSAMP_422,
        Subsampling::S420 => tj::TJSAMP_TJSAMP_420,
        Subsampling::Gray => tj::TJSAMP_TJSAMP_GRAY,
    };
    h.set(tj::TJPARAM_TJPARAM_SUBSAMP, samp);
    if opts.subsampling == Subsampling::Gray {
        h.set(tj::TJPARAM_TJPARAM_COLORSPACE, tj::TJCS_TJCS_GRAY as c_int);
    }
    h.set(tj::TJPARAM_TJPARAM_PROGRESSIVE, opts.progressive as c_int);
    h.set(tj::TJPARAM_TJPARAM_RESTARTROWS, opts.restart_rows as c_int);

    let mut out: *mut u8 = std::ptr::null_mut();
    let mut size: tj::size_t = 0;
    // SAFETY: the source buffer holds width * height RGBA pixels; TurboJPEG
    // allocates `out`, which we copy and then free with tj3Free.
    unsafe {
        let rc = tj::tj3Compress8(
            h.0,
            img.rgba.as_ptr(),
            img.width as c_int,
            0,
            img.height as c_int,
            tj::TJPF_TJPF_RGBA,
            &mut out,
            &mut size,
        );
        if rc != 0 || out.is_null() {
            if !out.is_null() {
                tj::tj3Free(out.cast());
            }
            return None;
        }
        let v = std::slice::from_raw_parts(out, size as usize).to_vec();
        tj::tj3Free(out.cast());
        Some(v)
    }
}

/// Encode pixels as PNG (RGB, alpha dropped since decodes are opaque).
pub fn to_png(img: &Image) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, img.width, img.height);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().expect("writing to a Vec cannot fail");
        let rgb: Vec<u8> = img
            .rgba
            .chunks_exact(4)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();
        w.write_image_data(&rgb)
            .expect("writing to a Vec cannot fail");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32) -> Image {
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                rgba.extend_from_slice(&[(x * 255 / w) as u8, (y * 255 / h) as u8, 90, 255]);
            }
        }
        Image {
            width: w,
            height: h,
            rgba,
        }
    }

    #[test]
    fn encode_decode_clean() {
        let jpg = encode(&gradient(64, 48), EncodeOptions::default()).unwrap();
        let (img, status) = decode(&jpg, Scale::Full);
        assert_eq!(status, DecodeStatus::Clean);
        let img = img.unwrap();
        assert_eq!((img.width, img.height), (64, 48));
        let (small, _) = decode(&jpg, Scale::Quarter);
        assert_eq!(small.unwrap().width, 16);
    }

    #[test]
    fn garbage_fails_without_panicking() {
        let (img, status) = decode(b"definitely not a jpeg", Scale::Full);
        assert!(img.is_none());
        assert_eq!(status.word(), "failed");
    }

    #[test]
    fn truncated_file_is_recovered_or_partial() {
        let jpg = encode(&gradient(256, 256), EncodeOptions::default()).unwrap();
        let cut = &jpg[..jpg.len() / 2];
        let (img, status) = decode(cut, Scale::Full);
        assert!(img.is_some(), "{status:?}");
        assert_ne!(status, DecodeStatus::Clean);
    }

    #[test]
    fn scale_to_fit() {
        assert_eq!(Scale::to_fit(6000, 4000, 700, 700), Scale::Eighth);
        assert_eq!(Scale::to_fit(6000, 4000, 1200, 800), Scale::Quarter);
        assert_eq!(Scale::to_fit(640, 480, 1000, 1000), Scale::Full);
    }

    #[test]
    fn png_has_signature() {
        assert!(to_png(&gradient(4, 4)).starts_with(b"\x89PNG"));
    }
}
