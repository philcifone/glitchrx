//! Effects and their registry.
//!
//! Each effect follows one of Audacity's (SPEC 6). It works on one *window*
//! of the raster at a time, in one of two ways:
//!
//! - **Filters** (echo, delay, reverb, amplify, invert, bitcrush, phaser,
//!   wahwah, distort) read the
//!   window's bytes as audio samples in the step's encoding, process them as
//!   floating point, and clip them back to bytes ([`Window::filter`]). They
//!   stream in blocks, so even a whole 26 MP photo never sits in memory as
//!   floats.
//! - **Edits** (backwards, shift, shuffle, stutter) move whole samples
//!   around, like cut and paste in Audacity. They never change a sample's
//!   value, so they work on the bytes directly, a whole frame at a time.
//!   `speed` is in between: it reads samples at a new rate and
//!   interpolates, so it needs the whole window at once, like an edit.
//!
//! - **Pixel effects** (pixelsort) are not audio at all: they see the
//!   window as whole RGB pixels laid out in rows, so the step's encoding
//!   and channels do not apply, and their windows always cover whole
//!   pixels.
//!
//! Effects only ever write inside their window.

use crate::error::{Error, Result};
use crate::params::{Kind, ParamSpec, Params, Units};
use crate::rng::Rng;
use crate::samples::Encoding;

mod amplify;
mod backwards;
mod bitcrush;
mod delay;
mod distort;
mod echo;
mod invert;
mod phaser;
mod pixelsort;
mod reverb;
mod shift;
mod shuffle;
mod speed;
mod stutter;
mod wahwah;

/// Samples processed per block by [`Window::filter`]; also how often a
/// render checks whether it has been cancelled.
const BLOCK: usize = 1 << 16;

/// A configured effect.
pub trait Effect: Send + Sync {
    /// Process one window. `rng` is this hit's own random stream.
    fn run(&self, w: &mut Window<'_>, rng: &mut Rng) -> Result<()>;
}

/// A streaming sample processor; a fresh one is made for every window and
/// channel, so state (delay lines) never leaks between them.
pub trait Filter {
    fn process(&mut self, x: &mut [f64]);
}

/// One window of the raster, as an effect sees it.
pub struct Window<'a> {
    /// The window's bytes, a whole number of frames.
    pub bytes: &'a mut [u8],
    /// Where the window starts in the raster, in bytes. Pixel effects use
    /// it to find rows and columns; for them it is a multiple of 3.
    pub start: usize,
    pub encoding: Encoding,
    /// 1 (mono), 2 (stereo: samples alternate left/right) or 3 (one channel
    /// per colour: with 8-bit samples, R, G and B each processed alone).
    pub channels: usize,
    /// Converts sizes for this raster (full resolution or a preview).
    pub units: &'a Units,
    pub cancel: &'a dyn Fn() -> bool,
}

impl Window<'_> {
    /// Bytes per frame: one sample for every channel.
    pub fn frame(&self) -> usize {
        self.encoding.width() * self.channels
    }

    /// Number of frames (samples per channel).
    pub fn frames(&self) -> usize {
        self.bytes.len() / self.frame()
    }

    /// A byte distance as a whole number of samples per channel (at least 1).
    pub fn samples(&self, bytes: usize) -> usize {
        (bytes / self.frame()).max(1)
    }

    /// Run every channel through a fresh filter from `make`: decode a block
    /// of samples, process it, clip and write it back.
    pub fn filter<F: Filter>(&mut self, make: impl Fn() -> F) -> Result<()> {
        let (enc, width, frame) = (self.encoding, self.encoding.width(), self.frame());
        let frames = self.frames();
        let mut x = Vec::with_capacity(BLOCK.min(frames));
        for ch in 0..self.channels {
            let mut f = make();
            let mut start = 0;
            while start < frames {
                if (self.cancel)() {
                    return Err(Error::Cancelled);
                }
                let end = (start + BLOCK).min(frames);
                x.clear();
                for i in start..end {
                    let at = i * frame + ch * width;
                    x.push(enc.read(&self.bytes[at..at + width]));
                }
                f.process(&mut x);
                for (i, &v) in (start..end).zip(&x) {
                    let at = i * frame + ch * width;
                    enc.write(v, &mut self.bytes[at..at + width]);
                }
                start = end;
            }
        }
        Ok(())
    }
}

/// Default placement for an effect (the shared `region`, `hits`, `len`).
#[derive(Debug, Clone, Copy)]
pub struct PlacementDefaults {
    pub region: &'static str,
    pub hits: &'static str,
    pub len: &'static str,
}

/// Which section of the effect picker an effect belongs to, and how it sees
/// the raster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// Behaves like its Audacity namesake: repeats, ghosts, gain, sweeps.
    Audio,
    /// Treats the bytes as data: flipping, crushing, moving them around.
    Data,
    /// Works on whole RGB pixels in two dimensions (pixel sorting); the
    /// sample encoding and channels do not apply.
    Pixel,
}

