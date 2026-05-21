#!/bin/sh
# ilo installer — downloads the latest release binary from GitHub and verifies
# its sha256 against the checksums file published with the same release before
# placing it on $PATH. Aborts on mismatch.
#
# Canonical source: https://github.com/ilo-lang/ilo/blob/main/scripts/install/install.sh
# Served from:     https://ilo-lang.ai/install.sh
set -eu

REPO="ilo-lang/ilo"
RELEASE_BASE="https://github.com/${REPO}/releases/latest/download"

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
ASSET="ilo-${TARGET}"
BIN_URL="${RELEASE_BASE}/${ASSET}"
SUMS_URL="${RELEASE_BASE}/checksums-sha256.txt"

# Pick a sha256 tool. macOS ships `shasum`; most Linuxes ship `sha256sum`.
if command -v sha256sum >/dev/null 2>&1; then
  sha256_cmd="sha256sum"
elif command -v shasum >/dev/null 2>&1; then
  sha256_cmd="shasum -a 256"
else
  echo "Neither sha256sum nor shasum found; cannot verify download integrity." >&2
  echo "Install one of them and re-run, or install ilo manually." >&2
  exit 1
fi

# Allow callers (and tests) to override the install location. Otherwise
# prefer /usr/local/bin if it's writable, else fall back to ~/.local/bin.
if [ -n "${ILO_INSTALL_DIR:-}" ]; then
  INSTALL_DIR="$ILO_INSTALL_DIR"
  mkdir -p "$INSTALL_DIR"
elif [ -w /usr/local/bin ]; then
  INSTALL_DIR="/usr/local/bin"
else
  INSTALL_DIR="${HOME}/.local/bin"
  mkdir -p "$INSTALL_DIR"
fi

TMPDIR_ILO=$(mktemp -d 2>/dev/null || mktemp -d -t ilo-install)
# shellcheck disable=SC2064
trap "rm -rf \"$TMPDIR_ILO\"" EXIT INT HUP TERM

echo "Downloading ilo for ${TARGET}..."
curl -fsSL "$BIN_URL"  -o "${TMPDIR_ILO}/${ASSET}"
curl -fsSL "$SUMS_URL" -o "${TMPDIR_ILO}/checksums-sha256.txt"

# Pull the expected hash for this asset out of the combined checksum file.
expected=$(grep " ${ASSET}\$" "${TMPDIR_ILO}/checksums-sha256.txt" | awk '{print $1}')
if [ -z "$expected" ]; then
  echo "Could not find ${ASSET} in checksums-sha256.txt; aborting." >&2
  exit 1
fi

# Compute the actual hash, isolating just the hex digest.
actual=$(cd "$TMPDIR_ILO" && $sha256_cmd "$ASSET" | awk '{print $1}')

if [ "$expected" != "$actual" ]; then
  echo "Checksum mismatch for ${ASSET}:" >&2
  echo "  expected: $expected" >&2
  echo "  actual:   $actual" >&2
  echo "Refusing to install. The binary may be corrupted or tampered with." >&2
  exit 1
fi

echo "Checksum OK (${actual})."

mv "${TMPDIR_ILO}/${ASSET}" "${INSTALL_DIR}/ilo"
chmod +x "${INSTALL_DIR}/ilo"

VERSION=$("${INSTALL_DIR}/ilo" --version 2>/dev/null || echo "ilo (unknown version)")
echo "Installed ${VERSION} to ${INSTALL_DIR}/ilo"

if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
  echo "Add ${INSTALL_DIR} to your PATH to use ilo"
fi
