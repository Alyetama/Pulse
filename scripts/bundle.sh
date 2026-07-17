#!/usr/bin/env bash
# Build (if needed), assemble Pulse.app, ad-hoc sign it, and install to /Applications.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NAME="Pulse"
BUNDLE_ID="com.alyetama.pulse"
VERSION="1.0.3"
APP="$ROOT/dist/$NAME.app"
BIN="$ROOT/target/release/$NAME"
ICON="$ROOT/assets/AppIcon.icns"

if [[ ! -x "$BIN" ]]; then
  echo "Release binary missing — building…"
  (cd "$ROOT" && cargo build --release)
fi
if [[ ! -f "$ICON" ]]; then
  echo "Icon missing — generating…"
  "$ROOT/scripts/make_icons.sh"
fi

echo "Assembling $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/$NAME"
cp "$ICON" "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>            <string>$NAME</string>
    <key>CFBundleDisplayName</key>     <string>$NAME</string>
    <key>CFBundleIdentifier</key>      <string>$BUNDLE_ID</string>
    <key>CFBundleExecutable</key>      <string>$NAME</string>
    <key>CFBundleIconFile</key>        <string>AppIcon</string>
    <key>CFBundlePackageType</key>     <string>APPL</string>
    <key>CFBundleShortVersionString</key> <string>$VERSION</string>
    <key>CFBundleVersion</key>         <string>4</string>
    <key>LSMinimumSystemVersion</key>  <string>11.0</string>
    <key>LSUIElement</key>             <true/>
    <key>NSHighResolutionCapable</key> <true/>
    <key>NSPrincipalClass</key>        <string>NSApplication</string>
    <key>NSSupportsAutomaticGraphicsSwitching</key> <true/>
    <key>NSHumanReadableCopyright</key> <string>Pulse — system monitor</string>
</dict>
</plist>
PLIST

# PkgInfo (harmless, expected by some tooling).
printf 'APPL????' > "$APP/Contents/PkgInfo"

echo "Ad-hoc signing…"
codesign --force --deep --sign - "$APP"
codesign --verify --deep --strict "$APP" && echo "signature OK"

# Install to /Applications (kill any running copy first).
DEST="/Applications/$NAME.app"
pkill -x "$NAME" 2>/dev/null || true
echo "Installing to $DEST"
rm -rf "$DEST"
cp -R "$APP" "$DEST"
echo "Installed: $DEST"
