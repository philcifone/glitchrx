//! `glitchrx`: databending from the command line.
//!
//! A thin front end over `glitchrx-core`: parse arguments, build a recipe,
//! decode each input, bend it, encode and write the output safely, print one
//! summary line per file. All the real work lives in the core so the GUI can share it.
//! `glitchrx serve` hands off to the web GUI (`glitchrx-serve`).

mod args;
mod serve_cmd;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{CommandFactory, FromArgMatches};
use glitchrx_core::codec::{self, Format};
use glitchrx_core::io::{WriteError, auto_output, write_output};
use glitchrx_core::{Error, Recipe, Step};

use args::{Cli, EffectArg};

/// Why the program stopped, mapped onto the documented exit codes.
enum Fail {
    /// 1: something went wrong while working (I/O, malformed file).
    Runtime(String),
    /// 2: the command line itself is wrong.
    Usage(String),
    /// 3: refused to do something unsafe (overwrite the input or a file).
    Refused(String),
    /// 4: the input is not an image we can handle.
    Unsupported(String),
}

impl Fail {
    fn code(&self) -> u8 {
        match self {
            Fail::Runtime(_) => 1,
            Fail::Usage(_) => 2,
            Fail::Refused(_) => 3,
            Fail::Unsupported(_) => 4,
        }
    }

    fn message(&self) -> &str {
        match self {
            Fail::Runtime(m) | Fail::Usage(m) | Fail::Refused(m) | Fail::Unsupported(m) => m,
        }
    }
}

impl From<Error> for Fail {
    fn from(e: Error) -> Fail {
        match e {
            Error::UnknownFormat | Error::Unsupported(_) => Fail::Unsupported(e.to_string()),
            Error::UnknownEffect(_) | Error::Param { .. } => Fail::Usage(e.to_string()),
            Error::Malformed(_) | Error::Recipe(_) | Error::Cancelled => {
                Fail::Runtime(e.to_string())
            }
        }
    }
}

impl From<WriteError> for Fail {
    fn from(e: WriteError) -> Fail {
        match e {
            WriteError::SameAsInput(_) | WriteError::Exists(_) => Fail::Refused(e.to_string()),
            WriteError::Io { .. } => Fail::Runtime(e.to_string()),
        }
    }
}

fn main() -> ExitCode {
    let mut argv: Vec<_> = std::env::args_os().skip(1).collect();
    if argv.first().is_some_and(|a| a == "serve") {
        argv.remove(0);
        return serve_cmd::main(argv);
    }
    let (effects, rest) = args::split_effects(argv);
    let cmd = Cli::command()
        .after_help(args::after_help())
        .arg_required_else_help(effects.is_empty());
    let matches = cmd.get_matches_from(std::iter::once("glitchrx".into()).chain(rest));
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(c) => c,
        Err(e) => e.exit(),
    };
    match run(&cli, &effects) {
        Ok(()) => ExitCode::SUCCESS,
        Err(f) => {
            eprintln!("glitchrx: {}", f.message());
            ExitCode::from(f.code())
        }
    }
}

/// One input and where its output goes (`None` = automatic name).
struct Job {
    input: PathBuf,
    output: Option<PathBuf>,
}

