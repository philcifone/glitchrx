//! Generate the committed JPEG test fixtures.
//!
//! Usage: `cargo run -p gen-fixtures -- tests/fixtures`
//!
//! Every image is drawn procedurally (so the fixtures are CC0 and contain no
//! personal photos) and then encoded with libjpeg-turbo in each structural
//! variant the parser has to handle. The output is committed, so tests do
//! not depend on the installed encoder version.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use glitchrx_core::rng::Rng;
use glitchrx_core::turbo::{EncodeOptions, Image, Subsampling, encode};

const W: u32 = 480;
const H: u32 = 320;

/// A small landscape-like scene: sky gradient, sun, textured hills, a band
/// of fine stripes (text-like edges) and colour bars. Different regions
/// exercise smooth areas, edges, saturated colour and noise.
fn scene(w: u32, h: u32) -> Image {
    let mut rng = Rng::new(2026);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
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
            rgba.extend_from_slice(&[c(r), c(g), c(b), 255]);
        }
    }
    Image {
        width: w,
        height: h,
        rgba,
    }
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
    let enc = |opts: EncodeOptions| encode(&img, opts).context("libjpeg-turbo encode failed");
    let base = EncodeOptions::default();

    let mut files: Vec<(&str, Vec<u8>)> = vec![
        ("baseline-420.jpg", enc(base)?),
        (
            "baseline-422.jpg",
            enc(EncodeOptions {
                subsampling: Subsampling::S422,
                ..base
            })?,
        ),
        (
            "baseline-444.jpg",
            enc(EncodeOptions {
                subsampling: Subsampling::S444,
                ..base
            })?,
        ),
        (
            "grayscale.jpg",
            enc(EncodeOptions {
                subsampling: Subsampling::Gray,
                ..base
            })?,
        ),
        (
            "progressive-420.jpg",
            enc(EncodeOptions {
                progressive: true,
                ..base
            })?,
        ),
        (
            "restart-420.jpg",
            enc(EncodeOptions {
                restart_rows: 1,
                ..base
            })?,
        ),
    ];

    // EXIF thumbnail: insert APP1 right after SOI.
    let thumb = encode(
        &scene(80, 54),
        EncodeOptions {
            quality: 70,
            ..base
        },
    )
    .context("thumbnail encode failed")?;
    let mut exif = enc(base)?;
    exif.splice(2..2, exif_app1(&thumb));
    files.push(("exif-thumbnail.jpg", exif));

    // Trailing data: a second complete JPEG after EOI, like the gain map or
    // MPF images phones append.
    let mut trailing = enc(base)?;
    trailing.extend_from_slice(&thumb);
    files.push(("trailing-data.jpg", trailing));

    // Fill bytes: extra FFs before the SOS marker and before EOI.
    let mut fill = enc(base)?;
    let sos = fill
        .windows(2)
        .position(|w| w == [0xFF, 0xDA])
        .context("no SOS in encoded image")?;
    fill.splice(sos..sos, [0xFF, 0xFF]);
    let eoi = fill.len() - 2;
    fill.splice(eoi..eoi, [0xFF, 0xFF, 0xFF]);
    files.push(("fill-bytes.jpg", fill));

    for (name, bytes) in &files {
        let path = out.join(name);
        fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
        println!("{:>8} bytes  {}", bytes.len(), path.display());
    }
    Ok(())
}
