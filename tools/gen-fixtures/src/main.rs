//! Generate the committed test fixtures.
//!
//! Usage: `cargo run -p gen-fixtures -- tests/fixtures`
//!
//! Every image is drawn procedurally (so the fixtures are CC0 and contain no
//! personal photos) and then written in each variant the decoders have to
//! handle: JPEG (subsampling, grayscale, progressive, CMYK, EXIF orientation
//! 1-8, ICC profile, odd marker layouts), PNG (8/16-bit, alpha, palette,
//! grayscale) and TIFF (uncompressed, LZW, 16-bit, orientation). The output
//! is committed, so tests do not depend on the installed encoder versions.

use std::fs;
use std::io::Cursor;
use std::path::Path;

use anyhow::{Context, Result};
use glitchrx_core::codec::{self, Format};
use glitchrx_core::raster::Raster;
use glitchrx_core::rng::Rng;
use glitchrx_core::turbo::{EncodeOptions, PixelFormat, Subsampling, encode};

const W: u32 = 480;
const H: u32 = 320;

/// A small landscape-like scene: sky gradient, sun, textured hills, a band
/// of fine stripes (text-like edges) and colour bars. Different regions
/// exercise smooth areas, edges, saturated colour and noise.
fn scene(w: u32, h: u32) -> Raster {
    let mut rng = Rng::new(2026);
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            let fy = y as f32 / h as f32;
            let fx = x as f32 / w as f32;
            let noise = (rng.next_u64() % 24) as f32 - 12.0;
            // Rolling hill line.
            let hill = 0.62 + 0.08 * (fx * 9.0).sin() + 0.04 * (fx * 23.0).cos();
            let (mut r, mut g, mut b);
            if fy < hill {
                // Sky: deep blue to warm horizon.
                r = 40.0 + 200.0 * fy;
                g = 80.0 + 120.0 * fy;
                b = 200.0 - 60.0 * fy;
                // Sun.
                let (dx, dy) = (fx - 0.72, fy - 0.25);
                let d = (dx * dx * 2.25 + dy * dy).sqrt();
                if d < 0.09 {
                    r = 255.0;
                    g = 220.0;
                    b = 120.0;
                } else if d < 0.16 {
                    let t = (0.16 - d) / 0.07;
                    r += (255.0 - r) * t * 0.6;
                    g += (210.0 - g) * t * 0.6;
                }
            } else {
                // Hills: textured green with noise.
                r = 50.0 + noise * 2.0;
                g = 120.0 + 60.0 * (fy - hill) + noise * 3.0;
                b = 40.0 + noise;
            }
            // Fine vertical stripes, like text or fences.
            if (0.80..0.86).contains(&fy) && x % 6 < 3 && x < w / 2 {
                r = 20.0;
                g = 20.0;
                b = 20.0;
            }
            // Colour bars along the bottom.
            if fy >= 0.9 {
                let bar = (fx * 8.0) as u32;
                let bits = [7, 6, 3, 2, 5, 4, 1, 0][bar.min(7) as usize];
                r = if bits & 4 != 0 { 230.0 } else { 15.0 };
                g = if bits & 2 != 0 { 230.0 } else { 15.0 };
                b = if bits & 1 != 0 { 230.0 } else { 15.0 };
            }
            let c = |v: f32| v.clamp(0.0, 255.0) as u8;
            rgb.extend_from_slice(&[c(r), c(g), c(b)]);
        }
    }
    Raster::new(w as usize, h as usize, rgb)
}

/// A stand-in colour profile. The decoders only carry it through, so its
/// content does not matter; real profiles are binary blobs like this.
pub const ICC: &[u8] = b"glitcharaxia test profile; not a real ICC profile.";

/// Minimal EXIF APP1 segment carrying only an orientation tag (0x0112).
fn exif_orientation(o: u16) -> Vec<u8> {
    let mut t = Vec::new();
    t.extend_from_slice(b"MM\0*"); // big endian, as many cameras write it
    t.extend_from_slice(&8u32.to_be_bytes());
    t.extend_from_slice(&1u16.to_be_bytes()); // one entry
    t.extend_from_slice(&0x0112u16.to_be_bytes());
    t.extend_from_slice(&3u16.to_be_bytes()); // SHORT
    t.extend_from_slice(&1u32.to_be_bytes());
    t.extend_from_slice(&o.to_be_bytes());
    t.extend_from_slice(&[0, 0]);
    t.extend_from_slice(&0u32.to_be_bytes()); // no next IFD
    let mut seg = vec![0xFF, 0xE1];
    seg.extend_from_slice(&((2 + 6 + t.len()) as u16).to_be_bytes());
    seg.extend_from_slice(b"Exif\0\0");
    seg.extend_from_slice(&t);
    seg
}

/// The orientation that undoes `o`: 6 and 8 are each other's inverse, every
/// other orientation is its own.
fn inverse(o: u8) -> u8 {
    match o {
        6 => 8,
        8 => 6,
        o => o,
    }
}

