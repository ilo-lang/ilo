#!/bin/sh
set -eu

REPO="ilo-lang/ilo"

OS=$(uname -s)
ARCH=$(uname -m)

case "$OS" in
  Linux)  OS_TARGET="unknown-linux-gnu" ;;
  Darwin) OS_TARGET="apple-darwin" ;;
  *)      echo "Unsupported OS: $OS" >&2; exit 1 ;;
esac

case "$ARCH" in
  x86_64|amd64)  ARCH_TARGET="x86_64" ;;
  aarch64|arm64) ARCH_TARGET="aarch64" ;;
  *)             echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

TARGET="${ARCH_TARGET}-${OS_TARGET}"
URL="https://github.com/${REPO}/releases/latest/download/ilo-${TARGET}"

echo "Downloading ilo for ${TARGET}..."

if [ -w /usr/local/bin ]; then
  INSTALL_DIR="/usr/local/bin"
else
  INSTALL_DIR="${HOME}/.local/bin"
  mkdir -p "$INSTALL_DIR"
fi

curl -fsSL "$URL" -o "${INSTALL_DIR}/ilo"
chmod +x "${INSTALL_DIR}/ilo"

VERSION=$("${INSTALL_DIR}/ilo" --version 2>/dev/null || echo "ilo (unknown version)")
echo "Installed ${VERSION} to ${INSTALL_DIR}/ilo"

if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
  echo "Add ${INSTALL_DIR} to your PATH to use ilo"
fi
