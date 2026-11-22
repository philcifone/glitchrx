# glitcharaxia

Glitch art from photographs, by applying audio-style effects (echo, reverb, shuffle, bit-crush and others) to the compressed data inside a JPEG. It is the old "open a JPEG in Audacity as raw audio" technique, rebuilt so that it:

- never corrupts the file header, so the result always opens
- never modifies your original
- is repeatable: the same seed gives the same glitch, byte for byte
- can be undone exactly, for most effects

Everything runs locally. No image is ever uploaded anywhere.

Status: Phase 2. A command-line tool (`glitchrx`) and a web GUI (`glitchrx serve`) with live preview, usable from a phone over your LAN or VPN. JPEG only for now; more formats are planned. See `SPEC.md` for the roadmap and `VOCAB.md` for terminology.

## Install

Requirements:
- A Rust toolchain (stable, 1.88 or newer).
- libjpeg-turbo with its TurboJPEG headers. On Arch Linux this is the `libjpeg-turbo` package; on Debian and Ubuntu, `libturbojpeg0-dev`.

```
just build-all          # builds the web GUI (needs Node.js and npm), then the binary
# binary: target/release/glitchrx, with the GUI embedded
cp target/release/glitchrx ~/.local/bin/    # optional
```

Without Node.js, `cargo build --release -p glitchrx-cli` builds everything except the GUI; `glitchrx serve` then shows a page explaining how to add it. Node is only needed to build. The finished binary contains the whole GUI and needs nothing else at runtime.

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

## Web GUI

```
glitchrx serve                                   # http://127.0.0.1:7878, this machine only
glitchrx serve --root ~/Pictures                 # also browse, open and save photos there
glitchrx serve --host 0.0.0.0 --token --root ~/Pictures   # from a phone over LAN or WireGuard
```

`--host 0.0.0.0` listens on every interface and prints a URL for each address. There is no login, so anyone who can reach the port can use the server. `--token` adds a generated secret to the printed URL; open that URL once per browser, and a cookie keeps you signed in. `--token=SECRET` sets your own.

In the GUI:
- **Open** a photo from your device (it is sent to your own server, nowhere else) or, with `--root`, from the server's folders.
- **Add steps** from the effect list. Steps run top to bottom. Each one can be switched off, re-rolled (new seed), moved, duplicated or removed. Shared placement settings (`region`, `hits`, `len`) are under "Placement".
- **Bands** sets the restart interval (the CLI's `--restart`); see below.
- **Hold "original"** (or the `\` key) to compare with the untouched photo.
- **Undo and redo** with the buttons or Ctrl+Z / Ctrl+Shift+Z. History holds recipes, not images.
- **Export** at full resolution: a baked PNG or JPEG (what the preview shows), or the glitched JPEG itself. With `--root`, results can be saved next to the original using the CLI's `-glitch`, `-glitch-1` naming. Nothing is overwritten. The recipe can be downloaded, then loaded again later or replayed with `glitchrx --recipe`.

The preview is decoded on the server with libjpeg-turbo, the same decoder browsers use, and sent to the page as a clean image. What you see is the baked result, whatever browser you use. On a 24 MP photo, a preview takes roughly 150-250 ms.

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
just web          # install web deps and build the GUI into web/dist
just web-check    # svelte-check and vitest (also part of `just check` once web deps exist)
just web-dev      # GUI with hot reload on :5173, proxying /api to `glitchrx serve` on :7878
```

Layout:
- `crates/glitchrx-core`: the engine
- `crates/glitchrx-cli`: the `glitchrx` command
- `crates/glitchrx-serve`: the web server behind `glitchrx serve`
- `web/`: the Svelte GUI, embedded into the binary at build time
- `tools/`: fixture generator, contact sheets
- `docs/`: effect reference and decision records

## Privacy

`glitchrx` makes no outgoing network connections. `glitchrx serve` only answers the browser that connects to it. Every font, script and icon of the GUI is built into the binary, and a Content-Security-Policy stops the page from loading anything from elsewhere. Test images in this repository are generated procedurally. Personal sample photos belong in `samples/`, which is ignored by git.

## License

GPL-3.0-or-later. See `LICENSE`.
