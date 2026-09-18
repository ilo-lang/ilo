#!/usr/bin/env bash
# comparator-matrix.sh — rerunnable multi-language benchmark legs (G1).
#
# Sets up (clone + build) each comparator language under bench/comparators/src/
# and runs the closed-loop LLM leg for every language whose toolchain builds.
# Clones are cached: re-runs skip setup unless --force.
#
# Usage:
#   DEEPSEEK_API_KEY=sk-... bench/comparators/comparator-matrix.sh [--langs python,bash,zero,ailang,nanolang,moonbit] [--force] [--cache warm|cold]
#
# Any language whose build fails is reported and skipped, not fatal.
#
# Legs whose CLI runs packages rather than single files (ailang, moonbit) go
# through a wrapper in bench/<lang>/ so the bench's `LANG file.ext` contract
# still holds. `python` and `bash` run doc-less: they are the pretraining-native
# floor, carrying no resident spec.
#
# Resident docs: the harness reads --lang2-docs as one text file
# (closed-loop-bench.py reads it with Path(...).read_text()), so each leg's
# agent-facing docs are concatenated into bench/comparators/docs/<lang>.md
# before the leg runs. Sources per leg are named in docs_bundle calls below and
# listed in bench/README.md.

set -u
cd "$(dirname "$0")/../.."   # repo root

LANGS="python,bash,zero,ailang,nanolang,moonbit"
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
DOCS=bench/comparators/docs
ZERO_HOME=${ZERO_HOME:-/tmp/agentlangs/zerolang}
export ZERO_HOME
mkdir -p "$SRC" "$LOGS" "$DOCS"
grep -qx "bench/comparators/src" .gitignore 2>/dev/null || echo "bench/comparators/src" >> .gitignore
grep -qx "bench/comparators/docs" .gitignore 2>/dev/null || echo "bench/comparators/docs" >> .gitignore
grep -qx "bench/comparators/logs" .gitignore 2>/dev/null || echo "bench/comparators/logs" >> .gitignore

want() { case ",$LANGS," in *",$1,"*) return 0;; *) return 1;; esac; }

docs_bundle() { # docs_bundle <name> <file>... — echo the bundle path
  local name=$1 out=$DOCS/$1.md f
  shift
  : >"$out"
  for f in "$@"; do
    [ -f "$f" ] || { echo "[docs] $name: missing $f" >&2; continue; }
    printf '\n\n<!-- %s -->\n' "$f" >>"$out"
    cat "$f" >>"$out"
  done
  [ -s "$out" ] || { echo "[docs] $name: empty bundle" >&2; return 1; }
  echo "[docs] $name: $(wc -c <"$out" | tr -d ' ') bytes from $# file(s)" >&2
  echo "$out"
}

zero_docs() { # zero_docs — bundle the CLI-served Zero agent docs (language + stdlib)
  local out=$DOCS/zero.md z=$ZERO_HOME/.zero/bin/zero m
  [ -x "$z" ] || { echo "[docs] zero: no CLI at $z" >&2; return 1; }
  : >"$out"
  for m in language stdlib; do
    "$z" skills get "$m" >>"$out" 2>/dev/null || echo "[docs] zero: module $m unavailable" >&2
  done
  [ -s "$out" ] || { echo "[docs] zero: empty bundle" >&2; return 1; }
  echo "[docs] zero: $(wc -c <"$out" | tr -d ' ') bytes (skills: language+stdlib)" >&2
  echo "$out"
}

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
  [ -x "$SRC/ailang/bin/ailang" ] && echo "$SRC/ailang/bin/ailang"
}

build_nanolang() {
  clone jordanhubbard/nanolang nanolang || return 1
  (cd "$SRC/nanolang" && make -j4 >"$LOGS/nanolang.build.log" 2>&1) || return 1
  # nano is the interpreter: it takes a .nano file and runs it (nanoc transpiles
  # to C first, which the bench's single-argument contract cannot express).
  [ -x "$SRC/nanolang/bin/nano" ] && echo "$SRC/nanolang/bin/nano"
}

