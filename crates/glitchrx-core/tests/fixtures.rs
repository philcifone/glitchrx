//! Whole-image tests over the committed fixtures: decoders, orientation,
//! colour profiles, robustness, golden hashes and preview fidelity.

use std::collections::BTreeMap;
use std::path::PathBuf;

use glitchrx_core::codec::{self, Format};
use glitchrx_core::effects::REGISTRY;
use glitchrx_core::mask::{Mask, MaskOp, Mode, Stroke};
use glitchrx_core::{Encoding, Raster, Recipe, Step, Units};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn read(name: &str) -> Vec<u8> {
    std::fs::read(fixture_dir().join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn decode(name: &str) -> codec::Image {
    codec::decode(&read(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// Every image fixture as `(name, bytes)`.
fn fixtures() -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<_> = std::fs::read_dir(fixture_dir())
        .expect("tests/fixtures missing; run `just fixtures`")
        .filter_map(|e| {
            let p = e.ok()?.path();
            let ext = p.extension()?.to_str()?;
            Format::parse(ext)?;
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            Some((name, std::fs::read(&p).unwrap()))
        })
        .collect();
    v.sort();
    assert!(
        v.len() >= 25,
        "expected the full fixture set, got {}",
        v.len()
    );
    v
}

/// Mean absolute difference per byte between two same-sized rasters.
fn mean_diff(a: &Raster, b: &Raster) -> f64 {
    assert_eq!((a.width, a.height), (b.width, b.height));
    let sum: u64 = a
        .bytes
        .iter()
        .zip(&b.bytes)
        .map(|(x, y)| x.abs_diff(*y) as u64)
        .sum();
    sum as f64 / a.bytes.len() as f64
}

/// A mask that uses every kind of operation, every mode, invert and feather.
fn busy_mask() -> Mask {
    Mask {
        invert: true,
        feather: 0.01,
        ops: vec![
            MaskOp::Brightness {
                mode: Mode::Add,
                low: 0.4,
                high: 0.9,
                soft: 0.1,
            },
            MaskOp::Brush {
                mode: Mode::Add,
                strokes: vec![
                    Stroke {
                        erase: false,
                        size: 0.08,
                        hardness: 0.4,
                        points: vec![0.1, 0.1, 0.4, 0.5, 0.9, 0.6],
                    },
                    Stroke {
                        erase: true,
                        size: 0.03,
                        hardness: 0.7,
                        points: vec![0.35, 0.45, 0.45, 0.55],
                    },
                ],
            },
            MaskOp::Linear {
                mode: Mode::Intersect,
                from: [0.0, 0.2],
                to: [0.0, 1.0],
            },
            MaskOp::Radial {
                mode: Mode::Subtract,
                from: [0.6, 0.5],
                to: [0.75, 0.5],
                hardness: 1.0,
            },
        ],
    }
}

fn recipe(effect: &str, params: &str, seed: u64, encoding: Encoding) -> Recipe {
    Recipe {
        encoding,
        steps: vec![Step::new(effect, params, seed).unwrap()],
        ..Recipe::default()
    }
}

#[test]
fn every_fixture_decodes_at_its_size() {
    for (name, bytes) in fixtures() {
        let img = codec::decode(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let want = if name.ends_with(".tif") {
            (240, 160)
        } else {
            (480, 320)
        };
        assert_eq!((img.raster.width, img.raster.height), want, "{name}");
        assert_eq!(img.raster.bytes.len(), want.0 * want.1 * 3, "{name}");
    }
}

#[test]
fn orientations_come_out_upright() {
    let upright = decode("orient-1.jpg").raster;
    for o in 2..=8 {
        let name = format!("orient-{o}.jpg");
        let img = decode(&name);
        assert_eq!(img.orientation, o, "{name}");
        // JPEG blocks fall differently once turned, so allow a little noise.
        let d = mean_diff(&upright, &img.raster);
        assert!(d < 2.0, "{name}: mean difference {d:.2}");
    }
    // TIFF is lossless, so its turned copy must match exactly.
    let tif = decode("orient-6.tif");
    assert_eq!(tif.orientation, 6);
    assert_eq!(tif.raster, decode("rgb8.tif").raster);
}

#[test]
fn lossless_variants_agree_exactly() {
    let png = decode("rgb8.png").raster;
    // 16-bit values v * 257 round back to v.
    assert_eq!(decode("rgb16.png").raster, png);
    // Alpha is dropped, colour kept.
    assert_eq!(decode("rgba8.png").raster, png);
    let tif = decode("rgb8.tif").raster;
    assert_eq!(decode("lzw.tif").raster, tif);
    assert_eq!(decode("rgb16.tif").raster, tif);
    // Grayscale and palette images come out as RGB.
    let g = decode("gray8.png").raster;
    assert!(
        g.bytes
            .chunks_exact(3)
            .all(|p| p[0] == p[1] && p[1] == p[2])
    );
    assert!(mean_diff(&decode("palette.png").raster, &png) < 40.0);
}

#[test]
fn colour_profiles_are_read() {
    let icc = b"glitcharaxia test profile; not a real ICC profile.";
    for name in ["icc-profile.jpg", "rgb8.png", "rgb8.tif"] {
        assert_eq!(decode(name).icc.as_deref(), Some(&icc[..]), "{name}");
    }
    assert!(decode("baseline-420.jpg").icc.is_none());
}

#[test]
fn cmyk_and_grayscale_jpegs_look_right() {
    let reference = decode("baseline-444.jpg").raster;
    let cmyk = decode("cmyk.jpg").raster;
    let d = mean_diff(&reference, &cmyk);
    assert!(d < 3.0, "cmyk differs by {d:.2}");
    let g = decode("grayscale.jpg").raster;
    assert!(
        g.bytes
            .chunks_exact(3)
            .all(|p| p[0] == p[1] && p[1] == p[2])
    );
}

/// Damaged files either decode (viewers would show them) or fail with an
/// error. They must never panic.
#[test]
fn damaged_files_never_panic() {
    for (name, bytes) in fixtures() {
        for cut in [8, 64, bytes.len() / 3, bytes.len() * 9 / 10] {
            let _ = codec::decode(&bytes[..cut.min(bytes.len())]);
        }
        let mut flipped = bytes.clone();
        for i in (20..flipped.len()).step_by(97) {
            flipped[i] ^= 0x5A;
        }
        let _ = codec::decode(&flipped);
        let _ = name;
    }
}

#[test]
fn every_effect_keeps_dimensions_and_stays_in_its_windows() {
    let base = decode("baseline-444.jpg").raster;
    let units = Units::full(base.width, base.height);
    for e in REGISTRY {
        // Masked runs too: a mask and opacity only soften a step's own
        // change, so they never write outside its windows either.
        for (enc, masked) in [
            (Encoding::U8, false),
            (Encoding::S16Be, false),
            (Encoding::S24Le, false),
            (Encoding::U8, true),
        ] {
            let mut r = recipe(
                e.id,
                if masked {
                    "hits=5,len=3%,opacity=0.8"
                } else {
                    "hits=5,len=3%"
                },
                7,
                enc,
            );
            if masked {
                r.steps[0].mask = Some(busy_mask());
            }
            let step = &r.prepare_all().unwrap()[0];
            let mut out = base.clone();
            r.apply(&mut out).unwrap();
            assert_eq!(out.bytes.len(), base.bytes.len());
            let mut inside = vec![false; base.bytes.len()];
            for w in step.placement.windows(&units, step.frame(), step.seed) {
                inside[w].iter_mut().for_each(|b| *b = true);
            }
            for (i, (a, b)) in base.bytes.iter().zip(&out.bytes).enumerate() {
                assert!(
                    inside[i] || a == b,
                    "{} ({enc}, masked {masked}) wrote byte {i} outside its windows",
                    e.id
                );
            }
            assert_ne!(
                out, base,
                "{} ({enc}, masked {masked}) changed nothing",
                e.id
            );
        }
    }
}

/// A new seed changes the result exactly when there is something random:
/// scattered hits, or an effect that draws inside its windows (shuffle).
/// With `hits=0` the GUI's dice button is disabled, so this must hold.
#[test]
fn seeds_matter_exactly_when_something_is_random() {
    let base = decode("baseline-444.jpg").raster;
    let render = |id: &str, params: &str, seed: u64| {
        let mut out = base.clone();
        recipe(id, params, seed, Encoding::U8)
            .apply(&mut out)
            .unwrap();
        out
    };
    for e in REGISTRY {
        for params in ["hits=0", "hits=4,len=5%"] {
            let random = e.random_within || params != "hits=0";
            let differs = render(e.id, params, 1) != render(e.id, params, 2);
            assert_eq!(differs, random, "{} with {params}", e.id);
        }
    }
}

/// BLAKE3 of decode-then-bend for every effect at its defaults in every
/// encoding (and `u8` in stereo and in three channels). Any change here means the output for an
/// existing recipe changed: bump that effect's `version`.
#[test]
fn golden_hashes() {
    let base = decode("baseline-420.jpg").raster;
    let mut hashes = BTreeMap::new();
    hashes.insert(
        "decoded".to_string(),
        blake3::hash(&base.bytes).to_hex().to_string(),
    );
    for e in REGISTRY {
        for enc in Encoding::ALL {
            let mut out = base.clone();
            recipe(e.id, "", 0xC0FFEE, enc).apply(&mut out).unwrap();
            hashes.insert(
                format!("{}/{enc}", e.id),
                blake3::hash(&out.bytes).to_hex().to_string(),
            );
        }
        let mut out = base.clone();
        let mut r = recipe(e.id, "", 0xC0FFEE, Encoding::U8);
        r.channels = 2;
        r.apply(&mut out).unwrap();
        hashes.insert(
            format!("{}/u8-stereo", e.id),
            blake3::hash(&out.bytes).to_hex().to_string(),
        );
        let mut out = base.clone();
        let mut r = recipe(e.id, "", 0xC0FFEE, Encoding::U8);
        r.channels = 3;
        r.apply(&mut out).unwrap();
        hashes.insert(
            format!("{}/u8-rgb", e.id),
            blake3::hash(&out.bytes).to_hex().to_string(),
        );
    }
    // Masks and opacity are part of the determinism contract too.
    let mut out = base.clone();
    let mut r = recipe("bitcrush", "opacity=0.7", 0xC0FFEE, Encoding::U8);
    r.steps[0].mask = Some(busy_mask());
    r.apply(&mut out).unwrap();
    hashes.insert(
        "mask/bitcrush".to_string(),
        blake3::hash(&out.bytes).to_hex().to_string(),
    );
    insta::assert_yaml_snapshot!(hashes);
}

/// A mask is drawn in picture coordinates, so a preview-sized mask lands
/// where the full-size one does. Invert at full strength makes any
/// misplacement show up as a large error.
#[test]
fn masked_previews_resemble_the_export() {
    let full = decode("baseline-444.jpg").raster;
    let (pw, ph) = (full.width / 4, full.height / 4);
    let small = full.downscale(pw, ph);
    let units = Units {
        full_width: full.width,
        full_height: full.height,
        width: small.width,
        height: small.height,
    };
    let mut r = recipe("invert", "hits=0", 1, Encoding::U8);
    r.steps[0].mask = Some(busy_mask());
    let mut export = full.clone();
    r.apply(&mut export).unwrap();
    let export = export.downscale(pw, ph);
    let mut preview = small.clone();
    r.apply_with(&mut preview.bytes, &units, glitchrx_core::recipe::NEVER)
        .unwrap();
    let strength = mean_diff(&small, &export);
    let error = mean_diff(&preview, &export);
    println!("masked invert: strength {strength:6.2}  preview error {error:6.2}");
    assert!(
        strength > 20.0,
        "the mask hid too much to test: {strength:.2}"
    );
    assert!(
        error < 4.0,
        "preview error {error:.2} vs strength {strength:.2}"
    );
}

/// A step run on a quarter-size preview should look like the full-size
/// result scaled down (SPEC D-26). The bound is loose on purpose: previews
/// are "close, not identical". The measured differences are printed so the
/// threshold can be tightened (`cargo test -- --nocapture`).
///
/// Two limits are inherent to scaled previews. A delay that wraps past the
/// end of a row moves its ghost down one row, and a preview row stands for
/// several full rows, so long feedback chains (`reverb`) drift. And sizes
/// below a preview row (`shuffle` blocks of half a row) leave fewer pieces
/// in the preview, so a seeded permutation of them cannot match. Effects
/// that cannot be previewed at scale at all set `exact_preview`.
#[test]
fn previews_resemble_the_export() {
    let full = decode("baseline-444.jpg").raster;
    let (pw, ph) = (full.width / 4, full.height / 4);
    let small = full.downscale(pw, ph);
    let units = Units {
        full_width: full.width,
        full_height: full.height,
        width: small.width,
        height: small.height,
    };
    for e in REGISTRY {
        let r = recipe(e.id, "", 11, Encoding::U8);
        let mut export = full.clone();
        r.apply(&mut export).unwrap();
        let export = export.downscale(pw, ph);
        let mut preview = small.clone();
        r.apply_with(&mut preview.bytes, &units, glitchrx_core::recipe::NEVER)
            .unwrap();
        // How far the effect moves the picture, and how far the preview is
        // from the truth. A useful preview is much closer than the effect
        // is strong.
        let strength = mean_diff(&small, &export);
        let error = mean_diff(&preview, &export);
        println!(
            "{:>10}: strength {strength:6.2}  preview error {error:6.2}{}",
            e.id,
            if e.exact_preview {
                "  (previewed exactly)"
            } else {
                ""
            }
        );
        if e.exact_preview {
            // Flagged effects are rendered at full resolution by the GUI
            // once the user pauses; see `EffectInfo::exact_preview`.
            continue;
        }
        assert!(
            error < 12.0,
            "{}: preview error {error:.2} vs strength {strength:.2}",
            e.id
        );
    }
}
