//! End-to-end tests of the `glitchrx` binary: naming, formats, file
//! safety, recipes, encodings, exit codes.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use glitchrx_core::codec::{self, Format};
use tempfile::TempDir;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/baseline-420.jpg")
}

/// Temp dir containing `photo.jpg` (a copy of a fixture).
fn workdir() -> TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::copy(fixture(), d.path().join("photo.jpg")).unwrap();
    d
}

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_glitchrx"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn code(o: &Output) -> i32 {
    o.status.code().unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn auto_names_increment_and_input_is_untouched() {
    let d = workdir();
    let original = fs::read(d.path().join("photo.jpg")).unwrap();
    for _ in 0..3 {
        let o = run(d.path(), &["--echo", "photo.jpg"]);
        assert_eq!(code(&o), 0, "{}", stderr(&o));
    }
    // TIFF by default.
    for name in [
        "photo-glitch.tif",
        "photo-glitch-1.tif",
        "photo-glitch-2.tif",
    ] {
        assert!(d.path().join(name).exists(), "{name} missing");
    }
    // Bending a bent file continues the sequence, across formats.
    let o = run(
        d.path(),
        &["--shift", "--format", "png", "photo-glitch-1.tif"],
    );
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(d.path().join("photo-glitch-3.png").exists());
    assert!(!d.path().join("photo-glitch-1-glitch.png").exists());
    assert_eq!(fs::read(d.path().join("photo.jpg")).unwrap(), original);
}