fn run(cli: &Cli, effects: &[EffectArg]) -> Result<(), Fail> {
    if cli.list {
        print!("{}", args::list_text());
        return Ok(());
    }
    if cli.files.is_empty() {
        return Err(Fail::Usage("no input file given (see --help)".into()));
    }
    if cli.info {
        for f in &cli.files {
            print_info(f)?;
        }
        return Ok(());
    }

    let (recipe, master_seed) = build_recipe(cli, effects)?;
    let jobs = plan_jobs(cli)?;
    if jobs.len() > 1 && matches!(cli.save_recipe, Some(Some(_))) {
        return Err(Fail::Usage(
            "--save-recipe=FILE takes a single input; omit FILE in batch mode".into(),
        ));
    }

    if cli.verbose {
        let prepared = recipe.prepare_all()?;
        for (i, (s, p)) in recipe.steps.iter().zip(&prepared).enumerate() {
            eprintln!(
                "step {}: {} {} ({}, {} channel{}, seed {})",
                i + 1,
                s.effect,
                s.params_list(),
                p.encoding,
                p.channels,
                if p.channels == 1 { "" } else { "s" },
                s.seed
            );
        }
    }

    // Keep going after a failed file in batch mode; report the worst error.
    let mut worst: Option<Fail> = None;
    for job in &jobs {
        if let Err(f) = process(cli, &recipe, master_seed, job) {
            if jobs.len() > 1 {
                eprintln!("glitchrx: {}: {}", job.input.display(), f.message());
            }
            if worst.as_ref().is_none_or(|w| f.code() > w.code()) {
                worst = Some(f);
            }
        }
    }
    match worst {
        Some(f) if jobs.len() > 1 => Err(Fail::Runtime(format!(
            "some files failed (last error: {})",
            f.message()
        ))),
        Some(f) => Err(f),
        None => Ok(()),
    }
}

/// Build the recipe from `--recipe` or the effect flags, with `--encoding`
/// and `--channels` applied recipe-wide. Returns the recipe and the master
/// seed (None when replaying a recipe file's own seeds).
fn build_recipe(cli: &Cli, effects: &[EffectArg]) -> Result<(Recipe, Option<u64>), Fail> {
    let (mut recipe, master) = if let Some(path) = &cli.recipe {
        if !effects.is_empty() {
            return Err(Fail::Usage(
                "use either --recipe or effect flags, not both".into(),
            ));
        }
        let text = fs::read_to_string(path)
            .map_err(|e| Fail::Runtime(format!("{}: {e}", path.display())))?;
        let mut r = Recipe::from_toml(&text)?;
        if let Some(seed) = cli.seed {
            r.reseed(seed);
        }
        (r, cli.seed)
    } else {
        if effects.is_empty() {
            return Err(Fail::Usage(
                "no effect given, e.g. `glitchrx --echo photo.jpg` (see --list)".into(),
            ));
        }
        let master = cli.seed.unwrap_or_else(random_seed);
        let steps = effects
            .iter()
            .enumerate()
            .map(|(i, e)| Step::new(e.id, &e.params, Recipe::step_seed(master, i)))
            .collect::<Result<Vec<_>, _>>()?;
        (
            Recipe {
                steps,
                ..Recipe::default()
            },
            Some(master),
        )
    };
    if let Some(e) = cli.encoding {
        recipe.encoding = e;
    }
    if let Some(c) = cli.channels {
        recipe.channels = c as usize;
    }
    recipe.validate()?;
    Ok((recipe, master))
}

/// A short, typeable seed (0..1,000,000). Not cryptographic, just varied.
fn random_seed() -> u64 {
    use std::hash::{BuildHasher, RandomState};
    // RandomState is seeded from OS randomness once per process.
    RandomState::new().hash_one(std::process::id()) % 1_000_000
}

/// Work out inputs and outputs from the positional arguments.
fn plan_jobs(cli: &Cli) -> Result<Vec<Job>, Fail> {
    let files = &cli.files;
    if cli.batch || files.len() > 2 {
        if cli.output.is_some() {
            return Err(Fail::Usage(
                "-o/--output cannot be used with several inputs".into(),
            ));
        }
        return Ok(files
            .iter()
            .map(|f| Job {
                input: f.clone(),
                output: None,
            })
            .collect());
    }
    match (files.as_slice(), &cli.output) {
        ([input], out) => Ok(vec![Job {
            input: input.clone(),
            output: out.clone(),
        }]),
        ([_, _], Some(_)) => Err(Fail::Usage(
            "two files given and -o as well; use INPUT OUTPUT or INPUT -o OUTPUT".into(),
        )),
        ([input, output], None) => {
            if output.exists() && !cli.force {
                return Err(Fail::Refused(format!(
                    "{} already exists. With two files the second is the output; \
                     use --batch to glitch both, or --force to overwrite",
                    output.display()
                )));
            }
            Ok(vec![Job {
                input: input.clone(),
                output: Some(output.clone()),
            }])
        }
        _ => unreachable!("files is non-empty"),
    }
}

