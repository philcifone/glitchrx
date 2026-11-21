# Glitch Art Tool: Project Brief

Prepared for Phil. Audience: a larger model that will write the full spec before any code is written. This is a brief, not a spec.

## 1. Vision

A cross-platform (Linux, macOS, Windows) tool for glitch art, intended as "Darktable or Photoshop for glitch." It reproduces the Audacity workflow (treat image bytes as audio, apply effects, re-open as an image) but adds what that workflow lacks: live visual feedback, repeatability, and safe file handling. Everything runs locally. No image is ever uploaded anywhere.

A CLI persists alongside the GUI for the life of the project. Both are thin front ends over one shared core library.

## 2. Background: how the Audacity method works

The spec model should understand this before designing anything.

- A JPEG is a header (dimensions, tables, color info) followed by a compressed scan. Corrupting the header usually makes the file unopenable. Corrupting the compressed scan data produces the visual glitches. Phil's old rule, "don't touch the header," is correct and is a hard requirement here.
- Audacity's Import Raw Data treats those bytes as audio samples. Effects like echo, delay, and reverb then rewrite the bytes. Exporting raw and renaming to .jpg lets an image viewer decode the damaged result.
- Endianness (Phil asked about this): it only matters when a sample is more than one byte, such as 16-bit. Big endian stores the most significant byte first; little endian stores it last. Picking the wrong one scrambles which bytes are "loud" versus "quiet" in the effect math. With 8-bit samples it is irrelevant. The tool should hide this entirely and default to 8-bit unsigned byte operations, exposing sample width and byte order only in an advanced panel.
- The old gap: no preview. The new tool must show the result as the effect is applied.

## 3. Core design principles

1. **Non-destructive.** Originals are never modified or overwritten. Internally, a glitch is a "recipe" (ordered list of effects plus parameters plus RNG seed) applied to the original bytes. This gives undo, history, and reproducibility for free.
2. **Header-safe by default.** Effects operate only on the entropy-coded scan region (after the SOS marker). Option to expand the target region is advanced and off by default.
3. **Deterministic.** Same input plus same recipe plus same seed gives the same output, byte for byte.
4. **Tolerant decoding.** Glitched files are malformed by design. Standard decoders often refuse them. The preview pipeline needs a decoder that recovers gracefully and always returns something viewable, with a clear "could not decode" state when it truly cannot.
5. **Format-extensible.** Phase 1 is JPEG only, but the core should treat "format" as a pluggable module so TIFF, PNG, WebP, AVIF, and RAW can be added later.

## 4. Effects

Two families, which the spec should keep distinct:

- **Byte-domain** (Audacity-style, operate on the compressed data): echo, delay/shift, stutter/repeat, bit-crush, amplify with clipping, reverse a chunk, swap or shuffle blocks.
- **Pixel-domain** (operate on decoded pixels, later phases): pixel sorting, channel offset, scanline displacement.

**Honest caveat on `--reverse`:** this is only possible for effects that are mathematically invertible. Audacity's real echo and reverb use floating-point math with clipping, which destroys information. Recommendation: define the tool's own byte-domain effects using integer modular arithmetic (wraparound mod 256), which makes echo, shift, and reverse-chunk exactly invertible. Effects that clip or discard data (bit-crush, true reverb) are flagged non-reversible. The spec should state which effects are reversible and the CLI should say so clearly. Because of the recipe model, "undo" in the GUI works for everything regardless. Confidence: high on the math, medium on how closely modular effects match the Audacity look; this should be tested visually.

## 5. Phases

Each phase ends in a working, tested tool.

**Phase 1: CLI, JPEG only.** At least 5 byte-domain effects. Example: `glitch --echo example.jpg example-glitch.jpg`. A `--help` listing every effect with a usage example. A `--reverse` flag for invertible effects. If TIFF, PNG, or WebP are trivial to include given the architecture, fine; otherwise defer.

*File safety rules:*

