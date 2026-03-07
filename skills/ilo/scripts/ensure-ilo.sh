#!/bin/sh
# Ensure ilo is installed and up to date (macOS, Linux, Windows/Git Bash).
# Tries GitHub releases first (native binary), falls back to npm (WASM).
set -eu

REPO="ilo-lang/ilo"

# Check if already installed
if command -v ilo >/dev/null 2>&1; then
  CURRENT_VER=$(ilo --version 2>/dev/null | sed 's/ilo //' | sed 's/v//')

  LATEST=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null | grep '"tag_name"' | head -1 | sed 's/.*"v//;s/".*//' || echo "unknown")

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
  echo "Installing ilo..."
fi

# --- Try GitHub releases (native binary) ---

install_from_github() {
  OS=$(uname -s)
  ARCH=$(uname -m)

  case "$OS" in
    Linux)              OS_TARGET="unknown-linux-gnu"; EXT="" ;;
    Darwin)             OS_TARGET="apple-darwin"; EXT="" ;;
    MINGW*|MSYS*|CYGWIN*|Windows_NT) OS_TARGET="pc-windows-msvc"; EXT=".exe" ;;
    *)                  return 1 ;;
  esac

  case "$ARCH" in
    x86_64|amd64|AMD64) ARCH_TARGET="x86_64" ;;
    aarch64|arm64)      ARCH_TARGET="aarch64" ;;
    *)                  return 1 ;;
  esac

  TARGET="${ARCH_TARGET}-${OS_TARGET}"
  URL="https://github.com/${REPO}/releases/latest/download/ilo-${TARGET}${EXT}"

  case "$OS" in
    MINGW*|MSYS*|CYGWIN*|Windows_NT)
      INSTALL_DIR="${LOCALAPPDATA:-${USERPROFILE:-$HOME}/AppData/Local}/ilo/bin"
      mkdir -p "$INSTALL_DIR"
      BINARY="${INSTALL_DIR}/ilo${EXT}"
      ;;
    *)
      if [ -w /usr/local/bin ]; then
        INSTALL_DIR="/usr/local/bin"
      else
        INSTALL_DIR="${HOME}/.local/bin"
        mkdir -p "$INSTALL_DIR"
      fi
      BINARY="${INSTALL_DIR}/ilo"
      ;;
  esac

  if curl -fsSL --connect-timeout 5 "$URL" -o "$BINARY" 2>/dev/null; then
    chmod +x "$BINARY" 2>/dev/null || true
    VERSION=$("$BINARY" --version 2>/dev/null || echo "ilo (unknown version)")
    echo "Installed ${VERSION} to ${BINARY} (native)"
    case ":${PATH}:" in
      *":${INSTALL_DIR}:"*) ;;
      *) echo "Note: add ${INSTALL_DIR} to your PATH" ;;
    esac
    return 0
  fi

  return 1
}

# --- Fallback: npm (WASM via Node.js) ---

install_from_npm() {
  if ! command -v npm >/dev/null 2>&1; then
    return 1
  fi

  echo "GitHub unreachable, installing via npm..."
  npm i -g ilo-lang 2>/dev/null

  if command -v ilo >/dev/null 2>&1; then
    VERSION=$(ilo --version 2>/dev/null || echo "ilo (unknown version)")
    echo "Installed ${VERSION} via npm (WASM)"
    return 0
  fi

  return 1
}

# --- Main ---

if install_from_github; then
  exit 0
fi

if install_from_npm; then
  exit 0
fi

echo "Failed to install ilo. Install manually: npm i -g ilo-lang" >&2
exit 1
