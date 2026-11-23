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
| D-5 | LAN/web GUI | Yes, and it is the **primary** GUI (`glitchrx serve`). The Tauri desktop app (D-35) wraps the same server. |
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
| D-26 | **Preview at screen resolution** | Live previews bend a screen-sized copy of the image, with sizes and delays scaled to match (section 9.2). Close to the export, not byte-identical. Exports always run at full resolution. This trades exact WYSIWYG for about 10-20 times less work per preview. Amended by D-40: once changes stop, the preview settles to exact by itself. |
| D-27 | **CPU limits, no GPU** | No GPU: Audacity's echo and reverb feed back on themselves, so each sample depends on the previous one, which GPUs cannot speed up; GPU float results also vary by hardware, which would break D-25. Instead: screen-resolution previews (D-26), cancelling stale renders mid-way, capped slider ranges, and a preview time budget (section 9.2). |
| D-28 | **Output is a normal image** | The glitched pixels are saved as uncompressed TIFF (default, for Phil's Photoshop masking), PNG, or JPEG (quality 95). There is no "glitched file itself" any more: the pixels *are* the result. |
| D-29 | **Recipe-wide encoding** | A recipe sets `encoding` and `channels` once, like Audacity's import dialog; any step may override them. Claude's call, delegated by Phil. |
| D-30 | **Sizes mapped from full resolution** | Found while measuring preview fidelity in 2b. Scaling byte counts by the area ratio misplaces things twice: a third of a row at full size became a twelfth of one in a quarter-size preview, and the byte remainder (the colour rotation) came out arbitrary, so `reverb` previews had different colours. Every size and hit position is now worked out at full resolution and mapped into the preview by rows (height ratio), the rest of the row (width ratio) and the 0-2 byte phase (kept). Hits are drawn as a fraction of the free space. Measured mean preview error at defaults, quarter size: 1.5-5.8 per byte for every effect except `reverb` (10.3, against an effect strength of 30); `tests/fixtures.rs` prints the table. The remaining gap is inherent: a delay that wraps past a row moves its ghost down one row, and a preview row stands for several full rows, so long feedback tails drift. 2c can offer a full-resolution preview on demand. |
| D-31 | **2b deviations** | (1) Decoding and encoding live in one `codec.rs` rather than `decode/` plus `encode.rs`; each format is small. (2) `reverb` has no `width`: it only applies to joint stereo processing, and channels are processed separately (section 5). Can be added with a stereo filter in 2d. (3) Screen-resolution previews and cancellation are already in the server (pulled forward from 2c), since the 2a cache did not fit rasters. (4) Fixtures dropped the restart-marker JPEG and added PNG, TIFF, CMYK, ICC and orientation variants. |
| D-32 | **2c GUI choices** | (1) An effect palette of icon buttons replaces the effect dropdown; Phil picked the icons from a contact sheet (echo mountain, delay copy, reverb iteration-cw, amplify megaphone, invert eclipse, bitcrush blocks, backwards rewind, shift move-horizontal, shuffle orbit, stutter drum). (2) Placement is always shown, not folded away ("that's where the juice is"); the region is one two-handle slider that shades the preview outside its band while dragged. (3) **Shake**: random settings for a step's own parameters, drawn from the middle 80% of each slider (sizes in rows), separate from re-rolling the seed. (4) **Exact preview** on demand (D-26 follow-up): full-resolution bend, then shrink. (5) Over budget, the server answers 503 with `X-Too-Heavy: 1`; the GUI keeps the last image and offers an exact render. (6) The dice is disabled when a step has nothing random (`random_within` in the registry). |
| D-33 | **2d effects** | (1) `phaser` and `wahwah` follow Audacity's algorithms, with the LFO period in picture units (rows) instead of Hz, and our own sine (`dsp::sin_turns`, range reduction in turns, Taylor to x^19). Swept coefficients are recomputed every 16 samples (Audacity: 20 and 30), which made them 4-9x faster with no visible steps. (2) The phaser cannot be previewed at scale: its all-pass stages act as delays of a few samples, and which colour bytes they mix depends on that delay modulo 3, which a smaller raster cannot reproduce (measured error 28.6 against strength 13.9). Warping the stage frequencies made it no better. Instead the registry flags it (`exact_preview`) and the GUI renders exactly 600 ms after the last change. Wahwah previews well (8.5 against 36.9). (3) Low-pass filters over interleaved R, G, B bytes average the colours into grey, so channels gained a third choice, `3`: with `u8` each colour is its own channel, as in Audacity's raw import with 3 channels. (4) `speed` resamples whole pixels by default (`step=pixel`). (5) Default tuning is postponed. |
| D-34 | **PWA pass** | The GUI installs to the home screen: a web app manifest, icons rendered from one SVG (`web/public/icon.svg`, an eclipse sliced into shifted scanlines, generated by `scripts/icon.py`; `scripts/icons.sh` makes the PNGs, which are committed; Phil: fine for now, iterate later), iOS meta tags, theme colours that follow the toggle, and an HTML launch splash. (1) The manifest and icons are public, since browsers fetch them without cookies; everything else still needs the token. (2) A browser without the token gets a sign-in page with one field, because an iOS home-screen app may start with its own cookie jar. (3) On touch devices, exports go to the share sheet via a Save button (sharing needs a fresh tap, and a full render outlasts the one that started it); a plain download in a standalone app could open the file with no way back. (4) No service worker: the app is useless offline, and a cache would only risk serving a stale GUI after an upgrade. (5) Status bar style `default`, since iOS reads it once at launch and `black-translucent` would hide the clock on the light theme. No per-device iOS startup images; the HTML splash covers the load. |
| D-35 | **2e desktop app** | (1) Tauri 2 (stable 2.11; 3.0 is alpha), crate `glitchrx-desktop`. The app starts the `glitchrx-serve` router in-process on `127.0.0.1:0` with a random per-launch token, and opens a native window on that URL. One code path for the LAN GUI and the desktop. (2) The page gets no Tauri IPC. Native dialogs come from the server side: a `Dialogs` trait in `glitchrx-serve`, implemented with `tauri-plugin-dialog`, behind `/api/desktop/open` and `/api/docs/{id}/save-as` (404 in `glitchrx serve`). (3) Save as: the system dialog's "replace?" confirmation counts as `--force` for that file; the input is still never written, and the recipe beside it is never replaced. (4) Linux install is user-local (`scripts/install-desktop.sh`: `~/.local/bin`, a launcher entry named after the binary to match the Wayland app id, hicolor icons). `deb`/AppImage bundling is configured for later (needs the Tauri CLI). (5) The workspace gate now builds the desktop crate (needs `webkit2gtk-4.1`). macOS next, on Apple Silicon. |
| D-36 | **Pixel sorting** | (1) Effects belong to a group, `audio`, `data` or `pixel`, reported by the server; the palette shows them as three sections (Phil: pixel sorting "is a totally different kind of glitch art", so it gets its own). (2) Pixel effects see whole RGB pixels in two dimensions: their windows cover whole pixels and the window's position in the raster is passed in, so a sort can find its columns. Encoding and channels do not apply and the GUI hides them. (3) `pixelsort` is the classic ASDF sort, matched to `samples/examples/PENN-6076-small-pixelsort.jpg`: each column (or row) is cut into runs of pixels whose brightness lies between `low` and `high`; pixels outside the band stay and break the runs; each run is sorted by brightness, hue or saturation, highest first by default; `max` caps run length. Integer Rec. 709 luma and a stable sort keep it deterministic. Defaults are a first guess; tuning is with the rest. (4) Where runs break depends on texture that a shrunken preview averages away (preview error 22.8 against strength 40.7), so it is `exact_preview`, like the phaser. It sorts 24 MP in about 0.4 s. |
| D-37 | **Masks** | (1) Per step, stored in the recipe as operations (components), not pixels: `brush` (a set of strokes, each with points, size and hardness, some of which may `erase` that brush's own paint), `linear` gradients, `radial` circles (hard-edged by default, an optional `hardness` fades them inward; Phil: "the feather should be what makes it fade"), and `brightness` ranges (read from the picture as it arrives at the step). Each is added, subtracted or intersected with the mask so far, starting from empty; then `invert` and `feather` (three box blurs, about a Gaussian). (2) Positions are fractions of the picture and sizes fractions of its longer side, so a mask lands in the same place on a phone preview and on the export (measured preview error 1.8 against strength 62.8). (3) After the step runs, its result is blended over its input: `out = in + (out - in) * mask * opacity`, rounded. A mask can only soften the step's own change, never add one, so it never writes outside the step's windows (tested). (4) In the GUI, Paint in / Paint out sets `invert` (Phil's "masking in a glitch or masking out the glitch"); intersect operations are kept last, so a brightness limit restricts everything painted; the overlay comes from the server (`/api/docs/{id}/mask`), so what is shown is exactly what renders. (5) Recipe version 3; version 2 loads unchanged. (6) Every component stays editable (Phil, 2026-11-22: "I can't seem to readjust the first one I put down"): in the GUI one is active at a time, a radial or linear is adjusted by its handles, and a brush keeps adding strokes. That is why a brush's eraser only reaches its own strokes; to cut into everything, add a brush that subtracts. The first cut of v3 had one `stroke` op per stroke; it never left the phase branch, and the GUI folds such masks into brushes when it loads them. (7) Each component has its own `opacity` and `feather` (Phil: "when a mask is active and I adjust feathering or opacity, it should only adjust that mask's settings"). Feather blurs the component's own coverage before it joins the mask. Opacity moves the mask that share of the way toward the full combination, which for add and subtract scales the shape, and for intersect limits only partly instead of darkening everything. The mask-wide feather is gone; a recipe that still has one is refused rather than rendered without it. The step's own opacity and Paint in / Paint out apply on top. (8) In the GUI a long press (or a shift, cmd or ctrl click) selects several components; opacity, feather, Copy and Cut act on all of them, and Paste adds copies to any step's mask, so Cut then Paste moves parts between steps. Dragging between steps is parked: the cards are a screen apart on a phone. |
| D-38 | **Opacity, strength** | Every step has `opacity` (0-1, a step key like `encoding`), which blends the whole result over the input; it also works from the CLI (`--echo opacity=0.5`). Phil asked whether a strength slider and an opacity slider are the same idea: for most effects they are close, and opacity is exact and effect-independent, so it ships now. Per-effect strength macros (one slider driving several parameters) wait for the tuning pass, where they need Phil's eye. |
| D-40 | **Previews settle to exact** | Phil, 2026-11-22: exports did not match what he had liked on screen. Pixel-level texture cannot scale with the screen-sized copy (a 1 px pattern is 1/1170 of a phone preview but 1/5500 of the export), so 600 ms after the last change the preview settles to a full-resolution render shrunk to the screen: exactly the export. Measured 0.3-1.6 s on 20-26 MP samples (16 cores). The Make exact button is gone; the status bar says approximate or exact. Later idea: a recipe "work size" (glitch at a chosen resolution, then scale up with hard pixels) would let the chunkier preview look export at full size. |
| D-41 | **New photo, old steps** | Opening another photo while the stack has steps asks: Keep steps, or Start fresh (the main button, undoable). "Don't ask again" remembers the choice on that device, and Presets holds the setting (until Phase 4's Settings). Presets are the way to reuse a look on purpose (agreed with Phil, 2026-11-22). Restoring the last photo after a reload does not ask. |
| D-42 | **Zoom** (Phase 4a) | (1) A zoomed view shows the whole-photo preview enlarged, with a sharp patch on top: the part on screen cut from the full-resolution render, asked for 150 ms after the view holds still. No part of a picture can be bent on its own, since effects run along the whole raster, so a patch is always exact. (2) The server keeps two full-resolution rasters per photo being worked on: the picture just before the step being edited (a first render guesses the last step) and the last finished render. On a 6000 x 3750 photo, editing a light last step after a phaser went from 2.0 s to 0.35 s per exact render, a pan's patch takes about 7 ms, and exporting what was just shown takes 29 ms instead of a full run. Opening another photo frees the others' rasters. Crops and whole-picture previews cancel only their own kind. (3) 100% shows one photo pixel per device pixel, as in Lightroom; zoom goes to 800%, and past 100% pixels show as squares. (4) Masking: one finger works the tool and two fingers pinch and pan; the zoom stays put between strokes. The brush is sized against the screen, as in Procreate, so zooming in paints finer lines; its setting is its size at fit. Positions are kept to 1/100,000 of the side (was 1/10,000) for strokes painted at 800%. The mask overlay stays at screen resolution, soft when zoomed. |
| D-43 | **Rectangles and pen paths** (Phase 4b) | (1) Two more mask components. `rect`: `center` (fractions), `size` (width and height as shares of the longer side, so a square stays square on any picture) and `angle` (degrees, clockwise, left out when 0). `path`: a closed outline of `points`, each `[x, y]` (a corner) or `[x, y, hx, hy]` (a smooth point whose handle bends the side after it; its mirror bends the side before, so the outline passes through smoothly, as with Photoshop's pen). The last point joins the first. (2) Both are filled with exact area coverage (`fill.rs`, the signed-area accumulation font rasterizers use): each pixel gets the share of it inside the outline, so edges are anti-aliased and crisp, and feather softens them like any component. Paths that cross themselves fill by the nonzero rule. Curves are flattened to within a tenth of a pixel (Wang's formula). Rotation uses `dsp.rs`'s sine, so all of it is basic f64 operations (D-25); a golden hash covers a turned rectangle and a curved path that runs off the picture. (3) GUI: a rectangle's corners resize it against the opposite corner, a handle 30 px above its top edge turns it (settling on multiples of 45 within 2 degrees; shift for 15 degree steps), inside moves it, and a drag elsewhere draws it again at the same angle. A pen path is drawn by tapping points; dragging while placing one pulls out its handle; tapping the first point (or Finish, Enter, Esc) ends drawing. Afterwards a point drags, the picked point shows its handles, a tap on a side adds a point (smooth on a curve, keeping its shape), inside moves the whole path, and the card offers Corner / Smooth and Remove (or Delete). Each placed point is one undo step. |
| D-44 | **WebP and AVIF export** (Phase 4d) | (1) WebP through libwebp (`libwebp-sys`, built from its bundled source): lossy with a quality (default 90) or lossless; the colour profile goes in an ICCP chunk; at most 16383 px a side (the format's limit), which the dialog checks first. (2) AVIF through ravif/rav1e, pure Rust, assembly off (no `nasm` needed; turning it on later is about twice as fast and gives the same file): quality (default 80) and speed 1-10 (default 6). rav1e makes one tile per thread and tiles change the file, so the thread count is fixed at 8: the same input and settings give the same bytes on any machine. (3) AVIF holds no ICC profile, only colour codes. sRGB and Display P3 (found by the profile's name) are written as codes (P3: primaries 12, sRGB transfer); any other profile is dropped with a warning in the CLI and a note in the dialog. (4) JPEG gains a quality setting (default 95, as before). (5) CLI: `--format webp\|avif`, `--quality 1-100`, `--lossless`, `--avif-speed 1-10` (not `--speed`, which is the Change Speed effect). (6) Timings on a 22 MP photo (whole run, one bitcrush step): TIFF 0.7 s / 65 MB, PNG 0.7 s / 17 MB, WebP lossy 2.1 s / 5.7 MB, WebP lossless 5.3 s / 4.9 MB, AVIF speed 10 2.7 s / 4.6 MB, speed 6 11 s / 4.0 MB. (7) The dialog says that lossy compression smooths fine glitch texture. WebP and AVIF are not read as inputs (parked). |
| D-45 | **Settings, library and export folders, Save As** (Phase 4c) | (1) A Settings sheet (the gear, which replaces the theme button): Appearance (Light, Dark, System), the library folder (what Open browses) and the export folder (where Save As starts), and the new-photo choice (moved from Presets). (2) The folders are kept by the server in `~/.config/glitchrx/config.toml` (`--config FILE` to move it), so every device shares them, and are picked with a folder browser that never leaves the server user's home folder; the browser sees them only as paths relative to home. A folder that has gone, or lies outside home, is dropped on load with a warning. `--root` still decides what Open browses for that run, and the sheet says so. The theme and the new-photo choice stay per device. (3) Save As (browser): a folder (default: the export folder, else the library) and a name, prefilled with the photo's next free `-glitch` name there. The name is one file name (no folders, no leading dot); the format's extension is added. It never overwrites: a taken name is refused (409) before rendering, and the dialog says so. The desktop app keeps its native Save dialog. (4) The export format and its settings are remembered per device. |
| D-46 | **Preset groups** (Phase 4e) | (1) Steps added from a preset, or a preset used in place of the recipe, form a group in the stack: a tinted panel labelled "From preset NAME" with its actions below (Revert, Update preset, Ungroup) and arrows to move it. (2) The group keeps the preset's steps as added (its base). It is modified when its steps that are switched on differ from the base, compared in the preset's own terms: a step's encoding and channels are spelled out against the preset's settings, so pins added to fit another recipe do not count, while a changed recipe-wide encoding does. Modified is worked out, not stored, so undo and setting a value back clear it. Turning a step off, reordering inside the group and removing a step all count. (3) Revert puts the base back in place (removed steps return, switched-off ones are replaced). Update preset asks, then writes the group's steps that are on to the preset file in its own encoding and channels, and they become the new base. Both are undoable in the stack; undoing an Update does not change the file back. (4) A group's steps stay together: they move only inside the group, a loose step hops over a whole group, and the group moves as a block. Duplicating a grouped step keeps it in the group. Ungroup keeps the steps and drops the label. (5) Groups live in the GUI's state (history, this device's saved recipe) only; recipes sent to the server or saved to files are unchanged. |
| D-39 | **Presets** | Presets are Phil's own saved recipes, not built-in looks. The server keeps them as ordinary recipe files in `~/.config/glitchrx/presets` (`--presets DIR` to move them), so phone and desktop share them and the CLI replays them with `--recipe`. Names are checked, not cleaned up (no slashes, no leading dot), so two names never share a file and nothing escapes the directory. Saving over a name needs an explicit confirmation. Using a preset replaces the steps (undoable); adding one appends its steps and pins their encoding and channels if they differ from the current recipe. |

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

### 6.5 Pixel effects (Phase 3)

Whole RGB pixels in two dimensions; encoding and channels do not apply (D-36).

| Effect | Counterpart | Character |
|---|---|---|
| `pixelsort` | ASDF pixel sort | Runs of pixels inside a brightness band, sorted by brightness, hue or saturation, down the columns or along the rows. Bright areas melt into streaks while shadows hold the picture. |

### 6.6 Later

- More pixel-space effects.
- Strength sliders (a 0-100 macro per effect; D-38).
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
| `--format tiff\|png\|jpeg\|webp\|avif` | Output format when OUTPUT is not given (default `tiff`); otherwise taken from OUTPUT's extension. |
| `--quality N`, `--lossless`, `--avif-speed N` | Encoder settings (D-44): quality 1-100 for JPEG (95), lossy WebP (90) and AVIF (80); lossless WebP; AVIF speed 1-10 (6). |
| `--encoding E`, `--channels 1\|2\|3` | Recipe-wide sample encoding and channel count (default `u8`, `1`). |
| `--force` | Allow overwriting an existing output. Never the input. |
| `--batch`, `--info`, `--list`, `-q`, `-v`, `--help` | As before. `--info` reports the format, size, orientation and colour profile. |

Removed: `--reverse`, `--mode`, `--restart`, `--bake` (every output is already a normal image).

File safety, auto naming (`-glitch`, `-glitch-1`, ...) and exit codes are unchanged, except that exit code 3 no longer covers irreversible chains.

## 8. Recipe format

TOML, recipe `version = 3`. Version 2 recipes load unchanged and are written back as version 3. Version 1 recipes (JPEG byte mode) are refused with a message naming `archive/jpeg-bytes`.

```toml
format = "glitcharaxia-recipe"
version = 3
encoding = "u8"        # recipe-wide; a step may override
channels = 1

[[step]]
effect = "echo"
effect_version = 1
seed = 81723
encoding = "ulaw"       # this step only
opacity = 0.8           # blend of the result over the step's input
delay = "0.33rows"
phase = 1
decay = 0.5
region = "30%:100%"

[step.mask]             # optional: where the result shows (9.3)
invert = true           # paint out instead of in

[[step.mask.op]]
shape = "brush"         # brush, linear, radial, rect, path or brightness
mode = "add"            # add, subtract or intersect
opacity = 0.8           # optional: how strongly this part counts
feather = 0.01          # optional: blur of its edges, share of the longer side

[[step.mask.op.stroke]]
size = 0.06             # diameter, share of the longer side
hardness = 0.5
points = [0.21, 0.40, 0.25, 0.43]   # x, y as fractions of the picture

[[step.mask.op.stroke]]
erase = true            # takes back this brush's own paint
size = 0.02
points = [0.23, 0.41]

[[step.mask.op]]
shape = "radial"
from = [0.6, 0.5]       # centre
to = [0.8, 0.5]         # a point on the edge

[[step.mask.op]]
shape = "rect"
center = [0.3, 0.6]
size = [0.2, 0.1]       # width, height: shares of the longer side
angle = 15              # degrees clockwise (optional)

[[step.mask.op]]
shape = "path"          # a closed pen outline
points = [[0.1, 0.1], [0.4, 0.1, 0.5, 0.2], [0.2, 0.4]]   # [x, y] corner, [x, y, hx, hy] smooth

[[step.mask.op]]
shape = "brightness"
mode = "intersect"
low = 0.5
high = 1.0
soft = 0.1
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
- **Scaled preview (D-26, D-40).** While things change, steps run on the preview raster; once they stop, the preview settles to an exact render. Relative sizes (`%`, `rows`, `px`) are scaled with it; byte sizes are scaled too, keeping their `phase`. Hit positions are percentages, so they land in the same places. The result should match the export closely; a contact-sheet test compares preview and export at matching scale and fails on large differences.
- **Cancellation.** A render checks between hits and every few hundred thousand samples whether a newer request arrived, and stops if so.
- **Budget.** A preview that runs over 3 s stops and the GUI says it is too heavy to preview live; export still works and shows progress.
- **Slider caps.** Sliders cover the useful range; the number field accepts the full range.
- **Target:** under 100 ms per preview for a typical step on a phone-sized screen, any source size.

### 9.3 Masking (Phase 3, D-37)

Pixel-domain glitches stay close to where they are applied, so masking works like a layer mask in Photoshop: each step's result is blended over its input through the step's mask.

- **Step card, Blend section:** the step's opacity; without a mask, six buttons (Brush, Linear, Radial, Rectangle, Pen, Brightness) start one with that component, and Paste when something is copied. With a mask: Edit / Done (no mask-wide delete: removing the last component removes the mask, and the step shows everywhere again); Paint in / Paint out; the list of components, each with Add / Subtract / Intersect and remove, and its opacity and feather on a second line when not the defaults; "Add to mask" (the same six) beside Copy, Cut and Paste; Show mask. Tapping a component makes it the only selected one and the active one (a green outline) and opens its controls: opacity and feather side by side, then a brush's Paint / Erase, size and hardness, or a brightness range's two-handle range and softness. A long press (shift, cmd or ctrl click) adds or removes a component from the selection, also outlined; the sliders then set every selected one.
- **Editing on the preview:** the active component decides what a gesture does. A brush paints or erases a stroke. A radial shows a green ring with a centre dot (drag to move it; dragging inside does too) and an edge dot (drag to resize; so does the ring). A linear shows a solid line where it is full and a dashed one where it has faded out, with a dot on each (drag either, or the middle to move both). Dragging anywhere else draws the active shape again. Rectangles and pen paths are edited as D-43 (3) describes. Shapes that are not active show a pin (a white dot) that makes them active when tapped; pins take precedence over the active shape's inside, but not over its dots. A new radial starts centred, a fifth of the longer side in radius; a new linear fades from a quarter of the way down to three quarters. The preview follows a drag live; each stroke, finished drag and added component is one undo step. The mask overlay (red, like a rubylith) can be shown or hidden; a stroke stays drawn until a render that includes it arrives. While editing, the top bar becomes an edit bar (what is being masked, undo, redo, Done), so nothing floats over the photo; Open, Presets and Export return after Done.
- **Desktop keys while editing:** B and E paint and erase with a brush (the active one, else the last, else a new one), [ and ] brush size, Enter or Esc to finish. A mouse shows the brush outline, and a grab or move cursor over handles.
- **Later:** blend modes.

### 9.4 Presets (Phase 3, D-39)

A Presets button in the top bar: save the recipe under a name, use a preset (replaces the steps, undoable), add a preset's steps to the current recipe, delete. Presets with an effect version this build no longer has are listed with the reason and cannot be used. What opening another photo does with the steps (Ask, Keep steps, Start fresh; D-41) moved to Settings in 4c (D-45).

### 9.5 Step card

- **Header:** number, effect icon and name, tags (encoding override, a venetian-mask icon when masked, opacity below 100%), and the on/off switch. Effect names are lower case, as on the command line. When an effect is in the stack more than once, each step is numbered (`echo-0`, `echo-1`, ...); a step keeps its number when moved, a new one takes the next after the highest in use, and duplicates get a new number (Phil, 2026-11-23).
- **Body:** the effect's settings, headed by Shake (moved off the header, where it was easy to bump; Phil, 2026-11-23); Samples (encoding, channels); Placement, ending with the seed and the dice that re-rolls it (the seed only decides where hits land, so it belongs there); Blend (9.3); then the step's own actions in a row of their own: Up, Down, Duplicate, Remove.

### 9.6 Zoom (Phase 4a, D-42)

The same gestures with a finger or the mouse:

- **Press and hold** the photo: the original, until released (as in Instagram). The `\` key too.
- **Double tap**, the **magnifier** (bottom right, where "Hold for original" was) or **Z**: fit to 100% and back. Zoomed, the magnifier shows the zoom.
- **Pinch** with two fingers, ctrl + wheel, or a trackpad pinch: zoom about the fingers or the pointer.
- **Zoomed in:** one finger, the middle mouse button or the wheel pans.
- **While masking:** one finger works the tool; two fingers pinch and pan; the edit bar has a compare button (hold), since holding the photo paints.
- The status bar says exact once the sharp patch for the current recipe is on screen.

## 10. Phases

### Phase 1: CLI, JPEG byte effects (done, archived)

Built and tested on `phase-1-cli`; the byte engine is now at `archive/jpeg-bytes` (D-22).

### Phase 2: web GUI and pixel engine (branch `phase-2-gui`)

- **2a `glitchrx serve`:** done (D-16..D-21).
- **2b Pixel core.** Raster, decoders (JPEG with orientation, PNG, TIFF), encoders, sample encodings, the f64 effect framework, the first-batch effects, recipe v2, CLI changes, removal of the JPEG byte engine. Tests per section 11.
- **2c GUI on the pixel core.** Section 9.1 and 9.2: scaled previews, cancellation, budget, TIFF export.
- **2d Second-batch effects.** Phaser, wahwah, distort, speed, with contact sheets (D-33). Tuning defaults against `samples/examples/` with Phil is postponed (Phil, 2026-11-22).
- **2d' PWA pass.** App icon, home-screen install, name and splash, so the GUI feels like an app on iOS.
- **2e Tauri wrapper.** Linux done (D-35); macOS (Apple Silicon) later, steps in `docs/macos.md`. Reuses the PWA icon.

### Phase 3: depth (branch `phase-3-depth`)

- **Done:** pixel sorting (D-36), masks and opacity (D-37, D-38), presets (D-39); after Phil's first hands-on round: editable mask components with handles and pins, per-component opacity and feather, multi-select and copy / cut / paste (D-37), previews that settle to exact (D-40), the new-photo choice (D-41), the step card's seed row and action row (9.5), new icons.

### Phase 4: tools (planned with Phil, 2026-11-22; starts on his go-ahead)

- **4a Zoom and pan (done, D-42, 9.6).** Pinch to zoom anywhere, on the phone and the desktop (wheel or trackpad). Press and hold the photo to see the original, as in Instagram; a magnifier button takes the place of "Hold for original". Zoomed in, one finger pans. While masking, one finger works the tool and two fingers pinch and pan, and the zoom stays put between strokes (as in Procreate and Lightroom mobile); the edit bar gets a compare button, since holding the photo there paints. A zoomed view asks the server for just the visible region, so it stays sharp, and settles to exact like the full view. Exact renders also start reusing cached full-resolution steps (today a change to the last step re-runs them all).
- **4b Geometric masks (done, D-43).** A rectangle (corner handles to resize, a handle to rotate, drag inside to move) and a pen tool for polygons: tap to place corners, tap the first to close, drag while placing to bend a side into a curve; points stay editable. Both are components like the others (mode, opacity, feather). Coverage is computed exactly, anti-aliased, with basic f64 operations, so they replay byte for byte (D-25).
- **4c Library, export folder, Save As (done, D-45).** A Settings sheet: a library folder (what Open browses) and an export folder, picked with a folder browser limited to the home directory and saved in `~/.config/glitchrx/config.toml`; `--root` still overrides. The new-photo choice moves there. Export gains Save As (folder, name, format) beside "next to original" and "download to this device".
- **4d Export formats (done, D-44).** WebP (lossy with quality, or lossless; libwebp) and AVIF (quality and speed; a pure-Rust encoder). Same input and settings give the same file. Lossy compression smooths fine glitch texture, which the dialog says.
- **4e Preset groups (done, D-46).** Steps added from a preset form a labelled group in the stack ("From preset: NAME", a divider). Editing one marks the group modified, with Revert (back to the preset's settings) and Update preset. The group moves as a block. Chosen over locking (copies need no lock: the preset file only changes on Update).

### Parked

Not planned for now, kept so the ideas are not lost: more inputs (WebP, AVIF, RAW/DNG; RAW needs a demosaic and colour pipeline, and converting to DNG does not avoid it), 16-bit raster and BMP layout (section 4), DNG export (a finished-pixel DNG is a TIFF with extra tags; TIFF already goes into Lightroom losslessly), dragging mask parts between steps, a recipe "work size" (D-40), per-effect strength sliders (D-38), the macOS build (`docs/macos.md`), app icon iteration.

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
- **Dimensions are preserved:** effects never change the raster's length, and write only inside their windows. A mask or opacity only softens a step's own change.
- **Determinism:** same input, recipe and seed give the same output bytes on every supported platform (D-25).
- **No network calls** at runtime.

Dropped: header-safe (no compressed data is edited) and exact reversibility (D-24).

## 14. Open items

1. Default encoding: `u8`, unless the second round of contact sheets says otherwise.
2. Preview fidelity threshold (9.2), to be set from measurements.
3. Resolved: TIFF is the default output (D-28); encoding is recipe-wide with per-step override (D-29); close-not-identical previews accepted (D-26).
