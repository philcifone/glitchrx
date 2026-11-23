//! JSON shapes exchanged with the browser.
//!
//! The GUI speaks JSON, not TOML. These types translate between the two and
//! keep one quirk out of the frontend: **seeds travel as strings**. A step
//! seed is a full `u64`, and JavaScript numbers lose precision above 2^53, so
//! a seed sent as a JSON number could silently change on the way back.

use std::collections::BTreeMap;

use glitchrx_core::codec::Image;
use glitchrx_core::effects::{self, REGISTRY};
use glitchrx_core::mask::Mask;
use glitchrx_core::params::Kind;
use glitchrx_core::{Encoding, Recipe, Result, Step};
use serde::{Deserialize, Serialize};

/// A recipe as the GUI holds it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecipeDto {
    /// Recipe-wide sample encoding; a step may override it in its params.
    #[serde(default)]
    pub encoding: Encoding,
    #[serde(default = "one")]
    pub channels: usize,
    /// The canvas the steps run on (see `Recipe::work_size`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_size: Option<u32>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub scale_back: bool,
    #[serde(default)]
    pub steps: Vec<StepDto>,
}

fn one() -> usize {
    1
}

impl Default for RecipeDto {
    fn default() -> Self {
        RecipeDto {
            encoding: Encoding::default(),
            channels: 1,
            work_size: None,
            scale_back: false,
            steps: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StepDto {
    pub effect: String,
    /// Absent means "this build's version" (a step made in the GUI).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_version: Option<u32>,
    pub seed: SeedText,
    /// Parameter overrides as text; defaults are omitted. Also carries the
    /// step keys `encoding`, `channels` and `opacity`.
    #[serde(default)]
    pub params: BTreeMap<String, String>,
    /// Where the result shows; absent means everywhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<Mask>,
}

/// A seed that is written as a JSON string but also accepted as a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeedText(pub u64);

impl Serialize for SeedText {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for SeedText {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Num(u64),
            Text(String),
        }
        match Raw::deserialize(d)? {
            Raw::Num(n) => Ok(SeedText(n)),
            Raw::Text(t) => {
                t.trim().parse().map(SeedText).map_err(|_| {
                    serde::de::Error::custom(format!("seed `{t}` is not a whole number"))
                })
            }
        }
    }
}

impl RecipeDto {
    /// Build and validate a core recipe.
    pub fn to_recipe(&self) -> Result<Recipe> {
        let steps = self
            .steps
            .iter()
            .map(|s| {
                let info = effects::lookup(&s.effect)?;
                Ok(Step {
                    effect: s.effect.clone(),
                    effect_version: s.effect_version.unwrap_or(info.version),
                    seed: s.seed.0,
                    params: s.params.clone(),
                    mask: s.mask.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let r = Recipe {
            encoding: self.encoding,
            channels: self.channels,
            work_size: self.work_size,
            scale_back: self.scale_back,
            steps,
            ..Recipe::default()
        };
        r.validate()?; // fail fast with a message naming the step
        Ok(r)
    }

    pub fn from_recipe(r: &Recipe) -> RecipeDto {
        RecipeDto {
            encoding: r.encoding,
            channels: r.channels,
            work_size: r.work_size,
            scale_back: r.scale_back,
            steps: r
                .steps
                .iter()
                .map(|s| StepDto {
                    effect: s.effect.clone(),
                    effect_version: Some(s.effect_version),
                    seed: SeedText(s.seed),
                    params: s.params.clone(),
                    mask: s.mask.clone(),
                })
                .collect(),
        }
    }
}

/// The effect registry, for building the step editor.
#[derive(Debug, Serialize)]
pub struct EffectDto {
    pub id: &'static str,
    pub version: u32,
    /// Picker section: "audio", "data", "pixel" or "adjust". Pixel and adjust effects ignore
    /// the encoding and channels.
    pub group: &'static str,
    /// The Audacity effect this one follows.
    pub audacity: &'static str,
    pub summary: &'static str,
    pub details: &'static str,
    /// Draws random numbers inside each window (see `EffectInfo`).
    pub random_within: bool,
    /// A scaled preview cannot match; render exactly when idle.
    pub exact_preview: bool,
    pub params: Vec<ParamDto>,
}

#[derive(Debug, Serialize)]
pub struct ParamDto {
    pub name: &'static str,
    /// `int`, `float`, `size`, `region` or `choice`.
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choices: Option<&'static [&'static str]>,
    pub default: &'static str,
    pub help: &'static str,
    /// True for the shared placement parameters (`region`, `hits`, `len`),
    /// which the UI tucks away under "placement".
    pub placement: bool,
}

pub fn effects() -> Vec<EffectDto> {
    REGISTRY
        .iter()
        .map(|e| {
            let own = e.params.len();
            EffectDto {
                id: e.id,
                version: e.version,
                group: e.group.name(),
                audacity: e.audacity,
                summary: e.summary,
                details: e.details,
                random_within: e.random_within,
                exact_preview: e.exact_preview,
                params: e
                    .all_params()
                    .into_iter()
                    .enumerate()
                    .map(|(i, p)| {
                        let (kind, min, max, choices) = match p.kind {
                            Kind::Int { min, max } => {
                                ("int", Some(min as f64), Some(max as f64), None)
                            }
                            Kind::Float { min, max } => ("float", Some(min), Some(max), None),
                            Kind::Size => ("size", None, None, None),
                            Kind::Region => ("region", None, None, None),
                            Kind::Choice(c) => ("choice", None, None, Some(c)),
                            Kind::Curve => ("curve", None, None, None),
                        };
                        ParamDto {
                            name: p.name,
                            kind,
                            min,
                            max,
                            choices,
                            default: p.default,
                            help: p.help,
                            placement: i >= own,
                        }
                    })
                    .collect(),
            }
        })
        .collect()
}

/// A sample encoding, for the encoding pickers.
#[derive(Debug, Serialize)]
pub struct EncodingDto {
    pub id: &'static str,
    pub bytes: usize,
    pub describe: &'static str,
}

pub fn encodings() -> Vec<EncodingDto> {
    Encoding::ALL
        .iter()
        .map(|e| EncodingDto {
            id: e.name(),
            bytes: e.width(),
            describe: e.describe(),
        })
        .collect()
}

/// What the GUI shows about an opened photo.
#[derive(Debug, Serialize)]
pub struct DocDto {
    pub id: String,
    pub name: String,
    /// Path relative to `--root` when opened from the server.
    pub path: Option<String>,
    /// Upright size in pixels.
    pub width: usize,
    pub height: usize,
    /// `jpeg`, `png` or `tiff`.
    pub format: &'static str,
    /// The EXIF orientation that was applied (1 = none).
    pub orientation: u8,
    pub has_icc: bool,
    /// The colour profile's name, when it has one.
    pub profile: Option<String>,
    /// AVIF export keeps these colours (no profile, sRGB or Display P3).
    pub avif_keeps_profile: bool,
    pub file_len: usize,
    /// Raw RGB bytes the effects work on.
    pub raw_len: usize,
}

impl DocDto {
    pub fn new(id: &str, name: &str, path: Option<String>, file_len: usize, img: &Image) -> DocDto {
        DocDto {
            id: id.into(),
            name: name.into(),
            path,
            width: img.raster.width,
            height: img.raster.height,
            format: img.format.name(),
            orientation: img.orientation,
            has_icc: img.icc.is_some(),
            profile: img
                .icc
                .as_deref()
                .and_then(glitchrx_core::avif::profile_name),
            avif_keeps_profile: glitchrx_core::avif::keeps_profile(img.icc.as_deref()),
            file_len,
            raw_len: img.raster.bytes.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_seeds_survive_json() {
        let big = u64::MAX - 12345;
        let dto = RecipeDto {
            steps: vec![StepDto {
                effect: "echo".into(),
                effect_version: None,
                seed: SeedText(big),
                params: BTreeMap::new(),
                mask: None,
            }],
            ..RecipeDto::default()
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains(&format!("\"{big}\"")), "{json}");
        let back: RecipeDto = serde_json::from_str(&json).unwrap();
        assert_eq!(back.to_recipe().unwrap().steps[0].seed, big);
        // Numbers are accepted too, for hand-written requests.
        let n: StepDto = serde_json::from_str(r#"{"effect":"echo","seed":7}"#).unwrap();
        assert_eq!(n.seed, SeedText(7));
    }

    #[test]
    fn recipe_round_trip_and_validation() {
        let json = r#"{"encoding":"ulaw","channels":2,"steps":[
            {"effect":"echo","seed":"5","params":{"decay":"0.7","encoding":"s16be"}},
            {"effect":"backwards","seed":"6"}]}"#;
        let dto: RecipeDto = serde_json::from_str(json).unwrap();
        let r = dto.to_recipe().unwrap();
        assert_eq!((r.encoding, r.channels), (Encoding::ULaw, 2));
        assert_eq!(r.steps.len(), 2);
        assert_eq!(RecipeDto::from_recipe(&r).to_recipe().unwrap(), r);
        // Fields from the 2a GUI (mode, restart) are ignored, not errors.
        let old: RecipeDto = serde_json::from_str(r#"{"mode":"safe","restart":"10%"}"#).unwrap();
        assert_eq!(old, RecipeDto::default());

        let bad = r#"{"steps":[{"effect":"echo","seed":"1","params":{"decay":"3"}}]}"#;
        let dto: RecipeDto = serde_json::from_str(bad).unwrap();
        assert!(dto.to_recipe().unwrap_err().to_string().contains("echo"));
    }

    #[test]
    fn registry_lists_placement_params_last() {
        let all = effects();
        assert_eq!(all.len(), REGISTRY.len());
        for e in &all {
            let names: Vec<_> = e
                .params
                .iter()
                .filter(|p| p.placement)
                .map(|p| p.name)
                .collect();
            assert_eq!(names, ["region", "hits", "len"], "{}", e.id);
        }
    }
}
