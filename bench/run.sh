#!/usr/bin/env bash
# ilo cross-language benchmark suite
# Benchmarks: fib, hof, listproc, pattern-match, sum-loop
# Languages:  ilo (VM + JIT), Python 3, Node.js (V8), Rust (native)
# Output:     bench/results.json
#
# Usage: ./bench/run.sh [--quick] [--no-rust]
#   --quick   Fewer iterations (faster, less precise)
#   --no-rust Skip Rust (avoid compile time)
set -euo pipefail

cd "$(dirname "$0")/.."

# ── Parse flags ──────────────────────────────────────────────────────────────
QUICK=false
SKIP_RUST=false
for arg in "$@"; do
    case "$arg" in
        --quick)    QUICK=true ;;
        --no-rust)  SKIP_RUST=true ;;
    esac
done

# ── Config ───────────────────────────────────────────────────────────────────
BENCH_DIR="bench"
RESULTS_FILE="$BENCH_DIR/results.json"
BUILD_DIR="$BENCH_DIR/.build"
ILO="./target/release/ilo"

BENCHMARKS=(fib hof listproc pattern-match sum-loop)

# Argument passed to each benchmark program
bench_arg() {
    case "$1" in
        fib)     echo "15" ;;
        *)       echo "1000" ;;
    esac
}

# Function name for ilo --bench and direct invocation
bench_func() {
    case "$1" in
        fib) echo "fib" ;;
        *)   echo "bench" ;;
    esac
}

# Expected correct output for correctness checks
expected_for() {
    case "$1" in
        fib)           echo "610" ;;
        hof)           echo "332833500" ;;
        listproc)      echo "3417" ;;
        pattern-match) echo "1386050" ;;
        sum-loop)      echo "1353850" ;;
    esac
}

# ── Helpers ──────────────────────────────────────────────────────────────────
check_cmd() { command -v "$1" >/dev/null 2>&1; }

section() {
    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo "  $1"
    echo "═══════════════════════════════════════════════════════════"
}

# Extract "per call: NNNns" from output
extract_ns() {
    echo "$1" | sed -n 's/.*per call:[[:space:]]*\([0-9]*\)ns/\1/p' | tail -1
}

# ── Results store ─────────────────────────────────────────────────────────────
RESULTS_TMP=$(mktemp)
trap "rm -f $RESULTS_TMP" EXIT

record() {
    local bench="$1" lang="$2" ns="$3"
    echo "${bench}|${lang}|${ns}" >> "$RESULTS_TMP"
}

# ── Build ilo ────────────────────────────────────────────────────────────────
section "Building ilo (release)"
if check_cmd cargo; then
    if cargo build --release --features cranelift 2>/dev/null; then
        echo "  Built with Cranelift JIT"
    else
        cargo build --release
        echo "  Built without Cranelift JIT"
    fi
fi
if [[ ! -x "$ILO" ]]; then
    echo "  ERROR: $ILO not found. Run: cargo build --release" >&2
    exit 1
fi

# ── Verify ilo correctness ───────────────────────────────────────────────────
section "Verifying ilo programs"
all_ok=true
for bench in "${BENCHMARKS[@]}"; do
    ilo_file="$BENCH_DIR/$bench/$bench.ilo"
    arg=$(bench_arg "$bench")
    func=$(bench_func "$bench")
    result=$("$ILO" "$ilo_file" "$func" "$arg" 2>/dev/null || echo "ERROR")
    expected=$(expected_for "$bench")
    if [[ "$result" == "$expected" ]]; then
        echo "  $bench: OK ($result)"
    else
        echo "  $bench: FAIL — expected $expected, got '$result'" >&2
        all_ok=false
    fi
done
if [[ "$all_ok" != "true" ]]; then
    echo "" >&2
    echo "ERROR: Correctness check failed. Fix ilo programs before benchmarking." >&2
    exit 1
fi

# ── Compile Rust baselines ───────────────────────────────────────────────────
if [[ "$SKIP_RUST" == "false" ]] && check_cmd rustc; then
    section "Compiling Rust baselines"
    mkdir -p "$BUILD_DIR"
    for bench in "${BENCHMARKS[@]}"; do
        rs="$BENCH_DIR/$bench/$bench.rs"
        out="$BUILD_DIR/${bench}_rs"
        if [[ -f "$rs" ]]; then
            if rustc -O -o "$out" "$rs" 2>/dev/null; then
                echo "  rustc: $bench OK"
            else
                echo "  rustc: $bench FAIL (skipping)"
            fi
        fi
    done
else
    [[ "$SKIP_RUST" == "true" ]] || true
fi