impl Group {
    pub fn name(self) -> &'static str {
        match self {
            Group::Audio => "audio",
            Group::Data => "data",
            Group::Pixel => "pixel",
        }
    }
}

/// Static description of an effect: identity, docs, parameters, constructor.
pub struct EffectInfo {
    pub id: &'static str,
    pub group: Group,
    /// Bumped whenever the effect's output for a given input, parameters and
    /// seed changes, so old recipes are refused instead of silently
    /// replaying differently.
    pub version: u32,
    /// The Audacity effect this one follows.
    pub audacity: &'static str,
    pub summary: &'static str,
    pub details: &'static str,
    pub params: &'static [ParamSpec],
    pub placement: PlacementDefaults,
    /// True if the effect draws random numbers inside each window (beyond
    /// where its hits land). With `hits=0` and this false, the seed changes
    /// nothing: there is one window over the whole region and nothing to
    /// pick. Front ends use this to say when re-rolling a seed is pointless.
    pub random_within: bool,
    /// True if a scaled preview cannot resemble the export, so front ends
    /// should render it at full resolution once the user pauses. The
    /// phaser is the case: its all-pass stages act as delays of a few
    /// samples, and which colour channels they mix depends on that delay
    /// modulo 3, which no smaller raster can reproduce.
    pub exact_preview: bool,
    pub example: &'static str,
    pub build: fn(&Params) -> Result<Box<dyn Effect>>,
}

impl EffectInfo {
    /// The effect's own parameters followed by the shared placement ones.
    pub fn all_params(&self) -> Vec<ParamSpec> {
        let mut v = self.params.to_vec();
        v.push(ParamSpec {
            name: "region",
            kind: Kind::Region,
            default: self.placement.region,
            help: "where hits may land, START:END; percent of the image runs top to bottom",
        });
        v.push(ParamSpec {
            name: "hits",
            kind: Kind::Int { min: 0, max: 1000 },
            default: self.placement.hits,
            help: "number of windows at seeded places; 0 = one window over the whole region",
        });
        v.push(ParamSpec {
            name: "len",
            kind: Kind::Size,
            default: self.placement.len,
            help: "length of each window (%, rows, px or bytes); unused when hits=0",
        });
        v
    }
}

/// Every effect, in the order `--list` shows them.
pub static REGISTRY: &[&EffectInfo] = &[
    &echo::INFO,
    &delay::INFO,
    &reverb::INFO,
    &amplify::INFO,
    &invert::INFO,
    &bitcrush::INFO,
    &backwards::INFO,
    &shift::INFO,
    &shuffle::INFO,
    &stutter::INFO,
    &phaser::INFO,
    &wahwah::INFO,
    &distort::INFO,
    &speed::INFO,
    &pixelsort::INFO,
];

/// Find an effect by id.
pub fn lookup(id: &str) -> Result<&'static EffectInfo> {
    REGISTRY
        .iter()
        .copied()
        .find(|e| e.id == id)
        .ok_or_else(|| Error::UnknownEffect(id.to_string()))
}

/// Shared parameter: a channel-phase offset added to a distance.
pub(crate) const PHASE: ParamSpec = ParamSpec {
    name: "phase",
    kind: Kind::Int { min: 0, max: 5 },
    default: "1",
    help: "extra bytes on top of the distance: 0 keeps colours, 1 or 2 rotates them",
};

