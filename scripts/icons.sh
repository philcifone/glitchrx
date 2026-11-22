#!/usr/bin/env bash
# Render the PNG app icons from web/public/icon.svg (needs rsvg-convert,
# from librsvg). The PNGs are committed, so building the app never needs
# this; rerun it only after changing the SVG.
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
echo "icons written to web/public"
