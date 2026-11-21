# glitcharaxia: Specification

Status: v0.2, 2026-11-21. It incorporates Phil's review of v0.1; his annotated copy of v0.1 lives in his Obsidian vault. The source documents are `pre-brief.md` and `BRIEF.md`, and `VOCAB.md` defines the terms used here.

Keywords: **must** means required for the phase to be done. **Should** means expected, but logged and deferred if it proves hard. **Later** means out of scope for the phase.

## Decision log (from Phil's review of v0.1)

| # | Topic | Decision |
| --- | --- | --- |
| D-1 | Binary name | `glitchrx`. Project name `glitcharaxia`. Crates `glitchrx-core`, `glitchrx-cli`. |
| D-2 | License | GPL-3.0-or-later for everything. |
| D-3 | `--save-recipe` default | Off. The CLI prints the replay/reverse command instead. |
| D-4 | `--reverse` priority | The core `invert` is built in Phase 1 because the round-trip tests need it. The CLI flag is a **should** and must not block early testing. Section 7.4 lists the use cases. |
| D-5 | LAN/web GUI | Yes, and it is the **primary** GUI. Phil works remotely over WireGuard, often from iOS Firefox. `glitchrx serve` comes first in Phase 2, and the Tauri desktop window becomes a later, optional wrapper around the same web UI (section 10). |
| D-6 | Frontend | Svelte 5 + TypeScript + Vite (section 10.1 gives the reasoning). |
| D-7 | Region default | Claude chooses it during tuning and tells Phil explicitly when there is a visible difference to judge. |
| D-8 | CLI output | The CLI is a normal terminal tool: it writes files and prints a summary line, and never displays images. "WYSIWYG" applies only to the GUI preview. `--bake` is an optional extra output file. |
| D-9 | DNG | In scope as input in Phase 2 (section 9, 2d). |
| D-10 | Icons | Lucide icons in the GUI, used sparingly (toolbox, save, export). No emojis anywhere. |
| D-11 | Test photos | Phil supplies them in `samples/`, which is git-ignored and never committed (section 11.5). |
| D-12 | 16-bit sample view | Deferred from Phase 1 to **later**. It adds a generic layer to every effect for little visual payoff before the GUI exists. |

## 0. Review of the brief (v0.1, kept for reference)

1. **Byte stuffing and spurious markers.**
   - Inside a JPEG scan, every `0xFF` data byte is followed by a stuffed `0x00`. A `0xFF` followed by anything else is a marker.
   - If an effect writes `FF D9` (EOI) into the scan, the decoder stops there and the rest of the image goes flat gray.
   - **Decision:** effects operate on the *de-stuffed* scan payload, and the core re-stuffs it on output, so no spurious markers can appear.
   - A `raw` mode (advanced, off by default) skips de-stuffing to reproduce the Audacity behaviour faithfully, truncations included.
2. **"After the SOS marker" is not enough for real files.**
   - Phone JPEGs carry an EXIF thumbnail (a complete JPEG inside APP1) and often MPF or Ultra HDR gain-map images after the primary EOI. Progressive JPEGs have many scans.
   - The parser walks segments by their length fields, finds every scan of the primary image, and leaves everything else alone.
3. **The decoder defines the look.** Browsers and most viewers use libjpeg-turbo, so a GUI preview decoded with libjpeg-turbo shows what other people will see (section 5).
4. **Byte glitches are not local.** Huffman desync and DC drift spread a single changed byte across the rest of the image. This shapes the effect defaults and the masking design.
5. **WebP and AVIF** decoders reject almost any corrupt bitstream. They are handled by transcoding (section 9).
6. **CLI syntax:** `glitchrx --echo in.jpg out.jpg` works as written (section 7).
7. **Naming:** `-glitch`, `-glitch-1`, `-glitch-2`.

## 1. Goals and non-goals

Goals:
- Local only. No network calls at runtime. Fonts and assets are bundled.
- Non-destructive, deterministic, header-safe by default.
- A CLI and a web GUI as thin front ends over one core crate.
- Linux first-class. macOS and Windows supported.
- The GUI is usable from a phone browser over the LAN or VPN.

Non-goals:
- General photo editing.
- Cloud anything.
- Telemetry.
- Plugin ABI.

## 2. Name and license

- Project `glitcharaxia`, binary `glitchrx`.
- GPL-3.0-or-later. libjpeg-turbo (BSD/IJG/zlib) and rawler (LGPL-2.1) are compatible.

