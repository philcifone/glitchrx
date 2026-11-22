# glitcharaxia: Specification

Status: **v0.3, 2026-11-21, approved by Phil.** v0.3 changes what the effects work on. v0.2 glitched the *compressed* data inside a JPEG. Phil's reference work (`samples/examples/`, uncompressed TIFFs bent in Audacity) shows that the look he wants comes from bending *raw pixels*. v0.3 makes the raw-pixel domain the whole engine and removes JPEG byte glitching from the live project. The source documents are `pre-brief.md` and `BRIEF.md`, and `VOCAB.md` defines the terms used here.

Keywords: **must** means required for the phase to be done. **Should** means expected, but logged and deferred if it proves hard. **Later** means out of scope for the phase.

## What changed from v0.2, in one paragraph

A JPEG stores Huffman-coded frequency data, not pixels. Changing a byte there derails the decoder, which then paints flat colour until it recovers: the bands Phil saw on every effect. An uncompressed image stores the pixels themselves, row by row, R G B. Run Audacity's echo over those bytes and you get ghosted copies of the picture, colours rotating through the channels, and smears: the look of Phil's examples. A prototype on his photos confirmed it (2026-11-21, contact sheets in the session). So glitchrx now decodes any input to a raw pixel buffer, bends that buffer the way Audacity would, and saves the result as a normal image. The recipe model, seeds, CLI, web GUI, undo, export and file safety all carry over.

## Decision log

Decisions from earlier versions keep their numbers. Superseded ones say so.

