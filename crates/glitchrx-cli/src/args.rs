//! Command-line parsing.
//!
//! Effect flags are handled by a small pre-pass, everything else by clap.
//! The pre-pass exists because an effect flag takes an *optional* parameter
//! list: in `glitchrx --echo photo.jpg` the word after `--echo` is the input,
//! while in `glitchrx --echo delay=400 photo.jpg` it is the parameters. clap
//! cannot tell those apart (it would swallow `photo.jpg` as the value), but
//! the rule is simple: the next argument is a parameter list if it contains
//! `=`, does not start with `-`, and is not an existing file.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use clap::{ArgAction, Parser, ValueEnum};
use glitchrx_core::Encoding;
use glitchrx_core::codec::Format;
use glitchrx_core::effects::REGISTRY;

/// One effect flag from the command line, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectArg {
    pub id: &'static str,
    pub params: String,
}

/// Pull effect flags (and their parameter lists) out of `args`, returning
/// them in command-line order plus the remaining arguments for clap.
pub fn split_effects(args: Vec<OsString>) -> (Vec<EffectArg>, Vec<OsString>) {
    let mut effects = Vec::new();
    let mut rest = Vec::new();
    let mut it = args.into_iter().peekable();
    while let Some(arg) = it.next() {
        let text = arg.to_string_lossy().into_owned();
        if text == "--" {
            // Everything after `--` is positional; hand it to clap untouched.
            rest.push(arg);
            rest.extend(it);
            break;
        }
        let Some(flag) = text.strip_prefix("--") else {
            rest.push(arg);
            continue;
        };
        let (name, inline) = match flag.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (flag, None),
        };
        let Some(info) = REGISTRY.iter().find(|e| e.id == name) else {
            rest.push(arg);
            continue;
        };
        let params = match inline {
            Some(v) => v,
            None => match it.peek() {
                Some(next) if looks_like_params(&next.to_string_lossy()) => {
                    it.next().unwrap().to_string_lossy().into_owned()
                }
                _ => String::new(),
            },
        };
        effects.push(EffectArg {
            id: info.id,
            params,
        });
    }
    (effects, rest)
}

fn looks_like_params(s: &str) -> bool {
    s.contains('=') && !s.starts_with('-') && !Path::new(s).exists()
}

/// Output formats for `--format`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum FormatArg {
    /// Uncompressed 8-bit RGB TIFF, for Photoshop (the default).
    Tiff,
    /// Lossless PNG.
    Png,
    /// JPEG, full chroma (quality 95 unless --quality).
    Jpeg,
    /// WebP: lossy (quality 90 unless --quality) or --lossless.
    Webp,
    /// AVIF (quality 80, speed 6 unless --quality, --avif-speed).
    Avif,
}

impl From<FormatArg> for Format {
    fn from(f: FormatArg) -> Format {
        match f {
            FormatArg::Tiff => Format::Tiff,
            FormatArg::Png => Format::Png,
            FormatArg::Jpeg => Format::Jpeg,
            FormatArg::Webp => Format::Webp,
            FormatArg::Avif => Format::Avif,
        }
    }
}

/// Everything except the effect flags.
#[derive(Debug, Parser)]
#[command(
    name = "glitchrx",
    version,
    about = "Databending: glitch art from photos, by running Audacity-style audio effects over their raw pixels.",
    long_about = None,
    override_usage = "glitchrx [--EFFECT [k=v,...]]... [OPTIONS] INPUT [OUTPUT]\n       \
                      glitchrx [--EFFECT [k=v,...]]... [OPTIONS] --batch INPUT...\n       \
                      glitchrx --info INPUT...\n       \
                      glitchrx --list\n       \
                      glitchrx serve [--host ADDR] [--port N] [--root DIR] [--token[=SECRET]]",
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Input file(s). With exactly two and no --batch, the second is the output.
    #[arg(value_name = "FILE")]
    pub files: Vec<PathBuf>,

    /// Output file (single input only). Default: INPUT-glitch.ext, -glitch-1, ...
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Master seed for all randomness. Default: random, always printed.
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u64).range(0..=i64::MAX as u64))]
    pub seed: Option<u64>,

    /// Apply a saved recipe instead of effect flags.
    #[arg(long, value_name = "FILE")]
    pub recipe: Option<PathBuf>,

    /// Also write the recipe used [default: OUTPUT-STEM.recipe.toml]
    #[arg(long, value_name = "FILE", num_args = 0..=1, require_equals = true)]
    pub save_recipe: Option<Option<PathBuf>>,

    /// Output format when no output file is named [default: tiff].
    /// A named output's extension decides its format.
    #[arg(long, value_enum, value_name = "FORMAT")]
    pub format: Option<FormatArg>,

    /// Quality for JPEG, lossy WebP and AVIF, 1 to 100. Lossy formats
    /// smooth fine glitch texture; TIFF, PNG and lossless WebP keep it.
    #[arg(long, value_name = "1-100", value_parser = clap::value_parser!(u8).range(1..=100))]
    pub quality: Option<u8>,

    /// WebP: lossless instead of lossy.
    #[arg(long)]
    pub lossless: bool,

    /// AVIF: 1 (slowest, smallest file) to 10 (fastest) [default: 6].
    /// (Not `--speed`, which is the Change Speed effect.)
    #[arg(long, value_name = "1-10", value_parser = clap::value_parser!(u8).range(1..=10))]
    pub avif_speed: Option<u8>,

    /// Run the steps on a copy whose longer side is N pixels (never larger
    /// than the photo): faster, with coarser glitch texture. The output is
    /// that size, unless --scale-back.
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(64..))]
    pub work_size: Option<u32>,

    /// With --work-size: enlarge the result back to the photo's size, with
    /// hard pixel edges (nearest neighbour).
    #[arg(long)]
    pub scale_back: bool,

    /// How steps read the pixels as audio samples [default: u8]. A step can
    /// override it with encoding=... in its parameters.
    #[arg(long, value_name = "ENC", value_parser = parse_encoding)]
    pub encoding: Option<Encoding>,

    /// 1 (mono), 2 (stereo: alternate samples are left and right) or 3 (one
    /// channel per colour, like importing raw data with 3 channels) [default: 1].
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u8).range(1..=3))]
    pub channels: Option<u8>,

    /// Allow overwriting an existing output file (never the input).
    #[arg(long)]
    pub force: bool,

    /// Treat every FILE as an input, with automatic output names.
    #[arg(long)]
    pub batch: bool,

    /// Show format, size, orientation and colour profile of the input(s), then exit.
    #[arg(long)]
    pub info: bool,

    /// List every effect with its parameters and exit.
    #[arg(long)]
    pub list: bool,

    /// Print nothing except errors.
    #[arg(short, long, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Print more detail (step seeds and encodings).
    #[arg(short, long, action = ArgAction::SetTrue)]
    pub verbose: bool,
}

