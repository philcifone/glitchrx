# Vocabulary

Shared terms for glitcharaxia, so we can name things precisely. Grouped by topic, alphabetical within each group. It grows as the project does.

## Pixel engine (SPEC v0.3)

- **A-law / U-law.** Two telephone audio encodings that squeeze a 16-bit sample into one byte along a curve. Reading pixel bytes this way and back gives posterized, neon colour blocks.
- **Channel phase.** How many extra bytes (0, 1 or 2) a delay adds beyond whole pixels. Phase 0 keeps a ghost's colours; 1 or 2 rotates red into green into blue.
- **Channels (mono / stereo / RGB).** With 2 or 3 channels, samples alternate between them, and effects treat each channel separately, as Audacity does. With `u8` and 3 channels, the channels are exactly red, green and blue ("RGB" in the GUI).
- **Clipping.** Clamping samples to the encoding's range after an effect. It loses information, which is why effects can no longer be reversed exactly, and it gives the bright, saturated look.
- **Databending.** Opening a file as the wrong kind of data (here: an image as audio), editing it with that kind of tool, and opening it as an image again.
- **Encoding (sample encoding).** How bytes are read as audio samples: `u8`, `s8`, `s16le`, `s16be`, `s24le`, `s24be`, `ulaw`, `alaw`. The choice from Audacity's Import Raw Data dialog.
- **Effect palette.** The GUI's grid of effect buttons; one tap adds a step. Each effect has an icon picked for feel rather than literally (a mountain for echo, a drum for stutter). Grouped into **audio** effects (behave like their Audacity namesakes: echo, delay, reverb, amplify, stutter), **data** effects (treat the bytes as data: invert, bitcrush, backwards, shift, shuffle) and **pixel** effects (whole pixels in two dimensions: pixelsort). The server reports each effect's group. On phones it opens from a floating + button as an **add sheet**.
- **Sticky hover.** iOS Safari keeps a tapped element in the CSS `:hover` state until the next tap elsewhere. The GUI applies hover styles only under `@media (hover: hover)` so tapped buttons never stay highlighted.
- **Exact preview, approximate preview.** An exact preview is bent at full resolution and then shrunk, so it is exactly the export. While you change things the GUI shows an approximate (scaled) preview, then settles to an exact one 600 ms after the last change; the status bar says which is showing. Pixel-level texture is why they differ: a 1 px pattern is a bigger share of a small picture. (Effects flagged `exact_preview`, the phaser and pixelsort, were the first to settle this way.)
- **Filter vs edit.** The two kinds of effect. **Filters** (echo, delay, reverb, amplify, invert, bitcrush, phaser, wahwah, distort) turn bytes into samples, process them, and clip them back. **Edits** (backwards, shift, shuffle, stutter) only move whole samples, like cut and paste. `speed` resamples, so it is in between.
- **Fit / resize.** `fit` picks the largest size inside a box that keeps the aspect ratio; `resize` shrinks to an exact size. They differ because fitting twice can round differently (480x320 into 200x200 is 200x133, but into 200x133 is 199x133).
- **Frame.** One sample for every channel: `encoding width x channels` bytes. Windows start and end on whole frames, so samples line up with the start of the raster. For pixel effects a frame is one pixel (3 bytes).
- **Freeverb.** A classic open reverb design (damped feedback combs plus allpass filters) that Audacity's Reverb is based on.
- **LFO.** Low-frequency oscillator: a slow wave that sweeps a filter, as in phaser and wahwah. Here its period is a picture size (`4rows`), not Hz, and it updates the filter every 16 samples.
- **Raster.** The decoded image as raw bytes: R, G, B for each pixel, row after row, top to bottom. What an uncompressed TIFF stores, and what every effect now edits.
- **Full-resolution mapping (Units).** How a preview stays faithful: every size and position is worked out at full resolution first, then mapped into the smaller raster, with whole rows scaled by height, the rest of a row by width, and the 0-2 byte channel phase kept.
- **Preview budget / too heavy.** A live preview that runs over 3 s (an exact one, 30 s) stops, and the GUI says it is too heavy to preview live instead of showing an error. Export has no limit.
- **Region band.** The shading drawn on the preview while a region's handles are dragged: the band of rows where hits may land.
- **Scaled preview.** Running the steps on a screen-sized copy of the raster, with sizes mapped to match, so previews are fast. Close to the export, not identical: long feedback tails drift, because a delay that wraps past a row moves its ghost down by one row, and one preview row stands for several full rows.
- **Units (rows, px, %).** Sizes in a recipe: `rows` (a fractional row is that share of the width), `px`, `%` of the image (top to bottom) or plain bytes.

