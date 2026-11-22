//! Render parameter-sweep grids ("contact sheets") for visual tuning.
//!
//! Usage: `cargo run --release -p contact-sheet -- INPUT OUT_DIR [SHEET]`
//!
//! For each sheet, two parameters are swept: rows take the first
//! parameter's values (top to bottom) and columns the second (left to right).
//! Each cell is the bent image at that combination, all with the same seed,
//! scaled down to fit. The values are written to `OUT_DIR/<sheet>.txt`
//! beside each PNG.
//!
//! The input is first shrunk to at most `WORK_SIDE` pixels, then bent at
//! that size like a full export (sizes in rows and percent mean the same
//! picture geometry at any size), which keeps a 16-cell sheet quick.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use glitchrx_core::codec::{self, Format};
use glitchrx_core::{Raster, Recipe, Step};

/// Longest side the input is bent at.
const WORK_SIDE: usize = 1600;
/// Longest side of one cell on the sheet.
const CELL_SIDE: usize = 400;

/// One grid: an effect, fixed extra params, and the two swept parameters.
/// `encoding` and `channels` can be swept like any parameter.
struct Sweep {
    name: &'static str,
    effect: &'static str,
    fixed: &'static str,
    rows: (&'static str, &'static [&'static str]),
    cols: (&'static str, &'static [&'static str]),
}

const ALL_ENCODINGS: &[&str] = &[
    "u8", "s8", "s16le", "s16be", "s24le", "s24be", "ulaw", "alaw",
];

const SWEEPS: &[Sweep] = &[
    Sweep {
        name: "echo",
        effect: "echo",
        fixed: "",
        rows: ("decay", &["0.3", "0.5", "0.7", "0.9"]),
        cols: ("delay", &["0.1rows", "0.33rows", "1rows", "6.5rows"]),
    },
    Sweep {
        name: "delay",
        effect: "delay",
        fixed: "decay=0.7",
        rows: ("taps", &["1", "3", "8"]),
        cols: ("delay", &["0.1rows", "0.33rows", "1rows", "6.5rows"]),
    },
    Sweep {
        name: "reverb",
        effect: "reverb",
        fixed: "",
        rows: ("room", &["0.3", "0.7", "0.95"]),
        cols: ("size", &["0.1rows", "0.5rows", "2rows", "8rows"]),
    },
    Sweep {
        name: "amplify",
        effect: "amplify",
        fixed: "hits=0",
        rows: ("encoding", &["u8", "s8", "s16le", "ulaw"]),
        cols: ("gain", &["3", "6", "12", "24"]),
    },
    Sweep {
        name: "invert",
        effect: "invert",
        fixed: "len=10%",
        rows: ("encoding", &["u8", "s8", "s16le", "s16be", "s24le", "ulaw"]),
        cols: ("hits", &["0", "3", "8"]),
    },
    Sweep {
        name: "bitcrush",
        effect: "bitcrush",
        fixed: "",
        rows: ("encoding", &["u8", "s16le", "s16be", "ulaw"]),
        cols: ("bits", &["1", "2", "4", "6"]),
    },
    Sweep {
        name: "backwards",
        effect: "backwards",
        fixed: "",
        rows: ("hits", &["1", "4", "12"]),
        cols: ("len", &["1rows", "3%", "10%", "30%"]),
    },
    Sweep {
        name: "shift",
        effect: "shift",
        fixed: "hits=6,len=3%",
        rows: ("phase", &["0", "1", "2"]),
        cols: ("by", &["10px", "0.25rows", "0.5rows", "3rows"]),
    },
    Sweep {
        name: "shuffle",
        effect: "shuffle",
        fixed: "",
        rows: ("fraction", &["0.2", "0.5", "1.0"]),
        cols: ("block", &["0.25rows", "0.5rows", "2rows", "10%"]),
    },
    Sweep {
        name: "stutter",
        effect: "stutter",
        fixed: "",
        rows: ("len", &["2%", "5%", "15%"]),
        cols: ("chunk", &["0.3rows", "1rows", "1.5rows", "4rows"]),
    },
    // The same echo read through every encoding, with each channel phase.
    Sweep {
        name: "encodings",
        effect: "echo",
        fixed: "delay=0.33rows,decay=0.6",
        rows: ("encoding", ALL_ENCODINGS),
        cols: ("phase", &["0", "1", "2"]),
    },
    Sweep {
        name: "stereo",
        effect: "echo",
        fixed: "delay=0.33rows,decay=0.6",
        rows: ("encoding", &["u8", "s16le", "ulaw"]),
        cols: ("channels", &["1", "2"]),
    },
    // Placement study, using echo at its defaults.
    Sweep {
        name: "placement",
        effect: "echo",
        fixed: "",
        rows: ("hits", &["1", "3", "8"]),
        cols: ("len", &["1rows", "2%", "10%", "30%"]),
    },
];

/// Paste `src` into `dst` at (x, y).
fn blit(dst: &mut Raster, src: &Raster, x: usize, y: usize) {
    for row in 0..src.height.min(dst.height.saturating_sub(y)) {
        let w = src.width.min(dst.width.saturating_sub(x)) * 3;
        let s = row * src.width * 3;
        let d = ((y + row) * dst.width + x) * 3;
        dst.bytes[d..d + w].copy_from_slice(&src.bytes[s..s + w]);
    }
}

fn render(input: &Raster, sweep: &Sweep) -> Result<(Raster, String)> {
    const GAP: usize = 6;
    let mut cells = Vec::new();
    let mut legend = format!(
        "{}: rows {} = {:?}, columns {} = {:?}, fixed: {:?}\n",
        sweep.effect, sweep.rows.0, sweep.rows.1, sweep.cols.0, sweep.cols.1, sweep.fixed
    );
    for rv in sweep.rows.1 {
        for cv in sweep.cols.1 {
            let mut params = format!("{}={rv},{}={cv}", sweep.rows.0, sweep.cols.0);
            if !sweep.fixed.is_empty() {
                params = format!("{},{params}", sweep.fixed);
            }
            let step = Step::new(sweep.effect, &params, 1)
                .with_context(|| format!("{} {params}", sweep.effect))?;
            let recipe = Recipe {
                steps: vec![step],
                ..Recipe::default()
            };
            let mut out = input.clone();
            recipe.apply(&mut out)?;
            legend.push_str(&format!("  {params}\n"));
            cells.push(out.downscale(CELL_SIDE, CELL_SIDE));
        }
    }
    let (cw, ch) = (cells[0].width, cells[0].height);
    let (nc, nr) = (sweep.cols.1.len(), sweep.rows.1.len());
    let (w, h) = (nc * cw + (nc + 1) * GAP, nr * ch + (nr + 1) * GAP);
    // Neutral dark background between cells.
    let mut sheet = Raster::new(w, h, [40u8, 40, 40].repeat(w * h));
    for (i, cell) in cells.iter().enumerate() {
        let (r, c) = (i / nc, i % nc);
        blit(&mut sheet, cell, GAP + c * (cw + GAP), GAP + r * (ch + GAP));
    }
    Ok((sheet, legend))
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let (Some(input), Some(out_dir)) = (args.next(), args.next()) else {
        bail!("usage: contact-sheet INPUT OUT_DIR [SHEET]");
    };
    let only = args.next();
    let bytes = fs::read(&input).with_context(|| input.clone())?;
    let image = codec::decode_scaled(&bytes, Some((WORK_SIDE, WORK_SIDE)))?;
    let out = Path::new(&out_dir);
    fs::create_dir_all(out)?;
    for sweep in SWEEPS {
        if only.as_deref().is_some_and(|o| o != sweep.name) {
            continue;
        }
        let (sheet, legend) = render(&image.raster, sweep)?;
        let png = out.join(format!("{}.png", sweep.name));
        fs::write(&png, codec::encode(&sheet, Format::Png, None)?)?;
        fs::write(out.join(format!("{}.txt", sweep.name)), &legend)?;
        print!("{}", legend.lines().next().unwrap_or_default());
        println!("  -> {}", png.display());
    }
    Ok(())
}