# ── Run benchmarks ───────────────────────────────────────────────────────────
for bench in "${BENCHMARKS[@]}"; do
    arg=$(bench_arg "$bench")
    func=$(bench_func "$bench")
    ilo_file="$BENCH_DIR/$bench/$bench.ilo"

    section "$bench (arg=$arg)"

    # ilo
    echo "--- ilo ---"
    ilo_out=$("$ILO" "$ilo_file" --bench "$func" "$arg" 2>&1 || true)
    echo "$ilo_out"

    # Parse JSON-lines output: {"engine":"vm","variant":"reusable","perCallNs":N,...}
    vm_ns=$(echo "$ilo_out" | python3 -c "
import sys,json
for line in sys.stdin:
    try:
        d=json.loads(line)
        if d.get('engine')=='vm' and d.get('variant')=='reusable':
            print(d['perCallNs'])
    except: pass
" 2>/dev/null || true)
    jit_ns=$(echo "$ilo_out" | python3 -c "
import sys,json
for line in sys.stdin:
    try:
        d=json.loads(line)
        if d.get('engine')=='jit':
            print(d['perCallNs'])
    except: pass
" 2>/dev/null || true)
    [[ -n "$vm_ns"  ]] && record "$bench" "ilo-vm"  "$vm_ns"
    [[ -n "$jit_ns" ]] && record "$bench" "ilo-jit" "$jit_ns"

    # Rust
    if [[ "$SKIP_RUST" == "false" ]] && [[ -x "$BUILD_DIR/${bench}_rs" ]]; then
        echo "--- Rust ---"
        rs_out=$("$BUILD_DIR/${bench}_rs" "$arg" 2>&1 || true)
        echo "$rs_out"
        ns=$(extract_ns "$rs_out")
        [[ -n "$ns" ]] && record "$bench" "Rust" "$ns"
    fi

    # Node.js
    js_file="$BENCH_DIR/$bench/$bench.js"
    if check_cmd node && [[ -f "$js_file" ]]; then
        echo "--- Node.js ---"
        node_out=$(node "$js_file" "$arg" 2>&1 || true)
        echo "$node_out"
        ns=$(extract_ns "$node_out")
        [[ -n "$ns" ]] && record "$bench" "Node" "$ns"
    fi

    # Python
    py_file="$BENCH_DIR/$bench/$bench.py"
    if check_cmd python3 && [[ -f "$py_file" ]]; then
        echo "--- Python 3 ---"
        py_out=$(python3 "$py_file" "$arg" 2>&1 || true)
        echo "$py_out"
        ns=$(extract_ns "$py_out")
        [[ -n "$ns" ]] && record "$bench" "Python" "$ns"
    fi
done

# ── Summary table ────────────────────────────────────────────────────────────
section "Summary: per-call time (ns)"

LANGS="ilo-jit ilo-vm Rust Node Python"

printf "\n%-16s" "Benchmark"
for lang in $LANGS; do
    printf " %10s" "$lang"
done
printf "\n%-16s" "----------------"
for lang in $LANGS; do
    printf " %10s" "----------"
done
printf "\n"

for bench in "${BENCHMARKS[@]}"; do
    printf "%-16s" "$bench"
    for lang in $LANGS; do
        val=$(awk -F'|' -v b="$bench" -v l="$lang" '$1==b && $2==l {print $3}' "$RESULTS_TMP")
        printf " %10s" "${val:-"-"}"
    done
    printf "\n"
done

# ── Emit results.json ─────────────────────────────────────────────────────────
section "Writing $RESULTS_FILE"

python3 - "$RESULTS_TMP" "$RESULTS_FILE" "$BENCH_DIR/.hw-info.json" << 'PYEOF'
import sys, json, datetime, os, pathlib

results_tmp  = sys.argv[1]
out_path     = sys.argv[2]
hw_info_path = sys.argv[3] if len(sys.argv) > 3 else None

data = {}
with open(results_tmp) as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
        bench, lang, ns = line.split("|")
        data.setdefault(bench, {})[lang] = int(ns)

# Collect hardware info: prefer the pre-written .hw-info.json (CI path),
# fall back to live detection (local runs).
hw = {}
if hw_info_path and pathlib.Path(hw_info_path).exists():
    hw = json.loads(pathlib.Path(hw_info_path).read_text())
else:
    cpu_model = "unknown"
    cpu_count = os.cpu_count() or 0
    mem_gb    = 0
    try:
        for l in pathlib.Path("/proc/cpuinfo").read_text().splitlines():
            if l.startswith("model name"):
                cpu_model = l.split(":", 1)[1].strip()
                break
    except Exception:
        pass
    try:
        mem_kb = int(next(
            l.split()[1] for l in pathlib.Path("/proc/meminfo").read_text().splitlines()
            if l.startswith("MemTotal")
        ))
        mem_gb = round(mem_kb / 1024 / 1024, 1)
    except Exception:
        pass
    hw = {"cpu_model": cpu_model, "cpu_count": cpu_count, "mem_gb": mem_gb}

output = {
    "generated": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "hardware": hw,
    "benchmarks": data,
}

with open(out_path, "w") as f:
    json.dump(output, f, indent=2)

print(f"  Written {out_path}")
PYEOF

section "Done"
echo "  Results saved to $RESULTS_FILE"
echo ""
