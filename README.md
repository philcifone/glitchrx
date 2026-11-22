# glitcharaxia

Glitch art from photographs, by applying audio-style effects (echo, reverb, shuffle, bit-crush and others) to the compressed data inside a JPEG. It is the old "open a JPEG in Audacity as raw audio" technique, rebuilt so that it:

- never corrupts the file header, so the result always opens
- never modifies your original
- is repeatable: the same seed gives the same glitch, byte for byte
- can be undone exactly, for most effects

Everything runs locally. No image is ever uploaded anywhere.

Status: Phase 1, a command-line tool (`glitchrx`) for JPEG. A web GUI with live preview, usable from a phone over your LAN or VPN, is Phase 2. See `SPEC.md` for the roadmap and `VOCAB.md` for terminology.

## Install

Requirements:
- A Rust toolchain (stable, 1.88 or newer).
- libjpeg-turbo with its TurboJPEG headers. On Arch Linux this is the `libjpeg-turbo` package; on Debian and Ubuntu, `libturbojpeg0-dev`.

```
cargo build --release -p glitchrx-cli
# binary: target/release/glitchrx
cp target/release/glitchrx ~/.local/bin/    # optional
```

On macOS or Windows without a system libjpeg-turbo, build with `--features glitchrx-core/vendored`. That needs cmake, and nasm for the SIMD code.

## Usage

```
glitchrx --echo photo.jpg                       # writes photo-glitch.jpg
glitchrx --echo photo.jpg                       # again: photo-glitch-1.jpg
glitchrx --echo photo.jpg out.jpg               # explicit output name
glitchrx --echo delay=4096,gain=0.7 --shift by=200 photo.jpg
glitchrx --shuffle --bake photo.jpg             # also writes photo-glitch.png
glitchrx --echo hits=8 --restart 20% photo.jpg   # taller glitch bands
glitchrx --echo --restart keep photo.jpg        # no bands: damage runs to the bottom
glitchrx --stutter --batch *.jpg                # every file, automatic names
glitchrx --info photo.jpg                       # what is inside the file
glitchrx --list                                 # every effect and parameter
```

Effects run in the order you give them, and each takes optional `key=value` parameters. Every run prints the output name, how well it decodes, and the seed. For reversible chains it also prints the exact command that undoes the run:

```
photo-glitch.jpg  (decode: recovered, seed 81723)
  undo: glitchrx --reverse --echo delay=4096,gain=0.7 --seed 81723 photo-glitch.jpg
```

Re-run with `--seed` to reproduce a glitch you liked. Use `--save-recipe` to keep the whole chain in a small TOML file, and replay it later with `--recipe`.

"decode: recovered" is normal. It means the decoder hit the damage you made and worked around it, which is the glitch.

### Effects

| Effect | What it does | Reversible |
| --- | --- | --- |
| `echo` | Adds quieter, delayed copies of the data | yes |
| `reverb` | Three feedback echoes in series | yes |
| `shift` | Rotates stretches of data sideways | yes |
| `backwards` | Reverses stretches of data | yes |
| `shuffle` | Swaps blocks of data around | yes |
| `amplify` | Scales data away from the midpoint | wrap mode yes, clip mode no |
| `bitcrush` | Throws away low bits | no |
| `stutter` | Repeats a chunk over the following data | no |

All parameters are documented in `docs/effects.md`, which is generated from the code.

### Safety rules

- The input file is never written. This holds even with `--force`, and also through symlinks and hard links.
- Existing files are never overwritten unless you pass `--force`.
- Output names continue a sequence: `-glitch`, `-glitch-1`, `-glitch-2`. Glitching `photo-glitch-1.jpg` produces the next free `photo-glitch-N.jpg`, not `photo-glitch-1-glitch.jpg`.
- Exit codes:
  - 0: success
  - 1: runtime error
  - 2: bad command line
  - 3: refused (for safety)
  - 4: unsupported file

## How it works

A JPEG is a header (dimensions, colour tables, metadata) followed by compressed image data. Changing the header breaks the file, while changing the image data produces glitches. `glitchrx` parses the file, finds the image data, and lets the effects edit only that.

One detail the Audacity method missed: inside the image data, the byte value 255 (`FF`) always appears as `FF 00`. Any other `FF xx` pair is a control marker, and an accidental `FF D9` means "end of image", which turns everything after it flat gray. `glitchrx` removes those `00`s before the effects run and puts them back afterwards, so no effect can forge a marker. `--mode raw` turns this protection off if you want the Audacity behaviour, truncations included.

### Glitch bands and restart markers

When the decoder hits damaged data, it loses its place, and the colour of every following block drifts. Left alone, one glitch near the top turns the rest of the picture into a flat wash. That is what most viewers show as a gray block.

JPEG has a built-in fix: *restart markers*, checkpoints where the decoder resets. Most cameras and phones do not write them. By default `glitchrx` first rewrites the file with a marker every 10% of the image height (`--restart 10%`), so each glitch stays in a band, and the rest of the photo survives. This rewrite is lossless: the pixels do not change.

- `--restart 3%` gives thin streaks, and `--restart 25%` gives large chunks.
- `--restart 0` removes all markers, and `--restart keep` leaves the file exactly as it was. Either way you get the wash-to-the-bottom look.
- Because of the rewrite, `--reverse` gives back the original *pixels* exactly, but not byte for byte. Add `--restart keep` to both runs if you need the exact original bytes back.

### About endianness

In Audacity, you chose between little endian and big endian when importing raw data. That only matters when one audio sample is made of two or more bytes (16-bit audio). It decides whether the first byte of each pair is the high part or the low part of the number. Choose the "wrong" order and the effect math treats the bytes differently, which changes the look but is not an error.

`glitchrx` works on single bytes (8-bit samples), where byte order does not exist, so there is nothing to choose. A 16-bit mode may come later as an option.

## Development

```
just check        # fmt, clippy (warnings denied), all tests; or scripts/check.sh
just fixtures     # regenerate tests/fixtures (procedural, CC0)
just sheets       # parameter-sweep contact sheets into out/sheets
```

Layout:
- `crates/glitchrx-core`: the engine
- `crates/glitchrx-cli`: the `glitchrx` command
- `tools/`: fixture generator, contact sheets
- `docs/`: effect reference and decision records

## Privacy

`glitchrx` makes no network connections. Test images in this repository are generated procedurally. Personal sample photos belong in `samples/`, which is ignored by git.

## License

GPL-3.0-or-later. See `LICENSE`.
