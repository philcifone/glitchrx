//! JSON shapes exchanged with the browser.
//!
//! The GUI speaks JSON, not TOML. These types translate between the two and
//! keep one quirk out of the frontend: **seeds travel as strings**. A step
//! seed is a full `u64`, and JavaScript numbers lose precision above 2^53, so
//! a seed sent as a JSON number could silently change on the way back.

use std::collections::BTreeMap;

use glitchrx_core::effects::{self, REGISTRY, Reversibility};
use glitchrx_core::formats::jpeg::Info;
use glitchrx_core::params::Kind;
use glitchrx_core::{Mode, Recipe, Restart, Result, Step};
use serde::{Deserialize, Serialize};

/// A recipe as the GUI holds it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RecipeDto {
    #[serde(default)]
    pub mode: Mode,
    /// `"10%"`, `"12"`, `"0"`, or `null` to leave the file's markers alone.
    #[serde(default)]
    pub restart: Option<String>,
    #[serde(default)]
    pub steps: Vec<StepDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StepDto {
    pub effect: String,
    /// Absent means "this build's version" (a step made in the GUI).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_version: Option<u32>,
    pub seed: SeedText,
    /// Parameter overrides as text; defaults are omitted.
    #[serde(default)]
    pub params: BTreeMap<String, String>,
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
        let restart = match self.restart.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(text) => Some(Restart::parse(text)?),
        };
        let steps = self
            .steps
            .iter()
            .map(|s| {
                let info = effects::lookup(&s.effect)?;
                let step = Step {
                    effect: s.effect.clone(),
                    effect_version: s.effect_version.unwrap_or(info.version),
                    seed: s.seed.0,
                    params: s.params.clone(),
                };
                step.prepare()?; // fail fast with a message naming the step
                Ok(step)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Recipe {
            mode: self.mode,
            restart,
            steps,
            ..Recipe::default()
        })
    }

    pub fn from_recipe(r: &Recipe) -> RecipeDto {
        RecipeDto {
            mode: r.mode,
            restart: r.restart.map(|x| x.to_string()),
            steps: r
                .steps
                .iter()
                .map(|s| StepDto {
                    effect: s.effect.clone(),
                    effect_version: Some(s.effect_version),
                    seed: SeedText(s.seed),
                    params: s.params.clone(),
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
    pub summary: &'static str,
    pub details: &'static str,
    /// `"always"`, `"never"`, or an explanation of what it depends on.
    pub reversible: String,
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
                summary: e.summary,
                details: e.details,
                reversible: match e.reversible {
                    Reversibility::Always => "always".into(),
                    Reversibility::Never => "never".into(),
                    Reversibility::Depends(why) => why.into(),
                },
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

/// What the GUI shows about an opened photo.
#[derive(Debug, Serialize)]
pub struct DocDto {
    pub id: String,
    pub name: String,
    /// Path relative to `--root` when opened from the server.
    pub path: Option<String>,
    pub width: u16,
    pub height: u16,
    pub encoding: String,
    pub subsampling: String,
    pub file_len: usize,
    pub payload_len: usize,
    pub restart_interval: u16,
    pub has_exif_thumbnail: bool,
    pub trailing_len: usize,
}

impl DocDto {
    pub fn new(id: &str, name: &str, path: Option<String>, info: &Info) -> DocDto {
        let f = &info.frame;
        DocDto {
            id: id.into(),
            name: name.into(),
            path,
            width: f.width,
            height: f.height,
            encoding: if f.progressive() {
                format!("progressive ({} scans)", info.scans)
            } else {
                "baseline".into()
            },
            subsampling: f.subsampling(),
            file_len: info.file_len,
            payload_len: info.payload_len,
            restart_interval: info.restart_interval,
            has_exif_thumbnail: info.has_exif_thumbnail,
            trailing_len: info.trailing_len,
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
        let json = r#"{"mode":"safe","restart":"10%","steps":[
            {"effect":"echo","seed":"5","params":{"gain":"0.7"}},
            {"effect":"backwards","seed":"6"}]}"#;
        let dto: RecipeDto = serde_json::from_str(json).unwrap();
        let r = dto.to_recipe().unwrap();
        assert_eq!(r.restart, Some(Restart::MilliPercent(10_000)));
        assert_eq!(r.steps.len(), 2);
        assert_eq!(RecipeDto::from_recipe(&r).to_recipe().unwrap(), r);

        let bad = r#"{"steps":[{"effect":"echo","seed":"1","params":{"gain":"3"}}]}"#;
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
