#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."

# ── Ensure cargo/rustc are in PATH ──────────────────────────────────
if [[ -f "$HOME/.cargo/env" ]]; then
    source "$HOME/.cargo/env"
fi
export PATH="$HOME/.cargo/bin:$PATH"

# ── Config ──────────────────────────────────────────────────────────
ILO=./target/release/ilo
DIR=research/explorations/bench-realistic
BUILD_DIR="$DIR/.build"

BENCHMARKS=(fib records nested-calls guards sum-loop strings hashmap listproc matmul pattern-match)
FIB_ARG=15
LOOP_ARG=1000

# Benchmark metadata: func name to call with --bench and data-type tag
bench_func() {
    case "$1" in
        fib) echo "fib" ;;
        records) echo "run" ;;
        *) echo "bench" ;;
    esac
}

bench_tag() {
    case "$1" in
        fib) echo "recursive" ;;
        records|hashmap) echo "record" ;;
        nested-calls) echo "call-heavy" ;;
        guards|pattern-match) echo "branching" ;;
        sum-loop|listproc|matmul) echo "numeric" ;;
        strings) echo "string" ;;
        *) echo "other" ;;
    esac
}

# ── Helpers ─────────────────────────────────────────────────────────
check_cmd() { command -v "$1" >/dev/null 2>&1; }

section() {
    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo "  $1"
    echo "═══════════════════════════════════════════════════════════"
}

subsection() {
    echo ""
    echo "--- $1 ---"
}

skip() {
    echo "  [SKIP] $1 not found"
}

bench_arg() {
    if [[ "$1" == "fib" ]]; then echo "$FIB_ARG"; else echo "$LOOP_ARG"; fi
}

# Extract per-call ns from output (line: "per call:   12345ns")
extract_ns() {
    local output="$1"
    echo "$output" | sed -n 's/.*per call:[[:space:]]*\([0-9]*\)ns/\1/p' | tail -1
}

# ── Results storage ─────────────────────────────────────────────────
RESULTS_FILE=$(mktemp)
record_result() {
    local bench="$1" lang="$2" ns="$3"
    echo "${bench}|${lang}|${ns}" >> "$RESULTS_FILE"
}

# ── Build ilo release ───────────────────────────────────────────────
section "Building ilo (release + cranelift)"

if check_cmd cargo; then
    if cargo build --release --features cranelift 2>/dev/null; then
        echo "  Built with cranelift"
    else
        cargo build --release
        echo "  Built without cranelift"
    fi
else
    echo "  [SKIP] cargo not found, using existing binary"
fi

if [[ ! -x "$ILO" ]]; then
    echo "  ERROR: $ILO not found. Build ilo first."
    exit 1
fi

# # ── Compile native benchmarks ───────────────────────────────────────
# section "Compiling native benchmarks"
# mkdir -p "$BUILD_DIR"
#
# # Rust
# if check_cmd rustc; then
#     for bench in "${BENCHMARKS[@]}"; do
#         if [[ -f "$DIR/$bench.rs" ]]; then
#             rustc -O -o "$BUILD_DIR/${bench}_rs" "$DIR/$bench.rs" 2>/dev/null && \
#                 echo "  rustc: $bench OK" || echo "  rustc: $bench FAIL"
#         fi
#     done
# else echo "  [SKIP] rustc not found"; fi
# # C
# if check_cmd cc; then
#     for bench in "${BENCHMARKS[@]}"; do
#         if [[ -f "$DIR/$bench.c" ]]; then
#             cc -O2 -o "$BUILD_DIR/${bench}_c" "$DIR/$bench.c" 2>/dev/null && \
#                 echo "  cc:    $bench OK" || echo "  cc:    $bench FAIL"
#         fi
#     done
# else echo "  [SKIP] cc not found"; fi
# # Go
# if check_cmd go; then
#     for bench in "${BENCHMARKS[@]}"; do
#         if [[ -f "$DIR/$bench.go" ]]; then
#             go build -o "$BUILD_DIR/${bench}_go" "$DIR/$bench.go" 2>/dev/null && \
#                 echo "  go:    $bench OK" || echo "  go:    $bench FAIL"
#         fi
#     done
# else echo "  [SKIP] go not found"; fi

