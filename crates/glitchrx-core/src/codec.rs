//! Image files in and out: decode JPEG, PNG or TIFF to a [`Raster`],
//! encode a raster as TIFF, PNG, JPEG, WebP or AVIF.
//!
//! Decoding applies the EXIF orientation (so the raster is upright) and
//! keeps the ICC colour profile, which is written into the output so the
//! colours match the original in Photoshop and other colour-managed apps.

use std::borrow::Cow;
use std::fmt;
use std::io::Cursor;

use crate::error::{Error, Result};
use crate::raster::Raster;
use crate::turbo::{self, EncodeOptions, PixelFormat, Scale, Subsampling};

/// File formats glitchrx writes. The first three it also reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Tiff,
    Png,
    Jpeg,
    Webp,
    Avif,
}

impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Format::Tiff => "tiff",
            Format::Png => "png",
            Format::Jpeg => "jpeg",
            Format::Webp => "webp",
            Format::Avif => "avif",
        }
    }

    /// Preferred file extension.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Tiff => "tif",
            Format::Png => "png",
            Format::Jpeg => "jpg",
            Format::Webp => "webp",
            Format::Avif => "avif",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Format::Tiff => "image/tiff",
            Format::Png => "image/png",
            Format::Jpeg => "image/jpeg",
            Format::Webp => "image/webp",
            Format::Avif => "image/avif",
        }
    }

    /// `tiff`, `tif`, `png`, `jpeg`, `jpg`, `webp`, `avif` (any case).
    pub fn parse(s: &str) -> Option<Format> {
        match s.trim().to_ascii_lowercase().as_str() {
            "tiff" | "tif" => Some(Format::Tiff),
            "png" => Some(Format::Png),
            "jpeg" | "jpg" | "jpe" | "jfif" => Some(Format::Jpeg),
            "webp" => Some(Format::Webp),
            "avif" => Some(Format::Avif),
            _ => None,
        }
    }

    /// Whether glitchrx can open files of this format (WebP and AVIF are
    /// export only).
    pub fn readable(self) -> bool {
        matches!(self, Format::Tiff | Format::Png | Format::Jpeg)
    }

    /// Whether some detail is thrown away when encoding with `settings`.
    pub fn lossy(self, settings: &EncodeSettings) -> bool {
        match self {
            Format::Tiff | Format::Png => false,
            Format::Webp => !settings.lossless,
            Format::Jpeg | Format::Avif => true,
        }
    }

    /// Recognise a file glitchrx can open by its first bytes.
    pub fn sniff(bytes: &[u8]) -> Option<Format> {
        if bytes.starts_with(&[0xFF, 0xD8]) {
            Some(Format::Jpeg)
        } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(Format::Png)
        } else if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
            Some(Format::Tiff)
        } else {
            None
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A decoded image, upright, with what we keep from its file.
#[derive(Debug, Clone)]
pub struct Image {
    pub raster: Raster,
    pub icc: Option<Vec<u8>>,
    /// The format it was decoded from.
    pub format: Format,
    /// The EXIF orientation that was applied (1 = none).
    pub orientation: u8,
}

/// Decode a JPEG, PNG or TIFF file.
pub fn decode(bytes: &[u8]) -> Result<Image> {
    decode_scaled(bytes, None)
}

/// Decode, shrinking to fit `max` (width, height) after orientation. JPEGs
/// are scaled inside the decoder, which is several times faster than a full
/// decode; other formats are decoded fully and then downscaled.
pub fn decode_scaled(bytes: &[u8], max: Option<(usize, usize)>) -> Result<Image> {
    let format = Format::sniff(bytes).ok_or(Error::UnknownFormat)?;
    let (raster, icc, orientation) = match format {
        Format::Jpeg => decode_jpeg(bytes, max)?,
        Format::Png => decode_png(bytes)?,
        Format::Tiff => decode_tiff(bytes)?,
        // sniff only names formats it can read.
        Format::Webp | Format::Avif => return Err(Error::UnknownFormat),
    };
    let mut raster = raster.orient(orientation);
    if let Some((w, h)) = max {
        raster = raster.downscale(w, h);
    }
    Ok(Image {
        raster,
        icc,
        format,
        orientation,
    })
}

fn decode_jpeg(bytes: &[u8], max: Option<(usize, usize)>) -> Result<(Raster, Option<Vec<u8>>, u8)> {
    let orientation = jpeg_orientation(bytes);
    // Scale inside the decoder to the smallest size that still covers the
    // box. The box is in display orientation; the decoder sees the stored
    // image, whose sides are swapped for orientations 5-8.
    let scale = match (max, jpeg_size(bytes)) {
        (Some((w, h)), Some((jw, jh))) => {
            let (bw, bh) = if orientation >= 5 { (h, w) } else { (w, h) };
            Scale::to_fit(jw as u32, jh as u32, bw as u32, bh as u32)
        }
        _ => Scale::Full,
    };
    let d = turbo::decode(bytes, scale).map_err(Error::Malformed)?;
    let raster = Raster::new(d.width as usize, d.height as usize, d.rgb);
    Ok((raster, d.icc, orientation))
}

fn decode_png(bytes: &[u8]) -> Result<(Raster, Option<Vec<u8>>, u8)> {
    let bad = |e: png::DecodingError| Error::Malformed(format!("png: {e}"));
    let mut dec = png::Decoder::new(Cursor::new(bytes));
    // Palette -> RGB(A), low bit depths -> 8, tRNS -> alpha.
    dec.set_transformations(png::Transformations::EXPAND);
    let mut reader = dec.read_info().map_err(bad)?;
    let icc = reader.info().icc_profile.as_ref().map(|c| c.to_vec());
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| Error::Unsupported("png: image too large".into()))?;
    let mut buf = vec![0u8; size];
    let out = reader.next_frame(&mut buf).map_err(bad)?;
    buf.truncate(out.buffer_size());
    let channels = match out.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => {
            return Err(Error::Unsupported("png: palette was not expanded".into()));
        }
    };
    let samples: Vec<u8> = match out.bit_depth {
        png::BitDepth::Sixteen => buf
            .chunks_exact(2)
            .map(|b| from16(u16::from_be_bytes([b[0], b[1]])))
            .collect(),
        _ => buf,
    };
    let raster = to_rgb(
        out.width as usize,
        out.height as usize,
        &samples,
        channels,
        false,
    )?;
    Ok((raster, icc, 1))
}

