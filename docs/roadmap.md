# Roadmap

Where glitcharaxia is going after the planned phases. `SPEC.md` section 10
holds the phases that were planned and built; this file holds everything
else: ideas not yet scheduled, things deliberately put off, and what is
needed before sharing the tool more widely. Updated 2026-11-24.

## Where it stands

Phases 1 to 7 and most of Phase 8 are built and merged into `main`: the
CLI, the web GUI and desktop app, 19 effects (audio-style, data, pixel and
adjustments), masks, presets, crop, work size, picking up old edits,
project files and the pull effect. The rest of Phase 8 (pull speed, and
brushes with strokes and pins in one list) waits on `phase-8-pull`.

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

**What it is.** Today every picture is 8 bits per colour channel (256
levels) inside the tool. 16-bit TIFFs and PNGs already open, but they are
reduced to 8 bits on the way in, and outputs are 8-bit.

**Why it matters.**
- Edits with many steps, especially levels and curves, keep smoother
  gradients with 65,536 levels than with 256 (less banding in skies).
- The byte effects would see different data: a 16-bit sample would line up
  with one 16-bit colour channel, so `s16le` and friends would bend whole
  colour values rather than pairs of neighbouring bytes. That is a new
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
colours. A recipe setting `layout = "bmp"` would reproduce that look.
Small to medium effort; the main work is tests and making masks and the
crop still line up.

### Desktop keyboard shortcuts

Phil, 2026-11-24: define a sensible set once he has time at a desktop.
No action until then. Today there are undo and redo (Ctrl or Cmd with Z,
Y), `\` held for the original, `Z` to zoom, Esc to leave full view, and
the mask shortcuts (B, E, `[` and `]`, Enter, Esc, Delete).

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
- Pull: a "Show markers" switch, if stroke markers crowd the photo.

## Done or crossed off

- **App icon.** Phil likes it (2026-11-23). Revisit only when he wants to
  play with it again.
- **Recipe work size.** Built in Phase 5b (D-48).
- **Several hsl colour ranges at once.** Built after Phase 6 (D-57).
- **A histogram in the levels editor.** Built as the classic levels view (D-60).

## Future (after the tool is shared)

### RAW import

Put off until well after sharing (Phil, 2026-11-23). There are dozens of
camera raw formats, and turning sensor data into a picture needs
demosaicing, white balance, colour profiles and a tone curve: a raw
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
