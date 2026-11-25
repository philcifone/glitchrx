//! Recipes: an ordered list of effect steps with parameters and seeds.
//!
//! A recipe is the whole description of a glitch. Applying the same recipe to
//! the same image always produces the same bytes, which is what gives the
//! GUI free undo and lets a glitch be saved, shared and replayed.
//!
//! On disk a recipe is TOML:
//!
//! ```toml
//! format = "glitcharaxia-recipe"
//! version = 8
//! encoding = "u8"     # how steps read bytes as samples; a step may override
//! channels = 1
//! work_size = 2048    # optional: steps run on a copy this long (see below)
//! scale_back = true   # optional: and the result is enlarged to full size
//!
//! source = "IMG_1234.jpg"   # optional: the photo it was made on (a name only)
//!
//! crop_result = true # optional: the crop frames the result (see below)
//!
//! [crop]              # optional: the part of the photo kept (see below)
//! x = 0.1             # left edge, share of the width
//! y = 0.0
//! w = 0.8             # share of the width
//! h = 1.0
//!
//! [[step]]
//! effect = "echo"
//! effect_version = 1
//! seed = 81723
//! encoding = "ulaw"   # this step only
//! opacity = 0.8       # blend the result over the step's input
//! delay = "0.33rows"
//! decay = 0.5
//!
//! [step.mask]         # optional: where the result shows (see `mask`)
//! invert = true
//!
//! [[step.mask.op]]
//! shape = "radial"
//! from = [0.5, 0.5]
//! to = [0.7, 0.5]
//! feather = 0.01
//! ```
//!
//! A `pull` step also holds what was drawn on it by hand, strokes and pins
//! in the order made (see `pull`):
//!
//! ```toml
//! [[step]]
//! effect = "pull"
//! effect_version = 1
//! seed = 0
//!
//! [[step.pull]]       # a stroke, of brush 1
//! size = 0.12
//! strength = 0.6
//! points = [0.40, 0.52, 0.43, 0.53]
//! brush = 1
//!
//! [[step.pull]]       # a pin
//! size = 0.2
//! from = [0.5, 0.5]
//! to = [0.56, 0.47]
//! ```
//!
//! Version 3 added `opacity` and `mask`; version 4 added the canvas the
//! steps run on (`work_size`, `scale_back`); version 5 added pull strokes;
//! version 6 added pull pins, in a list of their own (`[[step.pin]]`, run
//! after the strokes); version 7 put them among the strokes, in order, and
//! added brushes; version 8 added `crop_result`. A version 6 recipe loads
//! with its pins after its strokes, the picture it made.
//! Older recipes load unchanged (they have none of it) and are written
//! back as the oldest version that holds them, 7 or newer (`needed_version`). A build that only knows an older version
//! refuses a newer one, rather than ignore what it does not know and
//! quietly give a different picture.
//!
//! **The canvas.** Before the steps run, the picture can be shrunk so its
//! longer side is `work_size` pixels (never enlarged). Steps then run on
//! that copy: faster, and coarser glitch texture, since a pixel-level
//! effect covers more of the picture. The file is written at that size,
//! or with `scale_back`, enlarged to the original size with hard pixel
//! edges (nearest neighbour), so the coarse texture stays crisp.
//!
//! **Crop** comes first: the canvas is made from the cropped photo, so
//! the work size, the steps, their masks and positions, and `scale_back`
//! all measure against the crop, and the file is the crop's size. The
//! crop is kept as shares of the photo, which turn into whole pixels by
//! rounding (`crop_rect`).
//!
//! **Crop the result** (`crop_result = true`, D-69) instead frames the
//! finished picture: the steps run on the whole photo (shrunk to the work
//! size), exactly as without a crop, and the crop is cut from what they
//! give (after `scale_back`). The glitch then stays as it is while the
//! frame moves, and the file is the crop's size. Masks and pulls need no
//! moving: the steps see the whole photo.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::effects::{self, Effect, EffectInfo, Window};
use crate::error::{Error, Result};
use crate::mask::{self, Mask};
use crate::params::{Params, Units, parse_list};
use crate::placement::{Placement, hit_rng};
use crate::raster::Raster;
use crate::rng::derive;
use crate::samples::Encoding;

/// Magic string identifying a recipe file.
pub const FORMAT: &str = "glitcharaxia-recipe";
/// Recipe file format version.
pub const VERSION: u32 = 8;

/// Step keys that are not effect parameters: per-step overrides of the
/// recipe's sample format, and how strongly the result is blended in.
const ENCODING_KEY: &str = "encoding";
const CHANNELS_KEY: &str = "channels";
const OPACITY_KEY: &str = "opacity";

/// A render that can be abandoned: called between hits and between blocks
/// of samples; returning true stops the render with [`Error::Cancelled`].
pub type Cancel<'a> = &'a (dyn Fn() -> bool + Sync);

/// Never cancel.
pub const NEVER: Cancel<'static> = &|| false;

fn parse_channels(s: &str) -> Result<usize> {
    match s.trim() {
        "1" => Ok(1),
        "2" => Ok(2),
        "3" => Ok(3),
        other => Err(Error::Recipe(format!(
            "channels must be 1 (mono), 2 (stereo) or 3 (one per colour), not `{other}`"
        ))),
    }
}

fn parse_opacity(s: &str) -> Result<f64> {
    match s.trim().parse::<f64>() {
        Ok(v) if (0.0..=1.0).contains(&v) => Ok(v),
        _ => Err(Error::Recipe(format!(
            "opacity must be a number from 0 to 1, not `{s}`"
        ))),
    }
}

/// One effect application.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub effect: String,
    pub effect_version: u32,
    pub seed: u64,
    /// Parameters as given by the user (defaults are not written out, so a
    /// recipe stays short and readable), plus the optional `encoding` and
    /// `channels` overrides.
    #[serde(flatten, with = "param_values")]
    pub params: BTreeMap<String, String>,
    /// Where the result shows; none means everywhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<Mask>,
    /// A `pull` step's strokes and pins, in the order made (`[[step.pull]]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pull: Vec<crate::pull::Mark>,
}

impl Step {
    /// Build a step from an effect id and a `k=v,...` parameter list.
    pub fn new(effect: &str, params: &str, seed: u64) -> Result<Step> {
        let info = effects::lookup(effect)?;
        let step = Step {
            effect: effect.to_string(),
            effect_version: info.version,
            seed,
            params: parse_list(effect, params)?,
            mask: None,
            pull: Vec::new(),
        };
        // Validate now, not halfway through a batch.
        step.prepare(Encoding::default(), 1)?;
        Ok(step)
    }

