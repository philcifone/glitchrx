//! Recipes: an ordered list of effect steps with parameters and seeds.
//!
//! A recipe is the whole description of a glitch. Applying the same recipe to
//! the same input always produces the same bytes, which is what gives the
//! GUI free undo and lets a glitch be saved, shared and replayed.
//!
//! On disk a recipe is TOML:
//!
//! ```toml
//! format = "glitcharaxia-recipe"
//! version = 1
//! mode = "safe"
//!
//! [[step]]
//! effect = "echo"
//! effect_version = 1
//! seed = 81723
//! delay = 4096
//! gain = 0.7
//! ```

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::effects::{self, Effect, EffectInfo};
use crate::error::{Error, Result};
use crate::formats::jpeg::{Jpeg, Mode};
use crate::params::{Params, parse_list};
use crate::placement::{Placement, hit_rng};
use crate::restart::Restart;
use crate::rng::derive;

/// Magic string identifying a recipe file.
pub const FORMAT: &str = "glitcharaxia-recipe";
/// Recipe file format version.
pub const VERSION: u32 = 1;

/// One effect application.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub effect: String,
    pub effect_version: u32,
    pub seed: u64,
    /// Parameters as given by the user (defaults are not written out, so a
    /// recipe stays short and readable).
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
        step.prepare()?; // validate now, not halfway through a batch
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

    /// Validate and construct the effect and its placement.
    pub fn prepare(&self) -> Result<Prepared> {
        let info = effects::lookup(&self.effect)?;
        if self.effect_version != info.version {
            return Err(Error::Recipe(format!(
                "step `{}` was made with effect version {}, this build has version {}",
                self.effect, self.effect_version, info.version
            )));
        }
        let params = Params::new(info.id, &info.all_params(), &self.params)?;
        let effect = (info.build)(&params)?;
        Ok(Prepared {
            info,
            effect,
            placement: Placement::from_params(&params),
            seed: self.seed,
        })
    }
}

/// A step that has been validated and is ready to run.
pub struct Prepared {
    pub info: &'static EffectInfo,
    pub effect: Box<dyn Effect>,
    pub placement: Placement,
    pub seed: u64,
}

impl Prepared {
    /// Apply this step to a payload.
    pub fn apply(&self, payload: &mut [u8]) {
        for (h, w) in self
            .placement
            .windows(payload.len(), self.seed)
            .into_iter()
            .enumerate()
        {
            self.effect.apply(payload, w, &mut hit_rng(self.seed, h));
        }
    }

    /// Undo this step: the same windows, last hit first.
    pub fn invert(&self, payload: &mut [u8]) -> Result<()> {
        let windows = self.placement.windows(payload.len(), self.seed);
        for (h, w) in windows.into_iter().enumerate().rev() {
            self.effect.invert(payload, w, &mut hit_rng(self.seed, h))?;
        }
        Ok(())
    }
}

/// A complete glitch description.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub mode: Mode,
    /// Before glitching, losslessly rewrite the file with a restart marker
    /// every N MCU rows or N% of the image height (`0` removes them; absent
    /// leaves the file as is). This sets how far down the image one glitch
    /// can spread. See [`crate::restart`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restart: Option<Restart>,
    #[serde(rename = "step", default)]
    pub steps: Vec<Step>,
}

