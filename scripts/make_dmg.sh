#!/usr/bin/env bash
# Build a drag-to-Applications disk image from dist/Pulse.app.
# The filename is intentionally version-less so the GitHub
# releases/latest/download/Pulse.dmg URL stays stable.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NAME="Pulse"
APP="$ROOT/dist/$NAME.app"
DMG="$ROOT/dist/$NAME.dmg"

if [[ ! -d "$APP" ]]; then
  echo "dist/$NAME.app not found — run scripts/bundle.sh first." >&2
  exit 1
fi

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
cp -R "$APP" "$STAGE/$NAME.app"
ln -s /Applications "$STAGE/Applications"

rm -f "$DMG"
hdiutil create \
  -volname "$NAME" \
  -srcfolder "$STAGE" \
  -fs HFS+ \
  -format UDZO \
  -ov \
  "$DMG" >/dev/null

echo "Built $DMG ($(du -h "$DMG" | cut -f1))"
