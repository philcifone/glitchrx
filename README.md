# glitcharaxia

Glitch art from photographs by databending: reading a photo's raw pixels as if they were audio, running audio effects over them (echo, reverb, delay, amplify and others), and turning the result back into an image. It is the old "import a TIFF into Audacity as raw data" technique, rebuilt so that it:

- never modifies your original
- is repeatable: the same seed gives the same result, byte for byte
- previews live, fast enough to use from a phone
- keeps your photo's size, orientation and colour profile

Everything runs locally. No image is ever uploaded anywhere.

Status: Phase 2. A command-line tool (`glitchrx`) and a web GUI (`glitchrx serve`), usable from a phone over your LAN or VPN. Reads JPEG, PNG and TIFF; writes TIFF (the default), PNG and JPEG. See `SPEC.md` for the roadmap and `VOCAB.md` for terminology.

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
glitchrx --echo photo.jpg                       # writes photo-glitch.tif
glitchrx --echo photo.jpg                       # again: photo-glitch-1.tif
glitchrx --echo photo.jpg out.png               # explicit output; its extension sets the format
glitchrx --echo --format jpeg photo.jpg         # photo-glitch.jpg
glitchrx --echo delay=0.33rows,decay=0.6 --reverb photo.jpg
glitchrx --echo --encoding ulaw photo.jpg       # read the pixels as mu-law audio
glitchrx --shift by=0.5rows,phase=2,hits=12 photo.jpg
glitchrx --pixelsort low=0.3 photo.jpg          # melt the bright areas into streaks
glitchrx --echo opacity=0.5 photo.jpg           # blend the step at half strength
glitchrx --stutter --batch *.jpg                # every file, automatic names
glitchrx --info photo.jpg                       # format, size, orientation, colour profile
glitchrx --list                                 # every effect and parameter
```

Effects run in the order you give them, and each takes optional `key=value` parameters. Every run prints the output name, its size, and the seed:

```
photo-glitch.tif  (6240 x 4160, tiff, seed 81723)
```

Re-run with `--seed` to reproduce a result you liked. Use `--save-recipe` to keep the whole chain in a small TOML file, and replay it later with `--recipe`.

The output is TIFF by default: uncompressed 8-bit RGB, ready for masking in Photoshop. The colour profile of the original is written into every output, so colours match the original.

### Encodings

Audacity's Import Raw Data dialog asks how to read the bytes as samples. That choice changes the look more than any effect parameter, so it is a setting here too: `--encoding` for the whole chain, or `encoding=` on one effect.

| Encoding | Look |
| --- | --- |
| `u8` (default) | Clean ghosts; colours rotate through the channels |
| `s8` | Like `u8`, but highlights clip and invert |
| `s16le`, `s16be` | Ghosts under heavy rainbow grain; byte order changes the palette |
| `s24le`, `s24be` | One sample per pixel; posterized two-tone fields |
| `ulaw`, `alaw` | Posterized neon blocks with hard edges |

`--channels 2` reads the bytes as stereo, as Audacity did when you imported stereo: alternate samples are left and right, and each side is processed on its own. `--channels 3` does the same with three channels; with `u8` those are exactly red, green and blue, so filters that blend neighbouring samples (`wahwah`, `reverb`) keep the colours instead of turning them grey.

### Sizes

Every pixel is three bytes (R, G, B), and rows follow each other, so a distance in the data is a distance in the picture. Sizes take units: `rows` (`0.33rows` is a third of the width, sideways; `6rows` is six rows down), `px`, `%` of the image (which runs top to bottom, so `40%:60%` is the middle band), or plain bytes.

A distance of whole pixels keeps colours. `phase=1` or `phase=2` adds one or two bytes, which shifts red into green into blue: the colour fringes of databent images.

### Effects

| Effect | Audacity counterpart | What it does |
| --- | --- | --- |
| `echo` | Echo | Fading repeats that feed back on themselves |
| `delay` | Delay | A fixed number of crisp repeats |
| `reverb` | Reverb | A haze of overlapping ghosts (Freeverb) |
| `amplify` | Amplify | Gain in dB, with clipping |
| `invert` | Invert | Flips samples; close to a negative with `u8` |
| `bitcrush` | (plug-ins) | Fewer levels per sample: banding |
| `backwards` | Reverse | Mirrors stretches of the picture |
| `shift` | cut and paste | Slides bands sideways |
| `shuffle` | cut and paste | Swaps blocks around |
| `stutter` | Repeat | Repeats a strip down the picture |
| `phaser` | Phaser | Combed, wavy bands from swept all-pass filters |
| `wahwah` | Wahwah | A swept resonant filter: pulsing smears and ringing fringes |
| `distort` | Distortion | Hard clip, soft saturation, or folding highlights into bands |
| `speed` | Change Speed | Squeezes or stretches parts of the picture: shears |
| `pixelsort` | (pixel effect) | Sorts runs of pixels by brightness, hue or saturation: streaks down or across the picture |

`pixelsort` is not audio: it works on whole pixels, so the encoding and channels do not apply. Every step also takes `opacity` (0 to 1), which blends its result over the picture it received.

All parameters are documented in `docs/effects.md`, which is generated from the code. Where an effect lands is set by `region`, `hits` and `len`, shared by every effect.

### Safety rules

- The input file is never written. This holds even with `--force`, and also through symlinks and hard links.
- Existing files are never overwritten unless you pass `--force`.
- Output names continue a sequence: `-glitch`, `-glitch-1`, `-glitch-2`. Bending `photo-glitch-1.tif` produces the next free `photo-glitch-N.tif`, not `photo-glitch-1-glitch.tif`. The numbers are shared across formats.
- Exit codes:
  - 0: success
  - 1: runtime error
  - 2: bad command line
  - 3: refused (for safety)
  - 4: unsupported file (not a JPEG, PNG or TIFF)

## Web GUI

```
glitchrx serve                                   # http://127.0.0.1:7878, this machine only
glitchrx serve --root ~/Pictures                 # also browse, open and save photos there
glitchrx serve --host 0.0.0.0 --token --root ~/Pictures   # from a phone over LAN or WireGuard
```

`--host 0.0.0.0` listens on every interface and prints a URL for each address. There is no login, so anyone who can reach the port can use the server. `--token` adds a generated secret to the printed URL; open that URL once per browser, and a cookie keeps you signed in. `--token=SECRET` sets your own. A browser without the cookie gets a sign-in page where you can paste the token instead.

**On a phone, add it to the home screen** (Safari: Share, then Add to Home Screen). It then opens full screen with its own icon, like an app. The home-screen app may keep its own cookies, so it can ask for the token once. Exports open the share sheet (Save Image, Save to Files, AirDrop) through a Save button in the status bar.

In the GUI:
- **Open** a photo from your device (it is sent to your own server, nowhere else) or, with `--root`, from the server's folders. If steps are already in the stack, you are asked whether to keep them or start fresh (undoable; "Don't ask again" is changeable in Presets).
- **Pick the encoding** for the whole recipe (and mono, stereo or RGB) at the top. Any step can override both.
- **Add steps** by tapping an effect in the palette, grouped into audio effects (echo, delay, reverb, phaser, wahwah, amplify, distort, speed, stutter), data effects (invert, bitcrush, backwards, shift, shuffle) and pixel effects (pixelsort). On a phone the palette sits behind the floating + button. Steps run top to bottom. Each one can be switched off, moved, duplicated or removed.
- **Placement** (`region`, `hits`, `len`) is always shown on each step. Dragging the region's two handles shades the preview outside the band it covers.
- **Shake** (the shuffle arrows) rolls a step's own settings at random, within the useful range; placement and encoding stay. **Re-roll** (the dice, beside the seed at the end of Placement) picks a new seed, which moves where hits land; it is greyed out when a step has nothing random (`hits=0`, except for `shuffle`).
- **Blend** each step with its opacity, and **mask** it. A mask is built from components: brushes (paint and erase), linear gradients, radials (hard-edged circles; feather fades them), rectangles (resize, turn and move them), pen paths (tap out points, drag while placing one to curve the sides, tap the first to close; points stay editable) and brightness ranges that limit the mask to light or dark parts. Tap a component to make it active; a radial, linear or rectangle then shows handles on the photo to move or resize it, and the others show pins to tap. Each component adds, subtracts or intersects, and has its own opacity and feather (side by side when it is active); long-press to select several and set them together. Copy, Cut and Paste move components between steps. Paint in shows the step only inside the mask; Paint out shows it everywhere else. The red overlay shows the mask; Done ends editing. On a desktop, B and E paint and erase and [ and ] size the brush. Masks are saved in the recipe as strokes and shapes, so they replay exactly at full resolution.
- **Presets** (the bookmark) are your own saved recipes, kept by the server in `~/.config/glitchrx/presets` (`--presets DIR` to move them), so your phone and desktop share them. Use one to replace the steps, or add its steps to the current recipe. They are ordinary recipe files: `glitchrx --recipe ~/.config/glitchrx/presets/NAME.toml photo.jpg`.
- **Press and hold the photo** (or the `\` key) to see the untouched original; while masking, hold the compare button in the top bar instead.
- **Zoom** with a pinch, ctrl + wheel or a trackpad pinch; double-tap, tap the magnifier or press Z for 100% and back. Zoomed in, drag (or use the wheel) to pan. Once the view holds still, what is on screen is cut from the full-resolution render, so it is sharp and exactly the export. While masking, one finger paints and two fingers zoom and pan; the brush keeps its size on screen, so zooming in paints finer lines.
- **Undo and redo** with the buttons or Ctrl+Z / Ctrl+Shift+Z. History holds recipes, not images.
- **Export** at full resolution as TIFF, PNG or JPEG. With `--root`, results can be saved next to the original using the CLI's `-glitch`, `-glitch-1` naming. Nothing is overwritten. The recipe can be downloaded, then loaded again later or replayed with `glitchrx --recipe`.

While you change things, the preview runs the recipe on a copy of the photo shrunk to your screen, with every size scaled to match, so even a 26 MP photo answers quickly. That copy is close to the export but not identical, because pixel-level texture cannot shrink with the picture. So once you stop for a moment the preview settles to an exact render: the recipe at full resolution, shrunk to fit, which is exactly what export gives (0.3 to 1.6 s on a 26 MP photo). The status bar says whether the view is approximate or exact. A newer change cancels a render that is still running, and a live preview that runs over 3 s stops with a "too heavy to preview live" notice (export still works).

## Desktop app

The same GUI in a native window, for working on this machine. It runs its own copy of the server on `127.0.0.1` (a free port, a new token each launch), so everything behaves as in the browser, except that **Open** and **Export** use the system's file dialogs. Export suggests the next free `-glitch` name next to the original; replacing a file you confirmed in the dialog is allowed, but the original photo is never written.

Linux (needs `webkit2gtk-4.1`; on Arch: `pacman -S webkit2gtk-4.1`):

```
scripts/install-desktop.sh            # or: just install-desktop
scripts/install-desktop.sh --remove   # or: just uninstall-desktop
```

This builds the app and installs it for your user only: `~/.local/bin/glitchrx-desktop`, a launcher entry and icons under `~/.local/share`. `just desktop` runs a debug build instead, which prints its internal URL so you can open the same session in a browser's developer tools. macOS is planned; the build steps are in `docs/macos.md`.

## How it works

1. **Decode** the photo to a *raster*: R, G, B for each pixel, row after row, exactly what an uncompressed TIFF stores. A sideways phone photo is turned upright first, so rows are the rows you see.
2. **Bend.** Each effect reads a stretch of those bytes as audio samples in its encoding, processes them in floating point the way Audacity would, and clips the result back to bytes. Clipping is where the bright, saturated colours come from.
3. **Encode** the raster as TIFF, PNG or JPEG, with the original colour profile.

The raster always keeps its length, so the image keeps its dimensions. The math uses only basic floating-point operations, which give identical results on every machine, so a seed reproduces the same bytes everywhere.

### About endianness

In Audacity, you chose between little endian and big endian when importing raw data. That only matters when one sample is made of two or more bytes. It decides whether the first byte of each pair is the loud (high) part or the quiet (low) part of the number. With `s16le` the green and blue bytes of a pixel tend to be the loud parts; with `s16be` it flips. Neither is wrong; they are different palettes.

### Earlier: JPEG byte glitching

Until SPEC v0.3, `glitchrx` glitched the *compressed* data inside a JPEG. That gives flat colour bands, not the databent look, and was retired. It is kept in git at the tag `archive/jpeg-bytes`; its version 1 recipes only replay there.

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
- `crates/glitchrx-desktop`: the desktop app (Tauri), which embeds that server
- `web/`: the Svelte GUI, embedded into the binary at build time
- `tools/`: fixture generator, contact sheets (`just sheets photo.jpg`)
- `docs/`: effect reference and decision records

## Privacy

`glitchrx` makes no outgoing network connections. `glitchrx serve` only answers the browser that connects to it. Every font, script and icon of the GUI is built into the binary (body text uses the device's own system font), and a Content-Security-Policy stops the page from loading anything from elsewhere. Test images in this repository are generated procedurally. Personal sample photos belong in `samples/`, which is ignored by git.

## License

GPL-3.0-or-later. See `LICENSE`.
