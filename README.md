# glitchRx (glitcharaxia)

Create your own glitch art from digital images through databending. Inspired by my previous workflow for glitch art: importing a photo's raw data into Audacity pixels as if they were audio, running audio effects over them (echo, reverb, delay, amplify &c.) and turning the result back into an image. This old process was arduous, non-responsive, and if one wasn't careful: destructive. I've had this idea for years now, to create a glitch-specific photo editor that uses the same effects from Audacity but with a live visual editor so you can see in real time what the distan different settings and encodings do. Previously in audacity, all you would see was a sound wave. The glitch artist would have to take a wild guess at settings, export and add the file extension, and then open in an image viewer before you could see the result. With the help of Claude, I created a photo editor to replicate this workflow but also added the features listed below to overcome hurdles I'd often run into.

### glitchRx:

- never modifies your original
- is repeatable: the same seed gives the same result, byte for byte
- previews live, fast enough to use from a phone. This was developed outside of my home network on a WireGuard VPN, accessing and editing remotely was a breeze.
- keeps your photo's size, orientation and colour profile

Everything runs locally. No image is ever uploaded anywhere besides the server or computer you host it on

**Status: beta (0.1.0).** Three ways to use it, all from one build:

- **A web GUI** (`glitchrx serve`) for a browser on the same computer, or a phone or tablet over your LAN or a VPN. It can be added to a phone's home screen and used like an app.
- **A desktop app** (Linux today; macOS planned) with the same GUI in its own window.
- **A command-line tool** (`glitchrx`) for scripts and batches.

It reads JPEG, PNG and TIFF photos and its own project files (`.glrx`), and writes TIFF (the default), PNG, JPEG, WebP and AVIF. There are 20 effects: 14 audio and data effects that read the pixels as sound, pixel sorting, a hand-drawn pull, and four photo adjustments.

**One engine, three ways in.** All three use the same engine, so every effect, format and safety rule is the same everywhere, and an export from the GUI is byte for byte what the command line writes (the tests check this). Recipes and projects move freely between them: make something in the GUI, replay it with `glitchrx --recipe`, open the project on another device. The differences are in how you work, not in what comes out:

- The command line cannot draw: masks and pulls are made in the GUI, and the command line replays them exactly from a recipe or project.
- The desktop app uses the system's Open and Save dialogs instead of the in-app folder browser.
- Appearance, export format and brush settings are remembered per browser or device. Presets are kept per machine; they are plain recipe files, so copy them to share.

## Contents

