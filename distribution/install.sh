#!/bin/sh
# Strata Universal Frontend Engine - CLI Installer for Linux and macOS
# Zero Chromium. Pure Rust. Pixel-Perfect CSS & Flexbox/Grid.
#
# Usage:
#   curl -fsSL https://strata.dev/install.sh | sh

set -e

echo "▲ Installing Strata Universal Frontend Engine..."
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)
    TARGET_OS="linux"
    ;;
  Darwin)
    TARGET_OS="macos"
    ;;
  *)
    echo "error: Unsupported operating system: $OS"
    exit 1
    ;;
esac

case "$ARCH" in
  x86_64|amd64)
    TARGET_ARCH="x86_64"
    ;;
  arm64|aarch64)
    TARGET_ARCH="aarch64"
    ;;
  *)
    echo "error: Unsupported architecture: $ARCH"
    exit 1
    ;;
esac

INSTALL_DIR="${HOME}/.cargo/bin"
mkdir -p "$INSTALL_DIR"

echo "  › Target: ${TARGET_OS}-${TARGET_ARCH}"
echo "  › Destination: ${INSTALL_DIR}"

if command -v cargo >/dev/null 2>&1; then
  echo "  › Compiling and installing strata via cargo..."
  cargo install strata_cli --bin strata --quiet || true
fi

if [ -x "${INSTALL_DIR}/strata" ]; then
  echo "  √ Strata CLI successfully installed to ${INSTALL_DIR}/strata"
  echo "  › Run 'strata new' to create your first project!"
else
  echo "  › Please ensure ${INSTALL_DIR} is in your PATH."
fi
