# Roadmap

Where glitcharaxia is going after the planned phases. `SPEC.md` section 10
holds the phases that were planned and built; this file holds everything
else: ideas not yet scheduled, things deliberately put off, and what is
needed before sharing the tool more widely. Updated 2026-11-25.

## Where it stands

Phases 1 to 8 are built and merged into `main` (2026-11-24): the CLI, the
web GUI and desktop app, 19 effects (audio-style, data, pixel and
adjustments), masks, presets, crop, work size, picking up old edits,
project files, and the pull effect with brushes and pins. Releases since:
0.2.0 (crop the result) and 0.2.1 (panning past the edges); see
`CHANGELOG.md`. It runs on Phil's Arch LXC behind a Cloudflare Tunnel with
Access, and Phil is using it for a week (from 2026-11-25), collecting notes
before more changes.

## Next round (after the week of use)

Agreed on 2026-11-25, to be done together with Phil's notes from the week.

- **Keep the token off the command line and out of the journal.** Today
  the systemd unit passes `--token=${GLITCHRX_TOKEN}`, so any user on the
  machine can read it with `ps` or in `/proc`, and the startup lines print
  the URL with the token, which the journal keeps. Plan: `serve` reads the
  token from the `GLITCHRX_TOKEN` environment variable (and a
  `--token-file`), the unit drops `--token=...`, and a token that came
  from there prints as `?token=...` (masked); only a token generated on
  the spot is printed in full, since there is no other way to learn it.
  Tests for each source; update `docs/hosting.md` sections 6 and 8. No
  change to the app. Plain text in a `chmod 600` env file stays: the
  server must hold the secret to compare it, and a random token is not a
  reused password, so hashing it buys little.
