//! Byte-domain effects and their registry.
//!
//! Every effect works on one window of the payload at a time, may read bytes
//! anywhere in the payload, but only ever writes inside its window. All math
//! is integer and wraps modulo 256, which is what makes most effects exactly
//! reversible (see `docs/effects.md`).

use std::ops::Range;

use crate::error::{Error, Result};
use crate::params::{Kind, ParamSpec, Params};
use crate::rng::Rng;

mod amplify;
mod backwards;
mod bitcrush;
mod echo;
mod reverb;
mod shift;
mod shuffle;
mod stutter;

/// A configured effect, ready to run on windows of a payload.
pub trait Effect: Send + Sync {
    /// Glitch `buf[window]` in place. May read outside the window.
    fn apply(&self, buf: &mut [u8], window: Range<usize>, rng: &mut Rng);

    /// Undo [`Effect::apply`] exactly, given the same window and an RNG in the
    /// same state. Lossy effects return [`Error::NotReversible`].
    fn invert(&self, buf: &mut [u8], window: Range<usize>, rng: &mut Rng) -> Result<()>;

    /// Whether `invert` will succeed with this configuration.
    fn reversible(&self) -> bool;
}

/// Reversibility as advertised in help text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reversibility {
    Always,
    Never,
    /// Depends on a parameter; the string explains which.
    Depends(&'static str),
}

/// Default placement for an effect (the shared `region`, `hits`, `len`).
#[derive(Debug, Clone, Copy)]
pub struct PlacementDefaults {
    pub region: &'static str,
    pub hits: &'static str,
    pub len: &'static str,
}

/// Static description of an effect: identity, docs, parameters, constructor.
pub struct EffectInfo {
    pub id: &'static str,
    /// Bumped whenever the effect's output for a given input/params/seed
    /// changes, so old recipes can be refused instead of silently replaying
    /// differently.
    pub version: u32,
    pub summary: &'static str,
    pub details: &'static str,
    pub reversible: Reversibility,
    pub params: &'static [ParamSpec],
    pub placement: PlacementDefaults,
    pub example: &'static str,
    pub build: fn(&Params) -> Result<Box<dyn Effect>>,
}

impl EffectInfo {
    /// The effect's own parameters followed by the shared placement ones,
    /// with this effect's placement defaults.
    pub fn all_params(&self) -> Vec<ParamSpec> {
        let mut v = self.params.to_vec();
        v.push(ParamSpec {
            name: "region",
            kind: Kind::Region,
            default: self.placement.region,
            help: "where hits may land, START:END in bytes or percent of the image data",
        });
        v.push(ParamSpec {
            name: "hits",
            kind: Kind::Int {
                min: 0,
                max: 10_000,
            },
            default: self.placement.hits,
            help: "number of glitch windows; 0 = one window over the whole region",
        });
        v.push(ParamSpec {
            name: "len",
            kind: Kind::Size,
            default: self.placement.len,
            help: "length of each window, bytes (4k, 1m) or percent of the image data",
        });
        v
    }
}

/// Every effect, in the order `--list` shows them.
pub static REGISTRY: &[&EffectInfo] = &[
    &echo::INFO,
    &reverb::INFO,
    &shift::INFO,
    &backwards::INFO,
    &shuffle::INFO,
    &amplify::INFO,
    &bitcrush::INFO,
    &stutter::INFO,
];

/// Find an effect by id.
pub fn lookup(id: &str) -> Result<&'static EffectInfo> {
    REGISTRY
        .iter()
        .copied()
        .find(|e| e.id == id)
        .ok_or_else(|| Error::UnknownEffect(id.to_string()))
}

/// Shared helper for lossy effects.
pub(crate) fn not_reversible(id: &str) -> Result<()> {
    Err(Error::NotReversible(format!("`{id}`")))
}

/// Render the effect reference as Markdown (the source of `docs/effects.md`).
pub fn markdown_reference() -> String {
    let mut s = String::new();
    s.push_str("# Effect reference\n\n");
    s.push_str(
        "Generated from the effect registry (`crates/glitchrx-core/src/effects`). \
         Do not edit by hand; run `UPDATE_DOCS=1 cargo test -p glitchrx-core docs` to refresh.\n\n",
    );
    s.push_str(
        "All byte effects edit the de-stuffed scan data (the payload) in windows called hits. \
         Arithmetic wraps modulo 256, so values never clip, and that is what makes most effects \
         exactly reversible. Percent positions and lengths are relative to the payload size, so \
         defaults look similar across image resolutions.\n\n",
    );
    for e in REGISTRY {
        s.push_str(&format!(
            "## {}\n\n{}\n\n{}\n\n",
            e.id, e.summary, e.details
        ));
        let rev = match e.reversible {
            Reversibility::Always => "yes".to_string(),
            Reversibility::Never => "no".to_string(),
            Reversibility::Depends(why) => format!("depends: {why}"),
        };
        s.push_str(&format!("Reversible: {rev}. Version: {}.\n\n", e.version));
        s.push_str("| Parameter | Default | Meaning |\n| --- | --- | --- |\n");
        for p in e.all_params() {
            s.push_str(&format!(
                "| `{}` | `{}` | {} |\n",
                p.name, p.default, p.help
            ));
        }
        s.push_str(&format!("\nExample:\n\n```\n{}\n```\n\n", e.example));
    }
    s
}

#[cfg(test)]
pub(crate) mod testutil {
    //! Helpers shared by the per-effect unit tests.
    use super::*;
    use std::collections::BTreeMap;

    /// Build an effect from a `k=v` list (defaults for the rest).
    pub fn make(info: &EffectInfo, list: &str) -> Box<dyn Effect> {
        let given: BTreeMap<String, String> = crate::params::parse_list(info.id, list).unwrap();
        let p = Params::new(info.id, &info.all_params(), &given).unwrap();
        (info.build)(&p).unwrap()
    }

    /// Apply then invert on `buf[window]` and check we get the input back.
    pub fn round_trip(e: &dyn Effect, buf: &[u8], window: Range<usize>) -> Vec<u8> {
        let mut b = buf.to_vec();
        e.apply(&mut b, window.clone(), &mut Rng::new(1));
        let glitched = b.clone();
        e.invert(&mut b, window, &mut Rng::new(1)).unwrap();
        assert_eq!(b, buf, "invert did not restore the input");
        glitched
    }

    /// Deterministic pseudo-random test buffer.
    pub fn noise(n: usize, seed: u64) -> Vec<u8> {
        let mut r = Rng::new(seed);
        (0..n).map(|_| r.next_u64() as u8).collect()
    }
}
