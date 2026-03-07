#!/bin/sh
# Ensure ilo is installed and up to date (macOS, Linux, Windows/Git Bash).
set -eu

REPO="ilo-lang/ilo"

# Detect platform
OS=$(uname -s)
ARCH=$(uname -m)

case "$OS" in
  Linux)
    OS_TARGET="unknown-linux-gnu"
    EXT=""
    ;;
  Darwin)
    OS_TARGET="apple-darwin"
    EXT=""
    ;;
  MINGW*|MSYS*|CYGWIN*|Windows_NT)
    OS_TARGET="pc-windows-msvc"
    EXT=".exe"
    ;;
  *)
    echo "Unsupported OS: $OS" >&2
    exit 1
    ;;
esac

case "$ARCH" in
  x86_64|amd64|AMD64)  ARCH_TARGET="x86_64" ;;
  aarch64|arm64)       ARCH_TARGET="aarch64" ;;
  *)                   echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

TARGET="${ARCH_TARGET}-${OS_TARGET}"

# Fetch latest version from GitHub
LATEST=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null | grep '"tag_name"' | head -1 | sed 's/.*"v//;s/".*//' || echo "unknown")

# Check if already installed
if command -v ilo >/dev/null 2>&1; then
  CURRENT_VER=$(ilo --version 2>/dev/null | sed 's/ilo //' | sed 's/v//')

  if [ "$LATEST" = "unknown" ]; then
    echo "ilo is installed (${CURRENT_VER}), could not check for updates"
    exit 0
  fi

  if [ "$CURRENT_VER" = "$LATEST" ]; then
    echo "ilo is up to date (${CURRENT_VER})"
    exit 0
  fi

  echo "Updating ilo from ${CURRENT_VER} to ${LATEST}..."
else
  echo "Installing ilo${LATEST:+ ${LATEST}}..."
fi

URL="https://github.com/${REPO}/releases/latest/download/ilo-${TARGET}${EXT}"

# Determine install directory
case "$OS" in
  MINGW*|MSYS*|CYGWIN*|Windows_NT)
    # Windows: install to %LOCALAPPDATA%\ilo\bin
    INSTALL_DIR="${LOCALAPPDATA:-${USERPROFILE:-$HOME}/AppData/Local}/ilo/bin"
    mkdir -p "$INSTALL_DIR"
    BINARY="${INSTALL_DIR}/ilo${EXT}"
    ;;
  *)
    # macOS / Linux
    if [ -w /usr/local/bin ]; then
      INSTALL_DIR="/usr/local/bin"
    else
      INSTALL_DIR="${HOME}/.local/bin"
      mkdir -p "$INSTALL_DIR"
    fi
    BINARY="${INSTALL_DIR}/ilo"
    ;;
esac

curl -fsSL "$URL" -o "$BINARY"
chmod +x "$BINARY" 2>/dev/null || true

VERSION=$("$BINARY" --version 2>/dev/null || echo "ilo (unknown version)")
echo "Installed ${VERSION} to ${BINARY}"

# Check PATH
case ":${PATH}:" in
  *":${INSTALL_DIR}:"*) ;;
  *) echo "Note: add ${INSTALL_DIR} to your PATH" ;;
esac
