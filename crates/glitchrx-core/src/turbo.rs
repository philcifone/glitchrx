//! JPEG decoding and encoding through libjpeg-turbo (TurboJPEG 3 API).
//!
//! libjpeg-turbo is what browsers and most viewers use. Since SPEC v0.3 the
//! images it decodes are clean inputs (effects run on the decoded pixels),
//! so a fatal decode error is simply an error. Warnings about slightly
//! damaged files are tolerated, as viewers do.
//!
//! Decoding uses the accurate integer IDCT and default upsampling, never the
//! "fast" variants, so the decoded raster (and everything bent from it) is
//! the same on every machine (SPEC 3.3).

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

/// A decoded JPEG: RGB pixels plus its embedded color profile, if any.
#[derive(Debug, Clone)]
pub struct Decoded {
    pub width: u32,
    pub height: u32,
    /// 8-bit RGB, rows top to bottom (not yet oriented).
    pub rgb: Vec<u8>,
    pub icc: Option<Vec<u8>>,
}

/// Decode a JPEG to RGB, optionally scaled down inside the IDCT (much
/// faster than decoding at full size and shrinking afterwards).
pub fn decode(jpeg: &[u8], scale: Scale) -> Result<Decoded, String> {
    let h =
        Handle::new(tj::TJINIT_TJINIT_DECOMPRESS).ok_or("could not initialise libjpeg-turbo")?;
    // Keep going past minor corruption, as viewers do.
    h.set(tj::TJPARAM_TJPARAM_STOPONWARNING, 0);
    h.set(tj::TJPARAM_TJPARAM_FASTDCT, 0);
    h.set(tj::TJPARAM_TJPARAM_FASTUPSAMPLE, 0);
    // Keep APPn markers so the ICC profile can be read back.
    h.set(tj::TJPARAM_TJPARAM_SAVEMARKERS, 2);

    // SAFETY: valid handle, pointer and length describe the caller's slice.
    let rc = unsafe { tj::tj3DecompressHeader(h.0, jpeg.as_ptr(), jpeg.len() as tj::size_t) };
    if rc != 0 && h.error().0 {
        return Err(h.error().1);
    }
    let (w, ht) = (
        h.get(tj::TJPARAM_TJPARAM_JPEGWIDTH),
        h.get(tj::TJPARAM_TJPARAM_JPEGHEIGHT),
    );
    if w <= 0 || ht <= 0 {
        return Err("image has no dimensions".into());
    }
    let icc = icc_profile(&h);
    let cs = h.get(tj::TJPARAM_TJPARAM_COLORSPACE);
    let cmyk = cs == tj::TJCS_TJCS_CMYK as c_int || cs == tj::TJCS_TJCS_YCCK as c_int;

    let f = scale.factor();
    // SAFETY: valid handle; 1/2, 1/4 and 1/8 are always supported factors.
    unsafe { tj::tj3SetScalingFactor(h.0, f) };
    // TJSCALED(): scaled dimensions round up.
    let sw = (w as u32).div_ceil(f.denom as u32);
    let sh = (ht as u32).div_ceil(f.denom as u32);
    let (format, channels) = if cmyk {
        (tj::TJPF_TJPF_CMYK, 4)
    } else {
        (tj::TJPF_TJPF_RGB, 3)
    };
    let mut px = vec![0u8; sw as usize * sh as usize * channels];
    // SAFETY: the buffer is exactly width * height * channels bytes for the
    // scaled size, which is what tj3Decompress8 writes with pitch 0.
    let rc = unsafe {
        tj::tj3Decompress8(
            h.0,
            jpeg.as_ptr(),
            jpeg.len() as tj::size_t,
            px.as_mut_ptr(),
            0,
            format,
        )
    };
    if rc != 0 && h.error().0 {
        return Err(h.error().1);
    }
    let rgb = if cmyk { cmyk_to_rgb(&px) } else { px };
    Ok(Decoded {
        width: sw,
        height: sh,
        rgb,
        icc,
    })
}

/// Naive CMYK to RGB. libjpeg delivers CMYK as stored, and nearly every
/// CMYK JPEG comes from Adobe software, which stores the values inverted;
/// for those `r = c * k / 255`. (Color-managed conversion would need the
/// file's CMYK profile; this is for getting usable pixels to bend.)
fn cmyk_to_rgb(px: &[u8]) -> Vec<u8> {
    px.chunks_exact(4)
        .flat_map(|p| {
            let k = p[3] as u32;
            [0, 1, 2].map(|i| ((p[i] as u32 * k + 127) / 255) as u8)
        })
        .collect()
}