build_moonbit() {
  clone moonbitlang/core core || return 1
  local moon
  moon=$(command -v moon || { [ -x "$HOME/.moon/bin/moon" ] && echo "$HOME/.moon/bin/moon"; }) || true
  [ -n "$moon" ] || { echo "[setup] moonbit: 'moon' CLI not installed (see moonbitlang.com); skipping" >&2; return 1; }
  echo "$moon"
}

run_leg() { # run_leg <name> <bin> <ext> [docs] — docs required unless omitted
  local name=$1 bin=$2 ext=$3 docs=${4:-} ndocs=$#
  [ -x "$bin" ] || { echo "[leg] $name: no runnable binary ($bin) — skipped" >&2; return 1; }
  # The harness reads --lang2-docs as one text file; a directory, a missing
  # path, or a failed bundle builder (empty substitution) would silently drop
  # the leg to "(No formal language documentation available …)" and score
  # documentation instead of language. Only the pretraining-native floor
  # (`python`, `bash`) runs doc-less, by design: those two carry no resident
  # spec because no spec is needed to write them.
  if [ "$ndocs" -ge 4 ] && { [ -z "$docs" ] || [ ! -f "$docs" ]; }; then
    echo "[leg] $name: docs bundle missing, empty, or not a file ('$docs') — skipping" >&2
    return 1
  fi
  [ -n "${DEEPSEEK_API_KEY:-}" ] || { echo "[leg] $name: DEEPSEEK_API_KEY unset — skipping" >&2; return 1; }
  echo "[leg] $name: running ($bin), docs=${docs:-none}" | tee "$LOGS/$name-leg.log"
  python3 scripts/closed-loop-bench.py --model dsflash --cache "$CACHE" \
      --lang2-name "$name" --lang2-bin "$bin" --lang2-ext "$ext" \
      ${docs:+--lang2-docs "$docs"} 2>&1 | tee -a "$LOGS/$name-leg.log" | tail -18
  return "${PIPESTATUS[0]}"
}

STATUS=0
# The pretraining-native floor: python and bash carry no resident spec, so they
# are the honest baseline the spec-carrying languages are measured against.
if want python; then
  py=$(command -v python3 || true)
  [ -n "$py" ] && run_leg python "$py" .py || { [ -n "$py" ] && STATUS=1; }
fi
if want bash; then
  run_leg bash "$(command -v bash || echo /bin/bash)" .sh || STATUS=1
fi

if want zero; then
  if bash bench/zero/zero-bench.sh --selfcheck >"$LOGS/zero.selfcheck.log" 2>&1 && d=$(zero_docs); then
    run_leg zero bench/zero/zero-bench.sh .0 "$d" || STATUS=1
  else
    echo "[leg] zero: toolchain selfcheck or docs bundle failed — skipped" >&2; STATUS=1
  fi
fi

# ailang and moonbit run packages, not files; the wrapper paths keep the
# single-file contract of --lang2-bin. The build functions still gate the leg.
# Resident docs per leg: the language's own agent-facing entry points, bundled
# to one file (the harness reads --lang2-docs as text). bash takes none: it is
# the pretraining-native floor. A bundle that cannot be produced skips the leg
# rather than running it doc-less — that would score documentation, not language.
if want ailang && build_ailang >/dev/null; then
  d=$(docs_bundle ailang "$SRC/ailang/.agents/skills/use-ailang/SKILL.md" "$SRC/ailang/AGENTS.md") \
    && run_leg ailang bench/ailang/ailang-bench.sh .ail "$d" || STATUS=1
else
  want ailang && STATUS=1
fi
if want nanolang && build_nanolang >/dev/null; then
  d=$(docs_bundle nanolang "$SRC/nanolang/AGENTS.md" "$SRC/nanolang/docs/QUICK_REFERENCE.md") \
    && run_leg nanolang "$SRC/nanolang/bin/nano" .nano "$d" || STATUS=1
else
  want nanolang && STATUS=1
fi
if want moonbit && build_moonbit >/dev/null; then
  d=$(docs_bundle moonbit "$SRC/core/AGENTS.md" "$SRC/core/README.md") \
    && run_leg moonbit bench/moonbit/moonbit-bench.sh .mbt "$d" || STATUS=1
else
  want moonbit && STATUS=1
fi

echo "done (some legs may have been skipped — see $LOGS)"
exit $STATUS