    /// The parameters as a `k=v,...` list (as typed on the command line).
    pub fn params_list(&self) -> String {
        self.params
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(",")
    }

    /// This step's encoding override, if any. A bad value is a parameter
    /// error naming the effect, like any other parameter.
    pub fn encoding(&self) -> Result<Option<Encoding>> {
        self.params
            .get(ENCODING_KEY)
            .map(|e| Encoding::parse(e).map_err(|err| self.param_error(err)))
            .transpose()
    }

    /// This step's channel-count override, if any.
    pub fn channels(&self) -> Result<Option<usize>> {
        self.params
            .get(CHANNELS_KEY)
            .map(|c| parse_channels(c).map_err(|err| self.param_error(err)))
            .transpose()
    }

    /// How strongly the result is blended over the step's input, 0..1.
    pub fn opacity(&self) -> Result<f64> {
        self.params
            .get(OPACITY_KEY)
            .map(|o| parse_opacity(o).map_err(|err| self.param_error(err)))
            .transpose()
            .map(|o| o.unwrap_or(1.0))
    }

    fn param_error(&self, err: Error) -> Error {
        let msg = match err {
            Error::Recipe(m) => m,
            other => other.to_string(),
        };
        Error::Param {
            effect: self.effect.clone(),
            msg,
        }
    }

    /// Bring a step saved with an older effect version up to date, where
    /// the effect knows how (D-56): parameters whose default moved are
    /// written in with their old value, so the step makes the same
    /// picture. Unknown effects and unreachable versions are left for
    /// `prepare` to refuse.
    pub fn upgrade(&mut self) {
        let Ok(info) = effects::lookup(&self.effect) else {
            return;
        };
        for up in info.upgrades {
            if self.effect_version < up.to {
                for (k, v) in up.fill {
                    self.params
                        .entry(k.to_string())
                        .or_insert_with(|| v.to_string());
                }
                self.effect_version = up.to;
            }
        }
    }

    /// Validate and construct the effect and its placement, with the
    /// recipe's `encoding` and `channels` unless the step overrides them.
    pub fn prepare(&self, encoding: Encoding, channels: usize) -> Result<Prepared> {
        let info = effects::lookup(&self.effect)?;
        if self.effect_version != info.version {
            return Err(Error::Recipe(format!(
                "step `{}` was made with effect version {}, this build has version {}",
                self.effect, self.effect_version, info.version
            )));
        }
        let encoding = self.encoding()?.unwrap_or(encoding);
        let channels = self.channels()?.unwrap_or(channels);
        let opacity = self.opacity()?;
        if let Some(m) = &self.mask {
            m.validate().map_err(|err| self.param_error(err))?;
        }
        let mut given = self.params.clone();
        given.remove(ENCODING_KEY);
        given.remove(CHANNELS_KEY);
        given.remove(OPACITY_KEY);
        if info.strokes {
            // Placement is not among its parameters, so `hits=` and the
            // like are refused as unknown below.
            crate::pull::validate_marks(&self.pull).map_err(|err| self.param_error(err))?;
        } else if !self.pull.is_empty() {
            return Err(self.param_error(Error::Recipe(
                "only a pull step has strokes and pins".into(),
            )));
        }
        let params = Params::new(info.id, &info.all_params(), &given)?;
        let effect = (info.build)(&params)?;
        let pull = info
            .strokes
            .then(|| crate::pull::Pull::new(&params, &self.pull));
        // A pull covers the whole raster; its placement is never used for
        // windows, but every step has one.
        let placement = if info.strokes {
            Placement::from_params(&Params::new(
                info.id,
                &info.placement_params(),
                &BTreeMap::new(),
            )?)
        } else {
            Placement::from_params(&params)
        };
        Ok(Prepared {
            info,
            effect,
            placement,
            seed: self.seed,
            encoding,
            channels,
            opacity,
            mask: self.mask.clone(),
            pull,
        })
    }
}

/// A step that has been validated and is ready to run.
pub struct Prepared {
    pub info: &'static EffectInfo,
    pub effect: Box<dyn Effect>,
    pub placement: Placement,
    pub seed: u64,
    pub encoding: Encoding,
    pub channels: usize,
    /// Blend of the result over the step's input (1 = all of it).
    pub opacity: f64,
    pub mask: Option<Mask>,
    /// A pull step's strokes, ready to run on the whole raster.
    pub pull: Option<crate::pull::Pull>,
}

impl Prepared {
    /// True if the result is blended over the input rather than kept whole.
    pub fn blends(&self) -> bool {
        self.mask.is_some() || self.opacity < 1.0
    }

    /// Bytes per frame (one sample for each channel). Pixel effects work on
    /// whole pixels, so for them a frame is one pixel, and windows (which
    /// are whole frames from the start of the raster) cover whole pixels.
    pub fn frame(&self) -> usize {
        if self.info.group.on_pixels() {
            3
        } else {
            self.encoding.width() * self.channels
        }
    }

    /// Apply this step to raster bytes measured by `units`, then blend the
    /// result over the input through the mask and opacity, if any.
    pub fn apply(&self, bytes: &mut [u8], units: &Units, cancel: Cancel<'_>) -> Result<()> {
        if !self.blends() {
            return self.run(bytes, units, cancel);
        }
        let before = bytes.to_vec();
        self.run(bytes, units, cancel)?;
        let coverage = match &self.mask {
            Some(m) => Some(m.render(units.width, units.height, &before, cancel)?),
            None => None,
        };
        mask::composite(&before, bytes, coverage.as_deref(), self.opacity);
        Ok(())
    }

    /// The effect alone, window by window (a pull: the whole raster).
    fn run(&self, bytes: &mut [u8], units: &Units, cancel: Cancel<'_>) -> Result<()> {
        if let Some(p) = &self.pull {
            return p.apply(bytes, units, cancel);
        }
        assert_eq!(
            bytes.len(),
            units.len(),
            "raster of {} bytes measured as {}x{}",
            bytes.len(),
            units.width,
            units.height
        );
        let windows = self.placement.windows(units, self.frame(), self.seed);
        for (h, w) in windows.into_iter().enumerate() {
            if cancel() {
                return Err(Error::Cancelled);
            }
            let start = w.start;
            let mut win = Window {
                bytes: &mut bytes[w],
                start,
                encoding: self.encoding,
                channels: self.channels,
                units,
                cancel,
            };
            self.effect.run(&mut win, &mut hit_rng(self.seed, h))?;
        }
        Ok(())
    }
}

fn default_channels() -> usize {
    1
}