/// The embedded ICC profile, if the file has one.
fn icc_profile(h: &Handle) -> Option<Vec<u8>> {
    let mut buf: *mut u8 = std::ptr::null_mut();
    let mut size: tj::size_t = 0;
    // SAFETY: valid handle after reading the header; TurboJPEG allocates
    // the buffer, which we copy and free with tj3Free.
    unsafe {
        let rc = tj::tj3GetICCProfile(h.0, &mut buf, &mut size);
        if rc != 0 || buf.is_null() || size == 0 {
            if !buf.is_null() {
                tj::tj3Free(buf.cast());
            }
            return None;
        }
        let v = std::slice::from_raw_parts(buf, size as usize).to_vec();
        tj::tj3Free(buf.cast());
        Some(v)
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

/// Pixel layouts [`encode`] accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Rgb,
    /// Used to generate CMYK test fixtures.
    Cmyk,
}

/// Encode pixels as a JPEG, embedding `icc` if given. Returns `None` if
/// libjpeg-turbo fails.
pub fn encode(
    width: u32,
    height: u32,
    pixels: &[u8],
    format: PixelFormat,
    opts: EncodeOptions,
    icc: Option<&[u8]>,
) -> Option<Vec<u8>> {
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
    let pf = match format {
        PixelFormat::Rgb => tj::TJPF_TJPF_RGB,
        PixelFormat::Cmyk => tj::TJPF_TJPF_CMYK,
    };
    if let Some(icc) = icc {
        // SAFETY: TurboJPEG copies the profile into the handle.
        let rc =
            unsafe { tj::tj3SetICCProfile(h.0, icc.as_ptr() as *mut u8, icc.len() as tj::size_t) };
        if rc != 0 {
            return None;
        }
    }

    let mut out: *mut u8 = std::ptr::null_mut();
    let mut size: tj::size_t = 0;
    // SAFETY: the source buffer holds width * height pixels in `pf`;
    // TurboJPEG allocates `out`, which we copy and then free with tj3Free.
    unsafe {
        let rc = tj::tj3Compress8(
            h.0,
            pixels.as_ptr(),
            width as c_int,
            0,
            height as c_int,
            pf,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32) -> Vec<u8> {
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                rgb.extend_from_slice(&[(x * 255 / w) as u8, (y * 255 / h) as u8, 90]);
            }
        }
        rgb
    }

    #[test]
    fn encode_decode_with_icc() {
        let icc = b"fake profile bytes".repeat(10);
        let jpg = encode(
            64,
            48,
            &gradient(64, 48),
            PixelFormat::Rgb,
            EncodeOptions::default(),
            Some(&icc),
        )
        .unwrap();
        let d = decode(&jpg, Scale::Full).unwrap();
        assert_eq!((d.width, d.height, d.rgb.len()), (64, 48, 64 * 48 * 3));
        assert_eq!(d.icc.as_deref(), Some(&icc[..]));
        let small = decode(&jpg, Scale::Quarter).unwrap();
        assert_eq!(small.width, 16);
    }

    #[test]
    fn cmyk_decodes_to_rgb() {
        // Adobe-style inverted CMYK: c = m = y = 255, k = 128 is mid gray.
        let px: Vec<u8> = [255u8, 255, 255, 128].repeat(16 * 16);
        let opts = EncodeOptions {
            subsampling: Subsampling::S444,
            quality: 100,
            ..Default::default()
        };
        let jpg = encode(16, 16, &px, PixelFormat::Cmyk, opts, None).unwrap();
        let d = decode(&jpg, Scale::Full).unwrap();
        assert!(
            d.rgb.iter().all(|&v| v.abs_diff(128) <= 3),
            "{:?}",
            &d.rgb[..6]
        );
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(decode(b"definitely not a jpeg", Scale::Full).is_err());
    }

    #[test]
    fn scale_to_fit() {
        assert_eq!(Scale::to_fit(6000, 4000, 700, 700), Scale::Eighth);
        assert_eq!(Scale::to_fit(6000, 4000, 1200, 800), Scale::Quarter);
        assert_eq!(Scale::to_fit(640, 480, 1000, 1000), Scale::Full);
    }
}
