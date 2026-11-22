//! Amplify around the midpoint, wrapping (reversible) or clipping (lossy).

use std::ops::Range;

use super::{Effect, EffectInfo, PlacementDefaults, Reversibility, not_reversible};
use crate::error::{Error, Result};
use crate::params::{Kind, ParamSpec, Params, to_q8};
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "amplify",
    version: 1,
    summary: "Scale bytes away from the midpoint 128 by `factor`.",
    details: "`wrap` mode computes `(x - 128) * factor + 128` modulo 256 with an odd whole-number \
              factor. Odd numbers have a multiplicative inverse modulo 256 (3 * 171 = 513 = 2*256 + 1), \
              so multiplying by that inverse undoes the effect exactly. `clip` mode is the Audacity-style \
              version: any decimal factor, values clamped to 0..255, information lost.",
    reversible: Reversibility::Depends("wrap mode yes, clip mode no"),
    params: &[
        ParamSpec {
            name: "factor",
            kind: Kind::Float {
                min: -255.0,
                max: 255.0,
            },
            default: "3",
            help: "gain; wrap mode needs an odd whole number",
        },
        ParamSpec {
            name: "mode",
            kind: Kind::Choice(&["wrap", "clip"]),
            default: "wrap",
            help: "wrap (reversible) or clip (lossy, Audacity-like)",
        },
    ],
    placement: PlacementDefaults {
        region: "5%:100%",
        hits: "3",
        len: "0.3%",
    },
    example: "glitchrx --amplify factor=5 photo.jpg",
    build,
};

fn build(p: &Params) -> Result<Box<dyn Effect>> {
    let f = p.float("factor");
    match p.choice("mode") {
        "wrap" => {
            if f.fract() != 0.0 || (f as i64) % 2 == 0 {
                return Err(Error::param(
                    "amplify",
                    format!("wrap mode needs an odd whole-number factor, got {f}"),
                ));
            }
            let m = (f as i64).rem_euclid(256) as u8;
            Ok(Box::new(Wrap {
                m,
                m_inv: inverse_mod_256(m),
            }))
        }
        _ => Ok(Box::new(Clip { q: to_q8(f) })),
    }
}

/// Multiplicative inverse of an odd number modulo 256.
///
/// Newton's iteration `inv = inv * (2 - m * inv)` doubles the number of
/// correct low bits each step. Any odd `m` is its own inverse modulo 8
/// (3 correct bits), so two steps give 12 bits, more than the 8 we need.
fn inverse_mod_256(m: u8) -> u8 {
    debug_assert!(m % 2 == 1);
    let mut inv = m;
    for _ in 0..2 {
        inv = inv.wrapping_mul(2u8.wrapping_sub(m.wrapping_mul(inv)));
    }
    inv
}

/// `(x - 128) * m + 128` modulo 256.
fn scale_wrap(x: u8, m: u8) -> u8 {
    x.wrapping_sub(128).wrapping_mul(m).wrapping_add(128)
}

struct Wrap {
    m: u8,
    m_inv: u8,
}

impl Effect for Wrap {
    fn apply(&self, buf: &mut [u8], w: Range<usize>, _rng: &mut Rng) {
        for b in &mut buf[w] {
            *b = scale_wrap(*b, self.m);
        }
    }

    fn invert(&self, buf: &mut [u8], w: Range<usize>, _rng: &mut Rng) -> Result<()> {
        for b in &mut buf[w] {
            *b = scale_wrap(*b, self.m_inv);
        }
        Ok(())
    }

    fn reversible(&self) -> bool {
        true
    }
}

struct Clip {
    /// Gain in Q8.
    q: i32,
}

impl Effect for Clip {
    fn apply(&self, buf: &mut [u8], w: Range<usize>, _rng: &mut Rng) {
        for b in &mut buf[w] {
            // `>>` on i32 is an arithmetic shift (rounds toward -infinity),
            // identical on every platform.
            let v = (((*b as i32 - 128) * self.q) >> 8) + 128;
            *b = v.clamp(0, 255) as u8;
        }
    }

    fn invert(&self, _: &mut [u8], _: Range<usize>, _: &mut Rng) -> Result<()> {
        not_reversible("amplify (clip mode)")
    }

    fn reversible(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::testutil::*;

    #[test]
    fn inverse_is_correct_for_every_odd_byte() {
        for m in (1..=255u8).step_by(2) {
            assert_eq!(m.wrapping_mul(inverse_mod_256(m)), 1, "m = {m}");
        }
    }

    #[test]
    fn wrap_hand_checked_and_round_trips() {
        let e = make(&INFO, "factor=3");
        let mut b = vec![128, 129, 127];
        e.apply(&mut b, 0..3, &mut Rng::new(0));
        assert_eq!(b, vec![128, 131, 125]);
        round_trip(e.as_ref(), &noise(500, 6), 0..500);
        round_trip(make(&INFO, "factor=-7").as_ref(), &noise(500, 7), 0..500);
    }

    #[test]
    fn clip_clamps_and_refuses_inverse() {
        let e = make(&INFO, "factor=4,mode=clip");
        let mut b = vec![255, 0, 130];
        e.apply(&mut b, 0..3, &mut Rng::new(0));
        assert_eq!(b, vec![255, 0, 136]);
        assert!(!e.reversible());
        assert!(e.invert(&mut b, 0..3, &mut Rng::new(0)).is_err());
    }

    #[test]
    fn wrap_rejects_even_factor() {
        let p = crate::params::Params::new(
            "amplify",
            &INFO.all_params(),
            &crate::params::parse_list("amplify", "factor=2").unwrap(),
        )
        .unwrap();
        assert!(super::build(&p).is_err());
    }
}
