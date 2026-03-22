#!/usr/bin/env bash
set -euo pipefail

# macOS bundle assembly script for ADE
# Produces ADE.app at target/release/bundle/ADE.app

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BUNDLE_DIR="$PROJECT_DIR/target/release/bundle"
APP_DIR="$BUNDLE_DIR/ADE.app"

# Extract version from Cargo.toml
VERSION=$(grep '^version' "$PROJECT_DIR/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')
echo "Building ADE v${VERSION}..."

# Step 1: Build release binary
echo "Compiling release binary..."
cargo build --release

# Step 2: Generate .icns from resources/icon.png
echo "Generating app icon..."
ICON_SRC="$PROJECT_DIR/resources/icon.png"
ICONSET_TMP=$(mktemp -d)/ADE.iconset
mkdir -p "$ICONSET_TMP"

# Resize for each required size (base + @2x)
for SIZE in 16 32 128 256 512; do
    DOUBLE=$((SIZE * 2))
    sips -Z "$SIZE" "$ICON_SRC" --out "$ICONSET_TMP/icon_${SIZE}x${SIZE}.png" >/dev/null 2>&1
    sips -Z "$DOUBLE" "$ICON_SRC" --out "$ICONSET_TMP/icon_${SIZE}x${SIZE}@2x.png" >/dev/null 2>&1
done

# The 512@2x is the original 1024x1024
cp "$ICON_SRC" "$ICONSET_TMP/icon_512x512@2x.png"

iconutil -c icns "$ICONSET_TMP" -o "$ICONSET_TMP/../AppIcon.icns"
ICNS_PATH="$ICONSET_TMP/../AppIcon.icns"

# Step 3: Create bundle directory structure
echo "Assembling app bundle..."
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS"
mkdir -p "$APP_DIR/Contents/Resources"

# Step 4: Copy binary
cp "$PROJECT_DIR/target/release/ade" "$APP_DIR/Contents/MacOS/ade"

# Step 5: Copy icon
cp "$ICNS_PATH" "$APP_DIR/Contents/Resources/AppIcon.icns"

# Step 6: Generate Info.plist
cat > "$APP_DIR/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>ade</string>
    <key>CFBundleIdentifier</key>
    <string>com.alexsds.ade</string>
    <key>CFBundleName</key>
    <string>ADE</string>
    <key>CFBundleDisplayName</key>
    <string>ADE</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundleIconName</key>
    <string>AppIcon</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>LSMinimumSystemVersion</key>
    <string>13.0</string>
</dict>
</plist>
PLIST

# Step 7: Validate the bundle
echo "Validating bundle..."
plutil -lint "$APP_DIR/Contents/Info.plist"
test -x "$APP_DIR/Contents/MacOS/ade" || { echo "ERROR: Binary is not executable"; exit 1; }
test -f "$APP_DIR/Contents/Resources/AppIcon.icns" || { echo "ERROR: Icon not found"; exit 1; }

# Clean up temp iconset
rm -rf "$(dirname "$ICONSET_TMP")"

echo ""
echo "ADE.app built successfully at:"
echo "  $APP_DIR"
echo ""
echo "To launch: open \"$APP_DIR\""