fn decode_tiff(bytes: &[u8]) -> Result<(Raster, Option<Vec<u8>>, u8)> {
    use tiff::ColorType as C;
    use tiff::decoder::{Decoder, DecodingResult};
    use tiff::tags::Tag;

    let bad = |e: tiff::TiffError| Error::Malformed(format!("tiff: {e}"));
    let mut dec = Decoder::new(Cursor::new(bytes)).map_err(bad)?;
    let (w, h) = dec.dimensions().map_err(bad)?;
    let color = dec.colortype().map_err(bad)?;
    let orientation = dec
        .get_tag_u32(Tag::Orientation)
        .ok()
        .filter(|o| (1..=8).contains(o))
        .unwrap_or(1) as u8;
    let icc = dec.get_tag_u8_vec(Tag::IccProfile).ok();
    let samples: Vec<u8> = match dec.read_image().map_err(bad)? {
        DecodingResult::U8(v) => v,
        DecodingResult::U16(v) => v.into_iter().map(from16).collect(),
        _ => {
            return Err(Error::Unsupported(
                "tiff: only 8- and 16-bit integer samples".into(),
            ));
        }
    };
    let (channels, cmyk) = match color {
        C::Gray(_) => (1, false),
        C::GrayA(_) => (2, false),
        C::RGB(_) => (3, false),
        C::RGBA(_) => (4, false),
        C::CMYK(_) => (4, true),
        other => return Err(Error::Unsupported(format!("tiff: colour type {other:?}"))),
    };
    let raster = to_rgb(w as usize, h as usize, &samples, channels, cmyk)?;
    Ok((raster, icc, orientation))
}