| # | Topic | Decision |
| --- | --- | --- |
| D-1 | Binary name | `glitchrx`. Project name `glitcharaxia`. Crates `glitchrx-core`, `glitchrx-cli`, `glitchrx-serve`. |
| D-2 | License | GPL-3.0-or-later for everything. |
| D-3 | `--save-recipe` default | Off. The CLI prints the replay command instead. |
| D-4 | `--reverse` | **Superseded by D-24.** |
| D-5 | LAN/web GUI | Yes, and it is the **primary** GUI (`glitchrx serve`). Tauri is a later, optional wrapper. |
| D-6 | Frontend | Svelte 5 + TypeScript + Vite. |
| D-7 | Region default | Claude chooses it during tuning and tells Phil when there is a visible difference to judge. |
| D-8 | CLI output | A normal terminal tool: writes files, prints a summary line, never displays images. |
| D-9 | DNG | In scope as input. With a raw-pixel engine, every input format is "decode to pixels" (section 4). |
| D-10 | Icons | Lucide, sparingly. No emojis anywhere. |
| D-11 | Test photos | Phil's photos live in `samples/` (git-ignored). Reference work in `samples/examples/`. |
| D-12 | 16-bit sample view | **Superseded by D-23**: sample encodings, 16-bit included, are now central. |
| D-13 | `backwards`, not `reverse` | Kept. |
| D-14 | Window-relative sizes | Kept in spirit: sizes can be relative (section 6.2). |
| D-15 | Restart markers | **Superseded by D-22** (JPEG-only concept). |
| D-16 | `glitchrx serve` shape | Kept. |
| D-17 | Access token | Kept. |
| D-18 | Preview pipeline | **Revised by D-26.** Seeds still travel as JSON strings. |
| D-19 | Preview speed | **Revised by D-26 and D-27.** |
| D-20 | GUI step switch | Kept. |
| D-21 | Gate and the web GUI | Kept. |
| D-22 | **Raw pixels only** | Effects operate on the decoded image as a raw buffer of interleaved RGB bytes, like an uncompressed TIFF. JPEG byte glitching (scan parser, safe/raw mode, restart markers, header-safe tests) is removed from the live code and kept at git tag `archive/jpeg-bytes`. Phil's call, 2026-11-21. |
| D-23 | **Audacity sample encodings** | Each step reads the bytes as audio samples in a chosen encoding, as Audacity's Import Raw Data does: 8-bit unsigned (default), 8-bit signed, 16-bit and 24-bit signed (little or big endian), U-law, A-law, with 1 or 2 channels (section 5). All four tested encodings looked useful (Phil, 2026-11-21). |
| D-24 | **Audacity math, no `--reverse`** | Effects work like Audacity's: samples become floating-point audio, the effect runs, and the result is clipped back to the encoding. This gives the saturated look but loses information, so exact inversion and `--reverse` are dropped. Undo in the GUI works from recipes as before. Phil: "whatever it takes". |
| D-25 | **Deterministic floating point** | Same input, recipe and seed still give the same bytes on every machine. Effect math uses IEEE f64 with basic operations only (`+ - * /`, `sqrt`, comparisons). No `sin`, `exp` or `pow` from the platform math library in sample loops, since those can differ by a last bit between systems; oscillators use our own polynomial sine. Per-effect `VERSION` and golden-hash tests stay. This replaces "integer math only (Q8)". |
| D-26 | **Preview at screen resolution** | Live previews bend a screen-sized copy of the image, with sizes and delays scaled to match (section 9.2). Close to the export, not byte-identical. Exports always run at full resolution. This trades exact WYSIWYG for about 10-20 times less work per preview. |
| D-27 | **CPU limits, no GPU** | No GPU: Audacity's echo and reverb feed back on themselves, so each sample depends on the previous one, which GPUs cannot speed up; GPU float results also vary by hardware, which would break D-25. Instead: screen-resolution previews (D-26), cancelling stale renders mid-way, capped slider ranges, and a preview time budget (section 9.2). |
| D-28 | **Output is a normal image** | The glitched pixels are saved as uncompressed TIFF (default, for Phil's Photoshop masking), PNG, or JPEG (quality 95). There is no "glitched file itself" any more: the pixels *are* the result. |
| D-29 | **Recipe-wide encoding** | A recipe sets `encoding` and `channels` once, like Audacity's import dialog; any step may override them. Claude's call, delegated by Phil. |
| D-30 | **Sizes mapped from full resolution** | Found while measuring preview fidelity in 2b. Scaling byte counts by the area ratio misplaces things twice: a third of a row at full size became a twelfth of one in a quarter-size preview, and the byte remainder (the colour rotation) came out arbitrary, so `reverb` previews had different colours. Every size and hit position is now worked out at full resolution and mapped into the preview by rows (height ratio), the rest of the row (width ratio) and the 0-2 byte phase (kept). Hits are drawn as a fraction of the free space. Measured mean preview error at defaults, quarter size: 1.5-5.8 per byte for every effect except `reverb` (10.3, against an effect strength of 30); `tests/fixtures.rs` prints the table. The remaining gap is inherent: a delay that wraps past a row moves its ghost down one row, and a preview row stands for several full rows, so long feedback tails drift. 2c can offer a full-resolution preview on demand. |
| D-31 | **2b deviations** | (1) Decoding and encoding live in one `codec.rs` rather than `decode/` plus `encode.rs`; each format is small. (2) `reverb` has no `width`: it only applies to joint stereo processing, and channels are processed separately (section 5). Can be added with a stereo filter in 2d. (3) Screen-resolution previews and cancellation are already in the server (pulled forward from 2c), since the 2a cache did not fit rasters. (4) Fixtures dropped the restart-marker JPEG and added PNG, TIFF, CMYK, ICC and orientation variants. |
| D-32 | **2c GUI choices** | (1) An effect palette of icon buttons replaces the effect dropdown; Phil picked the icons from a contact sheet (echo mountain, delay copy, reverb iteration-cw, amplify megaphone, invert eclipse, bitcrush blocks, backwards rewind, shift move-horizontal, shuffle orbit, stutter drum). (2) Placement is always shown, not folded away ("that's where the juice is"); the region is one two-handle slider that shades the preview outside its band while dragged. (3) **Shake**: random settings for a step's own parameters, drawn from the middle 80% of each slider (sizes in rows), separate from re-rolling the seed. (4) **Exact preview** on demand (D-26 follow-up): full-resolution bend, then shrink. (5) Over budget, the server answers 503 with `X-Too-Heavy: 1`; the GUI keeps the last image and offers an exact render. (6) The dice is disabled when a step has nothing random (`random_within` in the registry). |
| D-33 | **2d effects** | (1) `phaser` and `wahwah` follow Audacity's algorithms, with the LFO period in picture units (rows) instead of Hz, and our own sine (`dsp::sin_turns`, range reduction in turns, Taylor to x^19). Swept coefficients are recomputed every 16 samples (Audacity: 20 and 30), which made them 4-9x faster with no visible steps. (2) The phaser cannot be previewed at scale: its all-pass stages act as delays of a few samples, and which colour bytes they mix depends on that delay modulo 3, which a smaller raster cannot reproduce (measured error 28.6 against strength 13.9). Warping the stage frequencies made it no better. Instead the registry flags it (`exact_preview`) and the GUI renders exactly 600 ms after the last change. Wahwah previews well (8.5 against 36.9). (3) Low-pass filters over interleaved R, G, B bytes average the colours into grey, so channels gained a third choice, `3`: with `u8` each colour is its own channel, as in Audacity's raw import with 3 channels. (4) `speed` resamples whole pixels by default (`step=pixel`). (5) Default tuning is postponed. |
| D-34 | **PWA pass** | The GUI installs to the home screen: a web app manifest, icons rendered from one SVG (`web/public/icon.svg`, an eclipse sliced into shifted scanlines, generated by `scripts/icon.py`; `scripts/icons.sh` makes the PNGs, which are committed; Phil: fine for now, iterate later), iOS meta tags, theme colours that follow the toggle, and an HTML launch splash. (1) The manifest and icons are public, since browsers fetch them without cookies; everything else still needs the token. (2) A browser without the token gets a sign-in page with one field, because an iOS home-screen app may start with its own cookie jar. (3) On touch devices, exports go to the share sheet via a Save button (sharing needs a fresh tap, and a full render outlasts the one that started it); a plain download in a standalone app could open the file with no way back. (4) No service worker: the app is useless offline, and a cache would only risk serving a stale GUI after an upgrade. (5) Status bar style `default`, since iOS reads it once at launch and `black-translucent` would hide the clock on the light theme. No per-device iOS startup images; the HTML splash covers the load. |

## 1. Goals and non-goals

Goals:
- Reproduce the look of Audacity databending on photographs, with live preview.
- Local only. No network calls at runtime. Fonts and assets are bundled.
- Non-destructive and deterministic: originals are never written, and a recipe plus seed always gives the same image.
- A CLI and a web GUI as thin front ends over one core crate.
- The GUI is usable from a phone browser over the LAN or VPN.
- Linux first-class. macOS and Windows supported.

Non-goals:
- General photo editing.
- Bit-exact reproduction of Audacity's own output. The goal is the same character, from the same kinds of math.
- Cloud anything. Telemetry. Plugin ABI.

## 2. Name and license

Project `glitcharaxia`, binary `glitchrx`. GPL-3.0-or-later. libjpeg-turbo (BSD/IJG/zlib), the `png` and `tiff` crates (MIT/Apache) and rawler (LGPL-2.1, later) are compatible.

## 3. Architecture

### 3.1 Pipeline

```
input file --decode--> raster --steps--> raster --encode--> PNG / TIFF / JPEG
                      (RGB bytes)  (each step: bytes -> samples -> effect -> clip -> bytes)
```

1. **Decode** the input to a *raster*: 8-bit RGB, interleaved, rows top to bottom, no padding. EXIF orientation is applied, so the raster is upright. The colour profile (ICC) is kept aside and written into the output (should).
2. **Steps** run in order. Each step views a stretch of the raster as audio samples in its encoding (section 5), applies its effect within its windows, and clips the result back to bytes.
3. **Encode** the raster in the output format.

Effects never change the raster's length, so the image keeps its dimensions.

### 3.2 Layout

```
crates/
  glitchrx-core/
    src/
      raster.rs           Raster { width, height, bytes }, scaling for previews
      decode/             jpeg.rs (libjpeg-turbo), png.rs, tiff.rs, orient.rs, icc.rs
      encode.rs           PNG, TIFF, JPEG output
      samples.rs          encodings: bytes <-> f64 samples, clipping
      effects/            one file per effect + mod.rs registry
      dsp.rs              shared building blocks: delay lines, combs, allpass, LFO, sine
      params.rs placement.rs recipe.rs rng.rs io.rs
  glitchrx-cli/
  glitchrx-serve/
web/
tools/ gen-fixtures/ contact-sheet/
```

Removed from the live tree (tag `archive/jpeg-bytes`): `formats/jpeg.rs`, `restart.rs`, the scan-level tests and the restart transform in `turbo.rs`. libjpeg-turbo stays, as the JPEG decoder and encoder.

### 3.3 Determinism

- RNG: ChaCha8 keyed from SplitMix64 expansion of the `u64` step seed, with our own sampling helpers (unchanged).
- Effect math: f64, basic operations only (D-25). Constants that need transcendental functions (for example a decay curve) are computed once with our own deterministic routines, never in a per-sample loop with the platform's `libm`.
- Decoding is part of the output: the same input must decode to the same raster. libjpeg-turbo is pinned to its accurate integer IDCT (`TJPARAM_FASTDCT=0`) with default upsampling; golden tests cover decode-then-bend.
- Every effect has a `VERSION`, every step stores its seed, and per-hit randomness derives from `(step seed, hit index)`.

## 4. Inputs and outputs

| Format | In | Out | Notes |
| --- | --- | --- | --- |
| JPEG | must | must | libjpeg-turbo. Output quality 95, 4:4:4. |
| PNG | must | must | 8- and 16-bit in; 16-bit is reduced to 8-bit (should: round, not truncate). Alpha is dropped. |
| TIFF | must | must (default) | Uncompressed, LZW and Deflate in; uncompressed RGB out, for Photoshop. 16-bit in is reduced to 8-bit. |
| WebP, AVIF | later | later | Decode to raster; AVIF behind a feature flag (dav1d). |
| DNG / camera RAW | later | no | rawler, demosaiced to 8-bit RGB. iPhone ProRAW (JPEG XL inside) to be verified. |

- Grayscale and CMYK inputs are converted to RGB.
- **Orientation:** must. A portrait photo shot sideways is bent upright, so "rows" are what Phil sees.
- **ICC profile:** should be carried from input to output, so colours match the original in Photoshop.
- **Later:** a 16-bit raster, where 16-bit samples line up with 16-bit pixel channels.
- **Later:** a BMP-style layout (BGR, rows bottom to top), which gives the mirrored look of bending BMP files.

## 5. Sample encodings (Audacity "Import Raw Data")

The recipe sets `encoding` and `channels` once; any step may override them (D-23, D-29):

| `encoding` | Bytes per sample | Character (from the 2026-11-21 contact sheets) |
| --- | --- | --- |
| `u8` (default) | 1 | Clean ghosts; colours rotate through channels. Closest to Phil's examples. |
| `s8` | 1 | Like `u8`, but the clipping point sits mid-brightness, which inverts highlights. |
| `s16le`, `s16be` | 2 | Ghosts under heavy rainbow grain. Byte order changes the palette. |
| `s24le`, `s24be` | 3 | One sample per pixel: the whole pixel becomes one number. |
| `ulaw`, `alaw` | 1 | Posterized neon blocks with hard edges. |

- **Channels:** `1` (mono), `2` (stereo) or `3`. Samples alternate between the channels, and effects process each channel separately, as Audacity does. With `u8`, three channels are exactly R, G and B, so a filter that blends neighbouring samples keeps colour instead of greying it (D-33).
- **Clipping:** after the effect, samples are clamped to the encoding's range and converted back to bytes. For `ulaw`/`alaw`, the companding curve is applied on the way back.
- **Byte order** only exists for multi-byte samples. It decides which byte the effect treats as the loud part, which is why it mattered in Audacity.
- **Channel phase.** A delay of a whole number of pixels keeps colours; one or two extra bytes rotate red into green into blue. Delays are therefore given as a size plus a `phase` of 0, 1 or 2 bytes (section 6.2).

## 6. Effects

### 6.1 Character, not bit-exactness

Effects follow Audacity's built-ins in structure and parameters, computed in f64 and clipped at the end. They are not guaranteed to match Audacity's output to the byte; contact sheets judge them against Phil's examples.

### 6.2 Placement and units (all effects)

- **`region=START:END`**: where hits may land, in percent of the raster. In the raster, percent of the data *is* percent of the image height, so `40%:60%` really is the middle band. Rows (`120rows`) are also accepted.
- **`hits=N`**: seeded windows inside the region; `hits=0` means one window covering the whole region.
- **`len`**: window length, in `%` (of the image), `rows`, `px`, or bytes.
- **Delays and block sizes** take the same units plus **`phase`** (0-2 extra bytes) for channel rotation. Relative units are what make screen-resolution previews possible (section 9.2).

### 6.3 First batch (must)

| Effect | Audacity counterpart | Definition |
| --- | --- | --- |
| `echo` | Echo | Feedback delay: `y[n] = x[n] + decay * y[n - d]`. |
| `delay` | Delay (Nyquist) | Feed-forward taps at `d, 2d, ...`, each `decay^k` loud (the v0.2 `echo`). |
| `reverb` | Reverb | Freeverb-style: parallel damped feedback combs into series allpasses, with `room`, `damping`, `wet`, `dry`, `width`. |
| `amplify` | Amplify | Gain in dB with clipping. |
| `invert` | Invert | `y = -x`. |
| `backwards` | Reverse | Reverse each window. |
| `shift` | (cut and paste) | Rotate each window. |
| `shuffle` | (cut and paste) | Permute blocks inside each window. |
| `stutter` | Repeat | Repeat the start of each window across it. |
| `bitcrush` | Bitcrush-style | Keep the top bits of each sample. |

### 6.4 Second batch (should, after tuning the first)

| Effect | Audacity counterpart | Notes |
| --- | --- | --- |
| `phaser` | Phaser | All-pass stages swept by an LFO (our own sine). Likely source of the wavy look in `Glitchpicturecool-notmine.jpg`. |
| `wahwah` | Wahwah | Resonant low-pass swept by an LFO (Audacity's biquad). |
| `distort` | Distortion | Waveshaping curves (hard clip, soft clip, fold). |
| `speed` | Change Speed | Resample a window to a new rate, wrapping around to keep its length. Shears and stretches the picture. Whole pixels by default (`step=pixel`); `step=sample` drifts across R, G and B like Audacity. |

### 6.5 Later

- Pixel sorting and pixel-space effects (Phase 3).
- Strength sliders (a 0-100 macro per effect).
- Defaults tuned with contact sheets; Phil judges where the choice is aesthetic (D-7).

## 7. CLI

```
glitchrx [--EFFECT [k=v,...]]... [OPTIONS] INPUT [OUTPUT]
glitchrx [--EFFECT [k=v,...]]... [OPTIONS] --batch INPUT...
glitchrx --info INPUT...     glitchrx --list     glitchrx serve ...
```

Examples:

```
glitchrx --echo photo.jpg                                writes photo-glitch.tif
glitchrx --echo delay=0.33rows,decay=0.5,encoding=ulaw photo.jpg
glitchrx --reverb --echo phase=1 photo.tif photo-bent.tif
glitchrx --recipe moody.toml --batch *.jpg
```

| Option | Behaviour |
| --- | --- |
| `--seed N` | Master seed. The default is random and is always printed. |
| `--recipe FILE` | Apply a saved recipe instead of effect flags. |
| `--save-recipe[=FILE]` | Write the recipe used. Default `<output-stem>.recipe.toml`. |
| `--format tiff\|png\|jpeg` | Output format when OUTPUT is not given (default `tiff`); otherwise taken from OUTPUT's extension. |
| `--encoding E`, `--channels 1\|2\|3` | Recipe-wide sample encoding and channel count (default `u8`, `1`). |
| `--force` | Allow overwriting an existing output. Never the input. |
| `--batch`, `--info`, `--list`, `-q`, `-v`, `--help` | As before. `--info` reports the format, size, orientation and colour profile. |

Removed: `--reverse`, `--mode`, `--restart`, `--bake` (every output is already a normal image).

File safety, auto naming (`-glitch`, `-glitch-1`, ...) and exit codes are unchanged, except that exit code 3 no longer covers irreversible chains.

## 8. Recipe format

TOML, recipe `version = 2`. Version 1 recipes (JPEG byte mode) are refused with a message naming `archive/jpeg-bytes`.

```toml
format = "glitcharaxia-recipe"
version = 2
encoding = "u8"        # recipe-wide; a step may override
channels = 1

[[step]]
effect = "echo"
effect_version = 1
seed = 81723
encoding = "ulaw"       # this step only
delay = "0.33rows"
phase = 1
decay = 0.5
region = "30%:100%"
```

## 9. GUI

### 9.1 Changes from 2a

- Remove the Bands and Mode controls and the decode status.
- Each step gets `encoding` and `channels` pickers; the effect list shows the Audacity counterpart.
- A recipe-wide encoding picker, with per-step override.
- Export offers TIFF (default), PNG and JPEG; "Save next to original" keeps the CLI naming.
- Everything else (open, step stack, undo/redo, compare, recipes, browsing, token, layout, look) stays.

### 9.2 Preview performance

- **Cached per document:** the decoded full raster, a preview raster sized to the screen, and the raster after each step (both resolutions, within a memory budget).
- **Scaled preview (D-26).** Steps run on the preview raster. Relative sizes (`%`, `rows`, `px`) are scaled with it; byte sizes are scaled too, keeping their `phase`. Hit positions are percentages, so they land in the same places. The result should match the export closely; a contact-sheet test compares preview and export at matching scale and fails on large differences.
- **Cancellation.** A render checks between hits and every few hundred thousand samples whether a newer request arrived, and stops if so.
- **Budget.** A preview that runs over 3 s stops and the GUI says it is too heavy to preview live; export still works and shows progress.
- **Slider caps.** Sliders cover the useful range; the number field accepts the full range.
- **Target:** under 100 ms per preview for a typical step on a phone-sized screen, any source size.

### 9.3 Masking (Phase 3)

Pixel-domain glitches stay close to where they are applied, so masking can work like Phil's Photoshop workflow: the bent raster composited over the original through a mask (brush, shape, vector), with the mask sources stored in the recipe.

## 10. Phases

### Phase 1: CLI, JPEG byte effects (done, archived)

Built and tested on `phase-1-cli`; the byte engine is now at `archive/jpeg-bytes` (D-22).

### Phase 2: web GUI and pixel engine (branch `phase-2-gui`)

- **2a `glitchrx serve`:** done (D-16..D-21).
- **2b Pixel core.** Raster, decoders (JPEG with orientation, PNG, TIFF), encoders, sample encodings, the f64 effect framework, the first-batch effects, recipe v2, CLI changes, removal of the JPEG byte engine. Tests per section 11.
- **2c GUI on the pixel core.** Section 9.1 and 9.2: scaled previews, cancellation, budget, TIFF export.
- **2d Second-batch effects.** Phaser, wahwah, distort, speed, with contact sheets (D-33). Tuning defaults against `samples/examples/` with Phil is postponed (Phil, 2026-11-22).
- **2d' PWA pass.** App icon, home-screen install, name and splash, so the GUI feels like an app on iOS.
- **2e Tauri wrapper.** Linux first; macOS (Apple Silicon) later. Reuses the PWA icon.

### Phase 3: depth

- Masking (9.3), pixel sorting, strength sliders, presets.
- More inputs: WebP, AVIF, DNG/RAW.
- 16-bit raster and BMP layout (section 4).

Each phase ends in a working, tested tool, followed by a stop-and-summarize.

## 11. Testing and quality

### 11.1 Gate

`just check` (`scripts/check.sh`): `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, plus `svelte-check` and `vitest` when `web/node_modules` exists.

### 11.2 Required tests

- **Per-effect unit tests** on small sample buffers, including clipping at the range edges.
- **Encodings:** round trip bytes -> samples -> bytes is the identity for every encoding (no effect applied); U-law and A-law match the G.711 tables.
- **Length and dimensions** never change.
- **Window confinement:** an effect writes only inside its windows.
- **Determinism:** golden BLAKE3 hashes of decode-then-bend for fixed seeds, every effect and encoding, via insta.
- **Decoders:** every fixture decodes; orientation 1-8 fixtures come out upright; malformed files are errors, never panics.
- **Preview fidelity:** scaled preview vs. downscaled export, mean difference under a threshold for each effect at defaults.
- **File safety and naming:** unchanged from v0.2.
- **API tests** (serve): preview, export in each format, cancellation.

### 11.3 Fixtures

Procedural, CC0, committed: JPEG (4:2:0, 4:4:4, grayscale, progressive, CMYK, EXIF orientation 1-8, ICC profile), PNG (8- and 16-bit, alpha, palette), TIFF (uncompressed, LZW, 16-bit).

### 11.4 Contact sheets

`tools/contact-sheet` sweeps one parameter or encoding per sheet. Claude reads them to tune and shows Phil the ones where the choice is his.

## 12. Repository and agent harness

- `main` holds the bootstrap only; each phase has a branch; feature branches merge back once the gate is green; Claude never pushes or merges into `main`.
- Conventional commits, no AI attribution lines.
- At each phase boundary, stop and summarize: what was built, real test output, deviations, open questions, next step, effort outlook.
- `CLAUDE.md` holds the operational rules; its invariants change with v0.3 (section 13). `VOCAB.md` grows with the project.

## 13. Invariants (replacing v0.2's)

- **Inputs are never written**, even with `--force`; outputs are never overwritten without `--force`.
- **Dimensions are preserved:** effects never change the raster's length.
- **Determinism:** same input, recipe and seed give the same output bytes on every supported platform (D-25).
- **No network calls** at runtime.

Dropped: header-safe (no compressed data is edited) and exact reversibility (D-24).

## 14. Open items

1. Default encoding: `u8`, unless the second round of contact sheets says otherwise.
2. Preview fidelity threshold (9.2), to be set from measurements.
3. Resolved: TIFF is the default output (D-28); encoding is recipe-wide with per-step override (D-29); close-not-identical previews accepted (D-26).