## JPEG anatomy

Background, mostly from the archived JPEG byte engine (`archive/jpeg-bytes`). JPEG is still read and written as a file format.

- **EXIF orientation.** A tag saying how a camera photo must be turned to display upright (values 1-8). The decoder applies it, so effects see upright rows.
- **ICC profile.** The colour profile embedded in a photo. It is carried from input to output so colours match in Photoshop.

- **Byte stuffing.** Inside scan data, a real `0xFF` byte is always written as `FF 00`, so it cannot be mistaken for a marker. "De-stuffing" removes the `00`s. "Re-stuffing" puts them back.
- **DC drift.** Each block's average brightness (its DC value) is stored as a difference from the previous block. Corrupt one, and every later block in that component is shifted. This causes the color and brightness bands that run to the end of the image.
- **EOI.** End Of Image marker, `FF D9`. A decoder stops reading here.
- **Entropy-coded data / scan data.** The compressed image itself, made of Huffman-coded bits that follow an SOS marker. This is the only part the effects touch.
- **Entropy run.** A stretch of scan data between two markers (for example between an SOS and an RST). The payload is all runs, de-stuffed and joined together.
- **Fill bytes.** Extra `FF` bytes allowed just before a marker. They are kept as structure.
- **Header / structure.** Everything that is not scan data: markers, tables, metadata, thumbnails. The tool copies it byte for byte (the "header-safe" rule).
- **Huffman desync.** Huffman codes have variable lengths. After one corrupted bit, the decoder reads every following code from the wrong position until something resynchronizes it. This is the main source of smeared, shifted glitches.
- **Marker.** A two-byte signal starting with `FF` that labels the next part of the file (`FF D8` start, `FF DA` scan, `FF D9` end, and so on).
- **MCU.** Minimum Coded Unit, the 8x8 or 16x16 pixel tile that JPEG encodes one at a time, left to right and top to bottom. An **MCU row** is one horizontal strip of them.
- **Progressive JPEG.** A JPEG stored as several scans that refine the image in passes. A **baseline** JPEG has a single scan.
- **Restart interval.** How often restart markers appear, counted in MCUs or MCU rows. The `DRI` header segment stores it. In glitchrx it sets the **band height**: how far one glitch can spread down the image before the decoder recovers.
- **Lossless transform.** Rewriting a JPEG by moving its DCT coefficients into a new file, without decoding to pixels and re-compressing. Adding restart markers this way changes the bytes but not the image.
- **RST marker.** Restart marker, `FF D0` to `FF D7`. It appears every N MCUs when the encoder enables a restart interval, and lets a decoder recover from damage. That makes glitches more contained.
- **SOS.** Start Of Scan marker, `FF DA`. The scan data follows its short header.
- **Subsampling.** Storing color (chroma) at lower resolution than brightness. Written as 4:2:0 (half width and half height), 4:2:2 (half width) or 4:4:4 (full).
- **Thumbnail (EXIF).** A small, complete JPEG embedded in the metadata of camera and phone photos. Left untouched.
- **Trailing data.** Bytes after the main image's EOI, such as phone gain maps (Ultra HDR) or extra MPF pictures. Left untouched.

## Project concepts