/// 16-bit to 8-bit, rounded (not truncated): `round(v * 255 / 65535)`.
fn from16(v: u16) -> u8 {
    ((v as u32 * 255 + 32767) / 65535) as u8
}

/// Interleaved samples with 1-4 channels to RGB. Alpha is dropped.
fn to_rgb(w: usize, h: usize, s: &[u8], channels: usize, cmyk: bool) -> Result<Raster> {
    if s.len() < w * h * channels {
        return Err(Error::Malformed(
            "image data is shorter than its size".into(),
        ));
    }
    let px = &s[..w * h * channels];
    let rgb: Vec<u8> = match (channels, cmyk) {
        (1, _) | (2, _) => px.chunks_exact(channels).flat_map(|p| [p[0]; 3]).collect(),
        (3, _) => px.to_vec(),
        (4, false) => px
            .chunks_exact(4)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect(),
        // TIFF CMYK is not inverted: ink 0 = white.
        (4, true) => px
            .chunks_exact(4)
            .flat_map(|p| {
                let k = 255 - p[3] as u32;
                [0, 1, 2].map(|i| (((255 - p[i] as u32) * k + 127) / 255) as u8)
            })
            .collect(),
        _ => unreachable!("channels is 1..=4"),
    };
    Ok(Raster::new(w, h, rgb))
}

/// How to encode the lossy formats. Each format reads only what applies to
/// it, and fills in its own default for anything left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EncodeSettings {
    /// 1..=100, for JPEG, lossy WebP and AVIF. Defaults: JPEG 95, WebP 90,
    /// AVIF 80.
    pub quality: Option<u8>,
    /// WebP: lossless instead of lossy.
    pub lossless: bool,
    /// AVIF: 1 (slowest, smallest) to 10 (fastest). Default 6.
    pub speed: Option<u8>,
}

impl EncodeSettings {
    pub const JPEG_QUALITY: u8 = 95;
    pub const WEBP_QUALITY: u8 = 90;
    pub const AVIF_QUALITY: u8 = 80;
    pub const AVIF_SPEED: u8 = 6;
}

/// Encode a raster with each format's default settings. `icc` is embedded
/// when given (AVIF: see [`crate::avif`]).
pub fn encode(raster: &Raster, format: Format, icc: Option<&[u8]>) -> Result<Vec<u8>> {
    encode_with(raster, format, icc, &EncodeSettings::default())
}

/// Encode a raster. The same raster and settings always give the same bytes.
pub fn encode_with(
    raster: &Raster,
    format: Format,
    icc: Option<&[u8]>,
    settings: &EncodeSettings,
) -> Result<Vec<u8>> {
    let quality = |default| settings.quality.unwrap_or(default).clamp(1, 100);
    match format {
        Format::Tiff => encode_tiff(raster, icc),
        Format::Png => encode_png(raster, icc),
        Format::Webp => crate::webp::encode(
            raster,
            quality(EncodeSettings::WEBP_QUALITY),
            settings.lossless,
            icc,
        ),
        Format::Avif => crate::avif::encode(
            raster,
            quality(EncodeSettings::AVIF_QUALITY),
            settings.speed.unwrap_or(EncodeSettings::AVIF_SPEED),
            icc,
        ),
        Format::Jpeg => turbo::encode(
            raster.width as u32,
            raster.height as u32,
            &raster.bytes,
            PixelFormat::Rgb,
            EncodeOptions {
                quality: quality(EncodeSettings::JPEG_QUALITY),
                // Full chroma: glitch colour fringes are the point.
                subsampling: Subsampling::S444,
                progressive: false,
                restart_rows: 0,
            },
            icc,
        )
        .ok_or_else(|| Error::Malformed("jpeg encoding failed".into())),
    }
}

