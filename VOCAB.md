# Vocabulary

Shared terms for glitcharaxia, so we can name things precisely. Grouped by topic, alphabetical within each group. It grows as the project does.

## Pixel engine (SPEC v0.3)

- **A-law / U-law.** Two telephone audio encodings that squeeze a 16-bit sample into one byte along a curve. Reading pixel bytes this way and back gives posterized, neon colour blocks.
- **Channel phase.** How many extra bytes (0, 1 or 2) a delay adds beyond whole pixels. Phase 0 keeps a ghost's colours; 1 or 2 rotates red into green into blue.
- **Channels (mono / stereo).** In stereo, samples alternate left and right, and effects treat each side separately, as Audacity does.
- **Clipping.** Clamping samples to the encoding's range after an effect. It loses information, which is why effects can no longer be reversed exactly, and it gives the bright, saturated look.
- **Databending.** Opening a file as the wrong kind of data (here: an image as audio), editing it with that kind of tool, and opening it as an image again.
- **Encoding (sample encoding).** How bytes are read as audio samples: `u8`, `s8`, `s16le`, `s16be`, `s24le`, `s24be`, `ulaw`, `alaw`. The choice from Audacity's Import Raw Data dialog.
- **Effect palette.** The GUI's grid of effect buttons; one tap adds a step. Each effect has an icon picked for feel rather than literally (a mountain for echo, a drum for stutter).
- **Exact preview.** A preview bent at full resolution and then shrunk, so it is exactly the export. Slower than the scaled preview; the "Make exact" button.
- **Filter vs edit.** The two kinds of effect. **Filters** (echo, delay, reverb, amplify, invert, bitcrush) turn bytes into samples, process them, and clip them back. **Edits** (backwards, shift, shuffle, stutter) only move whole samples, like cut and paste.
- **Fit / resize.** `fit` picks the largest size inside a box that keeps the aspect ratio; `resize` shrinks to an exact size. They differ because fitting twice can round differently (480x320 into 200x200 is 200x133, but into 200x133 is 199x133).
- **Frame.** One sample for every channel: `encoding width x channels` bytes. Windows start and end on whole frames, so samples line up with the start of the raster.
- **Freeverb.** A classic open reverb design (damped feedback combs plus allpass filters) that Audacity's Reverb is based on.
- **LFO.** Low-frequency oscillator: a slow wave that sweeps a filter, as in phaser and wahwah.
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
- **Effect.** One operation, such as echo or shuffle, following one of Audacity's. All of them work on the raster's raw bytes. **Pixel-space effects** such as pixel sorting (Phase 3) will work on pixels as colours instead.
- **Golden test.** A test that checks the output's hash matches a stored value. It proves that the same input, recipe and seed always give the same bytes.
- **Hit.** One window where an effect is applied. `hits=3` means three windows at seeded random places inside the region.
- **Mask.** A grayscale image saying where an effect shows (Phase 3).
- **Re-roll vs shake.** Re-roll (dice) gives a step a new seed, which moves where its hits land. Shake (wand) gives it new random settings for its own parameters, leaving placement and encoding alone. Both write into the recipe, so results stay replayable.
- **Recipe.** A TOML file (version 2) that lists the effects, parameters and seeds, plus the recipe-wide encoding and channels. Replaying it on the same input gives the same output.
- **Region.** The part of the raster where hits may land, for example `40%:60%`. In the raster, percent of the data is percent of the image height, so that really is the middle band.
- **Seed.** The number that drives all randomness. The same seed always gives the same result.
- **Target.** Choosing a byte region by image position, for example "rows 40-60%" (Phase 3). It is approximate, because damage bleeds downward.
- **Bottom sheet.** On a phone, the step panel sits under the preview and can be pulled taller with its handle. A common mobile pattern.
- **Compare.** Holding the "original" button (or the `\` key) in the GUI to see the untouched photo in place of the glitch.
- **Document (doc).** A photo open in `glitchrx serve`. The server keeps up to four, closing the least recently used.
- **Latest wins.** The preview scheduling rule: at most one render runs and one waits, and a new request replaces the waiting one. While a slider moves, the preview keeps up with the newest value instead of falling behind.
- **Preview.** The GUI's on-screen render: a scaled preview, bent on the server and sent as a JPEG. Exports are full resolution.
- **Render cache / step cache.** What the server keeps per document so re-renders are fast: the screen-sized raster, and the raster after each step. Changing step 3 re-runs steps 3 onwards only.
- **Root (`--root`).** The one folder `glitchrx serve` may browse, open from and save into. Paths outside it are refused.
- **Step switch.** The on/off box on a GUI step. A switched-off step stays in the stack but is left out of renders and recipes.
- **Token.** The shared secret of `glitchrx serve --token`. Opening the printed URL once stores it in a cookie.
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

- **Comb filter.** An echo fed back into itself at a fixed delay. Several in series make a cheap reverb.
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
