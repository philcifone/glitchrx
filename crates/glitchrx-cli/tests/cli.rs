//! End-to-end tests of the `glitchrx` binary: naming, file safety, reverse,
//! recipes, exit codes.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use glitchrx_core::turbo::{Scale, decode};
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
    for name in [
        "photo-glitch.jpg",
        "photo-glitch-1.jpg",
        "photo-glitch-2.jpg",
    ] {
        assert!(d.path().join(name).exists(), "{name} missing");
    }
    // Glitching a glitched file continues the sequence.
    let o = run(d.path(), &["--shift", "photo-glitch-1.jpg"]);
    assert_eq!(code(&o), 0);
    assert!(d.path().join("photo-glitch-3.jpg").exists());
    assert!(!d.path().join("photo-glitch-1-glitch.jpg").exists());
    assert_eq!(fs::read(d.path().join("photo.jpg")).unwrap(), original);
}

#[test]
fn second_positional_is_the_output() {
    let d = workdir();
    let o = run(d.path(), &["--echo", "photo.jpg", "mine.jpg"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(d.path().join("mine.jpg").exists());
    assert!(!d.path().join("photo-glitch.jpg").exists());
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
fn reverse_restores_the_original_bytes() {
    // Default run: restart markers are added first, so the restored file has
    // the original pixels exactly, but not the original bytes (it keeps the
    // markers). With `--restart keep` the bytes come back exactly too.
    for keep in [false, true] {
        let d = workdir();
        let original = fs::read(d.path().join("photo.jpg")).unwrap();
        let mut args = vec!["--echo", "delay=300,taps=2", "--shuffle", "--seed", "4242"];
        if keep {
            args.extend(["--restart", "keep"]);
        }
        args.push("photo.jpg");
        let o = run(d.path(), &args);
        assert_eq!(code(&o), 0, "{}", stderr(&o));
        // The printed undo command must be exactly what works.
        let out = stdout(&o);
        let undo = out
            .lines()
            .find_map(|l| l.trim().strip_prefix("undo: glitchrx "))
            .expect("undo hint printed");
        let args: Vec<&str> = undo.split_whitespace().collect();
        let o = run(d.path(), &args);
        assert_eq!(code(&o), 0, "{}", stderr(&o));
        let restored = fs::read(d.path().join("photo-restored.jpg")).unwrap();
        if keep {
            assert!(restored == original, "restored bytes differ");
        } else {
            let px = |b: &[u8]| decode(b, Scale::Full).0.unwrap().rgba;
            assert!(px(&restored) == px(&original), "restored pixels differ");
            assert!(restored != original, "expected restart markers to be added");
        }
    }
}

#[test]
fn reverse_refuses_lossy_effects() {
    let d = workdir();
    let o = run(d.path(), &["--reverse", "--bitcrush", "photo.jpg"]);
    assert_eq!(code(&o), 3);
    assert!(stderr(&o).contains("bitcrush"));
}

#[test]
fn recipe_save_and_replay_is_identical() {
    let d = workdir();
    let o = run(
        d.path(),
        &[
            "--reverb",
            "--stutter",
            "chunk=32",
            "--save-recipe",
            "photo.jpg",
            "-o",
            "a.jpg",
        ],
    );
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(d.path().join("a.recipe.toml").exists());
    let o = run(
        d.path(),
        &["--recipe", "a.recipe.toml", "photo.jpg", "-o", "b.jpg"],
    );
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert_eq!(
        fs::read(d.path().join("a.jpg")).unwrap(),
        fs::read(d.path().join("b.jpg")).unwrap()
    );
}

#[test]
fn bake_writes_png() {
    let d = workdir();
    let o = run(d.path(), &["--echo", "--bake", "photo.jpg"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let png = fs::read(d.path().join("photo-glitch.png")).unwrap();
    assert!(png.starts_with(b"\x89PNG"));
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
    assert!(d.path().join("photo-glitch.jpg").exists());
    assert!(d.path().join("two-glitch.jpg").exists());
}

#[test]
fn usage_errors() {
    let d = workdir();
    assert_eq!(code(&run(d.path(), &["photo.jpg"])), 2, "no effect");
    assert_eq!(code(&run(d.path(), &["--echo", "gain=5", "photo.jpg"])), 2);
    assert_eq!(code(&run(d.path(), &["--echo", "bogus=1", "photo.jpg"])), 2);
    assert_eq!(code(&run(d.path(), &["--nosuch", "photo.jpg"])), 2);
}

#[test]
fn non_jpeg_input_is_unsupported() {
    let d = workdir();
    fs::write(d.path().join("x.png"), b"\x89PNG\r\n\x1a\n").unwrap();
    assert_eq!(code(&run(d.path(), &["--echo", "x.png"])), 4);
}

#[test]
fn info_and_list() {
    let d = workdir();
    let o = run(d.path(), &["--info", "photo.jpg"]);
    assert_eq!(code(&o), 0);
    assert!(stdout(&o).contains("480 x 320"));
    let o = run(d.path(), &["--list"]);
    assert_eq!(code(&o), 0);
    for e in [
        "echo",
        "reverb",
        "shift",
        "backwards",
        "shuffle",
        "amplify",
        "bitcrush",
        "stutter",
    ] {
        assert!(
            stdout(&o).contains(&format!("--{e}")),
            "{e} missing from --list"
        );
    }
}
