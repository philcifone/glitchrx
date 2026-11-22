# Building the desktop app on macOS

Roadmap for 2e on Apple Silicon (an M4). Not done yet; these are the steps
and the decisions to make when it is. Nothing in the code is Linux-only
except `scripts/install-desktop.sh`; on a Mac the system web view is
WebKit (WKWebView), which ships with macOS, so there is no `webkit2gtk`.

## 1. One-time setup

1. **Apple's command line tools** (compiler, linker, `iconutil`). Full
   Xcode is not needed:

       xcode-select --install

2. **Homebrew** (https://brew.sh), then the tools and the one C library
   glitchrx links:

       brew install node just pkg-config jpeg-turbo

   Homebrew's `jpeg-turbo` is "keg-only" (not on the default paths), so
   tell `pkg-config` where it is, for example in `~/.zprofile`:

       export PKG_CONFIG_PATH="$(brew --prefix jpeg-turbo)/lib/pkgconfig"

3. **Rust** via rustup (https://rustup.rs). It picks the native target,
   `aarch64-apple-darwin`.

4. **Tauri's CLI**, which turns the binary into a `.app` and a `.dmg`:

       cargo install tauri-cli --version "^2" --locked

## 2. Check that it builds

    git clone <repo> && cd glitcharaxia
    just web            # builds web/dist, which the binary embeds
    just check          # the gate, desktop crate included
    just desktop        # runs a debug build in a window

## 3. Icon

macOS wants one `icon.icns` holding every size. Tauri's CLI makes it from
the 512 px tile, then add `icons/icon.icns` to `bundle.icon` in
`crates/glitchrx-desktop/tauri.conf.json`:

    cd crates/glitchrx-desktop
    cargo tauri icon icons/icon.png --output icons

That command also rewrites the PNGs and adds Windows sizes; keep only
`icon.icns` (`git checkout icons/*.png`, delete the rest). Apple's own
icons sit inside a rounded square at about 80 % of the canvas with a soft
shadow; ours is a plain rounded tile, which is fine for now (the icon is
due another pass anyway).

## 4. Build the app

    cd crates/glitchrx-desktop
    cargo tauri build --bundles app,dmg

Results, under `target/release/bundle/`:

- `macos/glitchrx.app`: drag it into `/Applications`.
- `dmg/glitchrx_0.1.0_aarch64.dmg`: the usual installer disk image.

## 5. Opening it: Gatekeeper and signing

Every Apple Silicon binary must carry a signature; the build gives it a
free "ad-hoc" one automatically. That is enough for your own Mac:

- An app you built yourself runs as is.
- A copy that arrived by AirDrop, a download or a USB stick is
  "quarantined", and macOS refuses it ("cannot be opened" or "damaged").
  Clear the flag once:

      xattr -dr com.apple.quarantine /Applications/glitchrx.app

  or right-click the app, Open, then confirm.

Proper signing only matters for giving the app to other people:

1. Join the Apple Developer Program (99 USD a year).
2. Create a "Developer ID Application" certificate in Xcode or on the
   developer site; it lands in the Keychain.
3. Set `bundle.macOS.signingIdentity` in `tauri.conf.json` to its name.
4. For notarization (Apple scans the app so Gatekeeper opens it without
   warnings), set `APPLE_ID`, `APPLE_PASSWORD` (an app-specific password)
   and `APPLE_TEAM_ID` in the environment; `cargo tauri build` then signs,
   submits and staples the result itself.

## 6. Decisions and checks for that session

- **libjpeg-turbo linking.** With `pkg-config`, the app links Homebrew's
  `libturbojpeg.dylib` dynamically, so it runs only on a Mac that has
  Homebrew's `jpeg-turbo`. Fine for personal use. To hand the app to
  someone, build it in statically instead (`turbojpeg-sys`'s `cmake`
  feature; needs `brew install cmake nasm`).
- **Intel Macs.** Only if someone needs it: `rustup target add
  x86_64-apple-darwin` and `--target universal-apple-darwin`, with a
  libjpeg-turbo for both architectures (another reason for the static
  build).
- **Title bar.** The default is a normal macOS title bar above our own top
  bar. Tauri can overlay the traffic-light buttons onto the page
  (`titleBarStyle: "Overlay"`) for a more native look; the top bar would
  then need a left inset for them.
- **Menus and shortcuts.** Tauri gives a Mac app the standard menu bar
  (Quit, Edit with copy and paste). The GUI already takes Cmd as well as
  Ctrl for undo; check that the Edit menu's own Undo does not swallow
  Cmd+Z before the page sees it.
- **Folders.** Dialogs start in `~/Pictures`, which exists on every Mac.
  The first Open may trigger macOS's "allow access to Pictures" prompt.