#[test]
fn second_positional_is_the_output() {
    let d = workdir();
    let o = run(d.path(), &["--echo", "photo.jpg", "mine.jpg"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(d.path().join("mine.jpg").exists());
    assert!(!d.path().join("photo-glitch.tif").exists());
}

#[test]
fn output_extension_decides_the_format() {
    let d = workdir();
    for (name, f) in [
        ("a.tif", Format::Tiff),
        ("b.png", Format::Png),
        ("c.jpeg", Format::Jpeg),
    ] {
        let o = run(d.path(), &["--echo", "photo.jpg", name]);
        assert_eq!(code(&o), 0, "{name}: {}", stderr(&o));
        let bytes = fs::read(d.path().join(name)).unwrap();
        assert_eq!(Format::sniff(&bytes), Some(f), "{name}");
        let img = codec::decode(&bytes).unwrap();
        assert_eq!((img.raster.width, img.raster.height), (480, 320));
    }
    // A contradicting --format and an unknown extension are usage errors.
    assert_eq!(
        code(&run(
            d.path(),
            &["--echo", "--format", "png", "photo.jpg", "x.tif"]
        )),
        2
    );
    assert_eq!(code(&run(d.path(), &["--echo", "photo.jpg", "x.webp"])), 2);
}

#[test]
fn every_input_format_is_read() {
    let d = workdir();
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    for name in ["rgb16.png", "lzw.tif", "cmyk.jpg", "orient-6.jpg"] {
        fs::copy(fixtures.join(name), d.path().join(name)).unwrap();
        let o = run(d.path(), &["--invert", "hits=0", "--format", "png", name]);
        assert_eq!(code(&o), 0, "{name}: {}", stderr(&o));
    }
}

#[test]
fn existing_output_is_refused_without_force() {
    let d = workdir();
    fs::write(d.path().join("taken.jpg"), b"keep me").unwrap();
    let o = run(d.path(), &["--echo", "photo.jpg", "-o", "taken.jpg"]);
    assert_eq!(code(&o), 3);
    assert_eq!(fs::read(d.path().join("taken.jpg")).unwrap(), b"keep me");
    let o = run(
        d.path(),
        &["--echo", "photo.jpg", "-o", "taken.jpg", "--force"],
    );
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert_ne!(fs::read(d.path().join("taken.jpg")).unwrap(), b"keep me");
}

#[test]
fn input_is_refused_even_with_force() {
    let d = workdir();
    let original = fs::read(d.path().join("photo.jpg")).unwrap();
    let o = run(d.path(), &["--echo", "photo.jpg", "photo.jpg", "--force"]);
    assert_eq!(code(&o), 3, "{}", stderr(&o));
    assert!(stderr(&o).contains("input"));

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("photo.jpg", d.path().join("link.jpg")).unwrap();
        fs::hard_link(d.path().join("photo.jpg"), d.path().join("hard.jpg")).unwrap();
        for target in ["link.jpg", "hard.jpg"] {
            let o = run(d.path(), &["--echo", "photo.jpg", "-o", target, "--force"]);
            assert_eq!(code(&o), 3, "{target}: {}", stderr(&o));
        }
    }
    assert_eq!(fs::read(d.path().join("photo.jpg")).unwrap(), original);
}

#[test]
fn recipe_save_and_replay_is_identical() {
    let d = workdir();
    let o = run(
        d.path(),
        &[
            "--reverb",
            "--stutter",
            "chunk=32,encoding=s16be",
            "--encoding",
            "ulaw",
            "--save-recipe",
            "photo.jpg",
            "-o",
            "a.tif",
        ],
    );
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let recipe = fs::read_to_string(d.path().join("a.recipe.toml")).unwrap();
    assert!(recipe.contains("encoding = \"ulaw\""), "{recipe}");
    assert!(recipe.contains("encoding = \"s16be\""), "{recipe}");
    let o = run(
        d.path(),
        &["--recipe", "a.recipe.toml", "photo.jpg", "-o", "b.tif"],
    );
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert_eq!(
        fs::read(d.path().join("a.tif")).unwrap(),
        fs::read(d.path().join("b.tif")).unwrap()
    );
    // --encoding on replay changes the recipe-wide encoding (not the
    // step's own override), so the result differs.
    let o = run(
        d.path(),
        &[
            "--recipe",
            "a.recipe.toml",
            "--encoding",
            "u8",
            "photo.jpg",
            "-o",
            "c.tif",
        ],
    );
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert_ne!(
        fs::read(d.path().join("a.tif")).unwrap(),
        fs::read(d.path().join("c.tif")).unwrap()
    );
}

#[test]
fn same_seed_same_output() {
    let d = workdir();
    for out in ["a.png", "b.png"] {
        let o = run(
            d.path(),
            &["--shuffle", "--echo", "--seed", "77", "photo.jpg", out],
        );
        assert_eq!(code(&o), 0, "{}", stderr(&o));
        assert!(stdout(&o).contains("seed 77"));
    }
    assert_eq!(
        fs::read(d.path().join("a.png")).unwrap(),
        fs::read(d.path().join("b.png")).unwrap()
    );
}

#[test]
fn version_1_recipes_are_refused_with_a_pointer() {
    let d = workdir();
    fs::write(
        d.path().join("old.toml"),
        "format = \"glitcharaxia-recipe\"\nversion = 1\nmode = \"safe\"\n",
    )
    .unwrap();
    let o = run(d.path(), &["--recipe", "old.toml", "photo.jpg"]);
    assert_eq!(code(&o), 1);
    assert!(stderr(&o).contains("archive/jpeg-bytes"), "{}", stderr(&o));
}

#[test]
fn batch_mode() {
    let d = workdir();
    fs::copy(fixture(), d.path().join("two.jpg")).unwrap();
    let o = run(
        d.path(),
        &["--backwards", "--batch", "photo.jpg", "two.jpg"],
    );
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(d.path().join("photo-glitch.tif").exists());
    assert!(d.path().join("two-glitch.tif").exists());
}

#[test]
fn usage_errors() {
    let d = workdir();
    assert_eq!(code(&run(d.path(), &["photo.jpg"])), 2, "no effect");
    assert_eq!(code(&run(d.path(), &["--echo", "decay=5", "photo.jpg"])), 2);
    assert_eq!(
        code(&run(d.path(), &["--echo", "encoding=mp3", "photo.jpg"])),
        2
    );
    assert_eq!(
        code(&run(d.path(), &["--echo", "--channels", "3", "photo.jpg"])),
        2
    );
    assert_eq!(
        code(&run(
            d.path(),
            &["--echo", "--encoding", "mp3", "photo.jpg"]
        )),
        2
    );
    // Retired 2a options.
    assert_eq!(
        code(&run(d.path(), &["--echo", "--reverse", "photo.jpg"])),
        2
    );
    assert_eq!(
        code(&run(d.path(), &["--echo", "--restart", "10%", "photo.jpg"])),
        2
    );
    assert_eq!(code(&run(d.path(), &["--echo", "bogus=1", "photo.jpg"])), 2);
    assert_eq!(code(&run(d.path(), &["--nosuch", "photo.jpg"])), 2);
}

#[test]
fn unknown_and_broken_inputs() {
    let d = workdir();
    fs::write(d.path().join("x.gif"), b"GIF89a....").unwrap();
    assert_eq!(code(&run(d.path(), &["--echo", "x.gif"])), 4);
    fs::write(d.path().join("x.png"), b"\x89PNG\r\n\x1a\n").unwrap();
    assert_eq!(code(&run(d.path(), &["--echo", "x.png"])), 1);
}

#[test]
fn info_and_list() {
    let d = workdir();
    let o = run(d.path(), &["--info", "photo.jpg"]);
    assert_eq!(code(&o), 0);
    assert!(stdout(&o).contains("480 x 320"));
    assert!(stdout(&o).contains("jpeg"));
    let o = run(d.path(), &["--list"]);
    assert_eq!(code(&o), 0);
    for e in [
        "echo",
        "delay",
        "reverb",
        "amplify",
        "invert",
        "bitcrush",
        "backwards",
        "shift",
        "shuffle",
        "stutter",
    ] {
        assert!(
            stdout(&o).contains(&format!("--{e}")),
            "{e} missing from --list"
        );
    }
}
