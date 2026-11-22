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
//! version = 2
//! encoding = "u8"     # how steps read bytes as samples; a step may override
//! channels = 1
//!
//! [[step]]
//! effect = "echo"
//! effect_version = 1
//! seed = 81723
//! encoding = "ulaw"   # this step only
//! delay = "0.33rows"
//! decay = 0.5
//! ```

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::effects::{self, Effect, EffectInfo, Window};
use crate::error::{Error, Result};
use crate::params::{Params, Units, parse_list};
use crate::placement::{Placement, hit_rng};
use crate::raster::Raster;
use crate::rng::derive;
use crate::samples::Encoding;

/// Magic string identifying a recipe file.
pub const FORMAT: &str = "glitcharaxia-recipe";
/// Recipe file format version.
pub const VERSION: u32 = 2;

/// Step keys that are not effect parameters but per-step overrides of the
/// recipe's sample format.
const ENCODING_KEY: &str = "encoding";
const CHANNELS_KEY: &str = "channels";

/// A render that can be abandoned: called between hits and between blocks
/// of samples; returning true stops the render with [`Error::Cancelled`].
pub type Cancel<'a> = &'a (dyn Fn() -> bool + Sync);

/// Never cancel.
pub const NEVER: Cancel<'static> = &|| false;

fn parse_channels(s: &str) -> Result<usize> {
    match s.trim() {
        "1" => Ok(1),
        "2" => Ok(2),
        other => Err(Error::Recipe(format!(
            "channels must be 1 (mono) or 2 (stereo), not `{other}`"
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

    /// This step's encoding override, if any.
    pub fn encoding(&self) -> Result<Option<Encoding>> {
        self.params
            .get(ENCODING_KEY)
            .map(|e| Encoding::parse(e))
            .transpose()
    }

    /// This step's channel-count override, if any.
    pub fn channels(&self) -> Result<Option<usize>> {
        self.params
            .get(CHANNELS_KEY)
            .map(|c| parse_channels(c))
            .transpose()
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
        let mut given = self.params.clone();
        given.remove(ENCODING_KEY);
        given.remove(CHANNELS_KEY);
        let params = Params::new(info.id, &info.all_params(), &given)?;
        let effect = (info.build)(&params)?;
        Ok(Prepared {
            info,
            effect,
            placement: Placement::from_params(&params),
            seed: self.seed,
            encoding,
            channels,
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
}

impl Prepared {
    /// Bytes per frame (one sample for each channel).
    pub fn frame(&self) -> usize {
        self.encoding.width() * self.channels
    }

    /// Apply this step to raster bytes measured by `units`.
    pub fn apply(&self, bytes: &mut [u8], units: &Units, cancel: Cancel<'_>) -> Result<()> {
        let windows = self.placement.windows(units, self.frame(), self.seed);
        for (h, w) in windows.into_iter().enumerate() {
            if cancel() {
                return Err(Error::Cancelled);
            }
            let mut win = Window {
                bytes: &mut bytes[w],
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
    #[serde(rename = "step", default)]
    pub steps: Vec<Step>,
}

impl Default for Recipe {
    fn default() -> Self {
        Recipe {
            format: FORMAT.into(),
            version: VERSION,
            encoding: Encoding::default(),
            channels: 1,
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
        let raw: toml::Table = toml::from_str(text).map_err(|e| Error::Recipe(e.to_string()))?;
        if raw.get("format").and_then(|v| v.as_str()) != Some(FORMAT) {
            return Err(Error::Recipe(format!(
                "not a glitcharaxia recipe (no `format = \"{FORMAT}\"`)"
            )));
        }
        match raw.get("version").and_then(|v| v.as_integer()) {
            Some(2) => {}
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
        let r: Recipe = toml::from_str(text).map_err(|e| Error::Recipe(e.to_string()))?;
        r.validate()?;
        Ok(r)
    }

    pub fn to_toml(&self) -> String {
        toml::to_string(self).expect("recipe serialization cannot fail")
    }

    /// Check every step (and the recipe-wide channel count).
    pub fn validate(&self) -> Result<()> {
        self.prepare_all().map(|_| ())
    }

    /// Validate and build every step.
    pub fn prepare_all(&self) -> Result<Vec<Prepared>> {
        parse_channels(&self.channels.to_string())?;
        self.steps
            .iter()
            .map(|s| s.prepare(self.encoding, self.channels))
            .collect()
    }

    /// Bend a raster at full resolution.
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
        assert!(text.contains("version = 2"), "{text}");
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
        let bad_ch = "format = \"glitcharaxia-recipe\"\nversion = 2\nchannels = 3\n";
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
}