/// The output format: a named output's extension, else `--format`, else
/// TIFF. A `--format` that contradicts the named output is an error rather
/// than a surprise.
fn output_format(cli: &Cli, output: Option<&Path>) -> Result<Format, Fail> {
    let flag = cli.format.map(Format::from);
    let Some(out) = output else {
        return Ok(flag.unwrap_or(Format::Tiff));
    };
    let ext = out
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    let named = Format::parse(&ext).ok_or_else(|| {
        Fail::Usage(format!(
            "{}: unknown output extension (use .tif, .png or .jpg)",
            out.display()
        ))
    })?;
    match flag {
        Some(f) if f != named => Err(Fail::Usage(format!(
            "--format {f} contradicts the output name {}",
            out.display()
        ))),
        _ => Ok(named),
    }
}

fn process(cli: &Cli, recipe: &Recipe, master_seed: Option<u64>, job: &Job) -> Result<(), Fail> {
    let input = &job.input;
    let format = output_format(cli, job.output.as_deref())?;
    let bytes = fs::read(input).map_err(|e| Fail::Runtime(format!("{}: {e}", input.display())))?;
    let mut image = codec::decode(&bytes)?;
    recipe.apply(&mut image.raster)?;
    let out_bytes = codec::encode(&image.raster, format, image.icc.as_deref())?;

    let output = match &job.output {
        Some(o) => o.clone(),
        None => auto_output(input, format.extension())
            .map_err(|e| Fail::Runtime(format!("{}: {e}", input.display())))?,
    };
    write_output(input, &output, &out_bytes, cli.force)?;

    let mut extra = Vec::new();
    if let Some(save) = &cli.save_recipe {
        let path = save
            .clone()
            .unwrap_or_else(|| output.with_extension("recipe.toml"));
        write_output(input, &path, recipe.to_toml().as_bytes(), cli.force)?;
        extra.push(format!("recipe {}", path.display()));
    }

    if !cli.quiet {
        let seed = master_seed.map_or(String::new(), |s| format!(", seed {s}"));
        println!(
            "{}  ({} x {}, {format}{seed})",
            output.display(),
            image.raster.width,
            image.raster.height
        );
        for e in extra {
            println!("  {e}");
        }
    }
    Ok(())
}

fn print_info(path: &Path) -> Result<(), Fail> {
    let bytes = fs::read(path).map_err(|e| Fail::Runtime(format!("{}: {e}", path.display())))?;
    let img = codec::decode(&bytes)?;
    let r = &img.raster;
    println!("{}", path.display());
    println!("  file         {}, {}", img.format, human_size(bytes.len()));
    println!("  dimensions   {} x {} (upright)", r.width, r.height);
    let turn = match img.orientation {
        1 => "none",
        2 => "mirrored",
        3 => "rotated 180",
        4 => "flipped vertically",
        5 => "mirrored and rotated 90 counter-clockwise",
        6 => "rotated 90 clockwise",
        7 => "mirrored and rotated 90 clockwise",
        _ => "rotated 90 counter-clockwise",
    };
    println!("  orientation  {} ({turn})", img.orientation);
    match &img.icc {
        Some(p) => println!(
            "  colour       ICC profile, {} (kept in the output)",
            human_size(p.len())
        ),
        None => println!("  colour       no ICC profile (sRGB assumed)"),
    }
    println!("  raw data     {} as samples", human_size(r.bytes.len()));
    Ok(())
}

fn human_size(n: usize) -> String {
    match n {
        n if n >= 1 << 20 => format!("{:.1} MB", n as f64 / (1 << 20) as f64),
        n if n >= 1 << 10 => format!("{:.1} KB", n as f64 / 1024.0),
        n => format!("{n} bytes"),
    }
}
