#!/usr/bin/env bash
# ilo cross-language benchmark suite
# Benchmarks: fib, hof, listproc, pattern-match, sum-loop
# Languages:  ilo (VM + JIT), Python 3, Node.js (V8), Rust (native)
# Output:     bench/results.json
#
# Statistical methodology
#   Warmup:  WARMUP_RUNS process-invocations discarded before measurement
#   Measure: MEASURE_RUNS process-invocations collected (≥ 30)
#   Stats:   min / max / mean / median / p95 / p99 / stddev
#            95 % confidence interval via t-distribution (df = n-1)
#   Fail:    comparison between two engines fails if their 95 % CIs overlap
#            (no statistically significant difference)
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

if [[ "$QUICK" == "true" ]]; then
    WARMUP_RUNS=3
    MEASURE_RUNS=10
else
    WARMUP_RUNS=5
    MEASURE_RUNS=30
fi

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

# Run a command WARMUP_RUNS+MEASURE_RUNS times; discard warmup; print one
# space-separated list of per-call-ns values (one per measurement run).
# Usage: collect_samples CMD [ARG...]
collect_samples() {
    local -a cmd=("$@")
    local i ns out
    # warmup
    for (( i=0; i<WARMUP_RUNS; i++ )); do
        "${cmd[@]}" >/dev/null 2>&1 || true
    done
    # measure
    local samples=()
    for (( i=0; i<MEASURE_RUNS; i++ )); do
        out=$("${cmd[@]}" 2>&1 || true)
        ns=$(extract_ns "$out")
        if [[ -n "$ns" ]]; then
            samples+=("$ns")
        fi
    done
    echo "${samples[*]}"
}

