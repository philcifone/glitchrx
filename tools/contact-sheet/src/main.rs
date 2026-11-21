//! Render parameter-sweep grids ("contact sheets") for visual tuning.
//!
//! Usage: `cargo run --release -p contact-sheet -- INPUT.jpg OUT_DIR [EFFECT]`
//!
//! For each effect, two of its parameters are swept: rows take the first
//! parameter's values (top to bottom) and columns the second (left to right).
//! Each cell is the decoded glitch at that combination, all with the same
//! seed. The values are written to `OUT_DIR/<sheet>.txt` beside each PNG.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use glitchrx_core::turbo::{Image, Scale, decode, to_png};
use glitchrx_core::{Recipe, Step};

/// One grid: an effect, fixed extra params, and the two swept parameters.
struct Sweep {
    name: &'static str,
    effect: &'static str,
    fixed: &'static str,
    rows: (&'static str, &'static [&'static str]),
    cols: (&'static str, &'static [&'static str]),
}

const SWEEPS: &[Sweep] = &[
    Sweep {
        name: "echo",
        effect: "echo",
        fixed: "",
        rows: ("gain", &["0.1", "0.3", "0.5", "0.8"]),
        cols: ("delay", &["16", "256", "1024", "8192"]),
    },
    Sweep {
        name: "reverb",
        effect: "reverb",
        fixed: "",
        rows: ("feedback", &["0.2", "0.4", "0.7", "0.95"]),
        cols: ("delay", &["16", "128", "512", "4096"]),
    },
    Sweep {
        name: "shift",
        effect: "shift",
        fixed: "",
        rows: ("len", &["0.2%", "1%", "3%"]),
        cols: ("by", &["1", "37", "400", "-2000"]),
    },
    Sweep {
        name: "backwards",
        effect: "backwards",
        fixed: "",
        rows: ("hits", &["1", "3", "8"]),
        cols: ("len", &["0.05%", "0.2%", "1%", "5%"]),
    },
    Sweep {
        name: "shuffle",
        effect: "shuffle",
        fixed: "",
        rows: ("fraction", &["0.2", "0.5", "1.0"]),
        cols: ("block", &["1%", "5%", "20%", "64"]),
    },
    Sweep {
        name: "amplify",
        effect: "amplify",
        fixed: "",
        rows: ("mode", &["wrap", "clip"]),
        cols: ("factor", &["-1", "3", "5", "31"]),
    },
    Sweep {
        name: "bitcrush",
        effect: "bitcrush",
        fixed: "",
        rows: ("hits", &["1", "3", "8"]),
        cols: ("bits", &["1", "3", "5", "7"]),
    },
    Sweep {
        name: "stutter",
        effect: "stutter",
        fixed: "",
        rows: ("len", &["0.2%", "0.5%", "2%"]),
        cols: ("chunk", &["2%", "12%", "40%", "16"]),
    },
    // Placement and region studies, using echo at its defaults.
    Sweep {
        name: "placement",
        effect: "echo",
        fixed: "",
        rows: ("hits", &["1", "3", "8"]),
        cols: ("len", &["0.1%", "0.5%", "2%", "10%"]),
    },
    Sweep {
        name: "region",
        effect: "shift",
        fixed: "hits=0,by=37",
        rows: ("len", &["1"]),
        cols: (
            "region",
            &["0%:100%", "2%:100%", "5%:100%", "15%:100%", "40%:100%"],
        ),
    },
];

/// Pick a decode scale so a cell is at most ~400 px wide.
fn cell_scale(width: u16) -> Scale {
    for (s, d) in [(Scale::Full, 1), (Scale::Half, 2), (Scale::Quarter, 4)] {
        if (width as u32).div_ceil(d) <= 400 {
            return s;
        }
    }
    Scale::Eighth
}

/// Paste `src` into `dst` at (x, y).
fn blit(dst: &mut Image, src: &Image, x: u32, y: u32) {
    for row in 0..src.height.min(dst.height.saturating_sub(y)) {
        let w = src.width.min(dst.width.saturating_sub(x)) as usize * 4;
        let s = (row * src.width) as usize * 4;
        let d = (((y + row) * dst.width) + x) as usize * 4;
        dst.rgba[d..d + w].copy_from_slice(&src.rgba[s..s + w]);
    }
}

fn render(input: &[u8], sweep: &Sweep, scale: Scale) -> Result<(Image, String)> {
    const GAP: u32 = 6;
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
            let out = recipe.apply(input.to_vec())?;
            let (img, status) = decode(&out, scale);
            legend.push_str(&format!("  {params}: decode {}\n", status.word()));
            let Some(img) = img else {
                bail!("{params}: could not decode");
            };
            cells.push(img);
        }
    }
    let (cw, ch) = (cells[0].width, cells[0].height);
    let (nc, nr) = (sweep.cols.1.len() as u32, sweep.rows.1.len() as u32);
    let mut sheet = Image {
        width: nc * cw + (nc + 1) * GAP,
        height: nr * ch + (nr + 1) * GAP,
        rgba: Vec::new(),
    };
    // Neutral dark background between cells.
    sheet.rgba = [40u8, 40, 40, 255].repeat((sheet.width * sheet.height) as usize);
    for (i, cell) in cells.iter().enumerate() {
        let (r, c) = (i as u32 / nc, i as u32 % nc);
        blit(&mut sheet, cell, GAP + c * (cw + GAP), GAP + r * (ch + GAP));
    }
    Ok((sheet, legend))
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let (Some(input), Some(out_dir)) = (args.next(), args.next()) else {
        bail!("usage: contact-sheet INPUT.jpg OUT_DIR [SHEET]");
    };
    let only = args.next();
    let bytes = fs::read(&input).with_context(|| input.clone())?;
    let info = glitchrx_core::Jpeg::parse(bytes.clone(), glitchrx_core::Mode::Safe)?;
    let scale = cell_scale(info.info().frame.width);
    let out = Path::new(&out_dir);
    fs::create_dir_all(out)?;
    for sweep in SWEEPS {
        if only.as_deref().is_some_and(|o| o != sweep.name) {
            continue;
        }
        let (sheet, legend) = render(&bytes, sweep, scale)?;
        let png = out.join(format!("{}.png", sweep.name));
        fs::write(&png, to_png(&sheet))?;
        fs::write(out.join(format!("{}.txt", sweep.name)), &legend)?;
        print!("{}", legend.lines().next().unwrap_or_default());
        println!("  -> {}", png.display());
    }
    Ok(())
}