## 3. Architecture

### 3.1 Stack

- Rust stable, edition 2024, Cargo workspace.
- The web GUI is served by the `glitchrx` binary itself (axum), with an optional Tauri v2 wrapper later.
- The algorithmic modules of the core have no UI, async or filesystem dependencies, so a WASM build stays possible.

### 3.2 Layout

```
glitcharaxia/
  Cargo.toml                 workspace
  crates/
    glitchrx-core/           library: formats, effects, recipe, rng, decode, io
      src/
        lib.rs
        rng.rs               seeded deterministic RNG + sampling helpers
        recipe.rs            Recipe, Step, TOML (de)serialization
        params.rs            parameter specs, parsing, validation
        placement.rs         region / hits / len -> windows
        effects/             one file per effect + mod.rs registry
        formats/jpeg/        parse.rs, scan.rs (destuff/restuff), info
        decode.rs            (feature "turbo") tolerant decoder, scaled decode
        io.rs                (feature "fs") naming rules, safe writes
    glitchrx-cli/            front end: argument parsing, summary output
  web/                       Phase 2: Svelte UI, served by `glitchrx serve`
  tools/
    gen-fixtures/            generates tests/fixtures (committed output)
    contact-sheet/           parameter-sweep grids for visual tuning
  tests/fixtures/            generated CC0 test images
  samples/                   Phil's photos, git-ignored
  docs/effects.md            effect reference
  docs/decisions/            ADRs (0001-decoder.md, ...)
  SPEC.md VOCAB.md README.md CLAUDE.md LICENSE justfile
```

### 3.3 Core model

- **JPEG container.** The parser produces an ordered list of chunks: *structure* chunks (bytes copied verbatim) and *entropy runs* (the de-stuffed bytes between markers). Restart markers and fill bytes are structure. All runs of the primary image, concatenated, form the **payload**, which is what effects edit. `finish()` re-stuffs every run and reassembles the file.
- **Length-preserving rule.** Every byte effect preserves payload length. Re-stuffing can change the file size by a few bytes, but structure outside the scans is never touched.
- **Effects** implement `apply` and `invert` on a window of the payload. Non-reversible effects return `NotReversible` from `invert`.
- **Decode result:** `Clean`, `Recovered(message)` (libjpeg-turbo warned but produced a full image), `Partial(message)` (a fatal error mid-image; the rows decoded so far are kept), or `Failed(message)`.

### 3.4 Determinism

- RNG is ChaCha8, keyed by a 32-byte seed that the core expands from the `u64` step seed with SplitMix64. The core does not rely on `seed_from_u64`, whose expansion is not promised stable across crate versions.
- The core uses its own sampling helpers (rejection-sampled bounded integers, Fisher-Yates).
- Integer-only effect math. Float parameters are quantized once to Q8 (`round(x * 256)`).
- Every effect has a `VERSION`. Every recipe step stores its own seed.
- Per-hit randomness is derived from `(step seed, hit index)`, so inversion can process hits in reverse order and still draw the same values.

## 4. JPEG handling

- **Supported:**
  - SOF0, SOF1, SOF2 (baseline, extended, progressive) with Huffman coding
  - any subsampling, grayscale, CMYK
  - restart intervals
- **Refused clearly:** arithmetic coding, lossless, hierarchical.
- **Parser:**
  - walks segments by length
  - treats `FF FF` fill as structure
  - stops at the primary EOI, preserving trailing bytes verbatim
- **Lenient re-parsing.** Once at least one scan has been found, any later structural surprise turns the rest of the file into trailing data instead of failing. Files glitched in raw mode can therefore be glitched again.
- **`--info`:**
  - dimensions, components, subsampling, progressive or baseline
  - scans, payload size, restart interval
  - thumbnail and trailing-data presence
- **Header-safe guarantee (tested):** every non-entropy byte is identical to the original in safe mode.

## 5. Decoding

- libjpeg-turbo through its TurboJPEG 3 API (`turbojpeg-sys`), linked to the system library through pkg-config on Linux.
- The decoder is told not to stop on warnings, and decodes scaled (1/2, 1/4, 1/8) for previews.
- The decoder is used in Phase 1 for `--bake`, the post-write decode check, and the contact sheets.
- Decision record: `docs/decisions/0001-decoder.md`.
- Contract: the decode wrapper never panics, and an undecodable file is a normal, reported state.

## 6. Effects

### 6.1 Placement (shared by all byte effects)