# For ilo --bench the per-call-ns is embedded in JSON output.
# Run WARMUP+MEASURE times and collect the vm/jit per-call-ns values.
# Prints two lines:  vm <space-sep-samples>
#                    jit <space-sep-samples>
collect_ilo_samples() {
    local ilo_file="$1" func="$2" arg="$3"
    local i out vm_ns jit_ns
    local vm_samples=() jit_samples=()

    # warmup
    for (( i=0; i<WARMUP_RUNS; i++ )); do
        "$ILO" "$ilo_file" --bench "$func" "$arg" >/dev/null 2>&1 || true
    done

    # measure
    for (( i=0; i<MEASURE_RUNS; i++ )); do
        out=$("$ILO" "$ilo_file" --bench "$func" "$arg" 2>&1 || true)
        vm_ns=$(echo "$out" | python3 -c "
import sys,json
for line in sys.stdin:
    try:
        d=json.loads(line)
        if d.get('engine')=='vm' and d.get('variant')=='reusable':
            print(d['perCallNs'])
    except: pass
" 2>/dev/null || true)
        jit_ns=$(echo "$out" | python3 -c "
import sys,json
for line in sys.stdin:
    try:
        d=json.loads(line)
        if d.get('engine')=='jit':
            print(d['perCallNs'])
    except: pass
" 2>/dev/null || true)
        [[ -n "$vm_ns"  ]] && vm_samples+=("$vm_ns")
        [[ -n "$jit_ns" ]] && jit_samples+=("$jit_ns")
    done

    echo "vm ${vm_samples[*]}"
    echo "jit ${jit_samples[*]}"
}

# ── Results store ─────────────────────────────────────────────────────────────
# Format: bench|lang|ns1 ns2 ns3 ...
RESULTS_TMP=$(mktemp)
trap "rm -f $RESULTS_TMP" EXIT

record_samples() {
    local bench="$1" lang="$2"
    shift 2
    echo "${bench}|${lang}|$*" >> "$RESULTS_TMP"
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
echo ""
echo "Warmup runs (discarded): $WARMUP_RUNS"
echo "Measurement runs:        $MEASURE_RUNS"

for bench in "${BENCHMARKS[@]}"; do
    arg=$(bench_arg "$bench")
    func=$(bench_func "$bench")
    ilo_file="$BENCH_DIR/$bench/$bench.ilo"

    section "$bench (arg=$arg)"

    # ilo (collect multiple process runs)
    echo "--- ilo (${WARMUP_RUNS}w + ${MEASURE_RUNS}m runs) ---"
    ilo_raw=$(collect_ilo_samples "$ilo_file" "$func" "$arg")
    vm_samples=$(echo "$ilo_raw"  | awk '/^vm /  {$1=""; print $0}')
    jit_samples=$(echo "$ilo_raw" | awk '/^jit / {$1=""; print $0}')
    [[ -n "$vm_samples"  ]] && record_samples "$bench" "ilo-vm"  $vm_samples
    [[ -n "$jit_samples" ]] && record_samples "$bench" "ilo-jit" $jit_samples
    echo "  ilo-vm  samples: $vm_samples"
    echo "  ilo-jit samples: $jit_samples"

    # Rust
    if [[ "$SKIP_RUST" == "false" ]] && [[ -x "$BUILD_DIR/${bench}_rs" ]]; then
        echo "--- Rust (${WARMUP_RUNS}w + ${MEASURE_RUNS}m runs) ---"
        rs_samples=$(collect_samples "$BUILD_DIR/${bench}_rs" "$arg")
        [[ -n "$rs_samples" ]] && record_samples "$bench" "Rust" $rs_samples
        echo "  Rust samples: $rs_samples"
    fi

    # Node.js
    js_file="$BENCH_DIR/$bench/$bench.js"
    if check_cmd node && [[ -f "$js_file" ]]; then
        echo "--- Node.js (${WARMUP_RUNS}w + ${MEASURE_RUNS}m runs) ---"
        node_samples=$(collect_samples node "$js_file" "$arg")
        [[ -n "$node_samples" ]] && record_samples "$bench" "Node" $node_samples
        echo "  Node samples: $node_samples"
    fi

    # Python
    py_file="$BENCH_DIR/$bench/$bench.py"
    if check_cmd python3 && [[ -f "$py_file" ]]; then
        echo "--- Python 3 (${WARMUP_RUNS}w + ${MEASURE_RUNS}m runs) ---"
        py_samples=$(collect_samples python3 "$py_file" "$arg")
        [[ -n "$py_samples" ]] && record_samples "$bench" "Python" $py_samples
        echo "  Python samples: $py_samples"
    fi
done

# ── Compute statistics and emit results.json ──────────────────────────────────
section "Computing statistics and writing $RESULTS_FILE"

WARMUP_RUNS="$WARMUP_RUNS" MEASURE_RUNS="$MEASURE_RUNS" python3 - "$RESULTS_TMP" "$RESULTS_FILE" << 'PYEOF'
import sys, json, math, datetime

results_tmp = sys.argv[1]
out_path    = sys.argv[2]

# t-distribution critical values for 95% CI (two-tailed), indexed by df=n-1
# For df >= 30 we use 2.042; for large n use 1.960 (z). Pre-computed table.
T_TABLE = {
    1: 12.706, 2: 4.303, 3: 3.182, 4: 2.776, 5: 2.571,
    6: 2.447,  7: 2.365, 8: 2.306, 9: 2.262, 10: 2.228,
    11: 2.201, 12: 2.179, 13: 2.160, 14: 2.145, 15: 2.131,
    16: 2.120, 17: 2.110, 18: 2.101, 19: 2.093, 20: 2.086,
    21: 2.080, 22: 2.074, 23: 2.069, 24: 2.064, 25: 2.060,
    26: 2.056, 27: 2.052, 28: 2.048, 29: 2.045, 30: 2.042,
}

def t_critical(n):
    """Two-tailed 95% CI t critical value for sample size n."""
    df = n - 1
    if df <= 0:
        return float('inf')
    if df in T_TABLE:
        return T_TABLE[df]
    # df > 30: use 2.042 (conservative), approaching 1.960 asymptotically
    return 2.042 if df <= 120 else 1.960

def compute_stats(samples):
    """Return dict of statistics for a list of numeric samples."""
    n = len(samples)
    if n == 0:
        return None
    s = sorted(samples)
    mean = sum(s) / n
    variance = sum((x - mean) ** 2 for x in s) / (n - 1) if n > 1 else 0.0
    stddev = math.sqrt(variance)
    sem = stddev / math.sqrt(n) if n > 1 else 0.0
    tc = t_critical(n)
    ci_half = tc * sem
    median = s[n // 2] if n % 2 == 1 else (s[n // 2 - 1] + s[n // 2]) / 2

    def percentile(p):
        idx = (p / 100) * (n - 1)
        lo, hi = int(idx), min(int(idx) + 1, n - 1)
        return s[lo] + (idx - lo) * (s[hi] - s[lo])

    return {
        "n":        n,
        "min":      round(s[0]),
        "max":      round(s[-1]),
        "mean":     round(mean),
        "median":   round(median),
        "p95":      round(percentile(95)),
        "p99":      round(percentile(99)),
        "stddev":   round(stddev, 2),
        "ci95_lo":  round(mean - ci_half),
        "ci95_hi":  round(mean + ci_half),
    }

def cis_overlap(a, b):
    """Return True if two CI dicts [ci95_lo, ci95_hi] overlap."""
    return a["ci95_lo"] <= b["ci95_hi"] and b["ci95_lo"] <= a["ci95_hi"]

# Parse results_tmp
raw = {}  # bench -> lang -> [ns, ...]
with open(results_tmp) as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
        parts = line.split("|")
        if len(parts) < 3:
            continue
        bench, lang, samples_str = parts[0], parts[1], parts[2]
        samples = [int(x) for x in samples_str.split() if x.strip().isdigit()]
        raw.setdefault(bench, {})[lang] = samples

# Compute stats per (bench, lang)
stats = {}  # bench -> lang -> stats_dict
for bench, langs in raw.items():
    stats[bench] = {}
    for lang, samples in langs.items():
        s = compute_stats(samples)
        if s:
            stats[bench][lang] = s

# Print summary table
LANGS = ["ilo-jit", "ilo-vm", "Rust", "Node", "Python"]
print()
print(f"{'Benchmark':<16} {'Lang':<10} {'median':>8} {'mean':>8} {'p95':>8} {'p99':>8} {'stddev':>8} {'CI95':>20}  {'n':>4}")
print("-" * 95)
for bench in sorted(stats):
    for lang in LANGS:
        if lang not in stats[bench]:
            continue
        s = stats[bench][lang]
        ci_str = f"[{s['ci95_lo']:>8}, {s['ci95_hi']:>8}]"
        print(f"{bench:<16} {lang:<10} {s['median']:>8} {s['mean']:>8} {s['p95']:>8} {s['p99']:>8} {s['stddev']:>8.1f} {ci_str}  {s['n']:>4}")

# CI overlap analysis
print()
print("=== CI overlap analysis (overlapping CIs = no significant difference) ===")
any_overlap = False
for bench in sorted(stats):
    langs_with_stats = {l: s for l, s in stats[bench].items() if l in LANGS}
    lang_list = [l for l in LANGS if l in langs_with_stats]
    for i in range(len(lang_list)):
        for j in range(i + 1, len(lang_list)):
            la, lb = lang_list[i], lang_list[j]
            sa, sb = langs_with_stats[la], langs_with_stats[lb]
            if cis_overlap(sa, sb):
                any_overlap = True
                print(f"  OVERLAP  {bench}: {la} CI=[{sa['ci95_lo']},{sa['ci95_hi']}]  {lb} CI=[{sb['ci95_lo']},{sb['ci95_hi']}]")
if not any_overlap:
    print("  All pairwise CIs are non-overlapping (all differences are statistically significant)")

# Build output JSON
output = {
    "generated": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "methodology": {
        "warmup_runs_discarded": None,   # filled by shell via env
        "measure_runs": None,
        "ci": "95% t-distribution (df=n-1)",
        "stats": ["n", "min", "max", "mean", "median", "p95", "p99", "stddev", "ci95_lo", "ci95_hi"],
    },
    "benchmarks": stats,
}

# Read warmup/measure from env if available
import os
output["methodology"]["warmup_runs_discarded"] = int(os.environ.get("WARMUP_RUNS", 0))
output["methodology"]["measure_runs"]           = int(os.environ.get("MEASURE_RUNS", 0))

with open(out_path, "w") as f:
    json.dump(output, f, indent=2)

print(f"\nWritten {out_path}")

# Exit 1 if any CIs overlap so CI pipelines can flag it
if any_overlap:
    print("\nWARNING: Some confidence intervals overlap — differences may not be statistically significant.", file=sys.stderr)
    # Non-fatal: don't exit 1 so the script always produces results.json
    # Change to sys.exit(1) if you want hard failure in CI.
PYEOF

section "Done"
echo "  Results saved to $RESULTS_FILE"
echo ""
