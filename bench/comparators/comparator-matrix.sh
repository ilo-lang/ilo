#!/usr/bin/env bash
# comparator-matrix.sh — rerunnable multi-language benchmark legs (G1).
#
# Sets up (clone + build) each comparator language under bench/comparators/src/
# and runs the closed-loop LLM leg for every language whose toolchain builds.
# Clones are cached: re-runs skip setup unless --force.
#
# Usage:
#   DEEPSEEK_API_KEY=sk-... bench/comparators/comparator-matrix.sh [--langs zero,bash,ailang,nanolang] [--force] [--cache warm|cold]
#
# Any language whose build fails is reported and skipped, not fatal.

set -u
cd "$(dirname "$0")/../.."   # repo root

LANGS="zero,bash,ailang,nanolang"
FORCE=0
CACHE=warm
while [ $# -gt 0 ]; do
  case "$1" in
    --langs) LANGS="$2"; shift 2;;
    --force) FORCE=1; shift;;
    --cache) CACHE="$2"; shift 2;;
    *) echo "unknown arg: $1"; exit 2;;
  esac
done

SRC=bench/comparators/src
LOGS=bench/comparators/logs
mkdir -p "$SRC" "$LOGS"
grep -qx "bench/comparators/src" .gitignore 2>/dev/null || echo "bench/comparators/src" >> .gitignore

want() { case ",$LANGS," in *",$1,"*) return 0;; *) return 1;; esac; }

clone() { # clone <repo> <dir>
  local repo=$1 dir=$SRC/$2
  if [ -d "$dir/.git" ] && [ "$FORCE" = 0 ]; then
    echo "[setup] $2: cached"; return 0
  fi
  rm -rf "$dir"
  git clone --depth 1 "https://github.com/$repo" "$dir" >"$LOGS/$2.clone.log" 2>&1
}

build_ailang() {
  clone sunholo-data/ailang ailang || return 1
  (cd "$SRC/ailang" && make build >"$LOGS/ailang.build.log" 2>&1) || return 1
  echo "$SRC/ailang/build/ailang"   # adjust if make output path differs
}

build_nanolang() {
  clone jordanhubbard/nanolang nanolang || return 1
  (cd "$SRC/nanolang" && make -j4 >"$LOGS/nanolang.build.log" 2>&1) || return 1
  nl=$(find "$SRC/nanolang" -maxdepth 2 -type f -name "nano*" -perm -u+x | head -1)
  [ -n "$nl" ] && echo "$nl"
}

build_moonbit() {
  clone moonbitlang/core core || return 1
  command -v moon >/dev/null 2>&1 || { echo "[setup] moonbit: 'moon' CLI not installed (see moonbitlang.com); skipping" >&2; return 1; }
  echo moon
}

run_leg() { # run_leg <name> <bin> <ext> <docs>
  local name=$1 bin=$2 ext=$3 docs=${4:-}
  [ -x "$bin" ] || { echo "[leg] $name: no runnable binary ($bin) — skipped"; return 1; }
  echo "[leg] $name: running ($bin)"
  DEEPSEEK_API_KEY="${DEEPSEEK_API_KEY:?set DEEPSEEK_API_KEY}" \
    python3 scripts/closed-loop-bench.py --model dsflash --cache "$CACHE" \
      --lang2-name "$name" --lang2-bin "$bin" --lang2-ext "$ext" \
      ${docs:+--lang2-docs "$docs"} 2>&1 | tail -18
}

STATUS=0
want bash && run_leg bash /usr/bin/bash .sh || STATUS=1

if want zero; then
  bash bench/zero/zero-bench.sh --selfcheck >"$LOGS/zero.selfcheck.log" 2>&1 \
    && run_leg zero bench/zero/zero-bench.sh .0 skills/zero 2>/dev/null || { echo "[leg] zero: selfcheck failed — skipped"; STATUS=1; }
fi

want ailang && { bin=$(build_ailang) && run_leg ailang "$bin" .ail "$SRC/ailang/docs" || STATUS=1; }
want nanolang && { bin=$(build_nanolang) && run_leg nanolang "$bin" .nano "$SRC/nanolang/docs" || STATUS=1; }
want moonbit && { bin=$(build_moonbit) && run_leg moonbit "$bin" .mbx "$SRC/core/src" || STATUS=1; }

echo "done (some legs may have been skipped — see $LOGS)"
exit $STATUS
