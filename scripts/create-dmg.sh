#!/usr/bin/env bash
set -euo pipefail

# DMG packaging script for Ade
# Produces Ade.dmg at target/release/Ade.dmg

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
APP_DIR="$PROJECT_DIR/target/release/bundle/Ade.app"
OUTPUT_DMG="$PROJECT_DIR/target/release/Ade.dmg"

# Validate Ade.app exists
if [[ ! -d "$APP_DIR" ]]; then
    echo "ERROR: Ade.app not found at $APP_DIR"
    echo "Run ./scripts/bundle-macos.sh first."
    exit 1
fi

echo "Creating Ade.dmg..."

# Create staging directory with app + Applications symlink
STAGING_DIR=$(mktemp -d)
cp -R "$APP_DIR" "$STAGING_DIR/"
ln -s /Applications "$STAGING_DIR/Applications"

# Create compressed read-only DMG
hdiutil create \
    -srcfolder "$STAGING_DIR" \
    -volname "Ade" \
    -format UDZO \
    -ov \
    "$OUTPUT_DMG"

# Clean up staging directory
rm -rf "$STAGING_DIR"

echo ""
echo "Ade.dmg created successfully at:"
echo "  $OUTPUT_DMG"
echo ""
DMG_SIZE=$(du -h "$OUTPUT_DMG" | cut -f1)
echo "Size: $DMG_SIZE"
echo "To test: open \"$OUTPUT_DMG\""