# ── Verify all ilo programs produce correct results ─────────────────
section "Verifying ilo programs"

expected_for() {
    case "$1" in
        fib) echo 610 ;;
        records) echo 3497500 ;;
        nested-calls) echo 334333000 ;;
        guards) echo 17500 ;;
        sum-loop) echo 1353850 ;;
        strings) echo 7890 ;;
        hashmap) echo 3996000 ;;
        listproc) echo 3417 ;;
        matmul) echo 454425000 ;;
        pattern-match) echo 1386050 ;;
    esac
}

all_ok=true
for bench in "${BENCHMARKS[@]}"; do
    arg=$(bench_arg "$bench")
    result=$($ILO "$DIR/$bench.ilo" "$arg" 2>/dev/null || echo "ERROR")
    expected=$(expected_for "$bench")
    if [[ "$result" == "$expected" ]]; then
        echo "  $bench: OK ($result)"
    else
        echo "  $bench: FAIL (expected $expected, got $result)"
        all_ok=false
    fi
done

if [[ "$all_ok" != "true" ]]; then
    echo ""
    echo "ERROR: Some ilo programs produced incorrect results. Fix before benchmarking."
    exit 1
fi

# ── Run benchmarks ──────────────────────────────────────────────────

run_and_record() {
    local bench="$1" lang="$2" output="$3"
    local ns
    ns=$(extract_ns "$output")
    if [[ -n "$ns" ]]; then
        record_result "$bench" "$lang" "$ns"
    fi
    echo "$output"
}

for bench in "${BENCHMARKS[@]}"; do
    arg=$(bench_arg "$bench")
    func="$(bench_func "$bench")"
    tag="$(bench_tag "$bench")"

    section "$bench [$tag] — arg=$arg"

    # ilo (all modes via --bench)
    subsection "ilo"
    ilo_output=$($ILO "$DIR/$bench.ilo" --bench "$func" "$arg" 2>&1 || true)
    echo "$ilo_output"
    # Extract ilo modes from the output
    interp_ns=$(echo "$ilo_output" | awk '/^Rust interpreter$/{found=1} found && /per call:/{gsub(/[^0-9]/,"",$NF); print $NF; found=0}')
    vm_ns=$(echo "$ilo_output" | awk '/^Register VM$/{found=1} found && /per call:/{gsub(/[^0-9]/,"",$NF); print $NF; found=0}')
    jit_ns=$(echo "$ilo_output" | awk '/^Cranelift JIT$/{found=1} found && /per call:/{gsub(/[^0-9]/,"",$NF); print $NF; found=0}')
    [[ -n "$interp_ns" ]] && record_result "$bench" "ilo-interp" "$interp_ns"
    [[ -n "$vm_ns" ]] && record_result "$bench" "ilo-vm" "$vm_ns"
    [[ -n "$jit_ns" ]] && record_result "$bench" "ilo-jit" "$jit_ns"

    # # Rust (compiled)
    # subsection "Rust (native)"
    # if [[ -x "$BUILD_DIR/${bench}_rs" ]]; then
    #     out=$("$BUILD_DIR/${bench}_rs" "$arg" 2>&1 || true)
    #     run_and_record "$bench" "Rust" "$out"
    # else
    #     skip "rustc binary"
    # fi

    # # C (compiled)
    # subsection "C (native)"
    # if [[ -x "$BUILD_DIR/${bench}_c" ]]; then
    #     out=$("$BUILD_DIR/${bench}_c" "$arg" 2>&1 || true)
    #     run_and_record "$bench" "C" "$out"
    # else
    #     skip "cc binary"
    # fi

    # # Go (compiled)
    # subsection "Go (native)"
    # if [[ -x "$BUILD_DIR/${bench}_go" ]]; then
    #     out=$("$BUILD_DIR/${bench}_go" "$arg" 2>&1 || true)
    #     run_and_record "$bench" "Go" "$out"
    # else
    #     skip "go binary"
    # fi

    # Node.js (V8 JIT)
    subsection "Node.js (V8)"
    if check_cmd node && [[ -f "$DIR/$bench.js" ]]; then
        out=$(node "$DIR/$bench.js" "$arg" 2>&1 || true)
        run_and_record "$bench" "Node" "$out"
    else
        skip "node"
    fi

    # LuaJIT
    subsection "LuaJIT"
    if check_cmd luajit && [[ -f "$DIR/$bench.lua" ]]; then
        out=$(luajit "$DIR/$bench.lua" "$arg" 2>&1 || true)
        run_and_record "$bench" "LuaJIT" "$out"
    else
        skip "luajit"
    fi

    # Python 3 (CPython)
    subsection "Python 3 (CPython)"
    if check_cmd python3 && [[ -f "$DIR/$bench.py" ]]; then
        out=$(python3 "$DIR/$bench.py" "$arg" 2>&1 || true)
        run_and_record "$bench" "Python" "$out"
    else
        skip "python3"
    fi

    # # Ruby
    # subsection "Ruby"
    # if check_cmd ruby && [[ -f "$DIR/$bench.rb" ]]; then
    #     out=$(ruby "$DIR/$bench.rb" "$arg" 2>&1 || true)
    #     run_and_record "$bench" "Ruby" "$out"
    # else
    #     skip "ruby"
    # fi