fn png_file(
    w: u32,
    h: u32,
    color: png::ColorType,
    depth: png::BitDepth,
    data: &[u8],
    palette: Option<Vec<u8>>,
) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(color);
        enc.set_depth(depth);
        if let Some(p) = palette {
            enc.set_palette(p);
        }
        let mut wr = enc.write_header()?;
        wr.write_image_data(data)?;
    }
    Ok(out)
}

fn tiff_file<C: tiff::encoder::colortype::ColorType>(
    w: u32,
    h: u32,
    data: &[C::Inner],
    compression: tiff::encoder::Compression,
    orientation: Option<u16>,
) -> Result<Vec<u8>>
where
    [C::Inner]: tiff::encoder::TiffValue,
{
    let mut out = Cursor::new(Vec::new());
    {
        let mut enc = tiff::encoder::TiffEncoder::new(&mut out)?.with_compression(compression);
        let mut img = enc.new_image::<C>(w, h)?;
        if let Some(o) = orientation {
            img.encoder().write_tag(tiff::tags::Tag::Orientation, o)?;
        }
        img.write_data(data)?;
    }
    Ok(out.into_inner())
}

/// Minimal EXIF APP1 segment carrying a JPEG thumbnail in IFD1, the way
/// cameras and phones store one.
fn exif_app1(thumb: &[u8]) -> Vec<u8> {
    // TIFF structure (little endian), offsets relative to the TIFF header:
    //   0: "II*\0" + offset of IFD0 (8)
    //   8: IFD0 with 0 entries, next IFD at 14
    //  14: IFD1 with 3 entries (Compression, JPEGInterchangeFormat, ...Length)
    //  56: thumbnail bytes
    let mut t = Vec::new();
    t.extend_from_slice(b"II*\0");
    t.extend_from_slice(&8u32.to_le_bytes());
    t.extend_from_slice(&0u16.to_le_bytes()); // IFD0: no entries
    t.extend_from_slice(&14u32.to_le_bytes()); // next IFD
    t.extend_from_slice(&3u16.to_le_bytes()); // IFD1: 3 entries
    let thumb_off = 14 + 2 + 3 * 12 + 4;
    for (tag, typ, val) in [
        (0x0103u16, 3u16, 6u32),         // Compression = JPEG
        (0x0201, 4, thumb_off as u32),   // JPEGInterchangeFormat
        (0x0202, 4, thumb.len() as u32), // JPEGInterchangeFormatLength
    ] {
        t.extend_from_slice(&tag.to_le_bytes());
        t.extend_from_slice(&typ.to_le_bytes());
        t.extend_from_slice(&1u32.to_le_bytes());
        t.extend_from_slice(&val.to_le_bytes());
    }
    t.extend_from_slice(&0u32.to_le_bytes()); // no further IFDs
    assert_eq!(t.len(), thumb_off);
    t.extend_from_slice(thumb);

    let mut seg = vec![0xFF, 0xE1];
    let len = 2 + 6 + t.len();
    assert!(len < 65536, "thumbnail too large for one APP1 segment");
    seg.extend_from_slice(&(len as u16).to_be_bytes());
    seg.extend_from_slice(b"Exif\0\0");
    seg.extend_from_slice(&t);
    seg
}

