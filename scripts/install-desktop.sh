#!/usr/bin/env bash
# Build the desktop app and install it for the current user, so it shows up
# in the application launcher with its icon. Nothing goes outside ~/.local,
# and no root is needed.
#
#   scripts/install-desktop.sh            build and install
#   scripts/install-desktop.sh --remove   uninstall
#
# Installs:
#   ~/.local/bin/glitchrx-desktop
#   ~/.local/share/applications/glitchrx-desktop.desktop
#   ~/.local/share/icons/hicolor/{32x32,128x128,256x256,512x512}/apps/org.glitcharaxia.glitchrx.png
#
# The launcher entry is named after the binary, which is the window's app
# id on Wayland (and its class on X11), so the desktop can match a running
# window to its icon.
set -euo pipefail
cd "$(dirname "$0")/.."

ID=org.glitcharaxia.glitchrx
ENTRY=glitchrx-desktop.desktop
BIN="$HOME/.local/bin/glitchrx-desktop"
APPS="$HOME/.local/share/applications"
ICONS="$HOME/.local/share/icons/hicolor"
SIZES=(32x32 128x128 256x256 512x512)
SOURCES=(32x32.png 128x128.png 128x128@2x.png icon.png)

refresh() {
  # Optional caches; harmless when the tools are missing.
  update-desktop-database "$APPS" 2>/dev/null || true
  gtk-update-icon-cache -q -t "$ICONS" 2>/dev/null || true
}

if [[ "${1:-}" == "--remove" ]]; then
  rm -f "$BIN" "$APPS/$ENTRY"
  for s in "${SIZES[@]}"; do rm -f "$ICONS/$s/apps/$ID.png"; done
  refresh
  echo "glitchrx desktop removed"
  exit 0
fi

# The GUI is embedded at build time, so build it first.
(cd web && { [[ -d node_modules ]] || npm ci; } && npm run build)
cargo build --release -p glitchrx-desktop

install -Dm755 target/release/glitchrx-desktop "$BIN"
for i in "${!SIZES[@]}"; do
  install -Dm644 "crates/glitchrx-desktop/icons/${SOURCES[$i]}" "$ICONS/${SIZES[$i]}/apps/$ID.png"
done
install -d "$APPS"
cat > "$APPS/$ENTRY" <<DESKTOP
[Desktop Entry]
Type=Application
Name=glitchrx
GenericName=Photo databender
Comment=Databend photos the way Audacity would
Exec=$BIN
Icon=$ID
Terminal=false
Categories=Graphics;2DGraphics;RasterGraphics;
StartupWMClass=glitchrx-desktop
DESKTOP
refresh
echo "installed: $BIN"
echo "launcher entry: $APPS/$ENTRY"