done

# ── Summary table ───────────────────────────────────────────────────
section "Summary: per-call time (ns)"

# Column order
LANGS="LuaJIT Node Python ilo-interp ilo-vm ilo-jit"

# Print header
printf "\n%-16s %-6s" "Benchmark" "Type"
for lang in $LANGS; do
    printf " %10s" "$lang"
done
printf "\n"
printf "%-16s %-6s" "----------------" "------"
for lang in $LANGS; do
    printf " %10s" "----------"
done
printf "\n"

# Print rows
for bench in "${BENCHMARKS[@]}"; do
    tag="$(bench_tag "$bench")"
    printf "%-16s %-6s" "$bench" "$tag"
    for lang in $LANGS; do
        val=$(awk -F'|' -v b="$bench" -v l="$lang" '$1==b && $2==l {print $3}' "$RESULTS_FILE")
        if [[ -n "$val" ]]; then
            printf " %10s" "$val"
        else
            printf " %10s" "-"
        fi
    done
    printf "\n"
done

# Print markdown version
echo ""
echo "--- Markdown table ---"
echo ""
printf "| %-16s | %-10s |" "Benchmark" "Type"
for lang in $LANGS; do
    printf " %-10s |" "$lang"
done
printf "\n"
printf "|%-18s|%-12s|" "------------------" "------------"
for lang in $LANGS; do
    printf "%-12s|" "------------"
done
printf "\n"

for bench in "${BENCHMARKS[@]}"; do
    tag="$(bench_tag "$bench")"
    printf "| %-16s | %-10s |" "$bench" "$tag"
    for lang in $LANGS; do
        val=$(awk -F'|' -v b="$bench" -v l="$lang" '$1==b && $2==l {print $3}' "$RESULTS_FILE")
        if [[ -n "$val" ]]; then
            printf " %-10s |" "${val}ns"
        else
            printf " %-10s |" "-"
        fi
    done
    printf "\n"
done

rm -f "$RESULTS_FILE"

# ── Done ────────────────────────────────────────────────────────────
section "Done"
echo "  All realistic benchmarks complete."
echo ""
