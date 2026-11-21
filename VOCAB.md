# Vocabulary

Shared terms for glitcharaxia, so we can name things precisely. Grouped by topic, alphabetical within each group. It grows as the project does.

## JPEG anatomy

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
- **RST marker.** Restart marker, `FF D0` to `FF D7`. It appears every N MCUs when the encoder enables a restart interval, and lets a decoder recover from damage. That makes glitches more contained.
- **SOS.** Start Of Scan marker, `FF DA`. The scan data follows its short header.
- **Subsampling.** Storing color (chroma) at lower resolution than brightness. Written as 4:2:0 (half width and half height), 4:2:2 (half width) or 4:4:4 (full).
- **Thumbnail (EXIF).** A small, complete JPEG embedded in the metadata of camera and phone photos. Left untouched.
- **Trailing data.** Bytes after the main image's EOI, such as phone gain maps (Ultra HDR) or extra MPF pictures. Left untouched.

## Project concepts

- **Backwards.** The effect that reverses byte order inside each hit, like playing audio in reverse. It is not called "reverse", because `--reverse` is the undo option.
- **Bake.** Decoding the glitched file and saving the result as a clean PNG. Different viewers render a broken JPEG differently; a baked PNG looks the same everywhere.
- **Chain.** Several effects applied in order. On the CLI, the order of the effect flags is the chain order.
- **Contact sheet.** A grid image showing one effect at a range of parameter values, side by side. Used to tune defaults.
- **Decode status.** The verdict from the decoder: `clean`, `recovered` (warned, but produced a full image), `partial` (stopped part way; the top part is kept) or `failed`.
- **Effect.** One glitch operation, such as echo or shuffle. **Byte effects** work on compressed bytes. **Pixel effects** (Phase 3) work on decoded pixels.
- **Golden test.** A test that checks the output's hash matches a stored value. It proves that the same input, recipe and seed always give the same bytes.
- **Hit.** One window where an effect is applied. `hits=3` means three windows at seeded random places inside the region.
- **Invert / reverse.** Running an effect's math backwards to recover the exact input. Possible only for reversible effects.
- **Mask.** A grayscale image saying where an effect shows (Phase 3).
- **Mode: safe / raw.** In **safe** mode (the default), effects see de-stuffed bytes, so they can never forge a marker. In **raw** mode, effects see the bytes exactly as stored, like Audacity did, so they can forge markers and truncate the image.
- **Payload.** The de-stuffed scan data of the main image, the buffer every byte effect edits.
- **Recipe.** A TOML file that lists the effects, parameters and seeds. Replaying it on the same input gives the same output.
- **Region.** The part of the payload where hits may land, for example `5%:100%`. Payload percent is not image percent: busy, detailed areas take up far more bytes than smooth ones, so `50%` of the payload is rarely halfway down the picture.
- **Seed.** The number that drives all randomness. The same seed always gives the same result.
- **Target.** Choosing a byte region by image position, for example "rows 40-60%" (Phase 3). It is approximate, because damage bleeds downward.
- **Undo hint.** The `undo: glitchrx --reverse ...` line printed after a reversible run. Copy and paste it to restore the input exactly.
- **Window.** The bytes a single hit covers (`len`). Some parameters (`shuffle` block, `stutter` chunk) can be a percent of the window.

## Math and audio terms

- **Comb filter.** An echo fed back into itself at a fixed delay. Several in series make a cheap reverb.
- **Endianness.** The order in which a multi-byte number is stored. Big endian puts the most significant byte first; little endian puts it last. It only matters for 16-bit samples, so it is irrelevant at the 8-bit default.
- **Feed-forward vs feedback.** A feed-forward echo adds delayed copies of the *input*. A feedback echo adds delayed copies of its own *output*, so the echoes repeat and decay.
- **Modular (wraparound) arithmetic.** Byte values wrap at 256, so 250 + 10 = 4. Unlike clipping, no information is lost, which is what makes the effects exactly reversible.
- **Q8.** A fixed-point number stored as an integer scaled by 256, so 0.5 is stored as 128. It keeps effect math integer-only and identical on every machine.

## Tools and stack

- **axum.** The Rust web server library behind `glitchrx serve` (Phase 2).
- **Cargo workspace.** One repository holding several Rust crates that build together.
- **clippy / rustfmt.** Rust's linter and formatter. Both are part of the gate.
- **Crate.** A Rust package. Ours are `glitchrx-core` (the engine) and `glitchrx-cli` (the `glitchrx` command), plus tools.
- **Gate.** `just check` (or `scripts/check.sh`): format check, lint, tests. It must pass before work is called done.
- **insta.** A snapshot-testing library. It stores expected outputs (here, hashes) in files next to the tests.
- **just.** A small command runner, like `make` but simpler. The recipes live in `justfile`.
- **libjpeg-turbo / TurboJPEG.** The JPEG library used by browsers and most viewers. TurboJPEG is its simple API. It is our decoder and fixture encoder.
- **proptest.** Property-based testing: it throws thousands of random inputs at a rule (such as "invert undoes apply") and shrinks any failure to the smallest example.
- **Svelte.** The frontend framework for the web GUI (Phase 2). It compiles to small plain JavaScript.
- **Tauri.** A toolkit for wrapping a web UI in a native desktop window (optional, Phase 2b).
- **WireGuard.** The VPN Phil uses to reach the home network. `glitchrx serve` is reached through it.
