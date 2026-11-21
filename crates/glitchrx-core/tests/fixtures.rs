//! Whole-file tests over the committed fixtures: the header-safe guarantee,
//! byte stuffing, exact reversal, decoder robustness and determinism.

use std::collections::BTreeMap;
use std::path::PathBuf;

use glitchrx_core::effects::REGISTRY;
use glitchrx_core::turbo::{DecodeStatus, Scale, decode};
use glitchrx_core::{Jpeg, Mode, Recipe, Step};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

/// Every fixture as `(name, bytes)`.
fn fixtures() -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<_> = std::fs::read_dir(fixture_dir())
        .expect("tests/fixtures missing; run `just fixtures`")
        .filter_map(|e| {
            let p = e.ok()?.path();
            (p.extension()? == "jpg").then(|| {
                let name = p.file_name().unwrap().to_string_lossy().into_owned();
                (name, std::fs::read(&p).unwrap())
            })
        })
        .collect();
    v.sort();
    assert!(v.len() >= 9, "expected the full fixture set");
    v
}

/// A recipe with one step of `effect` at its defaults, plus extra params.
fn recipe(effect: &str, params: &str, seed: u64, mode: Mode) -> Recipe {
    Recipe {
        mode,
        steps: vec![Step::new(effect, params, seed).unwrap()],
        ..Recipe::default()
    }
}

/// Heavier settings than the defaults so every effect visibly changes the
/// payload even on small fixtures.
fn strong(effect: &str) -> &'static str {
    match effect {
        "amplify" => "hits=8,len=2%",
        "shuffle" => "hits=4,len=10%,block=64",
        _ => "hits=8,len=2%",
    }
}

#[test]
fn parse_and_finish_is_identity() {
    for (name, bytes) in fixtures() {
        for mode in [Mode::Safe, Mode::Raw] {
            let j = Jpeg::parse(bytes.clone(), mode).unwrap();
            assert_eq!(
                j.finish(),
                bytes,
                "{name} ({mode:?}) changed without any effect"
            );
        }
    }
}

#[test]
fn fixture_layouts_are_detected() {
    let all: BTreeMap<_, _> = fixtures().into_iter().collect();
    let info = |n: &str| {
        Jpeg::parse(all[n].clone(), Mode::Safe)
            .unwrap()
            .info()
            .clone()
    };
    assert_eq!(info("baseline-420.jpg").frame.subsampling(), "4:2:0");
    assert_eq!(info("baseline-422.jpg").frame.subsampling(), "4:2:2");
    assert_eq!(info("baseline-444.jpg").frame.subsampling(), "4:4:4");
    assert_eq!(info("grayscale.jpg").frame.subsampling(), "grayscale");
    assert!(info("progressive-420.jpg").scans > 1);
    assert!(info("restart-420.jpg").restart_markers > 0);
    assert!(info("exif-thumbnail.jpg").has_exif_thumbnail);
    assert!(info("trailing-data.jpg").trailing_len > 0);
    let b = info("baseline-420.jpg");
    assert_eq!((b.frame.width, b.frame.height), (480, 320));
    assert!(!b.missing_eoi);
}

#[test]
fn header_is_byte_identical_after_every_effect() {
    for (name, bytes) in fixtures() {
        let original = Jpeg::parse(bytes.clone(), Mode::Safe).unwrap();
        for e in REGISTRY {
            let out = recipe(e.id, strong(e.id), 7, Mode::Safe)
                .apply(bytes.clone())
                .unwrap();
            assert_ne!(out, bytes, "{name}/{}: nothing changed", e.id);
            let glitched = Jpeg::parse(out, Mode::Safe).unwrap();
            assert_eq!(
                glitched.structure(),
                original.structure(),
                "{name}/{}: structure changed",
                e.id
            );
            assert_eq!(glitched.payload().len(), original.payload().len());
        }
    }
}