/// A complete glitch description.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub format: String,
    pub version: u32,
    /// How steps read the raster as samples, unless a step overrides it.
    #[serde(default)]
    pub encoding: Encoding,
    /// 1 (mono) or 2 (stereo), unless a step overrides it.
    #[serde(default = "default_channels")]
    pub channels: usize,
    /// The longest side of the canvas the steps run on, in pixels; none
    /// for the full size. See the module documentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_size: Option<u32>,
    /// Enlarge the result back to the full size (nearest neighbour).
    #[serde(default, skip_serializing_if = "is_false")]
    pub scale_back: bool,
    /// The file name (no folders) of the photo the recipe was made on, so
    /// the GUI can offer to continue from it when that photo is opened
    /// again. Information only: it changes nothing in the output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The part of the photo kept, before anything else. See the module
    /// documentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop: Option<Crop>,
    /// The crop frames the finished picture rather than the photo the
    /// steps run on (D-69). Nothing without a crop.
    #[serde(default, skip_serializing_if = "is_false")]
    pub crop_result: bool,
    #[serde(rename = "step", default)]
    pub steps: Vec<Step>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// A crop rectangle, as shares (0 to 1) of the photo's width and height.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Crop {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Crop {
    /// Whole pixels `(x, y, w, h)` in a `width` x `height` photo: each edge
    /// rounded to the nearest pixel (basic operations only, D-25), at least
    /// one pixel each way, inside the photo.
    pub fn rect(&self, width: usize, height: usize) -> (usize, usize, usize, usize) {
        let edge =
            |share: f64, n: usize| ((share * n as f64 + 0.5).floor().max(0.0) as usize).min(n);
        let span = |a: f64, len: f64, n: usize| {
            let lo = edge(a, n).min(n - 1);
            let hi = edge(a + len, n).clamp(lo + 1, n);
            (lo, hi - lo)
        };
        let (x, w) = span(self.x, self.w, width);
        let (y, h) = span(self.y, self.h, height);
        (x, y, w, h)
    }

    fn check(&self) -> std::result::Result<(), String> {
        let ok = |v: f64| v.is_finite() && (0.0..=1.0).contains(&v);
        // A hair of slack for shares that add up to 1 with rounding.
        let fits = |a: f64, l: f64| ok(a) && ok(l) && l > 0.0 && a + l <= 1.0 + 1e-9;
        if fits(self.x, self.w) && fits(self.y, self.h) {
            Ok(())
        } else {
            Err(format!(
                "crop must lie inside the photo (shares 0 to 1, width and height above 0): {self:?}"
            ))
        }
    }
}

/// Version 6 kept a pull step's pins in `[[step.pin]]`, run after all its
/// strokes; version 7 has one list in the order made (D-68). Moving each
/// step's pins to the end of its list gives the picture version 6 made.
fn pins_among_strokes(raw: &mut toml::Table) {
    let Some(toml::Value::Array(steps)) = raw.get_mut("step") else {
        return;
    };
    for step in steps.iter_mut().filter_map(|s| s.as_table_mut()) {
        if !matches!(step.get("pin"), Some(toml::Value::Array(_))) {
            continue; // nothing to move (anything else fails in the parse)
        }
        let Some(toml::Value::Array(pins)) = step.remove("pin") else {
            continue;
        };
        if let toml::Value::Array(marks) = step
            .entry("pull")
            .or_insert_with(|| toml::Value::Array(Vec::new()))
        {
            marks.extend(pins);
        }
    }
}

/// The smallest work size accepted: below it there is too little picture.
pub const MIN_WORK_SIZE: u32 = 64;

impl Default for Recipe {
    fn default() -> Self {
        Recipe {
            format: FORMAT.into(),
            version: VERSION,
            encoding: Encoding::default(),
            channels: 1,
            work_size: None,
            scale_back: false,
            source: None,
            crop: None,
            crop_result: false,
            steps: Vec::new(),
        }
    }
}

impl Recipe {
    /// Seed for step `index` of a chain built from a master seed. Each step
    /// gets an independent seed, so adding a step never changes the others.
    pub fn step_seed(master: u64, index: usize) -> u64 {
        derive(master, index as u64)
    }

    /// Replace every step's seed with one derived from `master`.
    pub fn reseed(&mut self, master: u64) {
        for (i, s) in self.steps.iter_mut().enumerate() {
            s.seed = Self::step_seed(master, i);
        }
    }

    pub fn from_toml(text: &str) -> Result<Recipe> {
        // Check identity and version before the full parse, so an old recipe
        // gets a useful message rather than a field error.
        let mut raw: toml::Table =
            toml::from_str(text).map_err(|e| Error::Recipe(e.to_string()))?;
        if raw.get("format").and_then(|v| v.as_str()) != Some(FORMAT) {
            return Err(Error::Recipe(format!(
                "not a glitcharaxia recipe (no `format = \"{FORMAT}\"`)"
            )));
        }
        let version = raw.get("version").and_then(|v| v.as_integer());
        match version {
            Some(2..=8) => {}
            Some(1) => {
                return Err(Error::Recipe(
                    "version 1 recipes are for the JPEG byte engine, which was retired; \
                     replay them with a build of the git tag `archive/jpeg-bytes`"
                        .into(),
                ));
            }
            Some(v) => return Err(Error::Recipe(format!("unsupported recipe version {v}"))),
            None => return Err(Error::Recipe("missing `version`".into())),
        }
        let mut r: Recipe = if version == Some(6) {
            pins_among_strokes(&mut raw);
            raw.try_into()
        } else {
            // From the text, so a mistake is reported with its line.
            toml::from_str(text)
        }
        .map_err(|e| Error::Recipe(e.to_string()))?;
        // Older versions are subsets of this one; from here on it is current.
        r.version = VERSION;
        r.upgrade();
        r.validate()?;
        Ok(r)
    }

    /// The recipe as TOML, marked with the oldest version that holds it
    /// ([`Recipe::needed_version`]).
    pub fn to_toml(&self) -> String {
        let written = Recipe {
            version: self.needed_version(),
            ..self.clone()
        };
        toml::to_string(&written).expect("recipe serialization cannot fail")
    }

    /// The oldest version that holds everything this recipe uses, so a file
    /// that uses nothing new still opens in builds that know only the
    /// version before (a project saved on a newer build, opened on an
    /// older server). Never below 7, the version every build since Phase 8
    /// writes; only version 8's `crop_result` raises it.
    pub fn needed_version(&self) -> u32 {
        if self.result_crop().is_some() { 8 } else { 7 }
    }