- [Install](#install)
- [Quick start](#quick-start)
- [Using the GUI](#using-the-gui)
- [Pull: moving pixels by hand](#pull-moving-pixels-by-hand)
- [Projects](#projects)
- [Command line](#command-line)
- [Encodings, sizes and effects](#encodings)
- [Recipes](#recipes)
- [Running it on a server or for a phone](#running-it-on-a-server-or-for-a-phone)
- [Desktop app](#desktop-app)
- [How it works](#how-it-works)
- [Safety rules](#safety-rules)
- [Privacy](#privacy)
- [Development](#development)

## Install

There are no ready-made downloads yet: you build it from source. It takes a few minutes and about 2 GB of memory; the result is a single program that needs nothing else to run but libjpeg-turbo.

### Requirements

- **Rust**, stable, 1.88 or newer: your distribution's package if it is new enough (Arch's is), otherwise [rustup](https://rustup.rs), then `rustup default stable`.
- **Node.js and npm** (a current LTS), only to build the GUI. Without them you get the command line alone.
- **libjpeg-turbo** with its TurboJPEG headers.
- **git**, and optionally [just](https://github.com/casey/just), which runs the build steps below by name.

| System | Install |
| --- | --- |
| Arch Linux | `sudo pacman -S --needed base-devel git rust nodejs npm libjpeg-turbo just` |
| Debian, Ubuntu | `sudo apt install build-essential git pkg-config nodejs npm libturbojpeg0-dev`, then rustup from rustup.rs |
| Fedora | `sudo dnf install gcc git pkgconf nodejs npm turbojpeg-devel`, then rustup |
| macOS, Windows | rustup and Node.js; build with `--features glitchrx-core/vendored` (below), which builds libjpeg-turbo itself and needs cmake and nasm |

Distribution Node.js packages are sometimes old; if the GUI build complains, install a current LTS from nodejs.org or with a version manager.

### Build

```
git clone <this repository's URL> glitchrx
cd glitchrx
just build-all                       # the GUI, then the program with the GUI inside
cp target/release/glitchrx ~/.local/bin/     # optional: put it on your PATH
```

Without `just`:

```
cd web && npm ci && npm run build && cd ..
cargo build --release -p glitchrx-cli
```

The program is `target/release/glitchrx`. The GUI is built into it, so it runs anywhere you copy it (with libjpeg-turbo installed). If you build without Node.js, `glitchrx serve` shows a page explaining how to add the GUI.

On macOS or Windows without a system libjpeg-turbo, add `--features glitchrx-core/vendored` to the cargo command.

### Update

`git pull` alone does not change the program you run: the GUI is built into it, so build again after pulling.

```
git pull && just build-all
```

Recipes and projects made with an older version always open in a newer one.

## Quick start

```
glitchrx serve
```

Open `http://127.0.0.1:7878` in a browser, then:

1. **Open** a photo (from this device, or from a folder on the computer once you choose one in Settings).
2. Pick an effect from the palette (on a phone, behind the floating **+**), say **echo**. The preview updates as you move its sliders.
3. Add more steps; they run top to bottom. Switch one off to compare, or hold the photo to see the original.
4. **Export** to download the picture, or save it beside the original. Your original is never changed.

To use it from your phone, see [Running it on a server or for a phone](#running-it-on-a-server-or-for-a-phone).

## Using the GUI

### The screen

- **Top bar:** Open, Presets, Undo, Redo, Export, Settings. While you paint a mask or pull, it becomes an edit bar instead: what you are working on, compare, undo, redo and **Done**.
- **The photo,** with a small toolbox: zoom, crop, and the work size.
- **The steps:** a card per effect, top to bottom, in the order they run. On a phone the effect palette sits behind the floating **+** button.
- **Status bar:** the photo's name and size, how long the last render took, and whether the view is approximate or exact.

Icon-only buttons show their name when you hold them on a phone (with a mouse, hover).

### Photos

- **Open** a photo from this device (it goes to your own server, nowhere else) or from the server's library folder, chosen in Settings or with `--root`. On an iPhone, a HEIC photo arrives as a JPEG.
- If there are already steps, you are asked whether to keep them or start fresh (undoable; "Don't ask again" can be changed in Settings).
- **Continue from a saved recipe.** Recipes saved beside an export remember the photo they were made on. Opening that photo again offers to continue from the newest one; Export, then "Saved on the server", lists them all.

### Steps

- **Encoding** for the whole recipe (and mono, stereo or RGB) sits above the steps. Any step can override both. See [Encodings](#encodings).
- **Add steps** from the palette, grouped into audio effects (echo, delay, reverb, phaser, wahwah, amplify, distort, speed, stutter), data effects (invert, bitcrush, backwards, shift, shuffle), pixel effects (pixelsort, pull) and adjustments (levels, curves, hsl, balance).
- Each step can be switched off, moved up or down, duplicated or removed (the icons at the foot of its card).
- **Sliders** mark their default with a small tick and snap onto it; double tap a slider to go back to the default. Fields without a slider have a reset arrow.
- **Shake** (the shuffle arrows at the top of a step's settings) rolls its settings at random, within the useful range; placement and encoding stay.
- **Placement:** every effect starts on the whole picture. The **Bands** switch scatters it in seeded bands instead and shows `hits`, `len` and the seed. Dragging the region's two handles shades the preview outside the band. **Re-roll** (the dice beside the seed) picks a new seed, which moves where the bands land; it is greyed out when nothing is random.
- **Edit mode:** hold a step's header to see every step on one line. Tap the circles to select several, then re-roll, duplicate or remove them together; drag the grip on the right to reorder. Hold a row (or tap Done) to go back. With nothing selected, the dice re-rolls every step.

### Blend and masks

Every step has an **opacity**, and can have a **mask** that limits where it shows. A mask is built from components:

- **Brush** strokes (paint and erase).
- **Linear** gradients and **radial** circles (hard-edged; feather fades them).
- **Rectangles** that resize, turn and move.
- **Pen** paths: tap out points, drag while placing one to curve the sides, tap the first to close. Points stay editable.
- **Brightness ranges** that limit the mask to light or dark parts.

Tap a component to make it active; a radial, linear or rectangle then shows handles on the photo, and the others show small pins to tap. Each component adds, subtracts or intersects, and has its own opacity and feather; long-press to select several and set them together. Copy, Cut and Paste move components between steps. **Paint in** shows the step only inside the mask; **Paint out** everywhere else. The red overlay shows the mask; Done ends editing. On a keyboard, B and E paint and erase, and `[` and `]` size the brush. Masks are saved as strokes and shapes, so they replay exactly at full resolution.

### Presets

Presets (the bookmark) are your own saved recipes, kept by the server in `~/.config/glitchrx/presets` (`--presets DIR` moves them), so your phone and computer share them. Use one to replace the steps, or add its steps to the current recipe: they stay together under the preset's name and move as a block. Once you change one, the group shows Modified, with Revert and Update preset. They are ordinary recipe files: `glitchrx --recipe ~/.config/glitchrx/presets/NAME.toml photo.jpg`.

### Viewing

- **Compare:** press and hold the photo (or the `\` key) to see the untouched original. While masking or pulling, the photo is for drawing, so hold the compare button in the top bar instead.
- **Zoom** with a pinch, ctrl + wheel or a trackpad pinch; double-tap, tap the magnifier or press Z for 100% and back. Zoomed in, drag (or scroll) to pan. Once the view holds still, what is on screen is cut from the full-resolution render, so it is sharp and exactly the export.
- **Crop** with the button beside the zoom: free, or at the photo's own shape, 1:1, 4:5 or 16:9, turned with the portrait and landscape switch. **Crop the: Photo | Result** decides when it happens. *Photo* crops first and glitches what is left, so the glitch is made for the crop (and changes with it). *Result* glitches the whole photo and then crops, so you frame the glitch you already have without changing it. Masks and pulls stay on the same part of the photo either way.
- **Work size** (Full, 4K, 2K, 1K) runs the steps on a smaller copy: the glitch texture gets coarser and everything is faster. Export writes that size, or enlarges it back to the photo's size with hard pixel edges.
- **Undo and redo** with the buttons, or Ctrl+Z and Ctrl+Shift+Z. History holds recipes, not images, so it costs almost nothing.

While you change things, the preview runs the recipe on a copy of the photo shrunk to your screen, so even a 26 MP photo answers quickly. That copy is close to the export but not identical, because pixel-level texture cannot shrink with the picture; once you stop for a moment the preview settles to an exact render, which is exactly what Export gives. The status bar says which you are looking at. A live preview that takes over 3 seconds stops with a "too heavy to preview live" notice; export still works.

### Export

- **Name** at the top: one name for every way of saving. It suggests the next free name (`photo-glitch`, `photo-glitch-1`, ...) until you type your own.
- **Picture or Project.** A picture is the finished image; a [project](#projects) keeps the original and every step to open again later.
- **Formats:** TIFF, PNG, JPEG (with quality), WebP (quality, or lossless) or AVIF (quality and speed; slow). Your choice is remembered on each device. Lossy formats smooth fine glitch texture a little.
- **Download** to this device. On a phone, the **Save** button in the status bar then opens the share sheet (Save Image, Save to Files, AirDrop).
- **Save next to original** puts it beside the photo in the library, under the name you typed, adding `-1`, `-2` if that is taken.
- **Save As** into any folder in the server user's home folder. **New folder** makes one on the spot.
- **Also save the recipe beside it** writes the steps as a small `.recipe.toml` next to a picture, so you can continue later.

Nothing is ever written over, except a project you opened and chose to save over, after a confirmation.

### Settings

The gear: light, dark or system appearance; the **library folder** (what Open browses) and the **export folder** (where Save As starts), picked from the server user's home folder and kept in `~/.config/glitchrx/config.toml` for every device; and what opening another photo does with the steps.

## Pull: moving pixels by hand

**pull** is a step you draw instead of set: drag on the photo and the pixels under your finger come along, as Photoshop's Liquify does. Like any step it switches on and off, takes a mask and an opacity, and moves in the stack. Adding one starts pulling straight away.

Its card has two tools, **Draw** and **Pins**. Tapping one starts pulling with it, and while the photo is taking your strokes that tool turns green; **Done** in the top bar stops. One finger draws, two fingers zoom and pan.

- **Draw:** drag to pull. Strokes belong to **brushes**: your first stroke starts one and selects it, and the strokes after it join it, with its size and strength, until you tap **New**. Each stroke has a small marker where it ends; tap one to select its brush, and Size and Strength then change all of its strokes together. The **Markers** switch hides them.
- **Pins:** a grid lies over the photo (its spacing, and dots, lines or nothing, are settings). Drag a grid point to pick it up and put it down elsewhere; the picture stretches smoothly around it. Drag a pin again to move it further or change its direction, or back home to remove it. The selected pin is green and larger, and Size changes it alone.
- **Order:** whatever you do last goes on top. A stroke drawn after moving pins goes over them, and a pin you drag again comes to the top.
- **Delete this brush** or **Delete this pin** appears while one is selected. Each stroke or move is one undo.
- **Sampling:** `pixels` (the default) moves whole pixels with hard edges; `smooth` blends between them.
- On a keyboard, `[` and `]` size the selection (or the next stroke), Delete removes it, and Enter or Esc finish.

Strokes are saved in the recipe as positions on the photo, so they replay exactly at full resolution and stay put when you crop.

## Projects

A project (`.glrx`) is to glitchRx what a `.psd` is to Photoshop: the original photo, every step and a preview in one file, to open again on any device and carry on. Choose **Project** in Export; open one with **A project** in Open, or from the library, where projects show their preview.

- It is an ordinary zip file: anyone can unzip it and find the photo, the recipe and a JPEG preview.
- The photo's **location is removed** as it goes in (the GPS data, as a phone does when sharing); camera, date, orientation and colour profile stay. The image data itself is untouched, so the recipe replays to the same bytes.
- A project you opened from the library can be saved over (after a confirmation) or saved as a new one. A picture exported from a project takes the project's name.
- From the command line: `glitchrx edit.glrx out.png` renders a project with its own steps, and `--project` writes one beside a picture.

## Command line

```
glitchrx --echo photo.jpg                       # writes photo-glitch.tif
glitchrx --echo photo.jpg                       # again: photo-glitch-1.tif
glitchrx --echo photo.jpg out.png               # explicit output; its extension sets the format
glitchrx --echo --format jpeg photo.jpg         # photo-glitch.jpg
glitchrx --echo --format webp --quality 85 photo.jpg   # smaller files; --lossless keeps every pixel
glitchrx --echo --format avif --avif-speed 8 photo.jpg # smallest; slow to encode
glitchrx --echo delay=0.33rows,decay=0.6 --reverb photo.jpg
glitchrx --echo --encoding ulaw photo.jpg       # read the pixels as mu-law audio
glitchrx --shift by=0.5rows,phase=2,hits=12 photo.jpg
glitchrx --pixelsort low=0.3 photo.jpg          # melt the bright areas into streaks
glitchrx --echo opacity=0.5 photo.jpg           # blend the step at half strength
glitchrx --levels black=20,white=230 --hsl saturation=-40 photo.jpg   # photo adjustments
glitchrx --curves points=0:20/128:150/255:235,channel=blue photo.jpg
glitchrx --shift --crop 0.1,0,0.8,1 photo.jpg   # crop first: X,Y,W,H as shares of the photo
glitchrx --shift --crop 0.1,0,0.8,1 --crop-result photo.jpg   # glitch the whole photo, then crop
glitchrx --pixelsort --work-size 1024 photo.jpg # run on a 1024 px copy: coarser, faster
glitchrx --shift --work-size 1024 --scale-back photo.jpg   # ...enlarged back, hard pixel edges
glitchrx --recipe moody.toml photo.jpg          # replay a saved recipe (a pull is drawn in the GUI and replayed this way)
glitchrx --recipe moody.toml --project photo.jpg   # also write photo-glitch.glrx
glitchrx edit.glrx out.png                      # render a project with its own steps
glitchrx --stutter --batch *.jpg                # every file, automatic names
glitchrx --info photo.jpg                       # format, size, orientation, colour profile
glitchrx --list                                 # every effect and parameter
```

Effects run in the order you give them, and each takes optional `key=value` parameters. Every run prints the output name, its size and the seed:

```
photo-glitch.tif  (6240 x 4160, tiff, seed 81723)
```

Re-run with `--seed` to reproduce a result you liked. `--save-recipe` keeps the whole chain in a small TOML file, and `--recipe` replays it later. `glitchrx --help` lists every option.

The output is TIFF by default: uncompressed 8-bit RGB, ready for masking in Photoshop. The colour profile of the original is written into every output, so colours match. AVIF is the exception: it can only mark sRGB and Display P3 (most phone photos are one of the two), and warns when it drops another profile. The same input, recipe, seed and settings give the same file every time, on every machine.

## Encodings

Audacity's Import Raw Data dialog asks how to read the bytes as samples. That choice changes the look more than any effect parameter, so it is a setting here too: `--encoding` for the whole chain, or `encoding=` on one effect (in the GUI, above the steps, or in a step's Samples section).

| Encoding | Look |
| --- | --- |
| `u8` (default) | Clean ghosts; colours rotate through the channels |
| `s8` | Like `u8`, but highlights clip and invert |
| `s16le`, `s16be` | Ghosts under heavy rainbow grain; byte order changes the palette |
| `s24le`, `s24be` | One sample per pixel; posterized two-tone fields |
| `ulaw`, `alaw` | Posterized neon blocks with hard edges |

`--channels 2` reads the bytes as stereo, as Audacity did when you imported stereo: alternate samples are left and right, and each side is processed on its own. `--channels 3` does the same with three channels; with `u8` those are exactly red, green and blue, so filters that blend neighbouring samples (`wahwah`, `reverb`) keep the colours instead of turning them grey.

**Endianness.** In Audacity you chose little or big endian when importing raw data. That only matters when one sample is two or more bytes: it decides whether the first byte of each pair is the loud (high) or the quiet (low) part. With `s16le` the green and blue bytes of a pixel tend to be the loud parts; with `s16be` it flips. Neither is wrong; they are different palettes.

### Sizes

Every pixel is three bytes (R, G, B), and rows follow each other, so a distance in the data is a distance in the picture. Sizes take units: `rows` (`0.33rows` is a third of the width, sideways; `6rows` is six rows down), `px`, `%` of the image (which runs top to bottom, so `40%:60%` is the middle band), or plain bytes.

A distance of whole pixels keeps colours. `phase=1` or `phase=2` adds one or two bytes, which shifts red into green into blue: the colour fringes of databent images.

### Effects

| Effect | Audacity counterpart | What it does |
| --- | --- | --- |
| `echo` | Echo | Fading repeats that feed back on themselves |
| `delay` | Delay | A fixed number of crisp repeats |
| `reverb` | Reverb | A haze of overlapping ghosts (Freeverb) |
| `amplify` | Amplify | Gain in dB, with clipping |
| `invert` | Invert | Flips samples; close to a negative with `u8` |
| `bitcrush` | (plug-ins) | Fewer levels per sample: banding |
| `backwards` | Reverse | Mirrors stretches of the picture |
| `shift` | cut and paste | Slides bands sideways |
| `shuffle` | cut and paste | Swaps blocks around |
| `stutter` | Repeat | Repeats a strip down the picture |
| `phaser` | Phaser | Combed, wavy bands from swept all-pass filters |
| `wahwah` | Wahwah | A swept resonant filter: pulsing smears and ringing fringes |
| `distort` | Distortion | Hard clip, soft saturation, or folding highlights into bands |
| `speed` | Change Speed | Squeezes or stretches parts of the picture: shears |
| `pixelsort` | (pixel effect) | Sorts runs of pixels by brightness, hue or saturation: streaks down or across the picture |
| `pull` | (pixel effect) | Drags pixels along with your finger, and moves grid points as pins ([Pull](#pull-moving-pixels-by-hand)) |
| `levels` | (adjustment) | Black point, white point and midtone gamma |
| `curves` | (adjustment) | A tone curve through up to 16 points, `points=0:0/64:52/192:204/255:255`; the GUI has a curve editor |
| `hsl` | (adjustment) | Hue turn, saturation and lightness, for every colour or some (`range=reds`, `range=reds+blues`, ...) |
| `balance` | (adjustment) | Tints the shadows, midtones or highlights, keeping brightness |

`pixelsort`, `pull` and the adjustments are not audio: they work on whole pixels, so the encoding and channels do not apply. Adjustments are ordinary steps, so masks and opacity limit them like any other. Every step also takes `opacity` (0 to 1), which blends its result over the picture it received. Where an effect lands is set by `region`, `hits` and `len`, shared by every audio and data effect.

Every parameter is documented in `docs/effects.md`, which is generated from the code, and by `glitchrx --list`.

## Recipes

A recipe is the whole description of a glitch: the steps, their settings and seeds, the crop and work size, masks and pulls, in a short TOML file you can read and edit by hand. The same recipe on the same photo always gives the same bytes. The GUI saves recipes beside exports and as presets; the command line writes them with `--save-recipe` and replays them with `--recipe`.

```toml
format = "glitcharaxia-recipe"
version = 8
encoding = "u8"
channels = 1

[[step]]
effect = "echo"
effect_version = 1
seed = 81723
delay = "0.33rows"
decay = 0.5
```

Every newer version opens older recipes and projects unchanged (and with the same picture). An older version refuses a newer recipe rather than quietly give a different result. Version 1 recipes, from an early design that glitched JPEG data, replay only at the git tag `archive/jpeg-bytes`.

## Running it on a server or for a phone

```
glitchrx serve                                   # http://127.0.0.1:7878, this machine only
glitchrx serve --root ~/Pictures                 # browse this folder, whatever Settings says
glitchrx serve --host 0.0.0.0 --token --root ~/Pictures   # from a phone over LAN or WireGuard
```

- `--host 0.0.0.0` listens on every network interface and prints a URL for each address.
- **Use `--token`** whenever anything but this computer can reach it: there are no accounts, so anyone who can reach the port can use the server, and through it read and write in the server user's home folder. `--token` adds a generated secret to the printed URL; open that URL once per browser and a cookie keeps you signed in. `--token=SECRET` sets your own, so the URL stays the same across restarts. A browser without the cookie gets a sign-in page where you can paste it.
- **Reach it over your own network or a VPN** such as WireGuard. Do not forward the port to the internet; `docs/hosting.md` covers sharing it with friends safely.
- **Folders:** `--root` fixes the folder Open browses. The folder pickers in Settings and Save As only reach folders inside the home folder of the user running the server.
- **On a phone, add it to the home screen** (Safari: Share, then Add to Home Screen). It opens full screen with its own icon, like an app; it may ask for the token once, since it keeps its own cookies.
- The server keeps the four most recently used photos in memory; give it about 2 GB for large photos.

`docs/hosting.md` has a worked example of running it as a service in a Linux container, with your photo library mounted, and updating it with `git pull`.

## Desktop app

The same GUI in a native window, for working on one computer. It runs its own copy of the server on `127.0.0.1` (a free port, a new token each launch), so everything behaves as in the browser, except that **Open** and **Export** use the system's file dialogs, and double-clicking a project opens it.

Linux (needs `webkit2gtk-4.1`; on Arch: `pacman -S webkit2gtk-4.1`):

```
scripts/install-desktop.sh            # or: just install-desktop
scripts/install-desktop.sh --remove   # or: just uninstall-desktop
```

This builds the app and installs it for your user only: `~/.local/bin/glitchrx-desktop`, a launcher entry, icons and the project file type under `~/.local/share`. macOS is planned; the build steps are in `docs/macos.md`.

## How it works

1. **Decode** the photo to a *raster*: R, G, B for each pixel, row after row, exactly what an uncompressed TIFF stores. A sideways phone photo is turned upright first, so rows are the rows you see.
2. **Bend.** Each audio or data effect reads a stretch of those bytes as audio samples in its encoding, processes them in floating point the way Audacity would, and clips the result back to bytes. Clipping is where the bright, saturated colours come from. Pixel effects and adjustments work on whole pixels instead; a pull builds a displacement map from its strokes that says, for every pixel, where to read from.
3. **Encode** the raster as TIFF, PNG, JPEG, WebP or AVIF, with the original colour profile.

The raster always keeps its length, so the image keeps its dimensions. The math uses only basic floating-point operations, which give identical results on every machine, and randomness comes from the seed alone, so a recipe reproduces the same bytes everywhere.

**Earlier: JPEG byte glitching.** Until SPEC v0.3, glitchRx glitched the *compressed* data inside a JPEG. That gives flat colour bands, not the databent look, and was retired. It is kept at the git tag `archive/jpeg-bytes`.

## Safety rules

- **Your original is never written,** even with `--force`, and also through symlinks and hard links.
- **Existing files are never overwritten** unless you pass `--force` (or confirm in the GUI). The one exception is a project you choose to save over.
- **Output names continue a sequence:** `-glitch`, `-glitch-1`, `-glitch-2`. Bending `photo-glitch-1.tif` produces the next free `photo-glitch-N.tif`, not `photo-glitch-1-glitch.tif`. The numbers are shared across formats.
- **Exit codes** of the command line: 0 success, 1 runtime error, 2 bad command line, 3 refused (for safety), 4 unsupported file (not a JPEG, PNG, TIFF or project).

## Privacy

- glitchRx makes **no outgoing network connections**, ever. `glitchrx serve` only answers the browsers that connect to it.
- Every font, script and icon of the GUI is built into the program (body text uses the device's own system font), and a Content-Security-Policy stops the page from loading anything from elsewhere. No analytics, no tracking.
- **Exports carry no EXIF:** no GPS position, camera serial or date leaves with a picture. The colour profile is kept.
- **Projects** keep the photo's metadata except its location, which is removed.
- Photos opened from a phone live in the server's memory only, unless you save them.
- Test images in this repository are generated procedurally. Personal sample photos belong in `samples/`, which git ignores.

## Development

```
just check        # fmt, clippy (warnings denied), all tests; or scripts/check.sh
just web          # install web deps and build the GUI into web/dist
just web-check    # svelte-check and vitest (also part of `just check` once web deps exist)
just web-dev      # GUI with hot reload on :5173, proxying /api to `glitchrx serve` on :7878
just fixtures     # regenerate tests/fixtures (procedural, CC0)
just sheets       # parameter-sweep contact sheets into out/sheets
```

Layout:

- `crates/glitchrx-core`: the engine (decoding, effects, masks, pull, recipes, projects)
- `crates/glitchrx-cli`: the `glitchrx` command
- `crates/glitchrx-serve`: the web server behind `glitchrx serve`
- `crates/glitchrx-desktop`: the desktop app (Tauri), which embeds that server
- `web/`: the Svelte GUI, embedded into the program at build time
- `tools/`: fixture generator, contact sheets
- `docs/`: effect reference, roadmap, hosting, macOS build, decision records

`SPEC.md` describes how everything behaves and why, with every design decision dated; `VOCAB.md` defines the terms. Plans and ideas are in `docs/roadmap.md`.

## License

GPL-3.0-or-later. See `LICENSE`.
