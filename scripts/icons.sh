#!/usr/bin/env bash
# Render the PNG app icons from web/public/icon.svg (needs rsvg-convert,
# from librsvg). The PNGs are committed, so building the app never needs
# this; rerun it only after changing the SVG. Also writes the desktop
# app's icons (crates/glitchrx-desktop/icons).
#
# The SVG itself comes from scripts/icon.py.
#
#   apple-touch-icon.png   180 px, iOS home screen (iOS rounds the corners)
#   icon-192.png, -512.png manifest icons
#   icon-maskable-512.png  artwork shrunk into the central safe circle, for
#                          launchers that crop icons to a circle or squircle
set -euo pipefail
cd "$(dirname "$0")/../web/public"
rsvg-convert -w 180 -h 180 icon.svg -o apple-touch-icon.png
rsvg-convert -w 192 -h 192 icon.svg -o icon-192.png
rsvg-convert -w 512 -h 512 icon.svg -o icon-512.png
# Maskable: scale the artwork group to 80% about the centre.
sed 's|<g id="art">|<g id="art" transform="translate(51.2 51.2) scale(0.8)">|' icon.svg |
  rsvg-convert -w 512 -h 512 -o icon-maskable-512.png
cp icon.svg favicon.svg

# Desktop app (Linux, later macOS): a rounded tile with a margin, as
# desktop launchers expect, instead of the full-bleed square phones mask.
desk=../../crates/glitchrx-desktop/icons
tile=$(sed -e 's|<rect width="512" height="512" fill="\(#[0-9a-f]*\)"/>|<rect x="24" y="24" width="464" height="464" rx="104" fill="\1"/>|' \
  -e 's|<g id="art">|<g id="art" transform="translate(24.16 24.16) scale(0.9056)">|' icon.svg)
for size in 32 128 256 512; do
  echo "$tile" | rsvg-convert -w $size -h $size -o "$desk/$size.png"
done
mv "$desk/32.png" "$desk/32x32.png"
mv "$desk/128.png" "$desk/128x128.png"
mv "$desk/256.png" "$desk/128x128@2x.png"
mv "$desk/512.png" "$desk/icon.png"
echo "icons written to web/public and crates/glitchrx-desktop/icons"
