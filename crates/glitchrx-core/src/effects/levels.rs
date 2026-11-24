//! `levels`: black point, white point and gamma, as in Photoshop's Levels.

use super::adjust::{CHANNEL, apply_table, channels, table};
use super::{Effect, EffectInfo, Group, PlacementDefaults, Upgrade, Window};
use crate::dsp::pow;
use crate::error::{Error, Result};
use crate::params::{Kind, ParamSpec, Params};
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "levels",
    group: Group::Adjust,
    version: 2,
    // Version 1 started with a visible change; adjustments now start
    // neutral (D-58).
    upgrades: &[Upgrade {
        to: 2,
        fill: &[("black", "12"), ("white", "240")],
    }],
    audacity: "none (an adjustment)",
    summary: "Set the black point, white point and midtone gamma.",
    details: "Input values at or below `black` become `out_black`, at or above `white` \
              become `out_white`, and the range between is stretched to fit. `gamma` \
              bends the middle: above 1 lightens the midtones, below 1 darkens them. \
              `channel` limits it to one colour, which shifts the colour balance.",
    params: &[
        CHANNEL,
        ParamSpec {
            name: "black",
            kind: Kind::Int { min: 0, max: 254 },
            default: "0",
            help: "input black point: this value and below become out_black",
        },
        ParamSpec {
            name: "white",
            kind: Kind::Int { min: 1, max: 255 },
            default: "255",
            help: "input white point: this value and above become out_white",
        },
        ParamSpec {
            name: "gamma",
            kind: Kind::Float {
                min: 0.1,
                max: 10.0,
            },
            default: "1",
            help: "midtones: above 1 lighter, below 1 darker",
        },
        ParamSpec {
            name: "out_black",
            kind: Kind::Int { min: 0, max: 255 },
            default: "0",
            help: "darkest output value",
        },
        ParamSpec {
            name: "out_white",
            kind: Kind::Int { min: 0, max: 255 },
            default: "255",
            help: "lightest output value (below out_black inverts)",
        },
    ],
    placement: PlacementDefaults {
        region: "0%:100%",
        hits: "0",
        len: "10%",
    },
    random_within: false,
    exact_preview: false,
    example: "glitchrx --levels black=20,white=230,gamma=1.2 photo.jpg",
    build,
};

fn build(p: &Params) -> Result<Box<dyn Effect>> {
    let (black, white) = (p.int("black") as f64, p.int("white") as f64);
    if white <= black {
        return Err(Error::param(
            "levels",
            format!("white ({white}) must be above black ({black})"),
        ));
    }
    let (out_black, out_white) = (
        p.int("out_black") as f64 / 255.0,
        p.int("out_white") as f64 / 255.0,
    );
    let inv_gamma = 1.0 / p.float("gamma");
    // Measured in whole input steps, so a value half way between the
    // points divides exactly to 0.5 (rather than via 1/255 rounding).
    Ok(Box::new(Levels {
        which: channels(p),
        table: table(|v| {
            let t = ((v - black) / (white - black)).clamp(0.0, 1.0);
            out_black + pow(t, inv_gamma) * (out_white - out_black)
        }),
    }))
}

struct Levels {
    which: [bool; 3],
    table: [u8; 256],
}

impl Effect for Levels {
    fn run(&self, w: &mut Window<'_>, _rng: &mut Rng) -> Result<()> {
        apply_table(w, &self.table, self.which)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::testutil::*;
    use crate::samples::Encoding;

    #[test]
    fn stretches_between_the_points() {
        let e = make(&INFO, "black=50,white=150");
        let mut b = vec![0, 50, 100, 150, 200, 255];
        run(e.as_ref(), &mut b, 2, Encoding::U8);
        assert_eq!(b, vec![0, 0, 128, 255, 255, 255]);
    }

    #[test]
    fn gamma_lightens_the_middle_and_keeps_the_ends() {
        let e = make(&INFO, "black=0,white=255,gamma=2");
        let mut b = vec![0, 64, 128, 192, 255, 255];
        run(e.as_ref(), &mut b, 2, Encoding::U8);
        // (128/255)^(1/2) * 255 = 180.7
        assert_eq!(b, vec![0, 128, 181, 221, 255, 255]);
    }

    #[test]
    fn one_channel_and_the_output_range() {
        let e = make(
            &INFO,
            "channel=green,black=0,white=255,out_black=100,out_white=200",
        );
        let mut b = vec![0, 0, 0, 255, 255, 255];
        run(e.as_ref(), &mut b, 2, Encoding::U8);
        assert_eq!(b, vec![0, 100, 0, 255, 200, 255]);
    }

    #[test]
    fn white_must_be_above_black() {
        let given = crate::params::parse_list("levels", "black=100,white=100").unwrap();
        let p = Params::new("levels", &INFO.all_params(), &given).unwrap();
        assert!((INFO.build)(&p).is_err());
    }
}
