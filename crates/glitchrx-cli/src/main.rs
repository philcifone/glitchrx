//! `glitchrx`: glitch art from the command line.
//!
//! A thin front end over `glitchrx-core`: parse arguments, build a recipe,
//! run it over each input, write outputs safely, print one summary line per
//! file. All the real work lives in the core so the GUI can share it.

mod args;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{CommandFactory, FromArgMatches};
use glitchrx_core::io::{Suffix, WriteError, auto_output, write_output};
use glitchrx_core::turbo::{Scale, decode, to_png};
use glitchrx_core::{Error, Jpeg, Mode, Recipe, Step};

use args::{Cli, EffectArg, ModeArg};

/// Why the program stopped, mapped onto the documented exit codes.
enum Fail {
    /// 1: something went wrong while working (I/O, malformed file).
    Runtime(String),
    /// 2: the command line itself is wrong.
    Usage(String),
    /// 3: refused to do something unsafe (overwrite, irreversible reverse).
    Refused(String),
    /// 4: the input is not a JPEG we can handle.
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
            Error::NotJpeg | Error::Unsupported(_) => Fail::Unsupported(e.to_string()),
            Error::NotReversible(_) => Fail::Refused(e.to_string()),
            Error::UnknownEffect(_) | Error::Param { .. } => Fail::Usage(e.to_string()),
            Error::Malformed(_) | Error::Recipe(_) => Fail::Runtime(e.to_string()),
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
    let (effects, rest) = args::split_effects(std::env::args_os().skip(1).collect());
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
    if cli.reverse {
        let bad = recipe.irreversible_parts()?;
        if !bad.is_empty() {
            return Err(Fail::Refused(format!(
                "cannot reverse: {} {} not reversible",
                bad.join(", "),
                if bad.len() == 1 { "is" } else { "are" }
            )));
        }
    }

    let jobs = plan_jobs(cli)?;
    if jobs.len() > 1
        && (matches!(cli.bake, Some(Some(_))) || matches!(cli.save_recipe, Some(Some(_))))
    {
        return Err(Fail::Usage(
            "--bake=FILE and --save-recipe=FILE take a single input; omit FILE in batch mode"
                .into(),
        ));
    }

    if cli.verbose {
        for (i, s) in recipe.steps.iter().enumerate() {
            eprintln!(
                "step {}: {} {} (seed {})",
                i + 1,
                s.effect,
                s.params_list(),
                s.seed
            );
        }
    }