#[test]
fn safe_mode_never_leaves_unstuffed_ff() {
    for (name, bytes) in fixtures() {
        for e in REGISTRY {
            let out = recipe(e.id, strong(e.id), 11, Mode::Safe)
                .apply(bytes.clone())
                .unwrap();
            let j = Jpeg::parse(out.clone(), Mode::Safe).unwrap();
            for run in j.run_ranges() {
                let data = &out[run];
                for (i, w) in data.windows(2).enumerate() {
                    if w[0] == 0xFF {
                        assert_eq!(w[1], 0x00, "{name}/{}: bare FF at run offset {i}", e.id);
                    }
                }
                if let Some(&last) = data.last() {
                    assert_ne!(last, 0xFF, "{name}/{}: run ends in a bare FF", e.id);
                }
            }
        }
    }
}

#[test]
fn reversible_effects_restore_the_exact_file() {
    for (name, bytes) in fixtures() {
        for e in REGISTRY {
            let r = recipe(e.id, strong(e.id), 3, Mode::Safe);
            if !r.irreversible_parts().unwrap().is_empty() {
                continue;
            }
            let glitched = r.apply(bytes.clone()).unwrap();
            let restored = r.invert(glitched).unwrap();
            assert!(
                restored == bytes,
                "{name}/{}: reverse did not restore",
                e.id
            );
        }
    }
}

#[test]
fn chained_recipe_round_trips() {
    let bytes = std::fs::read(fixture_dir().join("restart-420.jpg")).unwrap();
    let mut r = Recipe::default();
    for (e, p) in [
        ("echo", "taps=3"),
        ("shift", "by=-500"),
        ("shuffle", ""),
        ("amplify", "factor=-3"),
        ("reverb", ""),
        ("backwards", ""),
    ] {
        r.steps.push(Step::new(e, p, 0).unwrap());
    }
    r.reseed(99);
    let glitched = r.apply(bytes.clone()).unwrap();
    assert_ne!(glitched, bytes);
    assert!(r.invert(glitched).unwrap() == bytes);
}

#[test]
fn irreversible_recipes_are_refused() {
    let bytes = std::fs::read(fixture_dir().join("baseline-420.jpg")).unwrap();
    assert!(
        recipe("bitcrush", "", 1, Mode::Safe)
            .invert(bytes.clone())
            .is_err()
    );
    assert!(recipe("echo", "", 1, Mode::Raw).invert(bytes).is_err());
}

#[test]
fn decoder_never_panics_on_glitched_corpus() {
    // Raw and safe mode, every effect, extreme settings, several seeds.
    let mut statuses: BTreeMap<&str, u32> = BTreeMap::new();
    for (name, bytes) in fixtures() {
        let (_, clean) = decode(&bytes, Scale::Full);
        assert_eq!(clean, DecodeStatus::Clean, "{name} should decode cleanly");
        for e in REGISTRY {
            for (mode, seed) in [(Mode::Safe, 1), (Mode::Raw, 2), (Mode::Raw, 3)] {
                let r = recipe(e.id, "hits=0,region=1%:100%", seed, mode);
                let Ok(out) = r.apply(bytes.clone()) else {
                    continue; // e.g. amplify defaults are fine; nothing should fail
                };
                for scale in [Scale::Full, Scale::Eighth] {
                    let (img, status) = decode(&out, scale);
                    if let Some(img) = &img {
                        assert_eq!(img.rgba.len(), (img.width * img.height * 4) as usize);
                    }
                    *statuses.entry(status.word()).or_default() += 1;
                }
            }
        }
    }
    // Printed with --nocapture; useful when revisiting the decoder decision.
    println!("decode statuses over glitched corpus: {statuses:?}");
}

#[test]
fn golden_hashes() {
    // Determinism: these bytes must never change for a given effect version.
    // Snapshot updates need an effect VERSION bump or a written justification.
    let bytes = std::fs::read(fixture_dir().join("baseline-420.jpg")).unwrap();
    let mut hashes = BTreeMap::new();
    for e in REGISTRY {
        let out = recipe(e.id, "", 42, Mode::Safe)
            .apply(bytes.clone())
            .unwrap();
        hashes.insert(e.id, blake3::hash(&out).to_hex().to_string());
    }
    insta::assert_yaml_snapshot!(hashes);
}