- **One set of setup steps across the docs.** Setting up prod from the
  README, `docs/hosting.md` and the rest felt scattered, with steps missing
  from each (Phil, 2026-11-25). Review every place that tells someone how
  to build, install, serve or host it (README, `docs/hosting.md`,
  `docs/macos.md`, `CLAUDE.md`, `justfile` comments, the CLI's own help),
  give each step one home, and have the others link to it instead of
  repeating it. Then walk a fresh Arch container through the hosting doc
  from top to bottom and fix whatever is missing: packages, users and the
  idmap, the bind mounts, the env file, the unit, the tunnel, updating and
  rolling back.

## For Phil before sharing

- **Rewrite the README in your own voice.** It is accurate but written as
  reference. A short "what this is and why I made it" at the top, a couple
  of example pictures, and how to run it would suit a first reader. The
  reference parts (effects, CLI flags, recipe format) can stay below.
- Decide how friends will reach it: the desktop app (see
  [macOS](#macos-build-for-friends)) or a hosted page (see
  `docs/hosting.md`).

## Candidates for the next phase

Unscheduled; pick any.

### 16-bit pictures

**What it is.** Today every picture is 8 bits per color channel (256
levels) inside the tool. 16-bit TIFFs and PNGs already open, but they are
reduced to 8 bits on the way in, and outputs are 8-bit.

**Why it matters.**
- Edits with many steps, especially levels and curves, keep smoother
  gradients with 65,536 levels than with 256 (less banding in skies).
- The byte effects would see different data: a 16-bit sample would line up
  with one 16-bit color channel, so `s16le` and friends would bend whole
  color values rather than pairs of neighboring bytes. That is a new
  look, not just more precision.
- Exports could be 16-bit TIFF and PNG, which Lightroom and Photoshop edit
  with more headroom.

**Your TIFFs.** Lightroom, Photoshop and most scanners write 16-bit TIFFs
by default, so yours may well be. To check, in a folder of them:

    exiftool -BitsPerSample *.tif

(`8 8 8` is 8-bit, `16 16 16` is 16-bit). Or, without exiftool, `file
*.tif` on Linux or Get Info / Preview's inspector on a Mac.

**Effort.** Large. The raster type, every effect, the golden hashes and the
encoders change. A phase of its own, probably split into a 16-bit pipeline
first (precision, 16-bit export) and the new byte look second.

### DNG export

**What it is.** DNG is Adobe's open raw format (2004), meant as a lasting,
documented alternative to each camera maker's own raw files. Some cameras
write it directly (Leica, Pentax, Ricoh, and phones through Apple ProRAW
and Android's camera API), and archives such as the US Library of Congress
list it as a preferred format for raw camera files. Most big camera makers
never adopted it, so "the future of raw" came true only in part, but it is
a well-supported, well-documented format that will stay readable.

**What it would mean here.** glitcharaxia's output is finished pixels, not
sensor data, so it would write a "linear DNG": in effect a TIFF with DNG
tags. Lightroom would treat it like a raw file (its raw sliders, profiles,
non-destructive edits), which is a nice way to keep working on a glitch.
For pure archiving it adds little over the TIFF export, which already
keeps every pixel.

**Effort.** Small to medium: the TIFF writer plus the DNG tags, checked in
Lightroom and with Adobe's DNG validator. A good fit together with 16-bit.

### BMP layout

When people databend in Audacity, they often open a BMP file. BMP stores
each pixel as blue, green, red (not red, green, blue) and the rows from the
bottom of the picture to the top. So an echo that runs "forward" through
the file runs upwards on the picture, and channel effects hit different
colors. A recipe setting `layout = "bmp"` would reproduce that look.
Small to medium effort; the main work is tests and making masks and the
crop still line up.

### Desktop keyboard shortcuts

Phil, 2026-11-24: define a sensible set once he has time at a desktop.
No action until then. Today there are undo and redo (Ctrl or Cmd with Z,
Y), `\` held for the original, `Z` to zoom, Esc to leave full view, and
the mask shortcuts (B, E, `[` and `]`, Enter, Esc, Delete).

### More Audacity effects

Phil wants to try nearly all of these (2026-11-25): build them, let him
use them, then decide together which to keep and which to sunset. Listed
by priority. Already built: Echo, Delay, Reverb, Amplify, Invert, a
bitcrusher, Reverse, cut and paste (shift, shuffle), Repeat (stutter),
Phaser, Wahwah, Distortion and Change Speed.

Rules for all of them: the raster's length never changes, and the result
is deterministic (D-25: basic f64 operations, our own sine and FFT, seeded
ChaCha8). Effects that change length in Audacity (Change Tempo,
Paulstretch, Sliding Stretch) work inside their window, as `speed` does.
Several need an FFT, ours, written once and shared; that is the biggest
single piece of work here.

**High priority: what the databending community uses**

Specced on 2026-11-25 (SPEC 6.7, D-71), not built: Bass and Treble and the
four pass filters (items 1 to 4), in a new Filters palette group.

1. **Bass and Treble.** Probably the most used after Echo: a low and a
   high shelf. Bass smears color in soft horizontal waves; treble
   sharpens and fringes edges. Cheap (two biquads).
2. **Low-pass filter.** Smears along the rows like a motion blur that
   wraps.
3. **High-pass filter.** Leaves edges and outlines on grey, close to an
   emboss.
4. **Band-pass and Notch filters.** The same family: keep or remove one
   band of detail. Possibly one `filter` effect with a type setting
   rather than four effects; decide when building.
5. **Filter Curve EQ / Graphic EQ.** A drawn frequency curve, the general
   form of all the above. Could reuse the curves editor.
6. **Change Pitch.** Keeps the length, so it fits as is. Repeats and
   squeezes detail with a seam pattern, different from speed. (FFT or
   a time-domain pitch shifter.)
7. **Paulstretch.** Extreme stretching: soft streaks and washes, popular
   for dreamy results. Needs the FFT.
8. **Noise Reduction.** Used for its artifacts: smeary, watercolor-like
   blocks. Needs the FFT and a "noise profile" (a region of the picture,
   or the whole of it).
9. **Tremolo.** A wave on the volume: regular bands of light and dark that
   drift across the rows. Audacity's waveforms: sine, triangle, sawtooth,
   inverse sawtooth and square (the square gives hard stripes).

**Second priority: recommended, less common in the community**

1. **Vocoder, with a second picture.** One photo's texture carried by
   another's shapes. The first two-input effect: it needs a way to pick
   the second picture (a recipe path, a project asset) and a home for it
   in `.glrx` projects.
2. **Ring modulation.** Multiplying by a carrier wave: hard, regular
   stripes and moire. (A Nyquist plug-in in Audacity, not built in.)
3. **Flanger and Chorus.** Short delays that sway: wavy, doubled ghosts,
   a cousin of phaser. (Plug-ins in Audacity.)
4. **Sliding Stretch.** Speed or pitch that changes along the window: a
   gradient from intact to smeared, which masks only approximate today.
5. **Change Tempo.** Stretch without changing pitch, inside a window.
   Close to Paulstretch at small amounts; try both and keep one if they
   look alike.
6. **Generate Noise, mixed in.** Grain in the data domain (white, pink,
   brown noise), seeded.
7. **Generate Tone, mixed in.** A stripe or moire pattern laid over the
   photo (sine, square, sawtooth, at a chosen period in rows or px).

**Low priority: rarely wanted results, listed for completeness**

- **Compressor and Limiter.** Mostly move contrast, which levels and
  curves do with more control; the compressor's attack and release give a
  smeared contrast along the rows that levels cannot.
- **Normalize and Loudness Normalization.** Stretch contrast to the full
  range; close to auto levels.
- **Fade In, Fade Out, Adjustable Fade.** A brightness ramp, mostly
  covered by a gradient mask with amplify.
- **Leveler.** A soft, gentle distortion; likely looks like `distort` at
  low settings.
- **Click Removal, Clip Fix, Repair.** Meant to smooth spikes; on a photo
  probably a faint softening. Try once.
- **Auto Duck, Truncate Silence.** Need a second track or change length;
  probably not worth adapting.

**Organizing the palette.** Today it has four groups (Audio, Data, Pixel,
Adjust) in rows of five. With up to twenty more effects, Audio alone
would run to five rows. Plan before building: split Audio into smaller
groups, for example

- *Time:* echo, delay, reverb, stutter, speed, pitch, tempo, Paulstretch,
  sliding stretch
- *Filters:* bass and treble, the pass filters, EQ, phaser, wahwah,
  flanger, chorus
- *Volume:* amplify, tremolo, ring modulation, distort, compressor,
  normalize, fade
- *Spectral:* noise reduction, vocoder
- *Generate:* noise, tone

plus Data, Pixel and Adjust as now. Possibly also groups that fold shut,
or the most used ones first. Mock it up with Phil before building.

**Sunsetting.** An effect taken out must still render in old recipes and
projects, byte for byte. So "sunset" means hidden from the palette (and
`--list`, perhaps behind a flag), not removed from the engine. Decide the
details before the first one goes.

### Smaller ideas

- Dragging mask parts from one step to another (copy and paste already
  works).
- Per-effect strength sliders (D-38 explains why opacity covers most of
  this today).
- Pull: build the displacement map on several cores. Resizing a brush whose
  strokes go back a long way rebuilds everything after its first stroke
  (about 0.4 s a frame for 40 strokes on a 12 MP photo). Each small step of
  a stroke reads the map as it was and writes afterwards, so its rows can
  be shared out between threads without changing a byte.

## Done or crossed off

- **App icon.** Phil likes it (2026-11-23). Revisit only when he wants to
  play with it again.
- **Recipe work size.** Built in Phase 5b (D-48).
- **Several hsl color ranges at once.** Built after Phase 6 (D-57).
- **A histogram in the levels editor.** Built as the classic levels view (D-60).

## Future (after the tool is shared)

### RAW import

Put off until well after sharing (Phil, 2026-11-23). There are dozens of
camera raw formats, and turning sensor data into a picture needs
demosaicing, white balance, color profiles and a tone curve: a raw
developer in its own right. Converting to DNG first does not remove that
step. JPEG, PNG, TIFF, WebP and AVIF cover the need for now; export a TIFF
from Lightroom to glitch a raw photo.

If it ever happens, the likely route is a library such as LibRaw (C, LGPL)
or `rawler` (Rust), with the tool's own determinism rules applying only
after the picture is developed.

### macOS build for friends

The full build steps are in `docs/macos.md`. For a close friend or family
member without an Apple developer account:

- **Building it on their own Mac** avoids every security prompt: an app
  you build yourself runs as is. Section 1 and 2 of `docs/macos.md` are
  about 30 minutes of setup.
- **Sending them the app** (AirDrop, a download, a USB stick) works too,
  but macOS marks it as downloaded and refuses to open it the first time.
  On macOS 15 (Sequoia) and later, right-click and Open no longer offers a
  way past this. They need to either:
  - open System Settings, Privacy and Security, scroll down, and click
    **Open Anyway** next to the message about glitchrx, then confirm; or
  - run, once, in Terminal:

        xattr -dr com.apple.quarantine /Applications/glitchrx.app

  Before sending a build, make the static libjpeg-turbo build described in
  `docs/macos.md` section 6, or the app will only run on a Mac with
  Homebrew's `jpeg-turbo` installed.
- **Without any warnings at all** needs the Apple Developer Program (99 USD
  a year), signing and notarization: section 5 of `docs/macos.md`. Only
  worth it for a wider release.

### Public hosting

Steps, hardware and the Cloudflare Tunnel question are in
`docs/hosting.md`. In short: fine for a close circle soon, with a locked
down container and Cloudflare Access; a public site needs a "public mode"
built first.