    /// Bring every step up to its effect's current version (D-56).
    pub fn upgrade(&mut self) {
        for s in &mut self.steps {
            s.upgrade();
        }
    }

    /// Check every step (and the recipe-wide settings).
    pub fn validate(&self) -> Result<()> {
        if let Some(w) = self.work_size
            && w < MIN_WORK_SIZE
        {
            return Err(Error::Recipe(format!(
                "work_size must be at least {MIN_WORK_SIZE} pixels, not {w}"
            )));
        }
        if let Some(c) = &self.crop {
            c.check().map_err(Error::Recipe)?;
        }
        self.prepare_all().map(|_| ())
    }

    /// Everything that decides the canvas, for telling when it changes. A
    /// crop of the result is not part of it: moving that frame re-runs
    /// nothing.
    pub fn framing(&self) -> (Option<u32>, Option<Crop>) {
        (self.work_size, self.photo_crop())
    }

    /// The crop taken from the photo before the steps (none when the crop
    /// frames the result).
    pub fn photo_crop(&self) -> Option<Crop> {
        self.crop.filter(|_| !self.crop_result)
    }

    /// The crop cut from the finished picture, when it frames the result.
    pub fn result_crop(&self) -> Option<Crop> {
        self.crop.filter(|_| self.crop_result)
    }

    /// The crop of the result in whole pixels of a finished `w` x `h`
    /// picture (none unless the crop frames the result).
    pub fn result_rect(&self, w: usize, h: usize) -> Option<(usize, usize, usize, usize)> {
        self.result_crop().map(|c| c.rect(w, h))
    }

    /// The photo crop in whole pixels of a `w` x `h` photo (all of it
    /// without one, or when the crop frames the result).
    pub fn crop_rect(&self, w: usize, h: usize) -> (usize, usize, usize, usize) {
        match &self.photo_crop() {
            Some(c) => c.rect(w, h),
            None => (0, 0, w, h),
        }
    }

    /// The size of the photo crop of a `w` x `h` photo: what `scale_back`
    /// enlarges to.
    pub fn cropped_size(&self, w: usize, h: usize) -> (usize, usize) {
        let (_, _, cw, ch) = self.crop_rect(w, h);
        (cw, ch)
    }

    /// The canvas size for a photo of `w` x `h`: cropped, then shrunk to
    /// the work size if there is one and the crop is larger.
    pub fn canvas_size(&self, w: usize, h: usize) -> (usize, usize) {
        let (w, h) = self.cropped_size(w, h);
        match self.work_size {
            Some(n) => crate::raster::fit_size(w, h, n as usize, n as usize),
            None => (w, h),
        }
    }

    /// The raster the steps run on (see the module documentation).
    pub fn canvas(&self, original: &Raster) -> Raster {
        let (x, y, cw, ch) = self.crop_rect(original.width, original.height);
        let (w, h) = self.canvas_size(original.width, original.height);
        if (cw, ch) == (original.width, original.height) {
            original.resize(w, h)
        } else {
            original.crop(x, y, cw, ch).resize(w, h)
        }
    }

    /// What the bent canvas becomes in the file: itself, or with
    /// `scale_back`, enlarged to the crop's size (the size before the
    /// work size); then, when the crop frames the result, that crop of it.
    /// `original` is the photo's size.
    pub fn finish(&self, bent: Raster, original: (usize, usize)) -> Raster {
        let full = self.cropped_size(original.0, original.1);
        let done = if self.scale_back && (bent.width, bent.height) != full {
            bent.enlarge(full.0, full.1)
        } else {
            bent
        };
        match self.result_rect(done.width, done.height) {
            Some((x, y, w, h)) if (w, h) != (done.width, done.height) => done.crop(x, y, w, h),
            _ => done,
        }
    }

    /// The size of the file for a `w` x `h` photo: the canvas, enlarged
    /// with `scale_back`, cut to a crop of the result.
    pub fn output_size(&self, w: usize, h: usize) -> (usize, usize) {
        let (cw, ch) = if self.scale_back {
            self.cropped_size(w, h)
        } else {
            self.canvas_size(w, h)
        };
        match self.result_rect(cw, ch) {
            Some((_, _, rw, rh)) => (rw, rh),
            None => (cw, ch),
        }
    }

    /// The whole run, as the CLI and the exports do it: canvas, steps,
    /// finish.
    pub fn render(&self, original: &Raster) -> Result<Raster> {
        self.run_on(original)
    }

    /// Validate and build every step.
    pub fn prepare_all(&self) -> Result<Vec<Prepared>> {
        parse_channels(&self.channels.to_string())?;
        self.steps
            .iter()
            .map(|s| s.prepare(self.encoding, self.channels))
            .collect()
    }

    /// How masks drawn on a `w` x `h` photo land on this recipe's canvas
    /// (none without a crop: they already fit).
    pub fn mask_frame(&self, photo: (usize, usize)) -> Option<crate::mask::Frame> {
        self.photo_crop()
            .as_ref()
            .map(|_| crate::mask::Frame::new(self.crop_rect(photo.0, photo.1), photo.0, photo.1))
    }

    /// Every step built, with its mask and pull strokes moved onto the
    /// canvas cut from a photo of `photo` pixels (see `mask::Frame`).
    pub fn prepare_for(&self, photo: (usize, usize)) -> Result<Vec<Prepared>> {
        let mut steps = self.prepare_all()?;
        if let Some(f) = self.mask_frame(photo) {
            for s in &mut steps {
                if let Some(m) = &mut s.mask {
                    *m = m.reframed(&f);
                }
                if let Some(p) = &mut s.pull {
                    p.reframe(&f);
                }
            }
        }
        Ok(steps)
    }

    /// The whole run on a photo: canvas, steps, finish.
    fn run_on(&self, original: &Raster) -> Result<Raster> {
        let mut canvas = self.canvas(original);
        let units = Units::full(canvas.width, canvas.height);
        for s in self.prepare_for((original.width, original.height))? {
            s.apply(&mut canvas.bytes, &units, NEVER)?;
        }
        Ok(self.finish(canvas, (original.width, original.height)))
    }

    /// Bend a raster at full resolution. The raster is the canvas itself
    /// (no crop is taken here); use `render` to start from the photo.
    pub fn apply(&self, raster: &mut Raster) -> Result<()> {
        let units = Units::full(raster.width, raster.height);
        self.apply_with(&mut raster.bytes, &units, NEVER)
    }

