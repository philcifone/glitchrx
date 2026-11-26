# glitcharaxia: Specification

Status: **v0.3, 2026-11-21, approved by Phil.** v0.3 changes what the effects work on. v0.2 glitched the *compressed* data inside a JPEG. Phil's reference work (`samples/examples/`, uncompressed TIFFs bent in Audacity) shows that the look he wants comes from bending *raw pixels*. v0.3 makes the raw-pixel domain the whole engine and removes JPEG byte glitching from the live project. The source documents are `pre-brief.md` and `BRIEF.md`, and `VOCAB.md` defines the terms used here.

Keywords: **must** means required for the phase to be done. **Should** means expected, but logged and deferred if it proves hard. **Later** means out of scope for the phase.

## What changed from v0.2, in one paragraph

A JPEG stores Huffman-coded frequency data, not pixels. Changing a byte there derails the decoder, which then paints flat color until it recovers: the bands Phil saw on every effect. An uncompressed image stores the pixels themselves, row by row, R G B. Run Audacity's echo over those bytes and you get ghosted copies of the picture, colors rotating through the channels, and smears: the look of Phil's examples. A prototype on his photos confirmed it (2026-11-21, contact sheets in the session). So glitchrx now decodes any input to a raw pixel buffer, bends that buffer the way Audacity would, and saves the result as a normal image. The recipe model, seeds, CLI, web GUI, undo, export and file safety all carry over.

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
| D-30 | **Sizes mapped from full resolution** | Found while measuring preview fidelity in 2b. Scaling byte counts by the area ratio misplaces things twice: a third of a row at full size became a twelfth of one in a quarter-size preview, and the byte remainder (the color rotation) came out arbitrary, so `reverb` previews had different colors. Every size and hit position is now worked out at full resolution and mapped into the preview by rows (height ratio), the rest of the row (width ratio) and the 0-2 byte phase (kept). Hits are drawn as a fraction of the free space. Measured mean preview error at defaults, quarter size: 1.5-5.8 per byte for every effect except `reverb` (10.3, against an effect strength of 30); `tests/fixtures.rs` prints the table. The remaining gap is inherent: a delay that wraps past a row moves its ghost down one row, and a preview row stands for several full rows, so long feedback tails drift. 2c can offer a full-resolution preview on demand. |
| D-31 | **2b deviations** | (1) Decoding and encoding live in one `codec.rs` rather than `decode/` plus `encode.rs`; each format is small. (2) `reverb` has no `width`: it only applies to joint stereo processing, and channels are processed separately (section 5). Can be added with a stereo filter in 2d. (3) Screen-resolution previews and cancellation are already in the server (pulled forward from 2c), since the 2a cache did not fit rasters. (4) Fixtures dropped the restart-marker JPEG and added PNG, TIFF, CMYK, ICC and orientation variants. |
| D-32 | **2c GUI choices** | (1) An effect palette of icon buttons replaces the effect dropdown; Phil picked the icons from a contact sheet (echo mountain, delay copy, reverb iteration-cw, amplify megaphone, invert eclipse, bitcrush blocks, backwards rewind, shift move-horizontal, shuffle orbit, stutter drum). (2) Placement is always shown, not folded away ("that's where the juice is"); the region is one two-handle slider that shades the preview outside its band while dragged. (3) **Shake**: random settings for a step's own parameters, drawn from the middle 80% of each slider (sizes in rows), separate from re-rolling the seed. (4) **Exact preview** on demand (D-26 follow-up): full-resolution bend, then shrink. (5) Over budget, the server answers 503 with `X-Too-Heavy: 1`; the GUI keeps the last image and offers an exact render. (6) The dice is disabled when a step has nothing random (`random_within` in the registry). |
| D-33 | **2d effects** | (1) `phaser` and `wahwah` follow Audacity's algorithms, with the LFO period in picture units (rows) instead of Hz, and our own sine (`dsp::sin_turns`, range reduction in turns, Taylor to x^19). Swept coefficients are recomputed every 16 samples (Audacity: 20 and 30), which made them 4-9x faster with no visible steps. (2) The phaser cannot be previewed at scale: its all-pass stages act as delays of a few samples, and which color bytes they mix depends on that delay modulo 3, which a smaller raster cannot reproduce (measured error 28.6 against strength 13.9). Warping the stage frequencies made it no better. Instead the registry flags it (`exact_preview`) and the GUI renders exactly 600 ms after the last change. Wahwah previews well (8.5 against 36.9). (3) Low-pass filters over interleaved R, G, B bytes average the colors into grey, so channels gained a third choice, `3`: with `u8` each color is its own channel, as in Audacity's raw import with 3 channels. (4) `speed` resamples whole pixels by default (`step=pixel`). (5) Default tuning is postponed. |
| D-34 | **PWA pass** | The GUI installs to the home screen: a web app manifest, icons rendered from one SVG (`web/public/icon.svg`, an eclipse sliced into shifted scanlines, generated by `scripts/icon.py`; `scripts/icons.sh` makes the PNGs, which are committed; Phil: fine for now, iterate later), iOS meta tags, theme colors that follow the toggle, and an HTML launch splash. (1) The manifest and icons are public, since browsers fetch them without cookies; everything else still needs the token. (2) A browser without the token gets a sign-in page with one field, because an iOS home-screen app may start with its own cookie jar. (3) On touch devices, exports go to the share sheet via a Save button (sharing needs a fresh tap, and a full render outlasts the one that started it); a plain download in a standalone app could open the file with no way back. (4) No service worker: the app is useless offline, and a cache would only risk serving a stale GUI after an upgrade. (5) Status bar style `default`, since iOS reads it once at launch and `black-translucent` would hide the clock on the light theme. No per-device iOS startup images; the HTML splash covers the load. |
| D-35 | **2e desktop app** | (1) Tauri 2 (stable 2.11; 3.0 is alpha), crate `glitchrx-desktop`. The app starts the `glitchrx-serve` router in-process on `127.0.0.1:0` with a random per-launch token, and opens a native window on that URL. One code path for the LAN GUI and the desktop. (2) The page gets no Tauri IPC. Native dialogs come from the server side: a `Dialogs` trait in `glitchrx-serve`, implemented with `tauri-plugin-dialog`, behind `/api/desktop/open` and `/api/docs/{id}/save-as` (404 in `glitchrx serve`). (3) Save as: the system dialog's "replace?" confirmation counts as `--force` for that file; the input is still never written, and the recipe beside it is never replaced. (4) Linux install is user-local (`scripts/install-desktop.sh`: `~/.local/bin`, a launcher entry named after the binary to match the Wayland app id, hicolor icons). `deb`/AppImage bundling is configured for later (needs the Tauri CLI). (5) The workspace gate now builds the desktop crate (needs `webkit2gtk-4.1`). macOS next, on Apple Silicon. |
| D-36 | **Pixel sorting** | (1) Effects belong to a group, `audio`, `data` or `pixel`, reported by the server; the palette shows them as three sections (Phil: pixel sorting "is a totally different kind of glitch art", so it gets its own). (2) Pixel effects see whole RGB pixels in two dimensions: their windows cover whole pixels and the window's position in the raster is passed in, so a sort can find its columns. Encoding and channels do not apply and the GUI hides them. (3) `pixelsort` is the classic ASDF sort, matched to `samples/examples/PENN-6076-small-pixelsort.jpg`: each column (or row) is cut into runs of pixels whose brightness lies between `low` and `high`; pixels outside the band stay and break the runs; each run is sorted by brightness, hue or saturation, highest first by default; `max` caps run length. Integer Rec. 709 luma and a stable sort keep it deterministic. Defaults are a first guess; tuning is with the rest. (4) Where runs break depends on texture that a shrunken preview averages away (preview error 22.8 against strength 40.7), so it is `exact_preview`, like the phaser. It sorts 24 MP in about 0.4 s. |
| D-37 | **Masks** | (1) Per step, stored in the recipe as operations (components), not pixels: `brush` (a set of strokes, each with points, size and hardness, some of which may `erase` that brush's own paint), `linear` gradients, `radial` circles (hard-edged by default, an optional `hardness` fades them inward; Phil: "the feather should be what makes it fade"), and `brightness` ranges (read from the picture as it arrives at the step). Each is added, subtracted or intersected with the mask so far, starting from empty; then `invert` and `feather` (three box blurs, about a Gaussian). (2) Positions are fractions of the picture and sizes fractions of its longer side, so a mask lands in the same place on a phone preview and on the export (measured preview error 1.8 against strength 62.8). (3) After the step runs, its result is blended over its input: `out = in + (out - in) * mask * opacity`, rounded. A mask can only soften the step's own change, never add one, so it never writes outside the step's windows (tested). (4) In the GUI, Paint in / Paint out sets `invert` (Phil's "masking in a glitch or masking out the glitch"); intersect operations are kept last, so a brightness limit restricts everything painted; the overlay comes from the server (`/api/docs/{id}/mask`), so what is shown is exactly what renders. (5) Recipe version 3; version 2 loads unchanged. (6) Every component stays editable (Phil, 2026-11-22: "I can't seem to readjust the first one I put down"): in the GUI one is active at a time, a radial or linear is adjusted by its handles, and a brush keeps adding strokes. That is why a brush's eraser only reaches its own strokes; to cut into everything, add a brush that subtracts. The first cut of v3 had one `stroke` op per stroke; it never left the phase branch, and the GUI folds such masks into brushes when it loads them. (7) Each component has its own `opacity` and `feather` (Phil: "when a mask is active and I adjust feathering or opacity, it should only adjust that mask's settings"). Feather blurs the component's own coverage before it joins the mask. Opacity moves the mask that share of the way toward the full combination, which for add and subtract scales the shape, and for intersect limits only partly instead of darkening everything. The mask-wide feather is gone; a recipe that still has one is refused rather than rendered without it. The step's own opacity and Paint in / Paint out apply on top. (8) In the GUI a long press (or a shift, cmd or ctrl click) selects several components; opacity, feather, Copy and Cut act on all of them, and Paste adds copies to any step's mask, so Cut then Paste moves parts between steps. Dragging between steps is parked: the cards are a screen apart on a phone. |
| D-38 | **Opacity, strength** | Every step has `opacity` (0-1, a step key like `encoding`), which blends the whole result over the input; it also works from the CLI (`--echo opacity=0.5`). Phil asked whether a strength slider and an opacity slider are the same idea: for most effects they are close, and opacity is exact and effect-independent, so it ships now. Per-effect strength macros (one slider driving several parameters) wait for the tuning pass, where they need Phil's eye. |
| D-40 | **Previews settle to exact** | Phil, 2026-11-22: exports did not match what he had liked on screen. Pixel-level texture cannot scale with the screen-sized copy (a 1 px pattern is 1/1170 of a phone preview but 1/5500 of the export), so 600 ms after the last change the preview settles to a full-resolution render shrunk to the screen: exactly the export. Measured 0.3-1.6 s on 20-26 MP samples (16 cores). The Make exact button is gone; the status bar says approximate or exact. Later idea: a recipe "work size" (glitch at a chosen resolution, then scale up with hard pixels) would let the chunkier preview look export at full size. |
| D-41 | **New photo, old steps** | Opening another photo while the stack has steps asks: Keep steps, or Start fresh (the main button, undoable). "Don't ask again" remembers the choice on that device, and Presets holds the setting (until Phase 4's Settings). Presets are the way to reuse a look on purpose (agreed with Phil, 2026-11-22). Restoring the last photo after a reload does not ask. |
| D-42 | **Zoom** (Phase 4a) | (1) A zoomed view shows the whole-photo preview enlarged, with a sharp patch on top: the part on screen cut from the full-resolution render, asked for 150 ms after the view holds still. No part of a picture can be bent on its own, since effects run along the whole raster, so a patch is always exact. (2) The server keeps two full-resolution rasters per photo being worked on: the picture just before the step being edited (a first render guesses the last step) and the last finished render. On a 6000 x 3750 photo, editing a light last step after a phaser went from 2.0 s to 0.35 s per exact render, a pan's patch takes about 7 ms, and exporting what was just shown takes 29 ms instead of a full run. Opening another photo frees the others' rasters. Crops and whole-picture previews cancel only their own kind. (3) 100% shows one photo pixel per device pixel, as in Lightroom; zoom goes to 800%, and past 100% pixels show as squares. (4) Masking: one finger works the tool and two fingers pinch and pan; the zoom stays put between strokes. The brush is sized against the screen, as in Procreate, so zooming in paints finer lines; its setting is its size at fit. Positions are kept to 1/100,000 of the side (was 1/10,000) for strokes painted at 800%. The mask overlay stays at screen resolution, soft when zoomed. |
| D-43 | **Rectangles and pen paths** (Phase 4b) | (1) Two more mask components. `rect`: `center` (fractions), `size` (width and height as shares of the longer side, so a square stays square on any picture) and `angle` (degrees, clockwise, left out when 0). `path`: a closed outline of `points`, each `[x, y]` (a corner) or `[x, y, hx, hy]` (a smooth point whose handle bends the side after it; its mirror bends the side before, so the outline passes through smoothly, as with Photoshop's pen). The last point joins the first. (2) Both are filled with exact area coverage (`fill.rs`, the signed-area accumulation font rasterizers use): each pixel gets the share of it inside the outline, so edges are anti-aliased and crisp, and feather softens them like any component. Paths that cross themselves fill by the nonzero rule. Curves are flattened to within a tenth of a pixel (Wang's formula). Rotation uses `dsp.rs`'s sine, so all of it is basic f64 operations (D-25); a golden hash covers a turned rectangle and a curved path that runs off the picture. (3) GUI: a rectangle's corners resize it against the opposite corner, a handle 30 px above its top edge turns it (settling on multiples of 45 within 2 degrees; shift for 15 degree steps), inside moves it, and a drag elsewhere draws it again at the same angle. A pen path is drawn by tapping points; dragging while placing one pulls out its handle; tapping the first point (or Finish, Enter, Esc) ends drawing. Afterwards a point drags, the picked point shows its handles, a tap on a side adds a point (smooth on a curve, keeping its shape), inside moves the whole path, and the card offers Corner / Smooth and Remove (or Delete). Each placed point is one undo step. |
| D-44 | **WebP and AVIF export** (Phase 4d) | (1) WebP through libwebp (`libwebp-sys`, built from its bundled source): lossy with a quality (default 90) or lossless; the color profile goes in an ICCP chunk; at most 16383 px a side (the format's limit), which the dialog checks first. (2) AVIF through ravif/rav1e, pure Rust, assembly off (no `nasm` needed; turning it on later is about twice as fast and gives the same file): quality (default 80) and speed 1-10 (default 6). rav1e makes one tile per thread and tiles change the file, so the thread count is fixed at 8: the same input and settings give the same bytes on any machine. (3) AVIF holds no ICC profile, only color codes. sRGB and Display P3 (found by the profile's name) are written as codes (P3: primaries 12, sRGB transfer); any other profile is dropped with a warning in the CLI and a note in the dialog. (4) JPEG gains a quality setting (default 95, as before). (5) CLI: `--format webp\|avif`, `--quality 1-100`, `--lossless`, `--avif-speed 1-10` (not `--speed`, which is the Change Speed effect). (6) Timings on a 22 MP photo (whole run, one bitcrush step): TIFF 0.7 s / 65 MB, PNG 0.7 s / 17 MB, WebP lossy 2.1 s / 5.7 MB, WebP lossless 5.3 s / 4.9 MB, AVIF speed 10 2.7 s / 4.6 MB, speed 6 11 s / 4.0 MB. (7) The dialog says that lossy compression smooths fine glitch texture. WebP and AVIF are not read as inputs (parked). |
| D-45 | **Settings, library and export folders, Save As** (Phase 4c) | (1) A Settings sheet (the gear, which replaces the theme button): Appearance (Light, Dark, System), the library folder (what Open browses) and the export folder (where Save As starts), and the new-photo choice (moved from Presets). (2) The folders are kept by the server in `~/.config/glitchrx/config.toml` (`--config FILE` to move it), so every device shares them, and are picked with a folder browser that never leaves the server user's home folder; the browser sees them only as paths relative to home. A folder that has gone, or lies outside home, is dropped on load with a warning. `--root` still decides what Open browses for that run, and the sheet says so. The theme and the new-photo choice stay per device. (3) Save As (browser): a folder (default: the export folder, else the library) and a name, prefilled with the photo's next free `-glitch` name there. The name is one file name (no folders, no leading dot); the format's extension is added. It never overwrites: a taken name is refused (409) before rendering, and the dialog says so. The desktop app keeps its native Save dialog. (4) The export format and its settings are remembered per device. |
| D-46 | **Preset groups** (Phase 4e) | (1) Steps added from a preset, or a preset used in place of the recipe, form a group in the stack: a tinted panel labelled "From preset NAME" with its actions below (Revert, Update preset, Ungroup) and arrows to move it. (2) The group keeps the preset's steps as added (its base). It is modified when its steps that are switched on differ from the base, compared in the preset's own terms: a step's encoding and channels are spelled out against the preset's settings, so pins added to fit another recipe do not count, while a changed recipe-wide encoding does. Modified is worked out, not stored, so undo and setting a value back clear it. Turning a step off, reordering inside the group and removing a step all count. (3) Revert puts the base back in place (removed steps return, switched-off ones are replaced). Update preset asks, then writes the group's steps that are on to the preset file in its own encoding and channels, and they become the new base. Both are undoable in the stack; undoing an Update does not change the file back. (4) A group's steps stay together: they move only inside the group, a loose step hops over a whole group, and the group moves as a block. Duplicating a grouped step keeps it in the group. Ungroup keeps the steps and drops the label. (5) Groups live in the GUI's state (history, this device's saved recipe) only; recipes sent to the server or saved to files are unchanged. |
| D-47 | **Edit mode** (Phase 5a) | Phil wanted to reorder collapsed steps and select several at once, suggesting arrows, drag and drop, or press-and-hold. Chosen: an Edit button in the Steps header, as in iOS Mail and Reminders (press-and-hold and dragging whole cards both fight with scrolling on a phone). In Edit mode each step is one line: a circle to select it, its icon and name, and a grip on the right to drag it; a preset group keeps its panel and has a grip of its own. Selected steps can be duplicated (new seeds) or removed together, one undo each. Dragging uses the arrow buttons' moves, so groups stay whole; a row swaps with its neighbor once the finger passes the neighbor's middle (measured on the neighbor, so a tall group cannot make it bounce), the list scrolls near its edges, and a drop is one undo. The finger is followed on the window: a swapped row's element moves in the DOM and loses its pointer capture. The arrows stay on each card. |
| D-48 | **Work size** (Phase 5b) | Phil asked for preview-resolution buttons, having seen that effects look different at different resolutions. They do because pixel-level texture cannot shrink with the picture (D-40), so a preview resolution would not change what is exported. The size the steps run on is made a recipe setting instead. (1) `work_size` (the longer side, at least 64 px; never enlarges): the image is shrunk to it with the core's integer box filter, then the steps run on that canvas, measuring their sizes against it. Coarser texture and faster renders; at sizes no larger than the preview it is shown whole, so the live preview is already exact. (2) The file is the canvas size, or with `scale_back`, enlarged to the original size by repeating pixels (nearest neighbor, integer arithmetic), so the coarse texture stays crisp. (3) GUI: Full, 4K (4096), 2K (2048), 1K (1024) in the photo toolbox (D-62; sizes not below the photo's are greyed out), undoable like any edit; the export sheet offers the two output sizes; the status bar shows both sizes; 100% zoom is a canvas pixel. The original (press and hold) is the unbent canvas, so before and after show the same pixels. (4) CLI: `--work-size N`, `--scale-back`. (5) Recipe version 4. The server keeps one shrunk canvas per photo (the image itself at full size, not a copy) and clears its render caches when the work size changes. |
| D-49 | **Adjustments** (Phase 5c) | Phil asked for basic photo tools as steps with masking and opacity, with the scope kept tight. A fourth effect group, `adjust`, of ordinary steps: they take opacity, masks and placement (hits default 0, the whole picture) and work on whole RGB pixels like pixelsort, so encoding and channels do not apply. (1) `levels`: black, white, gamma, output black and white, optionally one channel. (2) `curves`: points `IN:OUT/...` (0 to 255, 2 to 16, inputs rising; `/` because commas separate CLI parameters), a new parameter kind `curve`, joined by a monotone cubic (Fritsch-Carlson), so the curve never overshoots its points; flat beyond the end points; optionally one channel. (3) `hsl`: hue turn in degrees, saturation and lightness -100 to 100, whole picture (no per-color ranges); greys stay grey. (4) `balance`: one tone range per step (shadows, midtones, highlights; weights from the quadratic Bernstein polynomials of Rec. 709 brightness), red/cyan, green/magenta, blue/yellow, and `luminosity=keep` (default) restores brightness. Three tone ranges in one step would be nine sliders; a step per range keeps cards short, and duplicating a step is quick. Defaults first did something visible, as every effect's do (gentle contrast, an S curve, +30 saturation, a warm midtone tint); since D-58 they start neutral. Determinism (D-25): basic f64 operations; `ln` and `pow` added to `dsp.rs` (atanh series, and exp of y ln x), and `sqrt`, which IEEE 754 rounds correctly; levels and curves compute a 256-entry table per step. GUI: an Adjust section in the picker, a curve editor (drag points, touch the grid to add one, remove the selected one, text form below), no Shake for adjustments (random black and white points can cross). |
| D-50 | **Crop** (Phase 5d) | Not a step: steps keep the size (D-2), and their masks, placement and seeds measure against the picture they get. A recipe-wide `[crop]` (x, y, w, h as shares of the upright photo) is the first stage of the canvas: crop, then work size (D-48), then the steps, then `scale_back` (to the crop's size). Shares become pixels by rounding each edge (floor(share x n + 0.5), at least one pixel, inside the photo), the same in the core and the GUI. Masks are shares of the whole photo, so they stay on it when the crop changes (D-52); placement is measured on the canvas. GUI: a crop button beside the zoom button opens a crop mode on the whole, unbent photo (outside the crop there is nothing bent to show): a frame with thirds lines, corner handles, edge handles when free, drag inside to move; ratios Free, Original, 1:1, 5:4 and 16:9, turned with a portrait/landscape switch (labels follow: 4:5, 9:16), starting the photo's way round; choosing one fits the largest frame of it, centered on the current one. Done (or Enter) applies it as one undoable change; Cancel (or Escape) leaves things as they were; Reset frames the whole photo, which removes the crop. CLI: `--crop X,Y,W,H`. Kept in recipe version 4 (not yet released outside this phase, so no version bump). |
| D-51 | **Picking up old edits** (Phase 5e) | Phil could not find how to continue from a recipe saved with an export; on iOS the file picker cannot see the server's folders. (1) Recipes carry an optional `source`: the file name (no folders) of the photo they were made on. Saves, downloads and the CLI's `--save-recipe` write it; presets do not (they belong to no photo). It changes nothing in the output. (2) The server finds `.recipe.toml` files in the library (or `--root`) and the export folder, up to three folders deep and a few thousand entries per folder, newest first. Read-only, confined to those folders, files up to 1 MB. API: `GET /api/recipes`, `GET /api/recipe?place=library|export&path=`. (3) A recipe matches a photo when its `source` is the photo's name, or (older recipes without one) when its file name is the export name: `photo-glitch`, `photo-glitch-N` or `photo`. Opening a photo returns its matches in `DocInfo.recipes`. (4) GUI: a banner over the photo, "Continue from NAME", with Continue (load the newest, replacing the steps; undoable), More (all of them) and Not now. The export sheet's Recipe section has "Saved on the server", which lists every recipe, those made on this photo first. Matching by name can only guess for recipes saved before `source`, and photos renamed since are not matched; the list still shows them. |
| D-52 | **Masks belong to the photo, not the crop** (Phase 6a) | Phil: masks must stay in place when cropping. Mask positions are shares of the whole, uncropped photo, and lengths (brush size, rectangle size, feather) shares of its longer side, so a crop never changes them and they cannot run into the limits `validate` checks, however tight the crop. To render, the core moves a mask onto the canvas (`mask::Frame`, from the crop in whole pixels): positions shift by the crop's corner and scale by photo size over crop size, lengths by photo longer side over crop longer side; in pixels that is just a shift. Previews, exports, the mask view and the CLI all go through `Recipe::prepare_for(photo size)`. The GUI's painter measures against the rectangle where the whole photo would sit on screen (reaching past the crop), so what it records is already a share of the photo; the server's mask view is drawn over the canvas. New components are laid out on the visible canvas and then converted, so they appear in the middle of the crop. Without a crop nothing changes. Placement (`region`, `hits`) is still measured on the canvas: it is a share of the raster's bytes, which has no place on the photo. |
| D-53 | **Curves editor: grid and histogram** (Phase 6b) | Phil asked for a grid (to look at, not to snap to). Eighths, with the quarters stronger. Behind it, a histogram of the picture as it reaches the step, as in Photoshop: `POST /api/docs/{id}/histogram {recipe, step, max_w, max_h}` returns 256 counts each for red, green, blue and brightness (Rec. 709 with integer weights), measured on the cached preview of the steps before it, so it costs almost nothing. The editor shows the channel the step changes (brightness for all three), scaled to the tallest level other than 0 and 255, where clipped pixels pile up, and refreshes a moment after each new preview. |
| D-54 | **Color ranges for hsl** (Phase 6c) | `range`: all (default), reds, yellows, greens, cyans, blues, magentas; one per step, like balance's tones (D-49). A pixel's share is its closeness on the color wheel to the range's center (full within 15 degrees, fading out by 45, so neighbors overlap and orange is half red, half yellow) times its colorfulness (full from a chroma of 1/8), so greys, whose hue is arbitrary, never change. The result moves that share of the way from the pixel to its adjusted color. `range=all` is exactly the old algorithm, so the effect's version and golden hashes stay as they were. |
| D-55 | **Sliders know their default** | Phil, 2026-11-23: the reset button above a slider vanished at the default, so the row changed height and the slider jumped under his finger. Sliders now have no reset button (Phil: one way to reset is enough): a small tick under the track marks the default, the thumb snaps onto it within 1.2 % of the track, and a double tap (or double click) on the slider resets it. Fields without a slider (curves, region, color dots, sizes in bytes or pixels) keep the button, which stays in place, dimmed, at the default. The double tap is detected by hand from pointer events, since iOS does not reliably send `dblclick` to a range input, and the second tap's own movement is ignored. |
| D-56 | **Every effect starts on the whole picture; bands are a choice** | Phil, 2026-11-23: seven effects (amplify, backwards, invert, shift, shuffle, speed, stutter) opened with bands across the picture. All now default to `hits=0`, and the Placement section has a **Bands** switch: off covers the whole region; on scatters the step in 4 seeded bands (or as many as it had) and shows hits, length and the seed. Recipes do not write defaults out, so the seven effects went to version 2, and each effect has `upgrades`: a version 1 step that relied on the old default gets it written in (`hits`, `len`) when read, by the core for files, presets and the CLI, and by the GUI for the recipe it keeps in the browser. Old recipes replay exactly. Two effects cannot be previewed at scale over the whole picture (shuffle's block permutation, stutter's copied first row); their live preview settles to the exact render once you pause (D-40). |
| D-57 | **Several color ranges at once; color dots** | Phil asked for a row of color dots instead of the dropdown, with several on together. A new parameter kind, `set`: `all`, or words from a fixed list joined by `+` (`range=reds+blues`), any order. hsl adds the shares of the chosen ranges (at most 1), so neighbors together take the color between them fully. In the GUI each range is a dot in its muted Gruvbox color: tap on, tap off; none on is all colors. A single range or `all` gives the same picture as before, so hsl keeps its version. |
| D-58 | **Adjustments start neutral** | Phil, 2026-11-24: color balance opening at 12, 0, -12 reads as a mistake; an adjustment should start at zero and be moved from there, like Photoshop's. levels (black 0, white 255), curves (`0:0/255:255`), hsl (saturation 0) and balance (0, 0, 0) now change nothing when added. The other effects keep defaults that do something visible: a glitch that does nothing at first gives nothing to judge. All four went to version 2 with an upgrade (D-56) that writes the old defaults into older steps, so old recipes replay exactly; the tests run them with the old values, and their golden hashes did not change. |
| D-59 | **Full view** | Phil asked for a way to see the photo bigger. A tap on the photo (once the double tap's window has passed, as in Photos), or a swipe down on the panel's grabber when it is not expanded, hides the top bar and the status bar and lowers the step panel to just its grabber, so the stage fills the screen; another tap, a tap or pull up on the grabber, or Esc brings them back. (At first a corner button did this and the panel went away entirely, leaving nothing to pull back up; Phil, 2026-11-24.) The grabber now swipes: up expands the panel, down shrinks it, then hides it; a tap still toggles. CSS only: the Fullscreen API does not work on the iPhone for anything but video. The panel follows the finger while dragged and the photo scales with the stage; on release it eases (0.38 s, an iOS sheet's curve) to the nearest place, or one place on after a real swipe, and the bars slide away rather than vanish. Grid rows animate only between sizes of the same kind, so the bars are measured and their rows written in pixels, and the stage and panel as shares in fr out of 100. A sharper render waits until the stage holds still. Not while masking or cropping, which need the top bar. The resume banner hides in full view. |
| D-60 | **Levels editor** | Phil wanted the classic levels view, which is also shorter than five sliders. As in Photoshop: the histogram of the picture reaching the step (as in curves, D-53), with the clipped ends shaded; an input strip under it with black, midtone and white handles; an output strip with its two handles between their numbers. Dragging anywhere on a strip moves the nearest handle; a double tap puts it back on its default; handles take the arrow keys. The midtone handle sits where an input comes out half way, `0.5^gamma` of the stretch, so it keeps its place in the stretch as black and white move, and dragging it left lightens. Black and white stay two levels apart, so the midtone has room. `channel` stays an ordinary field. |
| D-61 | **Remembered pictures** | Phil asked for switching a step off and on to be about as quick as holding for the original. Steps run in a chain, each on the output of the one before, so there is no per-step layer to show or hide: switching off step 2 changes what steps 3 and 4 make. What can be kept is whole results. Renders are deterministic (D-25), so the browser keeps the last 24 exact pictures (up to 64 MB) keyed by photo, render size and the recipe as sent, and shows one at once, without a request, when the recipe comes back to it. Switching a step back on, or off again, is then instant and exact; the status bar says "remembered". The first switch still renders, and its exact render follows after 120 ms instead of the 600 ms that waits out a slider. A result from an older change that arrives after a remembered one is dropped. |
| D-62 | **Photo toolbox** | Phil found the crop button, the magnifier and the work size distracting on the photo while editing. They move into a toolbox card at the top of the step panel, above the encoding: crop and zoom as icon buttons (zoom shows the percentage while zoomed in), the work size as a segmented control on the right, one row. Nothing floats over the photo now but messages and the resume banner. The full view button went with them; a tap on the photo does that instead (D-59). |
| D-63 | **Project files** (Phase 7) | Phil wanted an edit to open on any device, as a `.psd` does, without carrying the original and the recipe around separately. A zip container (the OpenDocument pattern) holding the original, the recipe and a JPEG preview; section 8.1. The extension is `.glrx` (Phil: four letters, and closer to the name). The original keeps its image data exactly, so replays match, but loses its location. A project may be saved over, after a confirmation or `--force`: the rule that inputs are never written protects photos, and a project is a working file. |
| D-64 | **Pull** (Phase 8) | Phil asked for a manual way to push pixels around, like a crop-style mode, but as a step that switches on and off and takes a mask. Strokes stored in shares of the photo, rendered through a fixed-size displacement map, hard pixels by default; section 8.2. Pull only; twirl and others later. |
| D-65 | **New folder** (Phase 8) | Phil, 2026-11-24: when saving on the server he wants to make a folder there and then (a `projects` folder, a folder per format), by his own choice; the app never sorts files into folders by itself. The folder browser that Save As and Settings share gains a **New folder** button: a name, then the folder is made inside the one being shown and opened, ready to save into. Same rules as a file name in Save As: one name (no slashes, no leading dot), never outside the server user's home folder, and an existing name is refused (409) rather than reused silently. Only a folder is made: nothing is moved or renamed, and an empty folder is left for the user to remove. Permissions are not a new risk: the server already writes files under home as the user it runs as, and making a folder there is the same right; a folder it may not write in gives a plain error. |
| D-66 | **Hold for Edit mode, icon bars, one export name, download links** (Phase 8) | Phil, 2026-11-24. (1) Edit mode has no button: holding a step's header (not its switch) for 0.45 s turns it on, and holding a row (not its grip) turns it off; Done stays. D-47 had passed on press-and-hold because it fights with scrolling; here a press that drifts more than 10 px, or that the browser takes for a scroll, cancels the hold, and the click after a hold is swallowed so it never also taps. The Steps header says "Hold a step to edit the list". (2) The Steps header's dice and Clear go. The dice moves into Edit mode's bar, where it re-rolls the selected steps, or all of them with none selected, and the status bar says how many changed; Clear is Select all and Remove. (3) The card's Up, Down, Duplicate, Remove row and Edit mode's bar are icons only (the selection box is a checkbox: empty, partial, all; the count sits in the middle). Holding an icon shows its name in a bubble, as iOS toolbars do; a mouse gets the title tooltip. The dice looks dimmed when nothing is random but still answers, to say why. (4) Export has one Name, at the top, for Download, Save next to original and Save As; it suggests the next free name and stops following the folder and format once typed over. Save next to original adds `-1`, `-2`, ... when the name (or its recipe) is taken; Save As still refuses a taken name. (5) Downloads are links: the server holds the finished file for 10 minutes at `/api/downloads/{id}/{name}` (128 random bits; at most 6 files, 768 MB) and the page follows the link, instead of a `blob:` URL that Firefox on iOS saves under the blob's id. The link needs no cookie, so a download manager fetching it outside the page still can; making one needs the token. A recipe download goes the same way. |
| D-67 | **Pull pins** (Phase 8) | Phil, 2026-11-24, after first using pull: more control, keeping the freehand drawing. A grid over the photo; a point snapped to it is picked up and stretched elsewhere, and can be picked up again, redirected or put back. Phil chose: the grid shown as dots or lines (or off, as he first imagined it), its spacing and the pin's reach as separate settings, and pins inside the pull step's card rather than a step of their own (one mask and one blend for both). A pin is built as a full-strength straight stroke run after the strokes (not a sum of weighted offsets, which folds the picture over itself once a move exceeds the reach); see 8.2. Recipe version 6. |
| D-68 | **Pull: one list, brushes, a card that stays put** (Phase 8) | Phil, 2026-11-24, after using pins: the card was hard to follow (Done twice, the settings vanished on Done, no clear way to start a new pin while one was selected), and a stroke drawn after moving pins went under them. (1) Strokes and pins are one list in the order made, so whatever is done last goes on top; a pin dragged again comes to the top too (tapping it does not), and its home then marks what it picks up in the picture as it is shown. (2) Strokes belong to **brushes** (Phil chose groups that resize together): drawing with a brush selected adds to it, with its size and strength; New starts another. Every stroke has a marker where it ends; tapping one selects its brush, and the sliders then change all its strokes. (3) The card is the same in and out of pull mode: Draw or Pins, what the sliders change ("Selected brush, 3 strokes", "Selected pin", "New brush", "New pin"), New, then the tool's settings. Done is in the top bar only. Clear all is gone, and there is no Delete all: only "Delete this pin" or "Delete this brush", while one is selected (Phil). (4) A Markers switch in Draw hides the markers (Phil liked the idea). Recipe version 7: `[[step.pull]]` holds both kinds (a stroke has `points` and maybe `brush`, a pin `from` and `to`); version 6 files load with their pins after their strokes, the picture they made. |
| D-69 | **Crop the result** | Phil, 2026-11-25: he wanted to crop over the glitch he had made, but the crop runs first (D-50), so cropping re-bends a narrower picture and the glitch changes. The crop editor gains **Crop the: Photo \| Result**. Photo is D-50 as before. Result runs every step on the whole photo (shrunk to the work size) and cuts the frame from what they give, after `scale_back`: the glitch inside the frame stays exactly as it was, and moving the frame re-runs nothing (it is not part of the canvas's framing, so no cache drops). Masks and pulls are unaffected, since the steps see the whole photo. In the editor, Result shows the whole glitch under the frame; Photo the unbent photo. Recipe-wide `crop_result = true` beside `[crop]`, version 8; `--crop-result` on the CLI (needs `--crop`). The server frames live previews (rendering the whole canvas larger so the frame fills the box, at most 8192 px), exact previews, the original for compare and the mask view; zoomed views are measured within the frame. Files are written as the oldest version that holds them, so only a recipe that crops the result is version 8 and everything else still opens on version 7 builds. On a phone the crop editor takes the whole screen (the bars and the panel slide away), and its button reads Crop rather than Done (Phil). Rejected: a crop step placed anywhere in the stack, since a step that changes the size breaks D-2 and moves every later mask and pull. |
| D-70 | **Polish from Phil's use, 2026-11-25** | (1) Open's two buttons are equal and neutral, **From Device** and **Open .glrx**: Phil picked a project under the photo button, whose type filter makes iOS grey projects out, and took it for a bug. (2) Opening another photo drops the crop (undoable): a crop belongs to the photo it was made on, and the old frame sat over the new picture. Steps are kept or not as before. (3) Edit mode: the step held to start it comes in selected; a selected row shows a green ring inside its edge and sinks slightly (scale 0.975), animated, instead of changing anything about the layout. (4) **Save As may update a project** of the same name, after "Update NAME.glrx?": the rule that nothing is overwritten exists to protect photos (Phil: in the Audacity workflow, renaming extensions by hand is how originals were lost), and a project is a working file (D-63). The server answers the first try with 409 and `X-Replaceable: 1`, and replaces only a file that is a project by its first bytes; pictures and anything else are still never overwritten. (5) Follow-up, same day: a new crop starts on **Result** (what is on screen); an existing crop keeps its mode. The Steps header gets two buttons in place of the "Hold a step" hint: double carets that fold every card shut (pointing together) or open them all (pointing apart), and a pencil into Edit mode. Holding a step still works. (6) The encoding card and the photo toolbox sit side by side (encoding and channels on the left; crop, zoom and an info button above the work size on the right), with their notes in one line under both. The info button shows a note on work sizes there, inline like the encoding's description: the app has no popups, and on a phone one would cover what it explains. Version 0.2.0, with `CHANGELOG.md`. |
| D-71 | **Filters: low-pass, high-pass, band-pass, notch, Bass and Treble** (specced and built 2026-11-25) | Phil's first pick from the Audacity list in `docs/roadmap.md`, high-pass above all ("my favorite"). Five effects in a new Filters group, defined in 6.7. (1) Separate effects, as in Audacity, rather than one `filter` with a type: each is a button in the palette, which is how people find them, and they share one biquad module. (2) The cutoff is a distance (`size`: rows, px, % or bytes, the units delays use), not a frequency in Hz: an image has no sample rate, and "detail finer than 3 px" means something on a photo. A size scales with the work size and the preview like every other distance (6.2); one under 2 samples is clamped there, as no filter can reach past half the sample rate. That clamp, and Mono's color mixing (which depends on the size in samples modulo 3), mean a scaled preview cannot match the export, so the filters render exactly when the view pauses, as the phaser does. (3) Causal, one pass forward, as in Audacity: the phase shift smears to the right and down the rows, which is the databending look; a zero-phase (forward and back) option is left for later if wanted. (4) Our own code, from the published math (Robert Bristow-Johnson's Audio EQ Cookbook and Butterworth pole placement), not Audacity's: its filters use 32-bit floats and the platform's trigonometry, and results must be the same on every machine (D-25), so coefficients use `dsp.rs`. No dry/wet setting: the step's opacity is that (D-38). |
| D-72 | **Settings you can see, and a gradient that stays level** (Phil, 2026-11-25) | (1) A choice shows as a segmented control when its options fit a phone's card (up to five, short), so every option is in view; longer lists keep the dropdown. A size in `px` gets a log slider (1 to 2000 px) beside its text, as sizes in rows and percent already had. (2) A linear gradient mask's ends do different jobs: the dashed end turns and stretches it, settling on level and plumb within 4 degrees and on 45 within 2; the solid end (or its line) slides along the gradient without turning it. A tap on an end selects it (a ring) or lets it go, and a tap elsewhere lets both go; with both selected, dragging either moves the whole gradient. Shipped without hints in the UI on purpose: Phil tries it cold first, as a blind test. |
| D-73 | **A mask can be switched off** (Phil, 2026-11-25) | A switch (since D-75 the venetian-mask badge) turns the step's mask off and on without losing it, to compare the step with and without it. Stored as `off = true` in `[step.mask]`, read as no mask; the mask still opens for editing. Recipe version 9, written only when some mask is off (D-70's oldest-version rule), so other files still open on 0.2. |
| D-74 | **Hiding the overlay** (Phil, 2026-11-25; moved 2026-11-26) | While masking, an apple on the part being edited hides and shows the red overlay (rubylith), so the picture can be judged while working; handles and pins stay. Struck through while hidden. It replaces the Show mask switch at the foot of the Blend section (too far from the photo to find) and a first try, an eye in the top bar. Lucide has no apple-off, so `Slashed.svelte` strikes any Lucide icon through with an SVG mask. Not to be confused with D-73 and D-75, which change the render. |
| D-75 | **Icons in place of switches in the mask list** (Phil, 2026-11-26) | The mask section had run out of room, so controls became icons, reusing ones already in use. (1) The venetian mask is a switch: on a step's header it switches the step's whole mask (D-73), and the same badge beside Add to mask does too; beside each part it switches that part. Accent-filled when used, struck through and dim when off. A part switched off is left out of the mask as if absent (`off = true` on its `[[step.mask.op]]`, recipe version 9 like D-73), and stays editable. No recipe-wide switch, by design. (2) Removing a part is a red trash can (destructive; undo brings it back) and does not end editing. (3) The part being edited has a check that ends editing, as the top bar's Done does; the Mask row with its switch and Edit / Done button is gone (tapping a part starts editing). (4) Add / Subtract / Intersect is a full-width segmented control under the part's sliders, not a menu in the row; the list shows a mode other than Add in the part's second line. (5) The notes under Paint in / Paint out and under each part are gone: the options read plainly. The pen's placing hint, which goes with its Finish button, stays. |
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

1. **Decode** the input to a *raster*: 8-bit RGB, interleaved, rows top to bottom, no padding. EXIF orientation is applied, so the raster is upright. The color profile (ICC) is kept aside and written into the output (should).
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
- **ICC profile:** should be carried from input to output, so colors match the original in Photoshop.
- **Later:** a 16-bit raster, where 16-bit samples line up with 16-bit pixel channels.
- **Later:** a BMP-style layout (BGR, rows bottom to top), which gives the mirrored look of bending BMP files.

## 5. Sample encodings (Audacity "Import Raw Data")

The recipe sets `encoding` and `channels` once; any step may override them (D-23, D-29):

| `encoding` | Bytes per sample | Character (from the 2026-11-21 contact sheets) |
| --- | --- | --- |
| `u8` (default) | 1 | Clean ghosts; colors rotate through channels. Closest to Phil's examples. |
| `s8` | 1 | Like `u8`, but the clipping point sits mid-brightness, which inverts highlights. |
| `s16le`, `s16be` | 2 | Ghosts under heavy rainbow grain. Byte order changes the palette. |
| `s24le`, `s24be` | 3 | One sample per pixel: the whole pixel becomes one number. |
| `ulaw`, `alaw` | 1 | Posterized neon blocks with hard edges. |

- **Channels:** `1` (mono), `2` (stereo) or `3`. Samples alternate between the channels, and effects process each channel separately, as Audacity does. With `u8`, three channels are exactly R, G and B, so a filter that blends neighboring samples keeps color instead of greying it (D-33).
- **Clipping:** after the effect, samples are clamped to the encoding's range and converted back to bytes. For `ulaw`/`alaw`, the companding curve is applied on the way back.
- **Byte order** only exists for multi-byte samples. It decides which byte the effect treats as the loud part, which is why it mattered in Audacity.
- **Channel phase.** A delay of a whole number of pixels keeps colors; one or two extra bytes rotate red into green into blue. Delays are therefore given as a size plus a `phase` of 0, 1 or 2 bytes (section 6.2).

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

### 6.7 Filters (D-71, built 2026-11-25 on branch `filters`)

Audio filters run along the data, as everything in sections 6.3 and 6.4
does: across each row, then on into the next. Sample-level, so the
encoding and channels apply: Mono on 8-bit samples filters R, G and B as
one stream, mixing neighbors' channels into color fringes; RGB filters
each channel alone (D-36 for the meaning of channels). Placement
(`region`, `hits`, `len`) as in 6.2; each window starts with the filter at
rest. Biquads in direct form I, in f64; the state never carries between
windows or steps. A new group in the palette, **Filters**, after Audio.

| Effect | Audacity counterpart | Definition and settings | Look on a photo |
| --- | --- | --- | --- |
| `lowpass` | Low-Pass Filter | Removes detail finer than `size`. `size` (distance, default `16px`): the cutoff's period, so the cutoff is 1/size cycles per sample. `rolloff` (choice `6`, `12`, `24`, `36`, `48` dB per octave, default `12`): 6 is one pole; the rest are 1 to 4 Butterworth biquads in series. | Smears along the rows like a blur that wraps from row to row; steep rolloffs ring. In Mono, colors drain to grey: R, G and B alternate in the data, the finest detail there is, and the filter blends them. With RGB channels each color keeps its hue. |
| `highpass` | High-Pass Filter | Keeps only detail finer than `size`. Same `size` (default `8px`) and `rolloff` (default `12`). | In Mono, brightness flattens while color survives (the R, G, B alternation passes): washed, lit from the side, edges leaning right. With RGB channels, flat areas fall to the middle value (grey in u8) and edges stand out: the classic grey emboss. |
| `bandpass` | (Filter Curve EQ in Audacity) | Keeps one band of detail around `size` (default `6px`). `q` (0.3 to 20, default `1`): how narrow the band is. One cookbook band-pass biquad (constant 0 dB peak). | One texture picked out on grey, colors muted to a tint in Mono; high `q` rings into ripples. |
| `notch` | Notch Filter | Removes one band of detail around `size` (default `6px`). `q` (0.3 to 20, default `1`), as Audacity's. One cookbook notch biquad. | Subtle on most photos; small sizes in Mono shift colors; on patterns (fabric, screens, halftones) it reshapes the moire. |
| `basstreble` | Bass and Treble | Two shelving biquads (cookbook low and high shelf, slope 1) then a gain. `bass` (dB, -30 to 30, default `6`): below `low`; `treble` (dB, -30 to 30, default `0`): above `high`; `gain` (dB, -30 to 30, default `0`). `low` (distance, default `32px`) and `high` (distance, default `3px`): the two corners, as periods. Clips at the limits, as amplify does. | Bass: soft horizontal waves of brightness that blow out to white and black when pushed. Treble sharpens and fringes edges; in Mono it boosts the R, G, B alternation, so colors grow vivid and neon. |

Defaults are first guesses, to be tuned with contact sheets and judged by
Phil (D-7). Each effect gets a `VERSION`, golden hashes, and the usual
tests: dimensions kept, writes only inside its windows, a filter at rest
on silence stays silent, and a check of each response against its
formula at a few frequencies (low-pass passes a slow ramp and removes an
alternating +1, -1; high-pass the reverse; a notch removes a sine at its
size).

### 6.8 Later

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
glitchrx --recipe moody.toml --project photo.jpg       also writes photo-glitch.glrx
glitchrx edit.glrx out.png                               renders a project with its own recipe
```

| Option | Behaviour |
| --- | --- |
| `--seed N` | Master seed. The default is random and is always printed. |
| `--recipe FILE` | Apply a saved recipe instead of effect flags. |
| `--save-recipe[=FILE]` | Write the recipe used. Default `<output-stem>.recipe.toml`. |
| `--project[=FILE]` | Also write a project (8.1): the original without its location, the recipe and a preview. Default `<output-stem>.glrx`. With `--force` it may replace the project given as input (D-63). |
| `--format tiff\|png\|jpeg\|webp\|avif` | Output format when OUTPUT is not given (default `tiff`); otherwise taken from OUTPUT's extension. |
| `--crop X,Y,W,H` | Keep part of the photo before anything else, as shares of its width and height (D-50). |
| `--crop-result` | With `--crop`: frame the finished picture instead; the steps run on the whole photo (D-69). |
| `--work-size N`, `--scale-back` | Run the steps on a copy whose longer side is N px, and optionally enlarge the result back with hard pixel edges (D-48). |
| `--quality N`, `--lossless`, `--avif-speed N` | Encoder settings (D-44): quality 1-100 for JPEG (95), lossy WebP (90) and AVIF (80); lossless WebP; AVIF speed 1-10 (6). |
| `--encoding E`, `--channels 1\|2\|3` | Recipe-wide sample encoding and channel count (default `u8`, `1`). |
| `--force` | Allow overwriting an existing output. Never the input, except a project saved over itself with `--project`. |
| `--batch`, `--info`, `--list`, `-q`, `-v`, `--help` | As before. `--info` reports the format, size, orientation and color profile, and for a project also its original's name, steps and preview. |

Removed: `--reverse`, `--mode`, `--restart`, `--bake` (every output is already a normal image).

File safety, auto naming (`-glitch`, `-glitch-1`, ...) and exit codes are unchanged, except that exit code 3 no longer covers irreversible chains.

## 8. Recipe format

TOML, recipe `version = 4`. Versions 2 and 3 load unchanged and are written back as version 4 (version 4 added the canvas, D-48; a build that only knows version 3 refuses it rather than quietly ignore the work size). Version 1 recipes (JPEG byte mode) are refused with a message naming `archive/jpeg-bytes`.

```toml
format = "glitcharaxia-recipe"
version = 4
encoding = "u8"        # recipe-wide; a step may override
channels = 1
work_size = 2048       # optional: steps run on a copy this long (D-48)
scale_back = true      # optional: enlarge the result to full size, hard edges

crop_result = true     # optional: the crop frames the result instead (D-69)

[crop]                 # optional: the part kept, before anything (D-50)
x = 0.1                # shares of the photo's width and height
y = 0
w = 0.8
h = 1

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
off = true              # optional: switched off, kept but not used (version 9)

[[step.mask.op]]
shape = "brush"         # brush, linear, radial, rect, path or brightness
mode = "add"            # add, subtract or intersect
opacity = 0.8           # optional: how strongly this part counts
feather = 0.01          # optional: blur of its edges, share of the longer side
off = true              # optional: this part switched off, kept but not used (version 9)

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
from = [0.6, 0.5]       # center
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

### 8.1 Project files, `.glrx` (Phase 7, done, D-63)

A project file is what a `.psd` is to Photoshop: everything needed to open an edit again on any device and carry on. A recipe alone is not enough, because it replays on the original photo, and the original lives on one device.

**Container.** A zip file, as OpenDocument, EPUB, Krita (`.kra`) and OpenRaster (`.ora`) are. Anyone can unzip it with standard tools; nothing about it is secret or proprietary. Entries, in this order:

| Entry | Stored | What it is |
| --- | --- | --- |
| `mimetype` | first, uncompressed | One line, `application/vnd.glitcharaxia.project`, so a file manager (and `file`) can tell what it is from its first bytes, whatever the name. The OpenDocument convention. |
| `project.toml` | deflated | The manifest: `format = "glitcharaxia-project"`, `version = 1`, the original's entry name and file name, the glitchrx version that wrote it. |
| `recipe.toml` | deflated | The recipe, exactly as section 8, with `source` naming the original. |
| `original.jpg` (or `.png`, `.tif`) | stored | The original file, never re-encoded, so it decodes to the same raster and the recipe replays to the same bytes (D-25). Only its location data is removed (below). |
| `preview.jpg` | stored | The finished picture at preview size (1024 px on the longer side), as a JPEG: a thumbnail for the Open dialog, a picture to show while the exact render runs, and something to look at for anyone without the app. Not needed to replay. JPEG because every file manager, unzip tool and phone shows it, and the GUI's previews are JPEGs already; WebP would be a little smaller and less widely shown. |

**Rules.**
- Readers take only the entries above, by exact name, and ignore anything else; entry paths with `..`, absolute paths or links are refused (zip slip). Sizes are capped before anything is unpacked (zip bombs): the original by the server's upload limit, the TOML files at 1 MB, the preview at 8 MB.
- `version` in the manifest lets the format change later: a reader refuses a version it does not know, with a message, as recipes do. Version 1 is written until something needs a change.
- Writing is atomic: a temporary file next to the target, then a rename, so a crash never leaves half a project.
- **A project can be saved over** (Phil, 2026-11-24): it is a working file, like a `.psd`. The GUI asks first, the CLI needs `--force`. Photos are still never written (section 13).
- **Location is removed** from the original as it goes in, as a phone does when sharing a photo: the GPS block of its EXIF (JPEG, TIFF, PNG's `eXIf`) is emptied in place, XMP and IPTC location fields are blanked, and in a PNG the text chunks that hold whole metadata blocks compressed (ImageMagick's and exiftool's "Raw profile type ...", compressed XMP) are dropped, all without touching the image data, so it decodes to the same raster (a test checks this on every fixture). Other metadata (camera, date, orientation, color profile) stays. Exports never carried EXIF.

**In the GUI.** The Export sheet starts with **Picture | Project** (remembered on the device). Picture is what Export always did. Project explains itself ("the original photo, every step and a preview, in one .glrx file") and says the location is left out; it downloads, saves next to the original, or saves into a folder like a picture, and a photo opened from a project on the server offers **Save over NAME.glrx** (after a confirmation) and **Save as new**. The "Also save the recipe beside it" switch is for pictures only. Open has a second button, **Open .glrx**, beside **From Device** (D-70): its file picker has no type filter, since iOS greys out types it does not know, while the photo button keeps its filter, which is what makes iOS hand over a JPEG for a HEIC photo. The library lists projects with a Project tag and their preview as the thumbnail. A project opens as its original with every step in place (undo goes back to the steps before), shows its own name in the status bar, and brings no "continue from" offer. Dropping a `.glrx` on the window opens it too.

**Naming pictures from a project** (Phil, 2026-11-24). A picture exported from a project takes the project's own name, as an editor exports `poster.psd` to `poster.jpg`, so it is plain which project it came from: `IMG_3096-glitch-2.glrx` gives `IMG_3096-glitch-2.jpg`, then `IMG_3096-glitch-2-1.jpg` if that is taken in the folder; `-glitch` is not added again. This holds for Download, Save As (the suggestion), Save next to the project and the command line given a `.glrx`. A new project from a project still takes the photo's next `-glitch-N`, a new version, and never names the input itself.

**From the CLI.** A `.glrx` input replays its own recipe (`glitchrx edit.glrx out.png`; `--recipe` and effect flags are refused with it, the recipe-wide options apply); `--project[=FILE]` writes one beside the picture; `--info` lists what is inside (section 7).

**In the desktop app.** The native Open dialog offers projects beside photos. On Linux, `scripts/install-desktop.sh` registers the file type (a shared-mime-info entry that knows a project by its name and by its first bytes, and `MimeType=` in the launcher entry), so a double click starts the app with the file, which the page opens first; `tauri.conf.json` declares the same for a bundled `.deb`. Each double click starts its own window for now. macOS delivers the file as an event instead, which waits for the macOS build (`docs/macos.md`).

**The server** keeps each opened photo's file bytes beside its decoded image (today it keeps only the decoded image), since a project must hold the original byte for byte.

### 8.2 Pull (Phase 8, D-64)

A step whose effect is not a formula but strokes drawn on the photo: drag to pull pixels along with the finger, as Photoshop's Liquify does with its forward warp. Because it is a step, it switches on and off, has opacity and a mask, moves in the stack and goes into presets like any other.

```toml
[[step]]
effect = "pull"
effect_version = 1
sampling = "pixels"     # pixels (hard, nearest) or smooth (bilinear)

[[step.pull]]           # one per stroke or pin, applied in order (D-68)
size = 0.12             # brush diameter, share of the photo's longer side
strength = 0.6          # 0 to 1: how far the pixels follow the finger
points = [0.40, 0.52, 0.43, 0.53, 0.47, 0.55]   # x, y as shares of the photo
brush = 1               # optional: the brush it was drawn with (GUI only)

[[step.pull]]           # a pin: from and to instead of points (D-67)
size = 0.2
from = [0.5, 0.5]
to = [0.56, 0.47]
```

Only pulling for now (Phil, 2026-11-24). Other tools (twirl, pinch, bloat) can come later as a `tool` key on a stroke, absent meaning pull, so today's recipes keep their meaning.

**Where strokes live.** In shares of the whole photo, like masks (D-52), so a crop or a work size change leaves them on the same features of the photo.

**How it renders** (deterministic, D-25). A displacement map at a fixed size (1024 points on the canvas's longer side, whatever the render's size) says, for every output point, where in the input to read. Each stroke updates the map in small steps (one every quarter of the brush radius along its path), each step reading the map as it was, so strokes build on one another as they do in Liquify rather than tearing. The brush falls off as `(1 - t^2)^2` from its center to its edge: basic operations only. Then every pixel is read from the input once, through the map: nearest pixel (`pixels`, hard-edged, the default, to match the raw-pixel look) or bilinear (`smooth`, closer to Liquify). The map has the same size for a preview and an export, so the preview matches the export more closely than most effects do. No seed: nothing is random. Placement (region, bands) does not apply; the strokes are the placement, so a pull has no `region`, `hits` or `len` parameters (giving one is an error), and strokes on any other step are refused. A stroke without `strength` takes 0.6. Added from the command line (`--pull`), the step has no strokes and changes nothing. The raster keeps its size.

**In the GUI.** A **pull mode**, entered like mask editing (and at once when the step is added): the top bar becomes an edit bar (what is being pulled, undo, redo, Done), and a tool bar under the photo has size and strength. One finger draws with the tool, two fingers pinch and pan, `[` and `]` size the brush on a desktop. The preview follows the drag live, rendering the pull step on the cached picture before it. Each stroke is one undo step; the step card says how many strokes there are and has Clear. To take a pull back from part of the photo, paint the step's mask, as with any step: there is no separate restore brush.

**Recipe version 5** (version 4 readers refuse it, as version 3 readers refuse 4; versions 2 to 4 load unchanged).

**Pins** (D-67, Phil, 2026-11-24). A second tool in the same pull step and card, for more control than freehand: **Draw | Pins**. In Pins, a grid lies over the photo (cells across its longer side, 2 to 32, default 8; square cells, centered), shown as dots, lines or not at all (per device, never in the picture). Touching near a grid point picks it up as a new pin; dragging puts it down elsewhere. Touching near a pin (where it was put, or its home) picks that pin up again, to pull it further or change its direction; dropping it back home removes it. Each move is one undo; while one is dragged its reach shows as a dashed circle. Every pin has its own size (Phil, 2026-11-24): the pin last touched is selected (green and larger, its reach drawn; the others plain and faded, like unselected stroke markers), and while one is, Size ("This pin", or `[` and `]`) resizes it alone, live, one undo per change; a tap on an empty spot selects none, and Size then sets the size of new pins. Undo and leaving pull mode clear the selection. The grid only picks points: pins keep plain positions, so a changed grid moves nothing. In the recipe a pin is a `[[step.pull]]` entry with `size`, `from = [x, y]` and `to = [x, y]` (shares of the photo; version 6 kept them in `[[step.pin]]`, after all strokes, D-68). It renders exactly as a straight stroke from `from` to `to` at full strength, in its place among the strokes, so the point lands where it was put and the picture around it stretches smoothly rather than folding; a move much longer than the pin's reach draws a thin streak behind it. **Recipe version 6** (version 5 readers refuse it; versions 2 to 5 load unchanged; output without pins is unchanged).

**One list, brushes, the card** (D-68, Phil, 2026-11-24). Strokes and pins run in the order made: a stroke drawn after a pin goes over it, and a pin dragged again moves to the end of the list (a tap only selects it). Each stroke belongs to a **brush**, a number kept with it (`brush = n`, which rendering ignores): while a brush is selected, new strokes join it with its size and strength; with none selected, the next stroke starts a new brush with the tool's settings and selects it, so the strokes after it join it until New. Strokes from before brushes count as brushes of one, and get a number when another joins them. A marker sits where each stroke ends; tapping it selects its brush (its markers turn green and show their reach), and a tap elsewhere (a press that never moves 8 px) selects none. Size and Strength then change every stroke of the selected brush, live, one undo per change. In Pins the pin last touched is selected in the same way. The card keeps its shape: Draw or Pins (picking one starts pulling), a line saying what the sliders change, New (labelled, since the add-effect button is a plus too; dimmed while nothing is selected), then Size, Strength and a Markers switch, or Size, Grid and Show. Only while something is selected, a red "Delete this brush" (all its strokes) or "Delete this pin" follows, under the settings so nothing above it moves; the Delete key does the same. There is no Delete all. With Markers off (per device) no markers show, and a tap only selects none, since nothing hidden can be picked. After Done the card keeps the tool and its settings, and says to tap Draw or Pins. The one thing that shows the mode is the tool's segment: filled with the accent while pulling (a drag on the photo pulls), the plain selected look after Done (Phil, 2026-11-24). Markers, pins and the grid show only while pulling: outside it the photo is for looking at the result, and their presence is itself a sign of the mode. Undo keeps a selected brush (its number is stable) and clears a selected pin. **Recipe version 7**: version 6 recipes load with their pins after their strokes, and are written back as version 7.

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

- **Step card, Blend section:** the step's opacity; without a mask, six buttons (Brush, Linear, Radial, Rectangle, Pen, Brightness) start one with that component, and Paste when something is copied. With a mask (D-75): Paint in / Paint out; the list of components, each with its venetian-mask switch and a trash can (no mask-wide delete: removing the last component removes the mask, and the step shows everywhere again), and its mode, opacity and feather on a second line when not the defaults; "Add to mask" (the same six), the whole mask's venetian-mask switch, Copy, Cut and Paste. Tapping a component starts editing, makes it the only selected one and the active one (a green outline) and opens its controls: the apple (overlay, D-74) and a check (end editing) in its row, opacity and feather side by side, then a brush's Paint / Erase, size and hardness, or a brightness range's two-handle range and softness, then Add / Subtract / Intersect. A long press (shift, cmd or ctrl click) adds or removes a component from the selection, also outlined; the sliders then set every selected one.
- **Editing on the preview:** the active component decides what a gesture does. A brush paints or erases a stroke. A radial shows a green ring with a center dot (drag to move it; dragging inside does too) and an edge dot (drag to resize; so does the ring). A linear shows a solid line where it is full and a dashed one where it has faded out, with a dot on each (drag either, or the middle to move both). Dragging anywhere else draws the active shape again. Rectangles and pen paths are edited as D-43 (3) describes. Shapes that are not active show a pin (a white dot) that makes them active when tapped; pins take precedence over the active shape's inside, but not over its dots. A new radial starts centered, a fifth of the longer side in radius; a new linear fades from a quarter of the way down to three quarters. The preview follows a drag live; each stroke, finished drag and added component is one undo step. The mask overlay (red, like a rubylith) can be shown or hidden; a stroke stays drawn until a render that includes it arrives. While editing, the top bar becomes an edit bar (what is being masked, undo, redo, Done), so nothing floats over the photo; Open, Presets and Export return after Done.
- **Desktop keys while editing:** B and E paint and erase with a brush (the active one, else the last, else a new one), [ and ] brush size, Enter or Esc to finish. A mouse shows the brush outline, and a grab or move cursor over handles.
- **Later:** blend modes.

### 9.4 Presets (Phase 3, D-39)

A Presets button in the top bar: save the recipe under a name, use a preset (replaces the steps, undoable), add a preset's steps to the current recipe, delete. Presets with an effect version this build no longer has are listed with the reason and cannot be used. What opening another photo does with the steps (Ask, Keep steps, Start fresh; D-41) moved to Settings in 4c (D-45).

### 9.5 Step card

- **Header:** number, effect icon and name, tags (encoding override, the venetian-mask switch when masked (D-75), opacity below 100%), and the on/off switch. Effect names are lower case, as on the command line. When an effect is in the stack more than once, each step is numbered (`echo-0`, `echo-1`, ...); a step keeps its number when moved, a new one takes the next after the highest in use, and duplicates get a new number (Phil, 2026-11-23).
- **Edit mode** (D-47, D-66): holding a step shows every step on one line, with a circle to select and a grip to drag; holding a row (or Done) goes back. Its bar, icons only: select all, the count, re-roll (the selection, or every step), Duplicate and Remove.
- **Body:** the effect's settings, headed by Shake (moved off the header, where it was easy to bump; Phil, 2026-11-23); Samples (encoding, channels); Placement, ending with the seed and the dice that re-rolls it (the seed only decides where hits land, so it belongs there); Blend (9.3); then the step's own actions in a row of their own, as icons (hold one for its name, D-66): Up, Down, Duplicate, Remove.

### 9.6 Zoom (Phase 4a, D-42)

The same gestures with a finger or the mouse:

- **Press and hold** the photo: the original, until released (as in Instagram). The `\` key too.
- **Double tap**, the **magnifier** (in the photo toolbox, D-62) or **Z**: fit to 100% and back. Zoomed, the magnifier shows the zoom.
- **A single tap:** full view and back (D-59).
- **Pinch** with two fingers, ctrl + wheel, or a trackpad pinch: zoom about the fingers or the pointer.
- **Zoomed in:** one finger, the middle mouse button or the wheel pans. Each edge of the picture can be dragged a little way into the stage, a quarter of the stage's side up to 160 px, so the photo's border can be worked on clear of the screen's edge and, on a phone, of the step panel, whose rounded top the stage stops above (Phil, 2026-11-25). The fit stays centered; the overpan grows in up to 1.5x.
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

### Phase 5: craft (planned with Phil, 2026-11-23, from his first notes on Phase 4)

- **5a Edit mode (done, D-47).** Select, remove, duplicate and drag steps, one line each.
- **5b Work size (done, D-48).** A segmented control under the photo (Full, 4K, 2K, 1K): the recipe runs at that size for the preview and the export, so small sizes are instant and exact, and the glitch texture is coarser. Export at the work size, or scaled back up with hard pixel edges. Saved in the recipe, so the CLI replays it. Replaces Phil's idea of preview-resolution buttons (the preview already settles to exact, D-40); it was parked as the recipe work size.
- **5c Adjustments (done, D-49).** A fourth effect group, as ordinary steps with masks and opacity: curves (all channels, or R, G, B), levels (black, white, gamma), hue / saturation / lightness (whole image), color balance (shadows, midtones, highlights). Deterministic like every effect (D-25).
- **5d Crop (done, D-50).** Not a step (steps keep the size; masks, placement and seeds depend on it): a crop at the top of the recipe, applied before every step. Free, or original, 1:1, 4:5, 16:9.
- **5e Picking up old edits (done, D-51).** Load recipe can browse the server's library; opening an original offers to continue from a recipe saved beside it.

### Phase 6: refinements (planned with Phil, 2026-11-23, from his notes on Phase 5)

- **6a Masks stay on the photo through a crop (done, D-52).** Masks are shares of the whole photo; the core moves them onto the cropped canvas to render.
- **6b Curves editor (done, D-53).** A visual grid (quarters, and fainter eighths; nothing snaps), and a histogram of the picture as it reaches the step.
- **6c Color ranges for hsl (done, D-54).** One range per step (all, reds, yellows, greens, cyans, blues, magentas), with a soft edge between colors.

Also from Phil's notes: step cards keep their collapsed state through Edit mode (fixed on `phase-5`).

Then, from his notes on Phase 6: sliders mark their default and reset with a double tap (D-55), every effect starts on the whole picture with a Bands switch (D-56), and hsl's color ranges became color dots, several at once (D-57).

And from his notes of 2026-11-24: adjustments start neutral (D-58); whole-number sliders spanning up to 360 (tone values, degrees) are linear, since a log scale made levels' output black jump; the curves grid uses the dim text color so it shows in dark mode; a parameter's help line shows in full after a tap; full view (D-59), entered with a tap on the photo, with the grabber left to pull the panel back; the photo toolbox (D-62); folders listed as a plain list with `..` for the folder above; the levels editor (D-60); and remembered pictures for switching steps (D-61).

### Phase 7: project files (done, D-63; branch `phase-7-projects`)

Section 8.1. Each part ends green and usable:

- **7a Format in the core (done).** Read and write `.glrx` (the `zip` crate, deflate only, no other features), the manifest, the safety rules, location removal, tests: round trip, the same raster after location removal, a refused unknown version, refused zip slip and oversized entries, a fixture `.glrx` committed to `tests/fixtures`.
- **7b CLI (done).** Render a `.glrx`, write one with `--project`, `--info`. Outputs are never overwritten without `--force`.
- **7c Server and GUI (done).** The server keeps opened photos' file bytes; new endpoints to make a project from the open photo (download, or save to a folder) and to open one (upload, or from the library); the export sheet's two choices; Open accepts `.glrx`, with thumbnails, and a project can be saved over after a confirmation in the library. To check on Phil's iPhone: that Safari's file picker offers `.glrx` files (it greys out types it does not recognise unless the page asks for them) and that a downloaded `.glrx` lands in Files.
- **7d Desktop (done for Linux).** The file type registered for Linux (installer and bundle) and a double click opens it; macOS with `docs/macos.md`.

Phil's answers (2026-11-24): the extension is `.glrx`; a project can be saved over; location is removed; the thumbnail is a JPEG.

Effort: about Phase 5's size; most of it is 7c.

### Phase 8: pull, and New folder (planned with Phil, 2026-11-24; branch `phase-8-pull`)

Section 8.2. Each part ends green and usable:

- **8a The effect in the core (done).** The displacement map, both samplings, the strokes in the recipe (version 5), mapping onto a cropped or shrunk canvas, golden hashes, a test that a preview and an export agree closely, a contact sheet at a few sizes and strengths.
- **8b Pull mode in the GUI** (done; first cut, waiting for Phil to try it). The edit bar and tool bar, pulling with a live preview, undo per stroke, the step card's count and Clear, `[` and `]` on a desktop. The brush settings (size, strength) are per device, like the mask brush, and sit in the pull step's card, under the photo on a phone. While a stroke is drawn it is in the recipe uncommitted (sent at most once a frame), so the preview follows the finger; a tap that does not move adds nothing.
- **8f One list, brushes, a steady card (done, D-68).** From Phil's use of pins: strokes and pins in one list in the order made (recipe version 7, version 6 loading as it rendered); brushes, with a marker per stroke, select, resize together, New and Delete; the card the same in and out of pull mode, with Done in the top bar only. Resizing a brush rebuilds the map from its first stroke on: about 0.4 s a frame on a 12 MP canvas when that is 40 strokes back (drawing and moving pins stay near 17 to 28 ms).
- **8e Pins (done, D-67).** A Draw or Pins switch in the pull card, always in view (picking one starts pulling with it; Phil could not find Pins while it showed only in pull mode); the grid (spacing; dots, lines or off); placing, moving and removing pins with a live preview, one undo each; `[[step.pin]]` in the recipe (version 6).
- **8c Speed (done).** Measured on a 12 MP canvas (`cargo test --release -p glitchrx-core speed -- --ignored --nocapture`): building the map is about 8.5 ms a stroke and was redone, with every earlier stroke, on each frame of a drag, so a live frame over 50 strokes took about 430 ms. Maps are now kept at checkpoints (where a build first differs from the last, which during a drag is the mark being changed, and whole, which is where the next stroke starts; at most 6, about 13 MB each), and a build starts from the longest one its strokes begin with: the same map, byte for byte (tested). Over 50 strokes a live frame now takes about 17 ms in the core and 26 ms through the server (37 ms with an echo after the pull); moving the first of 20 pins about 28 ms. A step changed after the pull never rebuilt it (the per-step render cache), and the exact render after a drag reuses the whole map.
- **Polish from Phil's first use (done, D-66).** Hold a step for Edit mode; icon-only bars with held names; the dice into Edit mode and Clear gone; one Name at the top of Export; downloads as links, named correctly in Firefox on iOS.
- **Crop the result (done, D-69).** Photo | Result in the crop editor, `crop_result` in the recipe (version 8), `--crop-result`, framed previews, zoom and mask view; tests in the core, the server and the CLI.
- **8d New folder (done, D-65).** `POST /api/folders` making one folder inside a home-relative path, with the name checks and tests for escapes (`..`, slashes, symlinks out of home) and a taken name; the button in the folder browser (Save As and Settings); the new folder opens at once.

Phil's answers (2026-11-24): hard `pixels` sampling by default; only the pull tool for now.

Effort: a little larger than Phase 7; the GUI mode is most of it, and it reuses the mask painter's gestures. 8d is small and independent, so it can go first.

### Parked

Ideas not yet scheduled, and the longer-term plans (RAW import, the macOS build, public hosting), live in `docs/roadmap.md`.

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

- **Inputs are never written**, even with `--force`; outputs are never overwritten without `--force`. The one exception is a project file (`.glrx`), a working file that may be saved over after a confirmation or with `--force` (D-63); the photo inside it is never changed except for removing its location.
- **Dimensions are preserved:** effects never change the raster's length, and write only inside their windows. A mask or opacity only softens a step's own change.
- **Determinism:** same input, recipe and seed give the same output bytes on every supported platform (D-25).
- **No network calls** at runtime.

Dropped: header-safe (no compressed data is edited) and exact reversibility (D-24).

## 14. Open items

1. Default encoding: `u8`, unless the second round of contact sheets says otherwise.
2. Preview fidelity threshold (9.2), to be set from measurements.
3. Resolved: TIFF is the default output (D-28); encoding is recipe-wide with per-step override (D-29); close-not-identical previews accepted (D-26).