- **`region=START:END`** sets where hits may land, as percent or byte offsets. The default skips the start of the payload (value set by tuning, D-7).
- **`hits=N`** is the number of seeded windows. `hits=0` means one window covering the whole region.
- **`len=BYTES`** is the window length. The `k` and `m` suffixes are accepted.
- Hits are applied in order and inverted in reverse order. An effect writes only inside its window.

### 6.2 Phase 1 effects

Arithmetic is mod 256 and `q` is a Q8 gain.

| Effect | Definition | Reversible |
| --- | --- | --- |
| `echo` | Feed-forward taps: `y[i] = x[i] + sum_k (q_k * x[i - k*d]) >> 8` with `q_k = Q8(gain^k)` | Yes |
| `reverb` | Three feedback combs in series, delays `d`, `1.37d`, `1.71d`: `y[i] = x[i] + (q * y[i-d]) >> 8` | Yes |
| `shift` | Circular rotate of each window by `by` | Yes |
| `reverse` | Reverse each window | Yes |
| `shuffle` | Permute a `fraction` of the `block`-sized blocks | Yes |
| `amplify` | `wrap` mode: `(x-128)*m + 128` with odd `m`. `clip` mode: gain with clamping | wrap yes, clip no |
| `bitcrush` | Keep the top `bits` bits | No |
| `stutter` | Repeat the first `chunk` bytes over the window | No |

All eight are **must** for Phase 1. Defaults are tuned visually with the contact-sheet tool. The final parameters and defaults are documented in `docs/effects.md`, generated from the effect registry.

### 6.3 Later: sample views and Audacity-faithful mode

- 16-bit little-endian and big-endian sample views (D-12).
- A mu-law and float mode, built only if the integer effects turn out to lack the Audacity look.

## 7. CLI (Phase 1)

### 7.1 Syntax

```
glitchrx [--EFFECT [k=v,...]]... [OPTIONS] INPUT [OUTPUT]
glitchrx [--EFFECT [k=v,...]]... [OPTIONS] INPUT INPUT INPUT...
```

- An effect flag may be followed by a parameter token. That token is any argument containing `=` that is not an existing file; `--echo=delay=4096` also works.
- Effect flags run in command-line order.
- **Positional arguments:**
  - With exactly two positionals, the second is the output, matching the brief's form. It is refused if it already exists, unless `--force` is given.
  - With three or more positionals, or with `--batch`, all of them are inputs and output names are generated automatically.

Examples:

```
glitchrx --echo photo.jpg
glitchrx --echo photo.jpg photo-echo.jpg
glitchrx --echo delay=4096,gain=0.7 --shift by=200,hits=5 photo.jpg
glitchrx --reverse --echo delay=4096,gain=0.7 --seed 81723 photo-glitch.jpg
glitchrx --recipe moody.toml --batch a.jpg b.jpg
glitchrx --info photo.jpg
glitchrx --list
```

### 7.2 Options

| Option | Behaviour |
| --- | --- |
| `--seed N` | Master seed. The default is random and is always printed. |
| `--reverse` | Invert the chain. Refused if any step is lossy or the mode is raw. Default suffix `-restored`. |
| `--recipe FILE` | Apply a saved recipe instead of effect flags. |
| `--save-recipe[=FILE]` | Write the recipe used. Default `<output-stem>.recipe.toml`. |
| `--mode safe\|raw` | `raw` skips de-stuffing (Audacity-faithful, may truncate, not reversible). |
| `--bake[=FILE.png]` | Also write the decoded result as PNG. Default `<output-stem>.png`. |
| `--force` | Allow overwriting an existing output. Never the input. |
| `--batch` | Treat every positional as an input. |
| `--info`, `--list`, `--help`, `-q`, `-v` | Structure report, effect reference, help, quiet, verbose. |

After each write, one summary line: the output path, the decode status, the seed, and (when the chain is reversible) the exact reverse command.

### 7.3 File safety and naming

- The input is never written, even with `--force`. Checks use the canonical path and same-file identity, which covers symlinks and hardlinks.
- Writes go to a temp file in the target directory and are then persisted with no-clobber (or with overwrite under `--force`).
- **Auto naming:**
  - Strip `-glitch`, `-glitch-N` or `-restored(-N)` from the stem to get the base.
  - Take the highest existing `base-glitch[-N]` with any extension, plus 1. Bare `-glitch` counts as 0.
  - Keep the input's extension.