impl Default for Recipe {
    fn default() -> Self {
        Recipe {
            format: FORMAT.into(),
            version: VERSION,
            mode: Mode::Safe,
            restart: None,
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
        let r: Recipe = toml::from_str(text).map_err(|e| Error::Recipe(e.to_string()))?;
        if r.format != FORMAT {
            return Err(Error::Recipe(format!(
                "not a glitcharaxia recipe (format = {:?})",
                r.format
            )));
        }
        if r.version != VERSION {
            return Err(Error::Recipe(format!(
                "unsupported recipe version {}",
                r.version
            )));
        }
        for s in &r.steps {
            s.prepare()?;
        }
        Ok(r)
    }

    pub fn to_toml(&self) -> String {
        toml::to_string(self).expect("recipe serialization cannot fail")
    }

    fn prepare_all(&self) -> Result<Vec<Prepared>> {
        self.steps.iter().map(Step::prepare).collect()
    }

    /// Steps that cannot be inverted (with their configured parameters),
    /// plus a note if the mode itself prevents reversal.
    pub fn irreversible_parts(&self) -> Result<Vec<String>> {
        let mut out = Vec::new();
        if self.mode == Mode::Raw {
            out.push("raw mode (forged markers change the file layout)".to_string());
        }
        for (i, p) in self.prepare_all()?.iter().enumerate() {
            if !p.effect.reversible() {
                out.push(format!("step {} `{}`", i + 1, p.info.id));
            }
        }
        Ok(out)
    }

    /// The file the effects actually run on: the input itself, or the input
    /// rewritten with restart markers when `restart` is set. The GUI caches
    /// this, since it only changes when the input or `restart` does.
    pub fn prepare_input(&self, input: Vec<u8>) -> Result<Vec<u8>> {
        match self.restart {
            None => Ok(input),
            Some(r) => with_restarts(input, r),
        }
    }

    /// Glitch a JPEG file's bytes.
    pub fn apply(&self, input: Vec<u8>) -> Result<Vec<u8>> {
        let input = self.prepare_input(input)?;
        self.apply_prepared(input)
    }

    /// Glitch a file that already went through [`Recipe::prepare_input`].
    pub fn apply_prepared(&self, input: Vec<u8>) -> Result<Vec<u8>> {
        let steps = self.prepare_all()?;
        let mut jpeg = Jpeg::parse(input, self.mode)?;
        for s in &steps {
            s.apply(jpeg.payload_mut());
        }
        Ok(jpeg.finish())
    }

    /// Undo the recipe on a glitched file: steps inverted, last step first.
    ///
    /// `restart` is ignored here: the glitched file already carries the
    /// markers, and the restored file keeps them (its pixels are identical to
    /// the original, its bytes are those of the prepared input).
    pub fn invert(&self, input: Vec<u8>) -> Result<Vec<u8>> {
        let bad = self.irreversible_parts()?;
        if !bad.is_empty() {
            return Err(Error::NotReversible(bad.join(", ")));
        }
        let steps = self.prepare_all()?;
        let mut jpeg = Jpeg::parse(input, self.mode)?;
        for s in steps.iter().rev() {
            s.invert(jpeg.payload_mut())?;
        }
        Ok(jpeg.finish())
    }
}

/// Rewrite `input` with restart markers at the given interval, keeping any
/// trailing data after the image. Lossless: the pixels do not change.
#[cfg(feature = "turbo")]
pub fn with_restarts(input: Vec<u8>, restart: Restart) -> Result<Vec<u8>> {
    // Parse first: it validates the file, gives the frame size for a percent
    // interval, and finds the trailing bytes, which the transform would
    // otherwise drop.
    let info = Jpeg::parse(input.clone(), Mode::Safe)?.info().clone();
    let rows = restart.rows_for(&info.frame);
    let (image, trailing) = input.split_at(input.len() - info.trailing_len);
    crate::turbo::set_restarts(image, rows, trailing)
        .map_err(|e| Error::Malformed(format!("adding restart markers failed: {e}")))
}

/// Without libjpeg-turbo there is no lossless transform.
#[cfg(not(feature = "turbo"))]
pub fn with_restarts(_input: Vec<u8>, _restart: Restart) -> Result<Vec<u8>> {
    Err(Error::Unsupported(
        "restart markers need the `turbo` feature".into(),
    ))
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
        let mut r = Recipe::default();
        r.steps
            .push(Step::new("echo", "delay=4096,gain=0.7,region=5%:100%,len=4k", 81723).unwrap());
        r.steps.push(Step::new("backwards", "", 5).unwrap());
        r.restart = Some(Restart::MilliPercent(8000));
        let text = r.to_toml();
        assert!(text.contains("restart = \"8%\""), "{text}");
        assert!(text.contains("delay = 4096"), "{text}");
        assert!(text.contains("gain = 0.7"), "{text}");
        assert!(text.contains("region = \"5%:100%\""), "{text}");
        assert_eq!(Recipe::from_toml(&text).unwrap(), r);
    }

    #[test]
    fn rejects_wrong_format_and_versions() {
        assert!(Recipe::from_toml("format = \"x\"\nversion = 1\n").is_err());
        let text = "format = \"glitcharaxia-recipe\"\nversion = 1\n[[step]]\neffect = \"echo\"\neffect_version = 99\nseed = 1\n";
        assert!(
            Recipe::from_toml(text)
                .unwrap_err()
                .to_string()
                .contains("version")
        );
    }

    #[test]
    fn bad_params_fail_at_build_time() {
        assert!(Step::new("echo", "gain=2", 1).is_err());
        assert!(Step::new("echo", "nope=1", 1).is_err());
        assert!(Step::new("nope", "", 1).is_err());
    }

    #[test]
    fn irreversible_detection() {
        let mut r = Recipe::default();
        r.steps.push(Step::new("echo", "", 1).unwrap());
        assert!(r.irreversible_parts().unwrap().is_empty());
        r.steps
            .push(Step::new("amplify", "mode=clip,factor=2", 1).unwrap());
        r.steps.push(Step::new("bitcrush", "", 1).unwrap());
        let bad = r.irreversible_parts().unwrap();
        assert_eq!(bad.len(), 2, "{bad:?}");
    }
}
