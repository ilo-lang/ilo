#!/usr/bin/env bash
# Offline regression test for scripts/install/install.sh.
#
# We can't make real network requests to GitHub Releases from CI on every PR,
# but we can verify the script's integrity-checking behaviour by stubbing out
# `curl` and feeding it a known fixture. The test runs the script in a sandbox
# directory with a stub `curl` on PATH that serves a fake binary + checksum
# file from a temp dir, then asserts:
#
#   - happy path: a matching checksum installs the binary
#   - tamper path: a mismatching checksum aborts before chmod+install
#   - missing-asset path: a checksum file that doesn't list the asset aborts
#
# This is a structural test of the install.sh logic, not an end-to-end install.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
INSTALL_SH="$SCRIPT_DIR/install.sh"

if [ ! -f "$INSTALL_SH" ]; then
  echo "install.sh not found at $INSTALL_SH" >&2
  exit 1
fi

# Detect OS/ARCH the same way install.sh does, so we know which asset name to fake.
OS=$(uname -s)
ARCH=$(uname -m)
case "$OS" in
  Linux)  OS_TARGET="unknown-linux-gnu" ;;
  Darwin) OS_TARGET="apple-darwin" ;;
  *)      echo "test skipped on unsupported OS: $OS"; exit 0 ;;
esac
case "$ARCH" in
  x86_64|amd64)  ARCH_TARGET="x86_64" ;;
  aarch64|arm64) ARCH_TARGET="aarch64" ;;
  *) echo "test skipped on unsupported arch: $ARCH"; exit 0 ;;
esac
ASSET="ilo-${ARCH_TARGET}-${OS_TARGET}"

if command -v sha256sum >/dev/null 2>&1; then
  sha256_cmd="sha256sum"
elif command -v shasum >/dev/null 2>&1; then
  sha256_cmd="shasum -a 256"
else
  echo "no sha256 tool available; cannot run test" >&2
  exit 1
fi

WORKROOT=$(mktemp -d)
# shellcheck disable=SC2064
trap "rm -rf '$WORKROOT'" EXIT

# Build a curl stub that serves files out of $FIXTURE_DIR. The real script
# calls `curl -fsSL <URL> -o <PATH>`; the stub maps the URL's basename to a
# file in $FIXTURE_DIR.
mkdir -p "$WORKROOT/bin"
cat > "$WORKROOT/bin/curl" <<'STUB'
#!/usr/bin/env bash
# curl stub for install.sh tests. Honours `-o <path>` and ignores other flags.
out=""
url=""
while [ $# -gt 0 ]; do
  case "$1" in
    -o) out="$2"; shift 2 ;;
    -*) shift ;;
    *)  url="$1"; shift ;;
  esac
done
base=$(basename "$url")
src="$FIXTURE_DIR/$base"
if [ ! -f "$src" ]; then
  echo "curl-stub: missing fixture $src" >&2
  exit 22
fi
cp "$src" "$out"
STUB
chmod +x "$WORKROOT/bin/curl"

run_case() {
  local name="$1"; shift
  local expected_exit="$1"; shift
  local fixture_dir="$1"; shift
  local home_dir="$WORKROOT/home-$name"
  mkdir -p "$home_dir/.local/bin"
  set +e
  PATH="$WORKROOT/bin:$PATH" \
    HOME="$home_dir" \
    FIXTURE_DIR="$fixture_dir" \
    sh "$INSTALL_SH" > "$WORKROOT/$name.out" 2> "$WORKROOT/$name.err"
  local code=$?
  set -e
  if [ "$code" -ne "$expected_exit" ]; then
    echo "FAIL: $name exited $code, expected $expected_exit" >&2
    echo "--- stdout ---" >&2; cat "$WORKROOT/$name.out" >&2
    echo "--- stderr ---" >&2; cat "$WORKROOT/$name.err" >&2
    exit 1
  fi
  echo "ok: $name (exit $code)"
}

# ---------- happy path ----------
HAPPY="$WORKROOT/fixture-happy"
mkdir -p "$HAPPY"
printf 'fake-binary-contents' > "$HAPPY/$ASSET"
sum=$(cd "$HAPPY" && $sha256_cmd "$ASSET" | awk '{print $1}')
printf '%s  %s\n' "$sum" "$ASSET" > "$HAPPY/checksums-sha256.txt"
run_case happy 0 "$HAPPY"

# Verify the binary actually landed (in $HOME/.local/bin since /usr/local/bin
# probably isn't writable in the test sandbox).
if ! [ -x "$WORKROOT/home-happy/.local/bin/ilo" ]; then
  echo "FAIL: happy path didn't install the binary" >&2
  exit 1
fi
echo "ok: happy path installed the binary"

# ---------- tamper path ----------
TAMPER="$WORKROOT/fixture-tamper"
mkdir -p "$TAMPER"
printf 'fake-binary-contents' > "$TAMPER/$ASSET"
# Hash a different payload so the checksum won't match what we actually serve.
bad=$(printf 'something-else' | $sha256_cmd | awk '{print $1}')
printf '%s  %s\n' "$bad" "$ASSET" > "$TAMPER/checksums-sha256.txt"
run_case tamper 1 "$TAMPER"
if [ -e "$WORKROOT/home-tamper/.local/bin/ilo" ]; then
  echo "FAIL: tamper path installed the binary anyway" >&2
  exit 1
fi
if ! grep -q "Checksum mismatch" "$WORKROOT/tamper.err"; then
  echo "FAIL: tamper path didn't print the mismatch message" >&2
  cat "$WORKROOT/tamper.err" >&2
  exit 1
fi
echo "ok: tamper path refused to install"

# ---------- missing-asset path ----------
MISSING="$WORKROOT/fixture-missing"
mkdir -p "$MISSING"
printf 'fake-binary-contents' > "$MISSING/$ASSET"
# Checksum file references a different asset name, so the grep returns empty.
printf '%s  %s\n' "$sum" "ilo-some-other-target" > "$MISSING/checksums-sha256.txt"
run_case missing 1 "$MISSING"
if [ -e "$WORKROOT/home-missing/.local/bin/ilo" ]; then
  echo "FAIL: missing-asset path installed the binary anyway" >&2
  exit 1
fi
if ! grep -q "Could not find" "$WORKROOT/missing.err"; then
  echo "FAIL: missing-asset path didn't print the expected diagnostic" >&2
  cat "$WORKROOT/missing.err" >&2
  exit 1
fi
echo "ok: missing-asset path refused to install"

echo "all install.sh tests passed"
