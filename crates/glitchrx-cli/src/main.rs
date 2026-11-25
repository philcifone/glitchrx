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
use glitchrx_core::io::{
    WriteError, auto_output, is_project_file, picture_output, write_output, write_project,
};
use glitchrx_core::project::{self, Project};
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
            Error::Malformed(_) | Error::Recipe(_) | Error::Project(_) | Error::Cancelled => {
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

    // A project carries its own recipe; anything else needs one given.
    let jobs = plan_jobs(cli)?;
    let given = cli.recipe.is_some() || !effects.is_empty();
    if given && jobs.iter().any(|j| is_project_path(&j.input)) {
        return Err(Fail::Usage(
            "a project (.glrx) replays its own recipe; leave out --recipe and the effect flags"
                .into(),
        ));
    }
    let base = if given || !jobs.iter().all(|j| is_project_path(&j.input)) {
        Some(build_recipe(cli, effects)?)
    } else {
        None
    };
    if jobs.len() > 1 && matches!(cli.save_recipe, Some(Some(_))) {
        return Err(Fail::Usage(
            "--save-recipe=FILE takes a single input; omit FILE in batch mode".into(),
        ));
    }
    if jobs.len() > 1 && matches!(cli.project, Some(Some(_))) {
        return Err(Fail::Usage(
            "--project=FILE takes a single input; omit FILE in batch mode".into(),
        ));
    }

    if let (true, Some((recipe, _))) = (cli.verbose, &base) {
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
        if let Err(f) = process(cli, base.as_ref(), job) {
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
        return adjust(cli, Recipe::from_toml(&text)?);
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
    overrides(cli, &mut recipe)?;
    Ok((recipe, master))
}

/// A recipe read from a file or a project: new seeds with `--seed`, then
/// the recipe-wide options. Returns it with the master seed, if any.
fn adjust(cli: &Cli, mut recipe: Recipe) -> Result<(Recipe, Option<u64>), Fail> {
    if let Some(seed) = cli.seed {
        recipe.reseed(seed);
    }
    overrides(cli, &mut recipe)?;
    Ok((recipe, cli.seed))
}

/// `--encoding`, `--channels`, `--work-size`, `--scale-back`, `--crop`,
/// `--crop-result`.
fn overrides(cli: &Cli, recipe: &mut Recipe) -> Result<(), Fail> {
    if let Some(e) = cli.encoding {
        recipe.encoding = e;
    }
    if let Some(c) = cli.channels {
        recipe.channels = c as usize;
    }
    if let Some(n) = cli.work_size {
        recipe.work_size = Some(n);
    }
    if cli.scale_back {
        recipe.scale_back = true;
    }
    if let Some(c) = cli.crop {
        recipe.crop = Some(c);
        recipe.crop_result = cli.crop_result;
    }
    recipe.validate()?;
    Ok(())
}

/// Named like a project (`.glrx`, any case).
fn is_project_path(p: &Path) -> bool {
    p.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(project::EXTENSION))
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
            "{}: unknown output extension (use .tif, .png, .jpg, .webp or .avif)",
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

fn process(cli: &Cli, base: Option<&(Recipe, Option<u64>)>, job: &Job) -> Result<(), Fail> {
    let input = &job.input;
    let format = output_format(cli, job.output.as_deref())?;
    let bytes = fs::read(input).map_err(|e| Fail::Runtime(format!("{}: {e}", input.display())))?;
    // The photo, its name, and the recipe to run on it: a project's own,
    // or the one given.
    let (original, name, recipe, master_seed) = if project::is_project(&bytes) {
        let p = Project::from_bytes(&bytes)?;
        let (recipe, seed) = adjust(cli, p.recipe)?;
        (p.original, p.name, recipe, seed)
    } else if is_project_path(input) {
        return Err(Error::Project("not a glitcharaxia project".into()).into());
    } else {
        let Some((recipe, seed)) = base else {
            return Err(Fail::Usage(
                "no effect given, e.g. `glitchrx --echo photo.jpg` (see --list)".into(),
            ));
        };
        let name = input
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        (bytes, name, recipe.clone(), *seed)
    };
    let recipe = &recipe;
    let mut image = codec::decode(&original)?;
    // Canvas (work size), steps, then scale back if asked.
    image.raster = recipe.render(&image.raster)?;
    let settings = codec::EncodeSettings {
        quality: cli.quality,
        lossless: cli.lossless,
        speed: cli.avif_speed,
    };
    let out_bytes = codec::encode_with(&image.raster, format, image.icc.as_deref(), &settings)?;
    if format == Format::Avif
        && !glitchrx_core::avif::keeps_profile(image.icc.as_deref())
        && !cli.quiet
    {
        eprintln!(
            "glitchrx: {}: AVIF cannot carry this photo's color profile; colors may shift (TIFF, PNG, JPEG and WebP keep it)",
            input.display()
        );
    }

    let output = match &job.output {
        Some(o) => o.clone(),
        None => picture_output(input, format.extension())
            .map_err(|e| Fail::Runtime(format!("{}: {e}", input.display())))?,
    };
    write_output(input, &output, &out_bytes, cli.force)?;

    let mut extra = Vec::new();
    if let Some(save) = &cli.save_recipe {
        let path = save
            .clone()
            .unwrap_or_else(|| output.with_extension("recipe.toml"));
        // Note the photo's name, so the GUI can offer to continue from it.
        let named = glitchrx_core::Recipe {
            source: Some(name.clone()),
            ..recipe.clone()
        };
        write_output(input, &path, named.to_toml().as_bytes(), cli.force)?;
        extra.push(format!("recipe {}", path.display()));
    }
    if let Some(save) = &cli.project {
        // Beside the picture, with its name; but a picture made from a
        // project carries that project's name, so a new project from one
        // takes the next -glitch name instead (a new version, never the
        // input itself).
        let path = match save {
            Some(p) => p.clone(),
            None if is_project_file(input) => auto_output(input, project::EXTENSION)
                .map_err(|e| Fail::Runtime(format!("{}: {e}", input.display())))?,
            None => output.with_extension(project::EXTENSION),
        };
        let preview = project::preview(&image.raster, image.icc.as_deref())?;
        let p = Project::new(recipe.clone(), &name, &original, Some(preview))?;
        write_project(input, &path, &p.to_bytes()?, cli.force)?;
        extra.push(format!("project {}", path.display()));
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
    println!("{}", path.display());
    if project::is_project(&bytes) {
        return print_project(&bytes);
    }
    print_photo(&bytes)
}

/// What a project holds, then its original as for a photo.
fn print_project(bytes: &[u8]) -> Result<(), Fail> {
    let p = Project::from_bytes(bytes)?;
    println!(
        "  project      {}{}",
        human_size(bytes.len()),
        p.writer
            .as_deref()
            .map_or(String::new(), |w| format!(", made by {w}"))
    );
    println!("  original     {}", p.name);
    let steps: Vec<_> = p.recipe.steps.iter().map(|s| s.effect.as_str()).collect();
    let mut recipe = match steps.len() {
        0 => "no steps".to_string(),
        1 => format!("1 step: {}", steps[0]),
        n => format!("{n} steps: {}", steps.join(", ")),
    };
    if p.recipe.crop.is_some() {
        recipe.push_str(if p.recipe.crop_result {
            "; result cropped"
        } else {
            "; cropped"
        });
    }
    if let Some(w) = p.recipe.work_size {
        recipe.push_str(&format!("; work size {w}"));
    }
    println!("  recipe       {recipe}");
    if let Some(pv) = &p.preview {
        let img = codec::decode(pv)?;
        println!(
            "  preview      {} x {}, {}",
            img.raster.width,
            img.raster.height,
            human_size(pv.len())
        );
    }
    print_photo(&p.original)
}

fn print_photo(bytes: &[u8]) -> Result<(), Fail> {
    let img = codec::decode(bytes)?;
    let r = &img.raster;
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
            "  color       ICC profile, {} (kept in the output)",
            human_size(p.len())
        ),
        None => println!("  color       no ICC profile (sRGB assumed)"),
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