    /// Bend raster bytes measured by `units` (a preview passes its own size
    /// and the full image's), stopping early if `cancel` says so.
    pub fn apply_with(&self, bytes: &mut [u8], units: &Units, cancel: Cancel<'_>) -> Result<()> {
        for s in self.prepare_all()? {
            s.apply(bytes, units, cancel)?;
        }
        Ok(())
    }
}

/// TOML (de)serialization of parameter values.
///
/// Internally every parameter is text (that is how the CLI receives them).
/// In a recipe file, numbers are written as TOML numbers and everything else
/// as strings, so files look natural (`delay = 4096`, `region = "5%:100%"`).
mod param_values {
    use std::collections::BTreeMap;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(m: &BTreeMap<String, String>, s: S) -> Result<S::Ok, S::Error> {
        let typed: BTreeMap<&str, toml::Value> = m
            .iter()
            .map(|(k, v)| {
                let tv = if let Ok(i) = v.parse::<i64>() {
                    toml::Value::Integer(i)
                } else if let Ok(f) = v.parse::<f64>() {
                    toml::Value::Float(f)
                } else {
                    toml::Value::String(v.clone())
                };
                (k.as_str(), tv)
            })
            .collect();
        typed.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<BTreeMap<String, String>, D::Error> {
        let raw: BTreeMap<String, toml::Value> = BTreeMap::deserialize(d)?;
        raw.into_iter()
            .map(|(k, v)| {
                let text = match v {
                    toml::Value::String(s) => s,
                    toml::Value::Integer(i) => i.to_string(),
                    toml::Value::Float(f) => f.to_string(),
                    toml::Value::Boolean(b) => b.to_string(),
                    other => {
                        return Err(serde::de::Error::custom(format!(
                            "parameter `{k}` has an unsupported value: {other}"
                        )));
                    }
                };
                Ok((k, text))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_round_trip() {
        let mut r = Recipe {
            encoding: Encoding::S16Le,
            channels: 2,
            ..Recipe::default()
        };
        r.steps.push(
            Step::new(
                "echo",
                "delay=0.33rows,decay=0.7,region=5%:100%,encoding=ulaw",
                81723,
            )
            .unwrap(),
        );
        r.steps
            .push(Step::new("backwards", "channels=1", 5).unwrap());
        let text = r.to_toml();
        assert!(text.contains("version = 7"), "{text}");
        assert!(text.contains("encoding = \"s16le\""), "{text}");
        assert!(text.contains("encoding = \"ulaw\""), "{text}");
        assert!(text.contains("decay = 0.7"), "{text}");
        assert!(text.contains("delay = \"0.33rows\""), "{text}");
        assert_eq!(Recipe::from_toml(&text).unwrap(), r);
    }

    #[test]
    fn step_overrides_win() {
        let mut r = Recipe::default();
        r.steps
            .push(Step::new("invert", "encoding=s24be", 1).unwrap());
        r.steps.push(Step::new("invert", "", 1).unwrap());
        let p = r.prepare_all().unwrap();
        assert_eq!((p[0].encoding, p[0].channels), (Encoding::S24Be, 1));
        assert_eq!((p[1].encoding, p[1].channels), (Encoding::U8, 1));
        assert_eq!(p[0].frame(), 3);
    }

    #[test]
    fn rejects_wrong_format_and_versions() {
        assert!(Recipe::from_toml("format = \"x\"\nversion = 2\n").is_err());
        let v1 = Recipe::from_toml("format = \"glitcharaxia-recipe\"\nversion = 1\n")
            .unwrap_err()
            .to_string();
        assert!(v1.contains("archive/jpeg-bytes"), "{v1}");
        let text = "format = \"glitcharaxia-recipe\"\nversion = 2\n[[step]]\neffect = \"echo\"\neffect_version = 99\nseed = 1\n";
        assert!(
            Recipe::from_toml(text)
                .unwrap_err()
                .to_string()
                .contains("version")
        );
        let bad_ch = "format = \"glitcharaxia-recipe\"\nversion = 2\nchannels = 4\n";
        assert!(Recipe::from_toml(bad_ch).is_err());
    }

    #[test]
    fn bad_params_fail_at_build_time() {
        assert!(Step::new("echo", "decay=2", 1).is_err());
        assert!(Step::new("echo", "nope=1", 1).is_err());
        assert!(Step::new("echo", "encoding=mp3", 1).is_err());
        assert!(Step::new("echo", "channels=6", 1).is_err());
        assert!(Step::new("nope", "", 1).is_err());
    }

    #[test]
    fn cancel_stops_a_render() {
        let mut r = Recipe::default();
        r.steps.push(Step::new("invert", "hits=0", 1).unwrap());
        let mut raster = Raster::new(10, 10, vec![0; 300]);
        let units = Units::full(10, 10);
        let err = r.apply_with(&mut raster.bytes, &units, &|| true);
        assert!(matches!(err, Err(Error::Cancelled)));
        r.apply(&mut raster).unwrap();
        assert!(raster.bytes.iter().all(|&b| b == 255));
    }

    fn masked_invert() -> Step {
        use crate::mask::{MaskOp, Mode, Shape};
        let mut s = Step::new("invert", "hits=0,opacity=0.5", 1).unwrap();
        s.mask = Some(Mask {
            invert: false,
            ops: vec![MaskOp::new(
                Shape::Linear {
                    from: [0.0, 0.0],
                    to: [0.0, 0.0],
                },
                Mode::Add,
            )],
        });
        s
    }

    #[test]
    fn masks_and_opacity_round_trip_through_toml() {
        use crate::mask::{MaskOp, Mode, Shape, Stroke};
        let mut r = Recipe::default();
        let mut s = masked_invert();
        let m = s.mask.as_mut().unwrap();
        m.invert = true; // a mask-level key, so `[step.mask]` is written
        m.ops.push(MaskOp {
            opacity: 0.75,
            feather: 0.01,
            ..MaskOp::new(
                Shape::Brush {
                    strokes: vec![
                        Stroke {
                            erase: false,
                            size: 0.05,
                            hardness: 0.25,
                            points: vec![0.1, 0.2, 0.3, 0.4],
                        },
                        Stroke {
                            erase: true,
                            size: 0.02,
                            hardness: 0.5,
                            points: vec![0.2, 0.3],
                        },
                    ],
                },
                Mode::Subtract,
            )
        });
        r.steps.push(s);
        r.steps.push(Step::new("backwards", "", 2).unwrap());
        let text = r.to_toml();
        assert!(text.contains("[step.mask]"), "{text}");
        assert!(text.contains("[[step.mask.op]]"), "{text}");
        assert!(text.contains("shape = \"brush\""), "{text}");
        assert!(text.contains("[[step.mask.op.stroke]]"), "{text}");
        assert!(text.contains("erase = true"), "{text}");
        assert!(text.contains("opacity = 0.75"), "{text}");
        assert!(text.contains("feather = 0.01"), "{text}");
        assert!(text.contains("opacity = 0.5"), "{text}");
        assert_eq!(Recipe::from_toml(&text).unwrap(), r);
    }

    /// A pull step's strokes and pins share `[[step.pull]]`, in order, and
    /// come back the same; other steps may not have any.
    #[test]
    fn pull_marks_round_trip() {
        use crate::pull::{Mark, Pin};
        let pin = Mark::Pin(Pin {
            size: 0.2,
            from: [0.5, 0.5],
            to: [0.56, 0.47],
        });
        let stroke = Mark::Stroke {
            stroke: crate::pull::Stroke {
                size: 0.1,
                strength: 0.5,
                points: vec![0.5, 0.5, 0.6, 0.5],
            },
            brush: Some(2),
        };
        let mut step = Step::new("pull", "", 0).unwrap();
        step.pull = vec![pin.clone(), stroke];
        let r = Recipe {
            steps: vec![step.clone()],
            ..Recipe::default()
        };
        let text = r.to_toml();
        assert!(
            !text.contains("[[step.pin]]") && text.contains("version = 7"),
            "{text}"
        );
        assert_eq!(Recipe::from_toml(&text).unwrap(), r);
        let mut echo = Step::new("echo", "", 1).unwrap();
        echo.pull = vec![pin];
        assert!(echo.prepare(Encoding::default(), 1).is_err());
    }

    /// A version 6 recipe with pins, as written by Phase 8's first builds,
    /// keeps loading, with its pins after its strokes: the same picture.
    /// Files saved now must open in every later build (Phil, 2026-11-24).
    #[test]
    fn version_6_recipes_with_pins_load() {
        use crate::pull::Mark;
        let v6 = "format = \"glitcharaxia-recipe\"\nversion = 6\n\n[[step]]\n\
                  effect = \"pull\"\neffect_version = 1\nseed = 0\n\n\
                  [[step.pin]]\nsize = 0.2\nfrom = [0.5, 0.5]\nto = [0.56, 0.47]\n\n\
                  [[step.pull]]\nsize = 0.1\nstrength = 0.6\npoints = [0.5, 0.5, 0.6, 0.5]\n\n\
                  [[step]]\neffect = \"pull\"\neffect_version = 1\nseed = 0\n\n\
                  [[step.pin]]\nsize = 0.2\nfrom = [0.4, 0.5]\nto = [0.3, 0.47]\n";
        let r = Recipe::from_toml(v6).unwrap();
        let pull = &r.steps[0].pull;
        assert_eq!(pull.len(), 2);
        assert!(matches!(pull[0], Mark::Stroke { brush: None, .. }));
        assert!(matches!(&pull[1], Mark::Pin(p) if p.to == [0.56, 0.47]));
        assert!(matches!(&r.steps[1].pull[..], [Mark::Pin(_)]));
        // Written back as the current version, with one list.
        let text = r.to_toml();
        assert!(!text.contains("step.pin"), "{text}");
        assert_eq!(Recipe::from_toml(&text).unwrap(), r);
        // Version 7 has no `[[step.pin]]`.
        assert!(Recipe::from_toml(&v6.replace("version = 6", "version = 7")).is_err());
    }

    /// D-69: a crop of the result runs the steps on the whole photo and
    /// cuts the frame from what they give, so moving the frame never
    /// changes the glitch inside it. With a work size and `scale_back`,
    /// the frame is cut after the enlargement, and `output_size` says
    /// what comes out.
    #[test]
    fn a_crop_of_the_result_frames_the_finished_picture() {
        let mut bytes = Vec::with_capacity(240 * 160 * 3);
        for i in 0..240 * 160 * 3 {
            bytes.push((i * 7 % 251) as u8);
        }
        let photo = Raster::new(240, 160, bytes);
        let crop = Crop {
            x: 0.25,
            y: 0.1,
            w: 0.5,
            h: 0.6,
        };
        let mut r = Recipe {
            steps: vec![Step::new("echo", "delay=0.4rows", 3).unwrap()],
            ..Recipe::default()
        };
        let whole = r.render(&photo).unwrap();
        r.crop = Some(crop);
        let before = r.render(&photo).unwrap();
        r.crop_result = true;
        let framed = r.render(&photo).unwrap();
        let (x, y, w, h) = crop.rect(240, 160);
        assert_eq!(framed, whole.crop(x, y, w, h));
        assert_eq!(r.output_size(240, 160), (w, h));
        // Cropping first bends a narrower picture: a different glitch.
        assert_eq!((before.width, before.height), (w, h));
        assert_ne!(before, framed);
        // The canvas and the masks ignore it; only the end changes.
        assert_eq!(r.framing(), (None, None));
        assert!(r.mask_frame((240, 160)).is_none());

        // Work size, enlarged back, then framed.
        r.work_size = Some(120);
        r.scale_back = true;
        let out = r.render(&photo).unwrap();
        assert_eq!((out.width, out.height), (w, h));
        assert_eq!(r.output_size(240, 160), (w, h));
        r.scale_back = false;
        let out = r.render(&photo).unwrap();
        let (_, _, sw, sh) = crop.rect(120, 80);
        assert_eq!((out.width, out.height), (sw, sh));
        assert_eq!(r.output_size(240, 160), (sw, sh));

        // Saved and read back; left out when false.
        let text = r.to_toml();
        assert!(text.contains("crop_result = true"), "{text}");
        assert!(text.contains("version = 8"), "{text}");
        assert_eq!(Recipe::from_toml(&text).unwrap(), r);
        // Without it, version 7: the file opens in builds that predate it.
        r.crop_result = false;
        let text = r.to_toml();
        assert!(
            !text.contains("crop_result") && text.contains("version = 7"),
            "{text}"
        );
    }

    /// A pull step's strokes go to `[[step.pull]]` and come back the same.
    #[test]
    fn pull_strokes_round_trip() {
        let mut step = Step::new("pull", "sampling=smooth", 0).unwrap();
        step.pull = vec![
            crate::pull::Stroke {
                size: 0.12,
                strength: 0.6,
                points: vec![0.4, 0.52, 0.43, 0.53, 0.47, 0.55],
            }
            .into(),
            crate::pull::Stroke {
                size: 0.05,
                strength: 1.0,
                points: vec![0.1, 0.1, 0.2, 0.1],
            }
            .into(),
        ];
        let r = Recipe {
            steps: vec![step, Step::new("invert", "", 1).unwrap()],
            ..Recipe::default()
        };
        let text = r.to_toml();
        assert!(text.contains("[[step.pull]]"), "{text}");
        assert!(text.contains("sampling = \"smooth\""), "{text}");
        let back = Recipe::from_toml(&text).unwrap();
        assert_eq!(back, r);
        assert!(back.steps[1].pull.is_empty());
        // Written by hand, `strength` may be left out.
        let hand = "format = \"glitcharaxia-recipe\"\nversion = 5\n\n[[step]]\n\
                    effect = \"pull\"\neffect_version = 1\nseed = 0\n\n\
                    [[step.pull]]\nsize = 0.1\npoints = [0.5, 0.5, 0.6, 0.5]\n";
        assert!(matches!(
            &Recipe::from_toml(hand).unwrap().steps[0].pull[0],
            crate::pull::Mark::Stroke { stroke, .. } if stroke.strength == 0.6
        ));
    }

    #[test]
    fn strokes_belong_to_pull_and_placement_does_not() {
        let stroke = crate::pull::Stroke {
            size: 0.1,
            strength: 0.5,
            points: vec![0.5, 0.5, 0.6, 0.5],
        };
        let mut echo = Step::new("echo", "", 1).unwrap();
        echo.pull = vec![stroke.clone().into()];
        assert!(echo.prepare(Encoding::U8, 1).is_err());
        assert!(Step::new("pull", "hits=4", 1).is_err());
        assert!(Step::new("pull", "region=0%:50%", 1).is_err());
        let mut bad = Step::new("pull", "", 1).unwrap();
        bad.pull = vec![
            crate::pull::Stroke {
                size: 0.0,
                ..stroke
            }
            .into(),
        ];
        assert!(bad.prepare(Encoding::U8, 1).is_err());
    }

    /// Strokes are drawn on the whole photo; on a crop's canvas they land
    /// on the same pixels. Pulling on the photo then cropping gives what
    /// cropping then pulling does, away from the crop's edges (where the
    /// photo outside the crop can be pulled in).
    #[test]
    fn pull_strokes_follow_a_crop() {
        let photo = ramp(200, 160);
        let stroke = crate::pull::Stroke {
            size: 0.2,
            strength: 1.0,
            points: vec![0.45, 0.5, 0.6, 0.55],
        };
        let mut step = Step::new("pull", "", 0).unwrap();
        step.pull = vec![stroke.into()];
        let whole = Recipe {
            steps: vec![step.clone()],
            ..Recipe::default()
        }
        .render(&photo)
        .unwrap();
        let crop = Crop {
            x: 0.25,
            y: 0.25,
            w: 0.5,
            h: 0.5,
        };
        let cropped = Recipe {
            crop: Some(crop),
            steps: vec![step],
            ..Recipe::default()
        }
        .render(&photo)
        .unwrap();
        assert_eq!((cropped.width, cropped.height), (100, 80));
        // The stroke changed the middle; compare it pixel for pixel. Map
        // points fall differently on the two canvases, so allow a pixel's
        // slack in where each one reads from.
        assert_ne!(whole.crop(50, 40, 100, 80), photo.crop(50, 40, 100, 80));
        let expect = whole.crop(50, 40, 100, 80);
        let (mut same, mut total) = (0, 0);
        for y in 20..60 {
            for x in 20..80 {
                let i = (y * 100 + x) * 3;
                total += 1;
                same += (cropped.bytes[i..i + 3] == expect.bytes[i..i + 3]) as usize;
            }
        }
        assert!(same * 10 >= total * 8, "{same} of {total} pixels agree");
    }

    #[test]
    fn version_2_recipes_still_load() {
        let v2 = "format = \"glitcharaxia-recipe\"\nversion = 2\n\n[[step]]\n\
                  effect = \"invert\"\neffect_version = 1\nseed = 3\n";
        let r = Recipe::from_toml(v2).unwrap();
        assert_eq!(r.version, VERSION);
        assert_eq!(r.steps[0].mask, None);
        assert!(Recipe::from_toml(&v2.replace("version = 2", "version = 9")).is_err());
    }

    /// A 6 x 4 picture whose every pixel differs, so resizing shows.
    fn ramp(w: usize, h: usize) -> Raster {
        Raster::new(w, h, (0..w * h * 3).map(|i| (i * 7 % 251) as u8).collect())
    }

    /// D-52: a mask is drawn on the whole photo, so a crop shows exactly the
    /// same masked pixels as cropping the uncropped result.
    #[test]
    fn masks_stay_on_the_photo_when_it_is_cropped() {
        use crate::mask::{Mask, MaskOp, Mode, Shape, Stroke};
        let (w, h) = (256, 128);
        let photo = Raster::new(w, h, (0..w * h * 3).map(|i| (i * 7 % 253) as u8).collect());
        let mut step = Step::new("invert", "hits=0", 1).unwrap();
        step.mask = Some(Mask {
            invert: false,
            ops: vec![
                MaskOp::new(
                    Shape::Radial {
                        from: [0.75, 0.5],
                        to: [0.8125, 0.5],
                        hardness: 1.0,
                    },
                    Mode::Add,
                ),
                MaskOp::new(
                    Shape::Rect {
                        center: [0.625, 0.25],
                        size: [0.125, 0.0625],
                        angle: 0.0,
                    },
                    Mode::Add,
                ),
                MaskOp::new(
                    Shape::Brush {
                        strokes: vec![Stroke {
                            erase: false,
                            size: 0.03125,
                            hardness: 1.0,
                            points: vec![0.5625, 0.75, 0.9375, 0.875],
                        }],
                    },
                    Mode::Add,
                ),
            ],
        });
        let mut r = Recipe {
            steps: vec![step],
            ..Recipe::default()
        };
        let whole = r.render(&photo).unwrap();
        // The right half: x from 128 px, the full height.
        r.crop = Some(Crop {
            x: 0.5,
            y: 0.0,
            w: 0.5,
            h: 1.0,
        });
        let cropped = r.render(&photo).unwrap();
        assert_eq!((cropped.width, cropped.height), (128, 128));
        assert_eq!(cropped, whole.crop(128, 0, 128, 128));
        // And the mask did something, inside and outside.
        assert_ne!(cropped, photo.crop(128, 0, 128, 128));
        let f = r.mask_frame((w, h)).unwrap();
        assert_eq!((f.x, f.sx, f.sy, f.sl), (0.5, 2.0, 1.0, 2.0));
    }

    #[test]
    fn crop_comes_first_and_rounds_to_whole_pixels() {
        let c = Crop {
            x: 0.1,
            y: 0.25,
            w: 0.5,
            h: 0.5,
        };
        assert_eq!(c.rect(480, 320), (48, 80, 240, 160));
        // Rounding each edge, never empty, never outside.
        let tiny = Crop {
            x: 0.999,
            y: 0.0,
            w: 0.0001,
            h: 1.0,
        };
        assert_eq!(tiny.rect(100, 10), (99, 0, 1, 10));
        let mut r = Recipe {
            crop: Some(c),
            work_size: Some(120),
            ..Recipe::default()
        };
        let original = Raster::new(
            480,
            320,
            (0..480 * 320 * 3).map(|i| (i % 251) as u8).collect(),
        );
        assert_eq!(r.canvas_size(480, 320), (120, 80));
        assert_eq!(r.render(&original).unwrap().width, 120);
        r.scale_back = true;
        let out = r.render(&original).unwrap();
        assert_eq!((out.width, out.height), (240, 160));
        // Without a work size the canvas is exactly the cropped pixels.
        r.work_size = None;
        assert_eq!(r.canvas(&original), original.crop(48, 80, 240, 160));
        // Kept in the file, refused when it leaves the photo.
        let back = Recipe::from_toml(&r.to_toml()).unwrap();
        assert_eq!(back.crop, Some(c));
        r.crop = Some(Crop {
            x: 0.6,
            y: 0.0,
            w: 0.5,
            h: 1.0,
        });
        assert!(r.validate().unwrap_err().to_string().contains("crop"));
        r.crop = Some(Crop {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 1.0,
        });
        assert!(r.validate().is_err());
    }

    #[test]
    fn the_canvas_shrinks_to_the_work_size_and_can_scale_back() {
        let original = ramp(600, 400);
        let mut r = Recipe {
            work_size: Some(150),
            steps: vec![Step::new("invert", "hits=0", 1).unwrap()],
            ..Recipe::default()
        };
        assert_eq!(r.canvas_size(600, 400), (150, 100));
        // Never enlarged.
        assert_eq!(r.canvas_size(100, 60), (100, 60));

        // The run: shrink, bend, write at the work size...
        let out = r.render(&original).unwrap();
        let mut want = original.resize(150, 100);
        r.apply(&mut want).unwrap();
        assert_eq!(out, want);
        // ...or enlarged back with hard edges: each canvas pixel becomes a
        // 4 x 4 block.
        r.scale_back = true;
        let big = r.render(&original).unwrap();
        assert_eq!((big.width, big.height), (600, 400));
        assert_eq!(big, want.enlarge(600, 400));
        assert_eq!(&big.bytes[..3], &big.bytes[9..12]);

        // Written to the file, read back the same; too small is refused.
        let back = Recipe::from_toml(&r.to_toml()).unwrap();
        assert_eq!(back, r);
        assert!(r.to_toml().contains("work_size = 150"), "{}", r.to_toml());
        r.work_size = Some(10);
        assert!(r.validate().unwrap_err().to_string().contains("work_size"));
        // Without one, nothing about it is written.
        assert!(!Recipe::default().to_toml().contains("work_size"));
        assert!(!Recipe::default().to_toml().contains("scale_back"));
    }

    #[test]
    fn a_mask_confines_the_step_and_opacity_softens_it() {
        // The linear op with equal ends covers everything (1), so the whole
        // picture is inverted at half opacity: 0 becomes 128 (127.5 rounds up).
        let mut r = Recipe::default();
        r.steps.push(masked_invert());
        let mut raster = Raster::new(4, 2, vec![0; 24]);
        r.apply(&mut raster).unwrap();
        assert!(raster.bytes.iter().all(|&b| b == 128), "{:?}", raster.bytes);

        // Inverted, the same mask hides the step entirely.
        r.steps[0].mask.as_mut().unwrap().invert = true;
        let mut raster = Raster::new(4, 2, vec![0; 24]);
        r.apply(&mut raster).unwrap();
        assert!(raster.bytes.iter().all(|&b| b == 0));
    }

    #[test]
    fn bad_opacity_and_masks_name_the_effect() {
        let err = Step::new("invert", "opacity=2", 1).unwrap_err().to_string();
        assert!(err.contains("invert") && err.contains("opacity"), "{err}");
        let mut s = masked_invert();
        s.mask.as_mut().unwrap().ops[0].feather = -1.0;
        let err = s.prepare(Encoding::U8, 1).err().unwrap().to_string();
        assert!(err.contains("invert") && err.contains("feather"), "{err}");
    }

    #[test]
    fn steps_from_before_the_whole_picture_default_keep_their_bands() {
        // Invert version 1 started with 4 hits of 4%; a step that left them
        // out still means that, and says so once upgraded.
        let old = "format = \"glitcharaxia-recipe\"\nversion = 4\nencoding = \"u8\"\nchannels = 1\n\n\
                   [[step]]\neffect = \"invert\"\neffect_version = 1\nseed = 7\n\n\
                   [[step]]\neffect = \"invert\"\neffect_version = 1\nseed = 7\nhits = \"9\"\n";
        let r = Recipe::from_toml(old).unwrap();
        assert_eq!(r.steps[0].effect_version, 2);
        assert_eq!(r.steps[0].params["hits"], "4");
        assert_eq!(r.steps[0].params["len"], "4%");
        // What the user gave stays.
        assert_eq!(r.steps[1].params["hits"], "9");
        // A new step starts on the whole picture.
        let new = Step::new("invert", "", 7).unwrap();
        assert!(!new.params.contains_key("hits"));
        let mut img = Raster::new(10, 10, vec![0u8; 300]);
        Recipe {
            steps: vec![new],
            ..Recipe::default()
        }
        .apply(&mut img)
        .unwrap();
        assert!(
            img.bytes.iter().all(|&b| b == 255),
            "the whole picture inverts"
        );
    }

    #[test]
    fn adjustments_from_before_the_neutral_defaults_keep_their_look() {
        // Balance version 1 tinted the midtones warm by default (D-58).
        let old = "format = \"glitcharaxia-recipe\"\nversion = 4\nencoding = \"u8\"\nchannels = 1\n\n\
                   [[step]]\neffect = \"balance\"\neffect_version = 1\nseed = 7\nblue = \"5\"\n";
        let r = Recipe::from_toml(old).unwrap();
        assert_eq!(r.steps[0].effect_version, 2);
        assert_eq!(r.steps[0].params["red"], "12");
        assert_eq!(r.steps[0].params["blue"], "5");
        assert!(!r.steps[0].params.contains_key("green"));
    }
}