- **Exit codes:**
  - 0: ok
  - 1: runtime error
  - 2: usage error
  - 3: refused (overwrite, irreversible)
  - 4: unsupported format

### 7.4 Why `--reverse` exists (D-4)

Originals are never modified, and the GUI's undo works from the recipe, so `--reverse` is rarely needed to recover a photo. Its real uses:

1. **Proof of correctness.** The round-trip tests prove the effects are exact. They need the core `invert`, not the CLI flag.
2. **Recovery without the original.** Useful if you only kept a glitched file and its printed command, for example after sharing or moving files around.
3. **Creative mis-reversal.** Reversing with *different* parameters than were applied, for example `--reverse --echo delay=4000` on a file glitched with `delay=4096`, produces new artifacts that layer on top of the old ones. This is a distinct look and the most practical creative use.
4. **Peeling one layer.** After a stacked chain, reversing only the last step's parameters removes that layer while keeping the earlier ones.

The flag itself is small once the core `invert` exists, so it is planned for Phase 1 as a should.

## 8. Recipe format

TOML. Both the CLI and the GUI accept it.

```toml
format = "glitcharaxia-recipe"
version = 1
mode = "safe"

[[step]]
effect = "echo"
effect_version = 1
seed = 81723
delay = 4096
gain = 0.7
region = "5%:100%"
hits = 3
len = "4k"
```

## 9. Phases

Each phase ends in a working, tested tool on its own branch, followed by a stop-and-summarize.

### Phase 1: CLI, JPEG (branch `phase-1-cli`)

1. Bootstrap: repo, workspace, docs, fixture generator and fixtures.
2. Decoder decision record.
3. JPEG parser, scan view, `--info`, header-preservation tests.
4. Effect framework: RNG, placement, params, registry, recipes.
5. All eight effects, with unit, round-trip and golden tests.
6. CLI: syntax, naming, safe writes, `--bake`, `--recipe`, `--reverse`.
7. Tuning and docs: contact sheets, defaults, `docs/effects.md`, README.

**Exit:**
- `just check` is green.
- A 24 MP JPEG is processed in under 1 s.
- Phil tests on his own photos.

### Phase 2: web GUI, then formats (branch `phase-2-gui`)

- **2a `glitchrx serve` (primary GUI).** Svelte UI served by the binary:
  - open, step stack, parameters, live preview, before/after, export
  - undo and redo over recipe states, seed re-roll
  - export as native glitched bytes or baked PNG/JPEG
  - mobile-first layout that also works on desktop
- **2b Tauri wrapper (optional).** The same UI in a desktop window, for local use without a browser.
- **2c PNG and TIFF.** Inflate, glitch the filtered stream, deflate, and fix CRCs. Uncompressed TIFF directly.
- **2d WebP, AVIF, RAW and DNG (input).** Decode to a working format, glitch, then bake to the target on export. RAW and DNG go through `rawler`.
  - Note: iPhone ProRAW DNGs may use JPEG XL compression. rawler support for these must be verified with Phil's samples.
  - AVIF decoding needs dav1d, so it sits behind a feature flag.

### Phase 3: depth (branch `phase-3-depth`)

- Strength sliders: a 0-100 macro per effect, mapped along tuned curves.
- Pixel effects: pixel sort, channel offset, scanline displacement.
- Pipeline: byte steps, then decode, then pixel steps, with an explicit `reencode` step for generation-loss loops.
- Masking (section 10.3). Presets.

## 10. GUI design

### 10.1 Stack and look

- **Serving.** `glitchrx serve` runs an axum HTTP server in the same binary, using the same core. It serves the Svelte frontend, which is compiled at build time and embedded in the binary, so there are no external requests.
- **Binding.** It binds `127.0.0.1` by default. `--host 0.0.0.0` (or a specific WireGuard or LAN address) is opt-in, prints the LAN URL, and warns that there is no authentication. An optional shared token is a **should**.
- **Images stay on your machine:**
  - From the phone, the browser file picker sends the photo to *your own* server.
  - On the server, a browser restricted to a configured directory (for example `~/Pictures`) opens files where they already are.
- **Why Svelte:**
  - The UI has real state: a step stack, sliders, undo history, and later masks and brush strokes.
  - Svelte compiles that state handling away into small vanilla JavaScript, which keeps the bundle light on a phone.
  - Vanilla TypeScript would mean hand-writing the same reactivity (DOM updates on every state change), which gets messy by Phase 3.
  - React would work but is heavier for no gain here.