- Never overwrite the original. Refuse to overwrite any existing output unless `--force` is given.
- With no output name, auto-create `name-glitch.jpg`. Further iterations increment: `-glitch`, `-glitch-1`, `-glitch-2`. (Suggested deviation from Phil's `-glitch 1`: use a hyphen, not a space, to avoid shell-quoting headaches. Spec model may confirm.) Running on an already-glitched file should increment the existing suffix, not stack `-glitch-glitch`.

**Phase 2: more formats and GUI shell.** Add TIFF, PNG, WebP, AVIF, and RAW handling. Build the GUI as a visual front end for the existing effects only: open image, pick effect, see live result, export. No new effects, to prove the plumbing.

**Phase 3: depth.** Per-effect strength sliders, pixel sorting, and masking (hand-drawn, shape, vector) so effects apply to selected regions. Masking is the hardest item because byte-domain glitches do not map cleanly to image regions. The spec model should flag this and propose an approach, likely applying pixel-domain effects through masks and byte-domain effects to a decode-then-region pipeline.

## 6. Architecture options

| Option | Pros | Cons |
| --- | --- | --- |
| **Rust core + CLI (clap) + Tauri GUI** | Fast, one core for CLI and GUI, small binaries, UI built in HTML/CSS so Phil's design system (Young Serif, Courier New, Gruvbox, sharp corners) is easy | Tauri on Linux uses WebKitGTK, which has occasional rendering quirks |
| Rust core compiled to WASM, run in the browser | Truly local (no upload), no install, same core | Large files and memory limits, no native file system integration |
| Rust + egui/iced (native UI) | Single native binary | Harder to make look polished; custom design system is more work |
| Python + Qt (PySide6) | Fast to prototype, Pillow and numpy are convenient | Packaging across three OSes is painful; slower for live preview on large images |
| Electron | Mature, consistent rendering | Heavy, against Phil's pragmatic-and-lean preference |

**Recommendation (confidence: medium-high):** Rust workspace with three crates: `glitch-core` (library), `glitch-cli`, and a Tauri GUI. Rust earns its place here because live preview means re-glitching and re-decoding large images repeatedly, and because byte manipulation with strong correctness guarantees suits it. Keep the core free of any UI dependency so a WASM build remains possible later.

**ImageMagick is not needed.** Byte-domain effects are simple buffer operations. ImageMagick would add a heavy external dependency and would not help with tolerant decoding. Reserve it, at most, as an optional format converter, and probably not even that.

Candidate libraries for the spec model to evaluate (verify current maintenance status): `image`, `zune-jpeg` or `jpeg-decoder` (tolerance of corrupt data is the key criterion), `rayon` (parallelism), `clap` (CLI), `rawler` or `rawloader` (RAW), `ravif`/`libavif` and `webp` (later formats), `proptest` and `criterion` (testing, benchmarks). Open question: does any Rust decoder tolerate corrupt scans well enough, or must the core fall back to a lenient libjpeg-turbo binding?

## 7. Harness for Claude

**Git**

- Create a repo with a `main` branch and one branch per phase or feature (for example `phase-1-cli`, `feature/echo-effect`).
- Commit early and often with clear conventional messages. Never push to main. Never merge to main. Phil reviews and tests, then merges.
- Stop and summarize at each phase boundary; do not begin the next phase without Phil's go-ahead.

**Testing and checks (must pass before any commit is called done)**

- `cargo fmt`, `cargo clippy` with warnings denied, `cargo test`.
- Unit tests per effect; property-based round-trip tests proving each reversible effect restores the exact original bytes.
- Test that the JPEG header (everything up to the scan start) is byte-identical after every effect.
- Test that the original file is never modified and existing outputs are never overwritten, and test the `-glitch`, `-glitch-1`, `-glitch-2` naming including already-suffixed inputs.
- Golden-output tests using seeded RNG for determinism.
- A small committed set of test images that are generated or CC0, not Phil's photos.
- Claude must actually run the tests and build, and report real results rather than assuming success.

**Repo hygiene**

- Bare-bones layout. Plain, mature README with no emojis, and no emojis in any markdown file. Well-commented code, with brief notes where a technique is interesting.
- A short `CLAUDE.md` or equivalent capturing these rules.
- License choice left to the spec model to propose, favoring open-source and privacy-respecting.

**Working style (ADHD-friendly)**

- Structured output, with a short "current status and next step" recap after any tangent.
- State assumptions and proceed; ask only when the answer materially changes the work.

## 8. Open questions for the spec model

1. Which decoder strategy gives reliable tolerant previews?
2. Exact definitions and parameters for the five Phase 1 effects, and which are reversible.
3. How should masking work given the byte-domain versus pixel-domain split?
4. Preview performance plan for large images (proxy resolution while dragging sliders, full quality on release?).
5. Recipe file format so a glitch can be saved, shared, and replayed, and whether the CLI should accept it.
6. Project name (placeholder: `glitch`) and license.