fn main() -> Result<()> {
    let out_dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "tests/fixtures".into());
    let out = Path::new(&out_dir);
    fs::create_dir_all(out)?;
    let img = scene(W, H);
    let jpeg = |r: &Raster, opts: EncodeOptions, icc: Option<&[u8]>| {
        encode(
            r.width as u32,
            r.height as u32,
            &r.bytes,
            PixelFormat::Rgb,
            opts,
            icc,
        )
        .context("libjpeg-turbo encode failed")
    };
    let enc = |opts: EncodeOptions| jpeg(&img, opts, None);
    let base = EncodeOptions::default();

    let mut files: Vec<(String, Vec<u8>)> = vec![
        ("baseline-420.jpg".into(), enc(base)?),
        (
            "baseline-422.jpg".into(),
            enc(EncodeOptions {
                subsampling: Subsampling::S422,
                ..base
            })?,
        ),
        (
            "baseline-444.jpg".into(),
            enc(EncodeOptions {
                subsampling: Subsampling::S444,
                ..base
            })?,
        ),
        (
            "grayscale.jpg".into(),
            enc(EncodeOptions {
                subsampling: Subsampling::Gray,
                ..base
            })?,
        ),
        (
            "progressive-420.jpg".into(),
            enc(EncodeOptions {
                progressive: true,
                ..base
            })?,
        ),
        ("icc-profile.jpg".into(), jpeg(&img, base, Some(ICC))?),
    ];

    // CMYK, Adobe style (inverted): c = 255 - r etc., k = 255.
    let cmyk: Vec<u8> = img
        .bytes
        .chunks_exact(3)
        .flat_map(|p| [p[0], p[1], p[2], 255])
        .collect();
    files.push((
        "cmyk.jpg".into(),
        encode(
            W,
            H,
            &cmyk,
            PixelFormat::Cmyk,
            EncodeOptions {
                subsampling: Subsampling::S444,
                ..base
            },
            None,
        )
        .context("CMYK encode failed")?,
    ));

    // EXIF orientation 1-8: the stored image is the scene turned the other
    // way, so every file decodes upright to the same picture.
    for o in 1..=8u8 {
        let stored = img.clone().orient(inverse(o));
        let mut j = jpeg(
            &stored,
            EncodeOptions {
                subsampling: Subsampling::S444,
                ..base
            },
            None,
        )?;
        j.splice(2..2, exif_orientation(o as u16));
        files.push((format!("orient-{o}.jpg"), j));
    }

    // EXIF thumbnail: insert APP1 right after SOI.
    let thumb = jpeg(
        &scene(80, 54),
        EncodeOptions {
            quality: 70,
            ..base
        },
        None,
    )
    .context("thumbnail encode failed")?;
    let mut exif = enc(base)?;
    exif.splice(2..2, exif_app1(&thumb));
    files.push(("exif-thumbnail.jpg".into(), exif));

    // Trailing data: a second complete JPEG after EOI, like the gain map or
    // MPF images phones append.
    let mut trailing = enc(base)?;
    trailing.extend_from_slice(&thumb);
    files.push(("trailing-data.jpg".into(), trailing));

    // Fill bytes: extra FFs before the SOS marker and before EOI.
    let mut fill = enc(base)?;
    let sos = fill
        .windows(2)
        .position(|w| w == [0xFF, 0xDA])
        .context("no SOS in encoded image")?;
    fill.splice(sos..sos, [0xFF, 0xFF]);
    let eoi = fill.len() - 2;
    fill.splice(eoi..eoi, [0xFF, 0xFF, 0xFF]);
    files.push(("fill-bytes.jpg".into(), fill));

    // PNG.
    use png::{BitDepth, ColorType};
    let wide: Vec<u8> = img
        .bytes
        .iter()
        .flat_map(|&v| (v as u16 * 257).to_be_bytes())
        .collect();
    let rgba: Vec<u8> = img
        .bytes
        .chunks_exact(3)
        .enumerate()
        .flat_map(|(i, p)| [p[0], p[1], p[2], (i % W as usize * 255 / W as usize) as u8])
        .collect();
    let gray: Vec<u8> = img
        .bytes
        .chunks_exact(3)
        .map(|p| ((p[0] as u32 * 77 + p[1] as u32 * 150 + p[2] as u32 * 29) >> 8) as u8)
        .collect();
    // Palette: 2 bits per channel, 64 colours.
    let palette: Vec<u8> = (0..64u8)
        .flat_map(|i| [(i >> 4) * 85, ((i >> 2) & 3) * 85, (i & 3) * 85])
        .collect();
    let indices: Vec<u8> = img
        .bytes
        .chunks_exact(3)
        .map(|p| ((p[0] >> 6) << 4) | ((p[1] >> 6) << 2) | (p[2] >> 6))
        .collect();
    files.push((
        "rgb8.png".into(),
        codec::encode(&img, Format::Png, Some(ICC))?,
    ));
    files.push((
        "rgb16.png".into(),
        png_file(W, H, ColorType::Rgb, BitDepth::Sixteen, &wide, None)?,
    ));
    files.push((
        "rgba8.png".into(),
        png_file(W, H, ColorType::Rgba, BitDepth::Eight, &rgba, None)?,
    ));
    files.push((
        "gray8.png".into(),
        png_file(W, H, ColorType::Grayscale, BitDepth::Eight, &gray, None)?,
    ));
    files.push((
        "palette.png".into(),
        png_file(
            W,
            H,
            ColorType::Indexed,
            BitDepth::Eight,
            &indices,
            Some(palette),
        )?,
    ));

    // TIFF, at half size: uncompressed files are big and gain nothing from
    // more pixels.
    use tiff::encoder::{Compression, colortype};
    let (tw, th) = (W / 2, H / 2);
    let small = scene(tw, th);
    files.push((
        "rgb8.tif".into(),
        codec::encode(&small, Format::Tiff, Some(ICC))?,
    ));
    files.push((
        "lzw.tif".into(),
        tiff_file::<colortype::RGB8>(tw, th, &small.bytes, Compression::Lzw, None)?,
    ));
    let wide16: Vec<u16> = small.bytes.iter().map(|&v| v as u16 * 257).collect();
    files.push((
        "rgb16.tif".into(),
        tiff_file::<colortype::RGB16>(tw, th, &wide16, Compression::Uncompressed, None)?,
    ));
    let turned = small.clone().orient(inverse(6));
    files.push((
        "orient-6.tif".into(),
        tiff_file::<colortype::RGB8>(
            turned.width as u32,
            turned.height as u32,
            &turned.bytes,
            Compression::Uncompressed,
            Some(6),
        )?,
    ));

    for (name, bytes) in &files {
        let path = out.join(name);
        fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
        println!("{:>8} bytes  {}", bytes.len(), path.display());
    }
    Ok(())
}