- **Look (Phil's defaults):**
  - Young Serif for headings, Courier New for body
  - sharp corners, bold lines
  - Gruvbox dark and light with a toggle, green accent
  - Lucide icons used sparingly
  - fonts bundled
- **Layout:**
  - Desktop: canvas in the centre, step stack on the right, status bar along the bottom.
  - Phone: the canvas on top and the step stack as a bottom sheet, with touch-sized controls.

### 10.2 Preview performance

- **Cached per document:**
  - original bytes
  - parsed container
  - per-step outputs, keyed by a hash of the recipe prefix
- **While dragging:**
  - Re-apply the byte effects (milliseconds).
  - Decode with libjpeg-turbo scaling to the smallest size that covers the viewport.
  - Target under 100 ms per frame for a 24 MP source.
- **After release (250 ms idle):** a higher-resolution decode up to the viewport size.
- **Full resolution** is only produced on export, on the server.
- **Phone constraint:** iOS browsers cap canvas size (roughly 16 MP total). Previews are always sized to the screen, never to the source.
- **Rendering:** latest-wins job scheduling, so stale renders are dropped. Previews travel to the browser as compressed images (JPEG or PNG at preview size), which keeps traffic small over WireGuard.

### 10.3 Masking

1. **Mask (exact, pixel-space).** The glitched render is composited over the original through an alpha mask, which can come from brush strokes, shapes or vectors. The vector sources are stored in the recipe.
2. **Target (approximate, byte-space).** MCU-row offsets are recorded during decode, so a byte region can be chosen as "rows 40-60%". The damage starts there and may bleed downward.

## 11. Testing and quality

### 11.1 Gate

`just check`, which runs `scripts/check.sh` underneath:
- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

Claude runs it and reports real output.

### 11.2 Required tests

- **Per-effect unit tests.**
- **Round trips:** proptest round trips for the reversible effects, plus whole-file glitch-then-reverse yielding the identical file.
- **Header preservation:** across every fixture and effect.
- **Stuffing:** no unstuffed `0xFF` in safe-mode scans except the original RST markers.
- **Decoder:** no panics across a glitched corpus.
- **File safety:**
  - the input is never modified
  - an existing output is refused without `--force`
  - the input is refused even with `--force`, including through symlinks and hardlinks
- **Naming:** all the suffix cases.
- **Determinism:** BLAKE3 golden hashes for fixed seeds, via insta snapshots.

### 11.3 Fixtures

Generated procedurally, CC0, and committed. Variants:
- 4:2:0, 4:2:2, 4:4:4
- grayscale
- progressive
- restart intervals
- EXIF thumbnail
- trailing data
- `FF FF` fill

### 11.4 Contact sheets

`tools/contact-sheet` writes grid PNGs of parameter sweeps. Claude reads them to tune the defaults and shows Phil the ones where a choice is his.

### 11.5 Phil's sample photos

- Location: `samples/` at the repo root, git-ignored.
- Suggested contents:
  - Ten to fifteen images: portraits or skin, sky gradients, foliage or fine detail, night or low light, high contrast, signage or text.
  - Sizes: at least one full-resolution camera JPEG and one phone JPEG.
- File types:
  - JPEG matters now.
  - A few PNG, TIFF, DNG, WebP and AVIF files can wait in `samples/` for Phase 2.
- **iPhone:** photos default to HEIC, which is not in scope. Use "Most Compatible" in the camera settings, or export as JPEG.
- **Organization:** subfolders by type (`samples/jpeg`, `samples/dng`, ...). Descriptive file names help, for example `night-street.jpg`.

## 12. Repository and agent harness

- **Branches:**
  - `main` holds the bootstrap only.
  - Each phase gets its own branch.
  - Feature branches are cut from the phase branch and merged back by Claude once the gate is green.
  - Claude never pushes and never merges into `main`.
  - Conventional commits, with no AI attribution lines.
- **Phase boundary:** Claude stops and summarizes. Each summary covers:
  - what was built
  - real test output
  - deviations from the spec
  - open questions
  - next step
  - a heads-up on effort level for the next phase
- **CLAUDE.md** holds the operational rules. **VOCAB.md** holds the shared vocabulary and grows with the project.
- **CI** is deferred until a remote exists.

## 13. Open items

1. The default region skip and the per-effect defaults. Claude will report what tuning shows and ask where the choice is aesthetic.
2. An optional auth token for `serve`, to be decided at Phase 2.