fn parse_encoding(s: &str) -> Result<Encoding, String> {
    Encoding::parse(s).map_err(|e| e.to_string())
}

/// Text appended to `--help`: the effect list, encodings and examples.
pub fn after_help() -> String {
    let mut s = String::from("Effects (chainable; applied in command-line order):\n");
    for e in REGISTRY {
        s.push_str(&format!("  --{:<10} {}\n", e.id, e.summary));
    }
    s.push_str("\nEncodings (--encoding, or encoding=... per effect):\n");
    for e in Encoding::ALL {
        s.push_str(&format!("  {:<7} {}\n", e.name(), e.describe()));
    }
    s.push_str(
        "\nEffect parameters follow the flag as key=value pairs, e.g. --echo delay=0.33rows,decay=0.6\n\
         Sizes take rows, px, % or bytes. Run `glitchrx --list` for every parameter.\n\
         \nExamples:\n  \
         glitchrx --echo photo.jpg                      writes photo-glitch.tif\n  \
         glitchrx --echo photo.jpg out.png              writes out.png\n  \
         glitchrx --reverb --echo phase=2 --encoding ulaw photo.jpg\n  \
         glitchrx --shift by=0.5rows,hits=12 --format jpeg photo.jpg\n  \
         glitchrx --recipe moody.toml --batch *.jpg\n  \
         glitchrx --info photo.jpg\n  \
         glitchrx serve --root ~/Pictures               web GUI (see glitchrx serve --help)\n\
         \nOriginals are never modified, and existing files are never overwritten without --force.",
    );
    s
}

/// Full parameter reference for `--list`.
pub fn list_text() -> String {
    let mut s = String::new();
    for e in REGISTRY {
        s.push_str(&format!(
            "--{}\n  {}\n  follows Audacity's {}\n",
            e.id, e.summary, e.audacity
        ));
        for p in e.all_params() {
            s.push_str(&format!(
                "    {:<10} {:<10} {}\n",
                p.name, p.default, p.help
            ));
        }
        s.push_str(&format!("  example: {}\n\n", e.example));
    }
    s.push_str(
        "Every effect also takes encoding=ENC and channels=1|2|3 to override the recipe's.\n\
         Columns: parameter, default, meaning.\n",
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(v: &[&str]) -> Vec<OsString> {
        v.iter().map(OsString::from).collect()
    }

    #[test]
    fn plain_effect_flag_does_not_eat_input() {
        let (e, rest) = split_effects(os(&["--echo", "photo.jpg"]));
        assert_eq!(
            e,
            vec![EffectArg {
                id: "echo",
                params: String::new()
            }]
        );
        assert_eq!(rest, os(&["photo.jpg"]));
    }

    #[test]
    fn params_and_order_are_kept() {
        let (e, rest) = split_effects(os(&[
            "--shift",
            "by=5",
            "--seed",
            "3",
            "--echo=delay=9,decay=0.1",
            "in.jpg",
        ]));
        assert_eq!(
            e[0],
            EffectArg {
                id: "shift",
                params: "by=5".into()
            }
        );
        assert_eq!(
            e[1],
            EffectArg {
                id: "echo",
                params: "delay=9,decay=0.1".into()
            }
        );
        assert_eq!(rest, os(&["--seed", "3", "in.jpg"]));
    }

    #[test]
    fn other_flags_are_not_effects() {
        let (e, rest) = split_effects(os(&["--format", "png", "--echo", "x.jpg"]));
        assert_eq!(e.len(), 1);
        assert_eq!(rest, os(&["--format", "png", "x.jpg"]));
    }

    #[test]
    fn double_dash_stops_effect_parsing() {
        let (e, rest) = split_effects(os(&["--", "--echo"]));
        assert!(e.is_empty());
        assert_eq!(rest, os(&["--", "--echo"]));
    }
}
