//! Shared parts of the adjustments (the `adjust` group, SPEC D-49): the
//! photo tools that are not glitches at all. Levels, curves, hue and
//! saturation, and color balance work on whole RGB pixels like pixel
//! sorting, and are ordinary steps, so masks, opacity and placement apply
//! to them as to any other step.
//!
//! Like everything else they use basic `f64` operations only (D-25), and
//! where an output depends on one channel value alone they compute a
//! 256-entry table once and look every pixel up in it.

use super::Window;
use crate::error::{Error, Result};
use crate::params::{Kind, ParamSpec, Params};

/// Which colors an adjustment changes.
pub(crate) const CHANNEL: ParamSpec = ParamSpec {
    name: "channel",
    kind: Kind::Choice(&["rgb", "red", "green", "blue"]),
    default: "rgb",
    help: "which colors it changes: all three, or one",
};

/// The channels `channel` selects, as R, G, B.
pub(crate) fn channels(p: &Params) -> [bool; 3] {
    match p.choice("channel") {
        "red" => [true, false, false],
        "green" => [false, true, false],
        "blue" => [false, false, true],
        _ => [true, true, true],
    }
}

/// Pixels between checks of the cancel flag.
const BLOCK: usize = 1 << 16;

/// Run `f` on every pixel of the window.
pub(crate) fn per_pixel(w: &mut Window<'_>, mut f: impl FnMut(&mut [u8])) -> Result<()> {
    for block in w.bytes.chunks_mut(BLOCK * 3) {
        if (w.cancel)() {
            return Err(Error::Cancelled);
        }
        block.chunks_exact_mut(3).for_each(&mut f);
    }
    Ok(())
}

/// Map the selected channels of every pixel through `table`.
pub(crate) fn apply_table(w: &mut Window<'_>, table: &[u8; 256], which: [bool; 3]) -> Result<()> {
    per_pixel(w, |px| {
        for c in 0..3 {
            if which[c] {
                px[c] = table[px[c] as usize];
            }
        }
    })
}

/// A table from a function of the input value (0 to 255, as a float) to
/// an output in 0..1.
pub(crate) fn table(f: impl Fn(f64) -> f64) -> [u8; 256] {
    let mut t = [0u8; 256];
    for (i, v) in t.iter_mut().enumerate() {
        *v = to_byte(f(i as f64));
    }
    t
}

/// 0..1 to a byte, rounding half up and clamping.
pub(crate) fn to_byte(v: f64) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Brightness 0..1 with the Rec. 709 weights (as pixel sorting uses).
pub(crate) fn luma(r: f64, g: f64, b: f64) -> f64 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}