    // Keep going after a failed file in batch mode; report the worst error.
    let mut worst: Option<Fail> = None;
    for job in &jobs {
        if let Err(f) = process(cli, effects, &recipe, master_seed, job) {
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

/// Build the recipe from `--recipe` or the effect flags.
/// Returns the recipe and the master seed (None when replaying a recipe
/// file's own seeds).
fn build_recipe(cli: &Cli, effects: &[EffectArg]) -> Result<(Recipe, Option<u64>), Fail> {
    let mode = match cli.mode {
        ModeArg::Safe => Mode::Safe,
        ModeArg::Raw => Mode::Raw,
    };
    if let Some(path) = &cli.recipe {
        if !effects.is_empty() {
            return Err(Fail::Usage(
                "use either --recipe or effect flags, not both".into(),
            ));
        }
        let text = fs::read_to_string(path)
            .map_err(|e| Fail::Runtime(format!("{}: {e}", path.display())))?;
        let mut r = Recipe::from_toml(&text)?;
        if cli.mode == ModeArg::Raw {
            r.mode = Mode::Raw;
        }
        if let Some(seed) = cli.seed {
            r.reseed(seed);
        }
        return Ok((r, cli.seed));
    }
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
    Ok((
        Recipe {
            mode,
            steps,
            ..Recipe::default()
        },
        Some(master),
    ))
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

fn process(
    cli: &Cli,
    effects: &[EffectArg],
    recipe: &Recipe,
    master_seed: Option<u64>,
    job: &Job,
) -> Result<(), Fail> {
    let input = &job.input;
    let bytes = fs::read(input).map_err(|e| Fail::Runtime(format!("{}: {e}", input.display())))?;
    let out_bytes = if cli.reverse {
        recipe.invert(bytes)?
    } else {
        recipe.apply(bytes)?
    };

    let suffix = if cli.reverse {
        Suffix::Restored
    } else {
        Suffix::Glitch
    };
    let output = match &job.output {
        Some(o) => o.clone(),
        None => auto_output(input, suffix)
            .map_err(|e| Fail::Runtime(format!("{}: {e}", input.display())))?,
    };
    write_output(input, &output, &out_bytes, cli.force)?;

    // Decode check at 1/8 scale: fast, and reports the same warnings as a
    // full decode because the entropy decoding is identical.
    let (_, status) = decode(&out_bytes, Scale::Eighth);

    let mut extra = Vec::new();
    if let Some(bake) = &cli.bake {
        let path = bake.clone().unwrap_or_else(|| output.with_extension("png"));
        let (img, st) = decode(&out_bytes, Scale::Full);
        let img = img.ok_or_else(|| {
            Fail::Runtime(format!(
                "cannot bake {}: {}",
                output.display(),
                st.message().unwrap_or("")
            ))
        })?;
        write_output(input, &path, &to_png(&img), cli.force)?;
        extra.push(format!("baked {}", path.display()));
    }
    if let Some(save) = &cli.save_recipe {
        let path = save
            .clone()
            .unwrap_or_else(|| output.with_extension("recipe.toml"));
        write_output(input, &path, recipe.to_toml().as_bytes(), cli.force)?;
        extra.push(format!("recipe {}", path.display()));
    }

    if !cli.quiet {
        let seed = master_seed.map_or(String::new(), |s| format!(", seed {s}"));
        println!("{}  (decode: {}{seed})", output.display(), status.word());
        if cli.verbose
            && let Some(m) = status.message()
        {
            println!("  decoder: {m}");
        }
        for e in extra {
            println!("  {e}");
        }
        if !cli.reverse {
            println!(
                "  {}",
                undo_hint(cli, effects, recipe, master_seed, &output)
            );
        }
    }
    Ok(())
}

/// The command that would undo this run, or why there is none.
fn undo_hint(
    cli: &Cli,
    effects: &[EffectArg],
    recipe: &Recipe,
    master_seed: Option<u64>,
    output: &Path,
) -> String {
    match recipe.irreversible_parts() {
        Ok(bad) if !bad.is_empty() => return format!("not reversible ({})", bad.join(", ")),
        Err(e) => return format!("not reversible ({e})"),
        _ => {}
    }
    let mut cmd = String::from("undo: glitchrx --reverse");
    if let Some(r) = &cli.recipe {
        cmd.push_str(&format!(" --recipe {}", shell_quote(&r.to_string_lossy())));
    } else {
        for e in effects {
            cmd.push_str(&format!(" --{}", e.id));
            if !e.params.is_empty() {
                cmd.push_str(&format!(" {}", shell_quote(&e.params)));
            }
        }
    }
    if let Some(s) = master_seed {
        cmd.push_str(&format!(" --seed {s}"));
    }
    cmd.push(' ');
    cmd.push_str(&shell_quote(&output.to_string_lossy()));
    cmd
}

/// Quote for a POSIX shell only when needed, so typical commands stay clean.
fn shell_quote(s: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "-_./=,:%+@".contains(c);
    if !s.is_empty() && s.chars().all(safe) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

fn print_info(path: &Path) -> Result<(), Fail> {
    let bytes = fs::read(path).map_err(|e| Fail::Runtime(format!("{}: {e}", path.display())))?;
    let j = Jpeg::parse(bytes, Mode::Safe)?;
    let i = j.info();
    let f = &i.frame;
    let kind = if f.progressive() {
        format!("progressive ({} scans)", i.scans)
    } else {
        format!(
            "baseline ({} scan{})",
            i.scans,
            if i.scans == 1 { "" } else { "s" }
        )
    };
    println!("{}", path.display());
    println!("  file        {}", human_size(i.file_len));
    println!("  dimensions  {} x {}", f.width, f.height);
    println!("  encoding    {kind}, {}", f.subsampling());
    println!(
        "  image data  {} ({:.0}% of file)",
        human_size(i.payload_len),
        100.0 * i.payload_len as f64 / i.file_len.max(1) as f64
    );
    if i.restart_interval > 0 {
        println!(
            "  restarts    every {} MCUs ({} markers)",
            i.restart_interval, i.restart_markers
        );
    }
    println!(
        "  thumbnail   {}",
        if i.has_exif_thumbnail {
            "yes (EXIF, left untouched)"
        } else {
            "no"
        }
    );
    if i.trailing_len > 0 {
        println!(
            "  trailing    {} after end of image (left untouched)",
            human_size(i.trailing_len)
        );
    }
    if i.missing_eoi {
        println!("  note        no end-of-image marker (truncated or previously glitched)");
    }
    Ok(())
}

fn human_size(n: usize) -> String {
    match n {
        n if n >= 1 << 20 => format!("{:.1} MB", n as f64 / (1 << 20) as f64),
        n if n >= 1 << 10 => format!("{:.1} KB", n as f64 / 1024.0),
        n => format!("{n} bytes"),
    }
}
