#!/usr/bin/env bash
set -euo pipefail

REPO="${NIMESVC_REPO:-nimeview/NimeScript}"
INSTALL_DIR="${NIMESVC_INSTALL_DIR:-$HOME/.nimesvc/bin}"

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

if [[ "$ARCH" == "x86_64" ]]; then
  ARCH="x64"
elif [[ "$ARCH" == "arm64" || "$ARCH" == "aarch64" ]]; then
  ARCH="arm64"
else
  echo "Unsupported arch: $ARCH" >&2
  exit 1
fi

if [[ "$OS" == "darwin" ]]; then
  ASSET="nimesvc-macos-$ARCH"
elif [[ "$OS" == "linux" ]]; then
  ASSET="nimesvc-linux-$ARCH"
else
  echo "Unsupported OS: $OS" >&2
  exit 1
fi

LATEST_JSON=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest")
TAG=$(echo "$LATEST_JSON" | grep -Eo '"tag_name":\s*"[^"]+"' | head -n1 | cut -d '"' -f4)
if [[ -z "$TAG" ]]; then
  echo "Failed to determine latest release" >&2
  exit 1
fi

URL="https://github.com/$REPO/releases/download/$TAG/$ASSET"
TMP_DIR=$(mktemp -d)
BIN_PATH="$TMP_DIR/$ASSET"

curl -fsSL "$URL" -o "$BIN_PATH"

mkdir -p "$INSTALL_DIR"

mv "$BIN_PATH" "$INSTALL_DIR/nimesvc"
chmod +x "$INSTALL_DIR/nimesvc"

if ! command -v nimesvc >/dev/null 2>&1; then
  echo "Installed to $INSTALL_DIR"
  echo "Add to PATH: export PATH=\"$INSTALL_DIR:\$PATH\""
else
  echo "Installed/updated nimesvc ($TAG)"
fi
