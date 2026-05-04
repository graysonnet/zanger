#!/bin/bash
set -e

REPO="graysonnet/zanger"
INSTALL_DIR="/usr/local/bin"

echo "Installing zanger..."

ARCH=$(uname -m)
OS=$(uname -s)

case "$OS" in
Linux)
  case "$ARCH" in
  x86_64) ASSET="zanger-linux-x86_64.tar.gz" ;;
  aarch64) ASSET="zanger-linux-arm64.tar.gz" ;;
  *)
    echo "Unsupported architecture: $ARCH"
    exit 1
    ;;
  esac
  ;;
Darwin)
  case "$ARCH" in
  x86_64) ASSET="zanger-macos-x86_64.tar.gz" ;;
  arm64) ASSET="zanger-macos-arm64.tar.gz" ;;
  *)
    echo "Unsupported architecture: $ARCH"
    exit 1
    ;;
  esac
  ;;
*)
  echo "Unsupported OS: $OS (use install.ps1 for Windows)"
  exit 1
  ;;
esac

TAG=$(curl -sL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | cut -d '"' -f 4)

if [ -z "$TAG" ]; then
  echo "Error: Could not determine latest release"
  exit 1
fi

echo "Downloading $ASSET ($TAG)..."

TMPDIR=$(mktemp -d)
curl -sL "https://github.com/$REPO/releases/download/$TAG/$ASSET" -o "$TMPDIR/$ASSET"

tar xzf "$TMPDIR/$ASSET" -C "$TMPDIR"

if [ -w "$INSTALL_DIR" ]; then
  mv "$TMPDIR/zanger" "$INSTALL_DIR/zanger"
else
  sudo mv "$TMPDIR/zanger" "$INSTALL_DIR/zanger"
fi

chmod +x "$INSTALL_DIR/zanger"
rm -rf "$TMPDIR"

echo "zanger $TAG installed to $INSTALL_DIR/zanger"
echo "Run 'zanger' to get started!"
