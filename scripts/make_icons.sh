#!/usr/bin/env bash
# Generates the full-color app icon (.icns) and the monochrome menu-bar
# template glyph (.png) for Pulse. Requires: rsvg-convert, iconutil, sips.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ASSETS="$ROOT/assets"
BUILD="$ROOT/target/icon-build"
mkdir -p "$ASSETS" "$BUILD"

# ----------------------------------------------------------------------------
# 1. Full-color app icon — Big Sur squircle, indigo→sky gradient, pulse trace.
# ----------------------------------------------------------------------------
cat > "$BUILD/appicon.svg" <<'SVG'
<svg width="1024" height="1024" viewBox="0 0 1024 1024" xmlns="http://www.w3.org/2000/svg">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1024" y2="1024" gradientUnits="userSpaceOnUse">
      <stop offset="0"   stop-color="#6D74FF"/>
      <stop offset="0.55" stop-color="#5468F2"/>
      <stop offset="1"   stop-color="#12B6E8"/>
    </linearGradient>
    <radialGradient id="hi" cx="0.32" cy="0.20" r="0.9">
      <stop offset="0" stop-color="#ffffff" stop-opacity="0.35"/>
      <stop offset="0.45" stop-color="#ffffff" stop-opacity="0.05"/>
      <stop offset="1" stop-color="#ffffff" stop-opacity="0"/>
    </radialGradient>
    <filter id="soft" x="-20%" y="-20%" width="140%" height="140%">
      <feDropShadow dx="0" dy="10" stdDeviation="18" flood-color="#0A1E5C" flood-opacity="0.35"/>
    </filter>
    <filter id="glow" x="-60%" y="-60%" width="220%" height="220%">
      <feGaussianBlur stdDeviation="14" result="b"/>
      <feMerge><feMergeNode in="b"/><feMergeNode in="SourceGraphic"/></feMerge>
    </filter>
  </defs>

  <!-- squircle background (Apple continuous corner ~ 22.37%) -->
  <rect x="64" y="64" width="896" height="896" rx="200" ry="200" fill="url(#bg)"/>
  <rect x="64" y="64" width="896" height="896" rx="200" ry="200" fill="url(#hi)"/>

  <!-- faint baseline grid -->
  <g stroke="#ffffff" stroke-opacity="0.12" stroke-width="4">
    <line x1="150" y1="512" x2="874" y2="512"/>
  </g>

  <!-- pulse / ECG trace -->
  <g fill="none" stroke="#ffffff" stroke-width="46"
     stroke-linecap="round" stroke-linejoin="round" filter="url(#glow)">
    <path d="M 168 512 L 356 512 L 432 512 L 476 372 L 548 690 L 612 300 L 676 512 L 748 512 L 856 512"/>
  </g>
  <!-- live cursor dot -->
  <circle cx="856" cy="512" r="30" fill="#ffffff" filter="url(#soft)"/>
</svg>
SVG

ICONSET="$BUILD/AppIcon.iconset"
rm -rf "$ICONSET"; mkdir -p "$ICONSET"
render() { rsvg-convert -w "$2" -h "$2" "$BUILD/appicon.svg" -o "$ICONSET/$1"; }
render icon_16x16.png       16
render icon_16x16@2x.png    32
render icon_32x32.png       32
render icon_32x32@2x.png    64
render icon_128x128.png     128
render icon_128x128@2x.png  256
render icon_256x256.png     256
render icon_256x256@2x.png  512
render icon_512x512.png     512
render icon_512x512@2x.png  1024
iconutil -c icns "$ICONSET" -o "$ASSETS/AppIcon.icns"
rsvg-convert -w 1024 -h 1024 "$BUILD/appicon.svg" -o "$ASSETS/AppIcon-1024.png"

# ----------------------------------------------------------------------------
# 2. Menu-bar template glyph — solid black pulse on transparency (macOS tints).
# ----------------------------------------------------------------------------
cat > "$BUILD/template.svg" <<'SVG'
<svg width="44" height="44" viewBox="0 0 44 44" xmlns="http://www.w3.org/2000/svg">
  <g fill="none" stroke="#000000" stroke-width="3.1"
     stroke-linecap="round" stroke-linejoin="round">
    <path d="M 4 22 L 13 22 L 17 22 L 20 12 L 25 33 L 29 9 L 33 22 L 37 22 L 40 22"/>
  </g>
  <circle cx="40" cy="22" r="2.4" fill="#000000"/>
</svg>
SVG
rsvg-convert -w 44 -h 44 "$BUILD/template.svg" -o "$ASSETS/tray-template.png"

echo "Icons written to $ASSETS:"
ls -la "$ASSETS"