/// Uncompressed 8-bit RGB TIFF, the format Phil masks in Photoshop.
fn encode_tiff(raster: &Raster, icc: Option<&[u8]>) -> Result<Vec<u8>> {
    use tiff::encoder::{TiffEncoder, colortype};
    use tiff::tags::Tag;

    let bad = |e: tiff::TiffError| Error::Malformed(format!("tiff encoding: {e}"));
    let mut out = Cursor::new(Vec::new());
    {
        let mut enc = TiffEncoder::new(&mut out).map_err(bad)?;
        let mut img = enc
            .new_image::<colortype::RGB8>(raster.width as u32, raster.height as u32)
            .map_err(bad)?;
        if let Some(icc) = icc {
            img.encoder().write_tag(Tag::IccProfile, icc).map_err(bad)?;
        }
        img.write_data(&raster.bytes).map_err(bad)?;
    }
    Ok(out.into_inner())
}

fn encode_png(raster: &Raster, icc: Option<&[u8]>) -> Result<Vec<u8>> {
    let bad = |e: png::EncodingError| Error::Malformed(format!("png encoding: {e}"));
    let mut info = png::Info::with_size(raster.width as u32, raster.height as u32);
    info.color_type = png::ColorType::Rgb;
    info.bit_depth = png::BitDepth::Eight;
    info.icc_profile = icc.map(Cow::Borrowed);
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::with_info(&mut out, info).map_err(bad)?;
        // Full-resolution exports are large; fast compression is about 3x
        // quicker for files only slightly bigger.
        enc.set_compression(png::Compression::Fast);
        let mut w = enc.write_header().map_err(bad)?;
        w.write_image_data(&raster.bytes).map_err(bad)?;
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Small JPEG header readers: frame size and EXIF orientation. They walk the
// marker segments up to the first scan and never fail; anything unexpected
// means "unknown".

/// Marker segments before the first scan: (marker, payload).
fn jpeg_segments(b: &[u8]) -> impl Iterator<Item = (u8, &[u8])> {
    let mut i = 2; // after SOI
    std::iter::from_fn(move || {
        loop {
            if i + 4 > b.len() || b[i] != 0xFF {
                return None;
            }
            let marker = b[i + 1];
            if marker == 0xFF {
                i += 1; // fill byte
                continue;
            }
            if marker == 0xDA || marker == 0xD9 {
                return None; // start of scan / end of image
            }
            let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
            if len < 2 || i + 2 + len > b.len() {
                return None;
            }
            let payload = &b[i + 4..i + 2 + len];
            i += 2 + len;
            return Some((marker, payload));
        }
    })
}

/// Width and height from the SOF segment.
fn jpeg_size(b: &[u8]) -> Option<(usize, usize)> {
    jpeg_segments(b)
        .find(|(m, p)| matches!(m, 0xC0..=0xCF) && !matches!(m, 0xC4 | 0xC8 | 0xCC) && p.len() >= 5)
        .map(|(_, p)| {
            (
                u16::from_be_bytes([p[3], p[4]]) as usize,
                u16::from_be_bytes([p[1], p[2]]) as usize,
            )
        })
}

/// The EXIF orientation tag (0x0112) from APP1, or 1.
pub fn jpeg_orientation(b: &[u8]) -> u8 {
    jpeg_segments(b)
        .filter(|(m, p)| *m == 0xE1 && p.starts_with(b"Exif\0\0"))
        .find_map(|(_, p)| exif_orientation(&p[6..]))
        .unwrap_or(1)
}

/// Read tag 0x0112 from IFD0 of a TIFF-structured EXIF block.
fn exif_orientation(t: &[u8]) -> Option<u8> {
    let le = match t.get(..4)? {
        b"II*\0" => true,
        b"MM\0*" => false,
        _ => return None,
    };
    let u16_at = |i: usize| -> Option<u16> {
        let b = t.get(i..i + 2)?;
        Some(if le {
            u16::from_le_bytes([b[0], b[1]])
        } else {
            u16::from_be_bytes([b[0], b[1]])
        })
    };
    let u32_at = |i: usize| -> Option<u32> {
        let b = t.get(i..i + 4)?;
        let a = [b[0], b[1], b[2], b[3]];
        Some(if le {
            u32::from_le_bytes(a)
        } else {
            u32::from_be_bytes(a)
        })
    };
    let ifd = u32_at(4)? as usize;
    let count = u16_at(ifd)? as usize;
    (0..count).find_map(|n| {
        let e = ifd + 2 + n * 12;
        (u16_at(e)? == 0x0112)
            .then(|| u16_at(e + 8))
            .flatten()
            .filter(|o| (1..=8).contains(o))
            .map(|o| o as u8)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Raster {
        let bytes = (0..12 * 8)
            .flat_map(|i| [(i * 3) as u8, (i * 5) as u8, (i * 7) as u8])
            .collect();
        Raster::new(12, 8, bytes)
    }

    #[test]
    fn lossless_formats_round_trip_with_icc() {
        let icc = b"not really a profile".to_vec();
        for f in [Format::Tiff, Format::Png] {
            let bytes = encode(&sample(), f, Some(&icc)).unwrap();
            assert_eq!(Format::sniff(&bytes), Some(f));
            let img = decode(&bytes).unwrap();
            assert_eq!(img.raster, sample(), "{f}");
            assert_eq!(img.icc.as_deref(), Some(&icc[..]), "{f}");
        }
    }

    /// A minimal ICC profile with only a description, v2 (`desc`) or v4
    /// (`mluc`) style.
    fn profile(name: &str, v4: bool) -> Vec<u8> {
        let mut tag = Vec::new();
        if v4 {
            let text: Vec<u8> = name.encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
            tag.extend_from_slice(b"mluc\0\0\0\0");
            tag.extend_from_slice(&1u32.to_be_bytes());
            tag.extend_from_slice(&12u32.to_be_bytes());
            tag.extend_from_slice(b"enUS");
            tag.extend_from_slice(&(text.len() as u32).to_be_bytes());
            tag.extend_from_slice(&28u32.to_be_bytes());
            tag.extend_from_slice(&text);
        } else {
            tag.extend_from_slice(b"desc\0\0\0\0");
            tag.extend_from_slice(&(name.len() as u32 + 1).to_be_bytes());
            tag.extend_from_slice(name.as_bytes());
            tag.push(0);
        }
        let mut p = vec![0u8; 128];
        p.extend_from_slice(&1u32.to_be_bytes());
        p.extend_from_slice(b"desc");
        p.extend_from_slice(&144u32.to_be_bytes());
        p.extend_from_slice(&(tag.len() as u32).to_be_bytes());
        p.extend_from_slice(&tag);
        p
    }

    /// A larger, busier picture than `sample`, for the lossy encoders.
    fn picture() -> Raster {
        let (w, h) = (64, 48);
        let bytes = (0..w * h)
            .flat_map(|i| {
                let (x, y) = (i % w, i / w);
                [(x * 4) as u8, (y * 5) as u8, ((x ^ y) * 8) as u8]
            })
            .collect();
        Raster::new(w, h, bytes)
    }

    fn decode_webp(bytes: &[u8]) -> Raster {
        let (mut w, mut h) = (0, 0);
        // SAFETY: libwebp allocates the pixels, copied then freed here.
        unsafe {
            let p = libwebp_sys::WebPDecodeRGB(bytes.as_ptr(), bytes.len(), &mut w, &mut h);
            assert!(!p.is_null(), "not a webp");
            let v = std::slice::from_raw_parts(p, (w * h * 3) as usize).to_vec();
            libwebp_sys::WebPFree(p.cast());
            Raster::new(w as usize, h as usize, v)
        }
    }

    #[test]
    fn webp_is_exact_when_lossless_and_close_when_lossy() {
        let img = picture();
        let lossless = EncodeSettings {
            lossless: true,
            ..EncodeSettings::default()
        };
        let bytes = encode_with(&img, Format::Webp, None, &lossless).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WEBP");
        assert_eq!(decode_webp(&bytes), img);
        let lossy = encode_with(&img, Format::Webp, None, &EncodeSettings::default()).unwrap();
        let back = decode_webp(&lossy);
        let err: f64 = img
            .bytes
            .iter()
            .zip(&back.bytes)
            .map(|(a, b)| (*a as f64 - *b as f64).abs())
            .sum::<f64>()
            / img.bytes.len() as f64;
        assert!(err < 8.0, "mean error {err}");
        // Lower quality, smaller file; and the same bytes every time.
        let low = EncodeSettings {
            quality: Some(20),
            ..EncodeSettings::default()
        };
        let small = encode_with(&img, Format::Webp, None, &low).unwrap();
        assert!(small.len() < lossy.len());
        assert_eq!(small, encode_with(&img, Format::Webp, None, &low).unwrap());
    }

    #[test]
    fn webp_carries_the_colour_profile() {
        let icc = profile("Display P3", true);
        let bytes = encode(&picture(), Format::Webp, Some(&icc)).unwrap();
        let at = bytes
            .windows(4)
            .position(|w| w == b"ICCP")
            .expect("an ICCP chunk");
        assert_eq!(&bytes[at + 8..at + 8 + icc.len()], &icc[..]);
        assert_eq!(decode_webp(&bytes).width, 64);
    }

    #[test]
    fn webp_refuses_what_it_cannot_hold() {
        let wide = Raster::new(
            crate::webp::MAX_SIDE + 1,
            1,
            vec![0; (crate::webp::MAX_SIDE + 1) * 3],
        );
        assert!(matches!(
            encode(&wide, Format::Webp, None),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn avif_is_deterministic_and_marks_display_p3() {
        let img = picture();
        let fast = EncodeSettings {
            speed: Some(10),
            ..EncodeSettings::default()
        };
        let a = encode_with(&img, Format::Avif, None, &fast).unwrap();
        assert_eq!(&a[4..12], b"ftypavif");
        assert_eq!(a, encode_with(&img, Format::Avif, None, &fast).unwrap());
        // sRGB is what readers assume, so it has no colour box; Display P3
        // gets one naming its primaries (code point 12), around the same AV1 data.
        let primaries = |f: &[u8]| {
            f.windows(8)
                .position(|w| w == b"colrnclx")
                .map(|at| u16::from_be_bytes([f[at + 8], f[at + 9]]))
        };
        assert_eq!(primaries(&a), None);
        let p3 = encode_with(
            &img,
            Format::Avif,
            Some(&profile("Display P3", false)),
            &fast,
        )
        .unwrap();
        assert_eq!(primaries(&p3), Some(12));
        let mdat = |f: &[u8]| f[f.windows(4).rposition(|w| w == b"mdat").unwrap()..].to_vec();
        assert_eq!(mdat(&p3), mdat(&a), "the picture data is the same");
        // Lower quality, smaller file.
        let low = EncodeSettings {
            quality: Some(20),
            ..fast
        };
        assert!(encode_with(&img, Format::Avif, None, &low).unwrap().len() < a.len());
    }

    #[test]
    fn profile_names_and_what_avif_keeps() {
        assert_eq!(
            crate::avif::profile_name(&profile("sRGB IEC61966-2.1", false)).as_deref(),
            Some("sRGB IEC61966-2.1")
        );
        assert_eq!(
            crate::avif::profile_name(&profile("Display P3", true)).as_deref(),
            Some("Display P3")
        );
        assert_eq!(crate::avif::profile_name(b"junk"), None);
        assert!(crate::avif::keeps_profile(None));
        assert!(crate::avif::keeps_profile(Some(&profile(
            "sRGB built-in",
            true
        ))));
        assert!(crate::avif::keeps_profile(Some(&profile(
            "Display P3",
            true
        ))));
        assert!(!crate::avif::keeps_profile(Some(&profile(
            "Adobe RGB (1998)",
            false
        ))));
        assert!(!crate::avif::keeps_profile(Some(b"junk")));
    }

    #[test]
    fn formats_know_what_they_read_and_lose() {
        assert_eq!(Format::parse("WEBP"), Some(Format::Webp));
        assert_eq!(Format::parse("avif"), Some(Format::Avif));
        assert!(!Format::Webp.readable() && !Format::Avif.readable() && Format::Jpeg.readable());
        let lossless = EncodeSettings {
            lossless: true,
            ..EncodeSettings::default()
        };
        assert!(!Format::Webp.lossy(&lossless) && Format::Webp.lossy(&EncodeSettings::default()));
        assert!(Format::Avif.lossy(&lossless) && !Format::Png.lossy(&EncodeSettings::default()));
    }

    #[test]
    fn jpeg_round_trip_is_close() {
        let bytes = encode(&sample(), Format::Jpeg, None).unwrap();
        let img = decode(&bytes).unwrap();
        assert_eq!((img.raster.width, img.raster.height), (12, 8));
        assert_eq!(img.format, Format::Jpeg);
    }

    /// A minimal APP1 EXIF block with orientation `o`, spliced after SOI.
    fn with_orientation(jpeg: &[u8], o: u16, le: bool) -> Vec<u8> {
        let mut tiff = Vec::new();
        let w16 = |v: u16| if le { v.to_le_bytes() } else { v.to_be_bytes() };
        let w32 = |v: u32| if le { v.to_le_bytes() } else { v.to_be_bytes() };
        tiff.extend_from_slice(if le { b"II*\0" } else { b"MM\0*" });
        tiff.extend_from_slice(&w32(8));
        tiff.extend_from_slice(&w16(1)); // one entry
        tiff.extend_from_slice(&w16(0x0112));
        tiff.extend_from_slice(&w16(3)); // SHORT
        tiff.extend_from_slice(&w32(1));
        tiff.extend_from_slice(&w16(o));
        tiff.extend_from_slice(&[0, 0]);
        tiff.extend_from_slice(&w32(0)); // no next IFD
        let mut app1 = b"Exif\0\0".to_vec();
        app1.extend_from_slice(&tiff);
        let mut out = jpeg[..2].to_vec();
        out.extend_from_slice(&[0xFF, 0xE1]);
        out.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
        out.extend_from_slice(&app1);
        out.extend_from_slice(&jpeg[2..]);
        out
    }

    #[test]
    fn exif_orientation_is_applied() {
        let base = encode(&sample(), Format::Jpeg, None).unwrap();
        assert_eq!(jpeg_size(&base), Some((12, 8)));
        for (o, le) in [(6, true), (8, false), (3, true)] {
            let j = with_orientation(&base, o, le);
            assert_eq!(jpeg_orientation(&j), o as u8);
            let img = decode(&j).unwrap();
            assert_eq!(img.orientation, o as u8);
            let dims = (img.raster.width, img.raster.height);
            assert_eq!(
                dims,
                if o >= 5 { (8, 12) } else { (12, 8) },
                "orientation {o}"
            );
        }
        assert_eq!(jpeg_orientation(&base), 1);
    }

    #[test]
    fn scaled_decode_fits_the_box() {
        let big = Raster::new(400, 300, vec![77; 400 * 300 * 3]);
        for f in [Format::Jpeg, Format::Png] {
            let bytes = encode(&big, f, None).unwrap();
            let img = decode_scaled(&bytes, Some((100, 100))).unwrap();
            assert_eq!((img.raster.width, img.raster.height), (100, 75), "{f}");
        }
    }

    #[test]
    fn unknown_and_broken_files_are_errors() {
        assert!(matches!(decode(b"GIF89a....."), Err(Error::UnknownFormat)));
        assert!(decode(b"\x89PNG\r\n\x1a\nbroken").is_err());
        assert!(decode(b"II*\0broken").is_err());
        assert_eq!(Format::parse("TIF"), Some(Format::Tiff));
    }
}
