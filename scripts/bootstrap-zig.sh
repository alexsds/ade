#!/usr/bin/env bash
set -euo pipefail

ZIG_VERSION="0.14.1"
INSTALL_DIR=".context/zig"

if command -v zig &>/dev/null; then
  CURRENT=$(zig version 2>/dev/null || echo "unknown")
  if [[ "$CURRENT" == "$ZIG_VERSION" ]]; then
    echo "Zig $ZIG_VERSION already installed"
    exit 0
  fi
fi

echo "Installing Zig $ZIG_VERSION..."
ARCH=$(uname -m)
OS=$(uname -s | tr '[:upper:]' '[:lower:]')

if [[ "$ARCH" == "arm64" ]]; then ARCH="aarch64"; fi
if [[ "$OS" == "darwin" ]]; then OS="macos"; fi

URL="https://ziglang.org/download/${ZIG_VERSION}/zig-${OS}-${ARCH}-${ZIG_VERSION}.tar.xz"
mkdir -p "$INSTALL_DIR"
curl -L "$URL" | tar xJ --strip-components=1 -C "$INSTALL_DIR"
echo "Zig installed to $INSTALL_DIR/zig"
echo "Add to PATH: export PATH=\"\$PWD/$INSTALL_DIR:\$PATH\""