- **Backwards.** The effect that reverses the order of samples inside each window, like playing audio in reverse (Audacity's Reverse). The name dates from when `--reverse` meant undo.
- **Chain.** Several effects applied in order. On the CLI, the order of the effect flags is the chain order.
- **Contact sheet.** A grid image showing one effect at a range of parameter values, side by side. Used to tune defaults.
- **Effect.** One operation, such as echo or shuffle. Most follow one of Audacity's and work on the raster's raw bytes as samples. **Pixel effects** (pixelsort) work on whole pixels as colours, in rows and columns, instead.
- **Adjustment.** A photo tool rather than a glitch (levels, curves, hsl, balance), in the effect group `adjust`. Adjustments are ordinary steps, so masks, opacity and placement apply.
- **Levels.** Black point (input values at or below it become black), white point, and **gamma**, which bends the midtones lighter (above 1) or darker.
- **Histogram.** How many pixels have each value, 0 to 255, drawn as a mountain range behind a curve: the picture as it reaches the step.
- **Curve.** A tone curve: points `IN:OUT` mapping input values to output values, joined by a **monotone cubic**, a smooth curve that never overshoots between its points (Fritsch-Carlson).
- **Colour range.** One family of colours on the colour wheel (reds, yellows, greens, cyans, blues, magentas) that an hsl step can be limited to, alone or several together (tapped on as colour dots); neighbouring ranges fade into each other, and greys belong to none.
- **Colour balance.** Tinting one tone range (shadows, midtones or highlights) towards red or cyan, green or magenta, blue or yellow; with luminosity kept, only the colour changes.
- **Pixel sorting.** Reordering pixels along a column or row by a key (brightness, hue, saturation). The classic version (Kim Asendorf's ASDF sort) only sorts **runs**: stretches of pixels whose brightness lies inside a band (`low` to `high`). Pixels outside the band stay put and break the runs, which is why shadows keep the picture recognisable while bright areas melt into streaks.
- **Luma (Rec. 709).** Perceived brightness from R, G and B: 0.2126 R + 0.7152 G + 0.0722 B. Pixelsort uses it scaled to integers, so its comparisons are exact everywhere.
- **Golden test.** A test that checks the output's hash matches a stored value. It proves that the same input, recipe and seed always give the same bytes.
- **Hit.** One window where an effect is applied. `hits=3` means three windows at seeded random places inside the region.
- **Mask.** Where a step's result shows: 1 shows it, 0 keeps the picture as the step received it, in between blends. Like a Photoshop layer mask, but stored in the recipe as **mask operations** (brushes, gradients, brightness ranges), not as an image, so it is exact at any size. Each step has its own.
- **Mask operation (op), mask component.** One part of a mask: a **brush** (its strokes, each with points, size and hardness; an **erase** stroke takes back that brush's own paint and nothing else), a **linear** gradient (full at `from`, faded out at `to`), a **radial** circle (centre `from`, edge through `to`; hard-edged, so the mask's feather is what fades it), a **rect** (rectangle: centre, size, angle), a **path** (a closed pen outline; see Pen path), or a **brightness** range (pixels whose brightness lies between `low` and `high`, with a `soft` edge). Each combines with the mask so far by **add** (toward 1), **subtract** (toward 0; a subtracting brush cuts into everything before it) or **intersect** (keep only where both are; a limit). The GUI calls them components and keeps intersections last.
- **Active component.** The one mask component being edited in the GUI, outlined in green in the list. Its handles show on the photo, and the photo's gestures act on it.
- **Selected components.** The active component plus any added by a long press (shift, cmd or ctrl click on a desktop), all outlined. Opacity and feather changes, Copy and Cut act on every selected one.
- **Mask clipboard.** Copy or Cut takes mask components; Paste adds copies to any step's mask (positions are fractions of the picture, so they land in the same place). Cut then Paste moves them between steps.
- **New photo choice.** What opening another photo does with the steps already in the stack: ask, keep them, or start fresh (undoable). Set per device, in Presets.
- **Pen path, corner, smooth point.** A mask component drawn with the pen tool: points joined into a closed outline. A corner is just a position; a smooth point also has a handle that bends the side after it, and the mirror of that handle bends the side before, so the outline runs through it without a kink. While drawing (placing), taps add points and tapping the first closes it.
- **Work size, canvas.** The canvas is the picture the steps run on: the photo, cropped (see Crop), then shrunk so its longer side is the work size (or kept at full size). Pixel-level effects look coarser on a smaller canvas. **Scale back** enlarges the result to the photo's size by repeating pixels (**nearest neighbour**), so edges stay hard.
- **Source (of a recipe).** The file name of the photo a recipe was made on, recorded when it is saved, so opening that photo again can offer to **continue from** it. Information only.
- **Crop.** The part of the photo kept, before the steps run; stored as shares of the photo (`[crop]` in a recipe). Not a step, since steps never change the size. Masks are shares of the whole photo, so a crop leaves them where they are; the core's **frame** moves them onto the cropped canvas to render.
- **Edit mode.** The step list as one line per step, for selecting several and dragging them into order. Turned on and off by **holding** a step: a press that stays put for about half a second (moving cancels it).
- **Download link.** A finished export the server holds for a few minutes at an unguessable address, so the browser downloads it as an ordinary file with its real name.
- **Step name.** The effect's name, with a number when the stack holds more than one of it (`echo-0`, `echo-1`). The number stays with the step when it moves.
- **Preset group.** Steps that came in from a preset, kept together in the stack under its name. It remembers the preset's steps as added (its **base**), so it can show that it was **modified**, **revert** to them, or **update** the preset with the edits.
- **Library folder, export folder.** Folders on the server, chosen in the GUI's Settings sheet and kept in `~/.config/glitchrx/config.toml`. The library is what Open browses (`--root` overrides it); the export folder is where Save As starts. Both must lie inside the server user's home folder.
- **Save As.** Exporting into a chosen folder under a chosen name. In the browser it writes on the server and never overwrites; the desktop app uses the system's Save dialog.
- **Lossy, lossless.** A lossless file decodes to exactly the pixels written (TIFF, PNG, lossless WebP). A lossy one (JPEG, WebP, AVIF) throws away detail the eye is said not to miss, which for glitch art includes single-pixel noise and hard stripes. **Quality** (1-100) sets how much is kept.
- **Exact area coverage.** How rectangles and pen paths are filled: each pixel gets the share of its area inside the outline, computed by adding up signed areas along each row (as font rasterizers do), so edges are anti-aliased with no extra samples.
- **Handle, pin.** A handle is a dot on the active shape (radial, linear, rectangle, pen point) that moves, resizes or turns it when dragged. A pin is the white dot that marks a shape that is not active; tapping it makes that one active. Both borrowed from Lightroom.
- **Paint in / paint out.** The mask's `invert` switch in the GUI. Paint in: the glitch shows only where you paint. Paint out: everywhere except where you paint.
- **Hardness vs feather.** Hardness is the edge of one brush stroke, or of a radial in a recipe file (1 crisp, 0 soft all the way in). Feather blurs one mask component's edges before it joins the mask, as a share of the picture's longer side; each component has its own, as it has its own opacity.
- **Mask overlay (rubylith).** The translucent red shown over the preview while painting, where the step shows. Named after the red masking film of print shops. The server renders it, so it is exactly the mask that will be used.
- **Opacity.** How much of a step's result is blended over the picture it received, 0 to 1. A step key like `encoding`, so `--echo opacity=0.5` works on the CLI. With a mask, the two multiply.
- **Strength slider (deferred).** A single 0-100 control per effect that would drive several of its parameters at once. Waiting for the tuning pass; opacity covers the simple case.
- **Preset.** A recipe saved under a name on the server (`~/.config/glitchrx/presets/NAME.toml`), shared by every device that uses the server. Your own looks, not built-in ones. Use one to replace the steps, or add its steps to the current recipe.
- **Re-roll vs shake.** Re-roll (dice) gives a step a new seed, which moves where its hits land. Shake (shuffle arrows) gives it new random settings for its own parameters, leaving placement and encoding alone. Both write into the recipe, so results stay replayable.
- **Recipe.** A TOML file (version 3) that lists the effects, parameters, seeds, opacities and masks, plus the recipe-wide encoding and channels. Replaying it on the same input gives the same output. Version 2 files (no opacity or masks) still load.
- **Region.** The part of the raster where hits may land, for example `40%:60%`. In the raster, percent of the data is percent of the image height, so that really is the middle band.
- **Bands.** A step scattered in seeded windows (`hits` above 0) instead of covering its whole region; a switch in Placement. Every effect starts without them.
- **Full view.** The photo on the whole screen, with the bars hidden and the step panel lowered to its grabber; a tap on the photo or a swipe down on the grabber.
- **Project file (`.glrx`).** A zip holding the original photo (without its location), the recipe and a preview, so an edit opens again on any device; like a `.psd` (SPEC 8.1, D-63).
- **Pull.** A step made of strokes drawn on the photo, each dragging pixels along with the finger (SPEC 8.2, D-64). Its **displacement map** says, for every point of the output, where in the input to read. **Pull mode** is drawing those strokes on the preview: the top bar becomes an edit bar, and the step's card holds the brush's size and strength.
- **Photo toolbox.** The card at the top of the step panel with crop, zoom and the work size (D-62).
- **Remembered picture.** An exact result the browser keeps, keyed by the recipe that made it, so going back to that recipe (a step switched off and on) shows it without rendering.
- **Midtone handle.** The grey handle of the levels editor; where it sits in the stretch between black and white sets `gamma`.
- **Upgrade.** How a step saved with an older version of its effect is brought up to date when its only change was a moved default: the old value is written in, so the picture stays the same.
- **Set (parameter).** Any of a fixed list of words, joined by `+` (`reds+blues`), or `all`; shown as toggles, such as hsl's colour dots.
- **Seed.** The number that drives a step's randomness: where its hits land, and anything else the effect randomizes. The same seed always gives the same result on any machine, which is what makes recipes replay. Any whole number from 0 to 2^64 - 1; the dice picks one under a million so it is easy to type. No seed is stronger than another and neighbours are unrelated (41 and 42 differ as much as 41 and 900,000): think of it as a variation number. With `hits=0` nothing is random, so the dice is greyed out. In the GUI it sits at the end of Placement.
- **Target.** Choosing a byte region by image position, for example "rows 40-60%" (Phase 3). It is approximate, because damage bleeds downward.
- **Bottom sheet.** On a phone, the step panel sits under the preview and can be pulled taller with its handle. A common mobile pattern.
- **Compare.** Pressing and holding the photo (or the `\` key) to see the untouched original in place of the glitch, as in Instagram. While masking, holding the photo paints, so the top bar has a compare button to hold instead.
- **Zoom, fit, 100%.** Fit shows the whole picture; 100% shows one photo pixel per device pixel (as in Lightroom). The magnifier in the photo toolbox, a double tap or Z switch between them; a pinch or ctrl + wheel goes anywhere up to 800%.
- **Patch.** The sharp part of a zoomed view: what is on screen, cut from the full-resolution render and laid over the enlarged preview once the view holds still. Always exact.
- **Full-resolution cache.** The server's two kept full-size rasters for the photo being worked on: the picture just before the step being edited, and the last finished render. They make exact previews, pans and exports of what was just shown cheap.
- **Document (doc).** A photo open in `glitchrx serve`. The server keeps up to four, closing the least recently used.
- **Latest wins.** The preview scheduling rule: at most one render runs and one waits, and a new request replaces the waiting one. While a slider moves, the preview keeps up with the newest value instead of falling behind.
- **Preview.** The GUI's on-screen render: a scaled preview, bent on the server and sent as a JPEG. Exports are full resolution.
- **Render cache / step cache.** What the server keeps per document so re-renders are fast: the screen-sized raster, and the raster after each step. Changing step 3 re-runs steps 3 onwards only.
- **Root (`--root`).** The one folder `glitchrx serve` may browse, open from and save into. Paths outside it are refused.
- **Step switch.** The on/off box on a GUI step. A switched-off step stays in the stack but is left out of renders and recipes.
- **Desktop app.** `glitchrx-desktop`: the GUI in a native window (Tauri). It runs its own server on 127.0.0.1 and uses the system's Open and Save dialogs.
- **Tauri / web view.** Tauri is the framework behind the desktop app: a small native shell around the system's web engine (the web view; WebKitGTK on Linux, WebKit on macOS), instead of bundling a browser.
- **Native dialogs (`Dialogs`).** The system's file choosers. The server asks for them through the `Dialogs` trait, which only the desktop app supplies.
- **Token.** The shared secret of `glitchrx serve --token`. Opening the printed URL once stores it in a cookie; without it, a browser gets the sign-in page to paste it into.
- **PWA (home-screen app).** The GUI added to a phone's home screen: it opens full screen with its own icon, from the web app manifest and iOS meta tags. Nothing is installed; it still talks to your server.
- **Share sheet.** The phone's system menu for sending a file somewhere (Save Image, Save to Files, AirDrop). Exports on touch devices go through it.
- **Window.** The bytes a single hit covers (`len`), a whole number of frames. Some parameters (`shuffle` block, `stutter` chunk) can be a percent of the window. Effects write only inside their windows.

## Archived: JPEG byte engine

Terms from Phase 1 and 2a, when effects edited the compressed data inside a JPEG (tag `archive/jpeg-bytes`).

- **Bake.** Decoding the glitched JPEG and saving the result as a clean PNG, since viewers rendered a broken JPEG differently. Every output is a normal image now.
- **Band.** The strip a glitch damaged, from the hit down to the next restart marker, set by `--restart`.
- **Decode status.** The decoder's verdict on a glitched JPEG: `clean`, `recovered`, `partial` or `failed`.
- **Invert / reverse / undo hint.** Running an effect's math backwards to recover the exact input, and the printed command that did it. Clipping makes this impossible in the pixel engine.
- **Mode: safe / raw.** Whether effects saw de-stuffed bytes (safe) or the bytes as stored, forged markers included (raw).
- **Payload.** The de-stuffed scan data of the main image, the buffer byte effects edited.
- **Prepared input.** The input losslessly rewritten with restart markers before glitching.

## Math and audio terms

- **All-pass filter.** A filter that keeps every frequency's loudness but delays some more than others. Mixed with the dry signal, a chain of them cancels some frequencies: the phaser's notches.
- **Biquad.** A two-pole, two-zero filter, the standard building block of equalisers; the wahwah is one, set as a resonant low-pass.
- **Box blur (three passes).** Averaging each value with its neighbours in a square window. Done three times in a row it closely approximates a Gaussian blur, at a cost that does not grow with the radius (a running sum). Used to feather masks.
- **Smoothstep.** The curve 3t^2 - 2t^3: a fade from 0 to 1 that starts and ends flat, so a soft brush edge shows no ring.
- **Comb filter.** An echo fed back into itself at a fixed delay. Several in series make a cheap reverb.
- **Waveshaping / fold.** Passing each sample through a fixed curve (`distort`). Hard clipping flattens at full scale, soft clipping rounds off before it, and folding reflects anything past full scale back in, so highlights turn into stacked bands.
- **Resampling.** Reading a signal at a different rate, interpolating between samples (`speed`).
- **Endianness.** The order in which a multi-byte number is stored. Big endian puts the most significant byte first; little endian puts it last. It only matters for 16- and 24-bit encodings, where it decides which colour byte is the loud part.
- **Feed-forward vs feedback.** A feed-forward echo adds delayed copies of the *input*. A feedback echo adds delayed copies of its own *output*, so the echoes repeat and decay.
- **Basic IEEE operations.** `+ - * /` and square root on floating-point numbers are exactly specified, so they give identical bits on every machine; library functions like `exp` and `sin` are not. Effects use only the former (`dsp.rs` has its own `exp`).
- **Modular (wraparound) arithmetic.** Byte values wrap at 256, so 250 + 10 = 4. The archived engine used it to make effects exactly reversible.
- **Q8.** A fixed-point number scaled by 256, used by the archived engine to keep math integer-only.

## Tools and stack

- **axum.** The Rust web server library behind `glitchrx serve` (Phase 2).
- **Cargo workspace.** One repository holding several Rust crates that build together.
- **clippy / rustfmt.** Rust's linter and formatter. Both are part of the gate.
- **Crate.** A Rust package. Ours are `glitchrx-core` (the engine) and `glitchrx-cli` (the `glitchrx` command), plus tools.
- **Gate.** `just check` (or `scripts/check.sh`): format check, lint, tests. It must pass before work is called done.
- **insta.** A snapshot-testing library. It stores expected outputs (here, hashes) in files next to the tests.
- **just.** A small command runner, like `make` but simpler. The recipes live in `justfile`.
- **libjpeg-turbo / TurboJPEG.** The JPEG library used by browsers and most viewers. TurboJPEG is its simple API. It is our JPEG decoder and encoder.
- **WebP, libwebp.** Google's image format, lossy or lossless, and its reference library, built into glitchrx from source.
- **AVIF, rav1e, ravif.** An image format built on the AV1 video codec; rav1e is a pure-Rust AV1 encoder and ravif wraps it for still images. Slow, and its **speed** setting (1-10) trades encoding time for file size. It stores colour as codes (primaries, transfer), not as an ICC profile.
- **Display P3.** The wider colour space of Apple devices and most recent phones. Photos from them carry a Display P3 profile.
- **png / tiff crates.** Pure-Rust PNG and TIFF readers and writers.
- **proptest.** Property-based testing: it throws hundreds of random inputs at a rule (such as "an effect writes only inside its windows") and shrinks any failure to the smallest example.
- **Svelte.** The frontend framework for the web GUI (Phase 2). It compiles to small plain JavaScript.
- **Tauri.** A toolkit for wrapping a web UI in a native desktop window (optional, Phase 2e).
- **Content-Security-Policy (CSP).** A response header telling the browser where a page may load things from. `glitchrx serve` allows only itself, which enforces "no external requests" in the browser.
- **Embedding.** Building the compiled web GUI into the `glitchrx` binary (`build.rs` + `include_bytes!`), so it runs with nothing on disk.
- **Lucide.** The open-source icon set used, sparingly, in the GUI.
- **svelte-check / vitest.** The frontend's type checker and unit-test runner. Both are part of the gate once web dependencies are installed.
- **tokio.** The async runtime under axum.
- **Vite.** The frontend build tool: it compiles the Svelte app into `web/dist` and runs the hot-reload dev server.
- **WireGuard.** The VPN Phil uses to reach the home network. `glitchrx serve` is reached through it.