/// Render the effect reference as Markdown (the source of `docs/effects.md`).
pub fn markdown_reference() -> String {
    let mut s = String::new();
    s.push_str("# Effect reference\n\n");
    s.push_str(
        "Generated from the effect registry (`crates/glitchrx-core/src/effects`). \
         Do not edit by hand; run `UPDATE_DOCS=1 cargo test -p glitchrx-core --test docs` to refresh.\n\n",
    );
    s.push_str(
        "Every effect works on the decoded image as raw bytes: R, G, B for each pixel, row \
         after row, the way an uncompressed TIFF stores it. Each step reads those bytes as \
         audio samples in an *encoding* (as Audacity's Import Raw Data does), applies the \
         effect, and clips the result back to bytes.\n\n",
    );
    s.push_str(
        "Sizes take units: `%` of the image (which runs top to bottom), `rows` (a fractional \
         row is that share of the width), `px`, or plain bytes. A distance of whole pixels \
         keeps colours; `phase` adds 1 or 2 bytes, which rotates red into green into blue.\n\n",
    );
    s.push_str("## Encodings\n\n| Encoding | Bytes | Character |\n| --- | --- | --- |\n");
    for e in Encoding::ALL {
        s.push_str(&format!(
            "| `{}` | {} | {} |\n",
            e.name(),
            e.width(),
            e.describe()
        ));
    }
    s.push_str(
        "\nSet once per recipe (`--encoding`, `--channels`), or per step with \
         `encoding=` and `channels=` in the step's parameters.\n\n",
    );
    for e in REGISTRY {
        s.push_str(&format!(
            "## {}\n\n{}\n\n{}\n\n",
            e.id, e.summary, e.details
        ));
        if e.group == Group::Pixel {
            s.push_str(&format!("Version: {}.\n\n", e.version));
        } else {
            s.push_str(&format!(
                "Follows Audacity's {}. Version: {}.\n\n",
                e.audacity, e.version
            ));
        }
        if e.group == Group::Pixel {
            s.push_str(
                "A pixel effect: it works on whole RGB pixels, so the encoding and channels \
                 do not apply.\n\n",
            );
        }
        if e.exact_preview {
            s.push_str(
                "Live previews of this effect are approximate; the GUI renders it at full \
                 resolution when you pause.\n\n",
            );
        }
        s.push_str(if e.random_within {
            "The seed picks where hits land and what happens inside each window.\n\n"
        } else {
            "The seed only picks where hits land, so it has no effect with `hits=0`.\n\n"
        });
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

    /// Run `e` over all of `bytes` as one window of a `width`-pixel-wide
    /// raster, mono, in `enc`.
    pub fn run(e: &dyn Effect, bytes: &mut [u8], width: usize, enc: Encoding) {
        let units = Units::full(width, bytes.len() / (width * 3));
        let mut w = Window {
            bytes,
            start: 0,
            encoding: enc,
            channels: 1,
            units: &units,
            cancel: &|| false,
        };
        e.run(&mut w, &mut Rng::new(1)).unwrap();
    }

    /// Deterministic pseudo-random test buffer.
    pub fn noise(n: usize, seed: u64) -> Vec<u8> {
        let mut r = Rng::new(seed);
        (0..n).map(|_| r.next_u64() as u8).collect()
    }

    /// u8 samples for the given levels (-1.0..1.0).
    pub fn u8_of(levels: &[f64]) -> Vec<u8> {
        levels
            .iter()
            .map(|&l| {
                let mut b = [0u8];
                Encoding::U8.write(l, &mut b);
                b[0]
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_channels_filter_each_colour_alone() {
        // A pure red picture through a wahwah: mono blends R, G and B into
        // grey-ish values; three channels leave G and B untouched at zero.
        let red: Vec<u8> = [255u8, 0, 0].repeat(400);
        let units = Units::full(20, 20);
        let e = testutil::make(lookup("wahwah").unwrap(), "");
        for (channels, keeps_red) in [(1, false), (3, true)] {
            let mut b = red.clone();
            let mut w = Window {
                bytes: &mut b,
                start: 0,
                encoding: Encoding::U8,
                channels,
                units: &units,
                cancel: &|| false,
            };
            e.run(&mut w, &mut Rng::new(1)).unwrap();
            let pure = b
                .chunks(3)
                .skip(50)
                .all(|p| p[1] < 4 && p[2] < 4 && p[0] > 250);
            assert_eq!(pure, keeps_red, "channels={channels}");
        }
    }

    #[test]
    fn stereo_filters_process_channels_separately() {
        // Invert only touches samples; with 2 channels every byte is still
        // processed exactly once.
        let e = testutil::make(&invert::INFO, "");
        let mut b = vec![128u8 + 10; 12];
        let units = Units::full(4, 1);
        let mut w = Window {
            bytes: &mut b,
            start: 0,
            encoding: Encoding::U8,
            channels: 2,
            units: &units,
            cancel: &|| false,
        };
        e.run(&mut w, &mut Rng::new(1)).unwrap();
        assert!(b.iter().all(|&v| v == 128 - 10), "{b:?}");
    }

    #[test]
    fn cancellation_stops_a_filter() {
        let e = testutil::make(&invert::INFO, "");
        let mut b = vec![0u8; 30];
        let units = Units::full(10, 1);
        let mut w = Window {
            bytes: &mut b,
            start: 0,
            encoding: Encoding::U8,
            channels: 1,
            units: &units,
            cancel: &|| true,
        };
        assert!(matches!(
            e.run(&mut w, &mut Rng::new(1)),
            Err(Error::Cancelled)
        ));
    }

    #[test]
    fn registry_ids_are_unique_and_documented() {
        let mut ids: Vec<_> = REGISTRY.iter().map(|e| e.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), REGISTRY.len());
        for e in REGISTRY {
            assert!(!e.summary.is_empty() && !e.audacity.is_empty(), "{}", e.id);
            // Every default must parse.
            let p = Params::new(e.id, &e.all_params(), &Default::default());
            assert!(p.is_ok(), "{}: {:?}", e.id, p.err());
        }
    }
}
