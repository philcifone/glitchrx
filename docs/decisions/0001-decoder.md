# 0001: JPEG decoder

Date: 2026-11-21. Status: accepted.

## Context

Glitched JPEGs are malformed on purpose. The tool needs a decoder that:

- returns pixels for damaged files instead of refusing them
- renders damage the way other people's viewers will
- decodes fast at reduced size, for GUI previews

## Options considered

- **libjpeg-turbo** through its TurboJPEG 3 API (`turbojpeg-sys`, raw bindings).
- **The `turbojpeg` crate** (a high-level wrapper over the same library).
- **`mozjpeg`**: Mozilla's fork of libjpeg-turbo.
- **`zune-jpeg`**: pure Rust.

## Decision

Use libjpeg-turbo through `turbojpeg-sys`, with a small safe wrapper in `glitchrx-core/src/turbo.rs`. On Linux it links the system library through pkg-config. The `vendored` cargo feature builds the bundled source with cmake, for macOS and Windows.

## Reasons

1. **It matches the audience's viewers.** Firefox, Chromium and most desktop viewers decode JPEG with libjpeg-turbo, so its rendering of corrupt data is what people will actually see.
2. **It is tolerant in the way we need.** It treats corrupt entropy data as a *warning*, keeps decoding and fills what it cannot read. A fatal error leaves the rows decoded so far in the buffer, and the wrapper detects those through the alpha channel.
3. **Measured robustness.** Over the glitched corpus in `crates/glitchrx-core/tests/fixtures.rs`:
   - Setup: 9 fixtures x 8 effects x (1 safe + 2 raw) runs, each decoded at full and 1/8 scale, so 432 decodes.
   - Outcome: 430 recovered, 2 clean, 0 partial, 0 failed, 0 panics.
4. **Speed.**
   - The DCT-domain scaling (1/2, 1/4, 1/8) suits GUI previews.
   - A 24 MP glitch, including the 1/8-scale verification decode, runs in 0.13 s.
   - The same run with a full-resolution PNG bake takes 0.35 s.
5. **Raw bindings over the high-level crate.** We need `TJPARAM_STOPONWARNING = 0` and the warning-versus-fatal distinction. The raw API exposes both directly, and the wrapper is about 100 lines.

## Not done, and why

- **`mozjpeg`** decodes like libjpeg-turbo (it is a fork) but needs nasm to build, and it is not what browsers ship.
- **`zune-jpeg`** was not benchmarked. Its error handling differs from what browsers do, which defeats reason 1. It stays the candidate if a pure-Rust or WASM build is ever needed (Phase 2+).
- **Visual parity** with Firefox and Chromium was not measured pixel by pixel. It rests on both browsers using libjpeg-turbo. Phil can check it by opening any `-glitch.jpg` in Firefox next to its `--bake` PNG.

## Consequences

- Building needs libjpeg-turbo development files. On Arch Linux it is the `libjpeg-turbo` package, which is normally already installed. On Debian and Ubuntu it is `libturbojpeg0-dev`.
- The bindings target TurboJPEG 3.1. The system library here is 3.2.1, and the 3.x API is stable.
