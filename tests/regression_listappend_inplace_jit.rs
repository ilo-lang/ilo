// Regression tests for the in-process Cranelift JIT in-place OP_LISTAPPEND
// peephole, ported from the AOT path in `src/vm/compile_cranelift.rs`.
//
// Background:
//
// Commit `74668bb` ("prevent OP_LISTAPPEND non-rebind aliasing on VM and
// Cranelift") split the JIT helper into two:
//
//   * `jit_listappend`         — always clones the source list (safe for the
//                                non-rebind shape `ys = +=xs item`).
//   * `jit_listappend_inplace` — RC=1 in-place fast path with fall-back clone
//                                (safe only when destination and source are
//                                the same SSA variable).
//
// It then updated the AOT lowering (`src/vm/compile_cranelift.rs`) to pick
// `listappend_inplace` only when `a_idx == b_idx` (the rebind shape the
// compiler peephole `name = += name item` guarantees). But it forgot to make
// the same change in the in-process JIT (`src/vm/jit_cranelift.rs`), which
// continued to call the clone-only `listappend` unconditionally.
//
// Effect: every iteration of a foreach-build accumulator (`xs = += xs item`)
// on the default engine cloned the entire list, giving O(n²) memory and time.
// A 210k-line log forensics workload that ran end-to-end on v0.11.2 was
// OOM-killed at ~51 GB RSS on v0.11.3. Smaller measurements with the v0.11.3
// release binary showed Cranelift in-process JIT using 32× more memory than
// tree/VM for the same 10k accumulator workload, while v0.11.2's JIT used
// roughly the same as tree/VM (linear).
//
// Fix: port the `a_idx == b_idx` peephole into `jit_cranelift.rs` verbatim.
// This file pins:
//
//   1. The rebind-shape foreach accumulator scales linearly in wall-clock on
//      every engine, including the default in-process Cranelift JIT.
//   2. The non-rebind shape (`ys = +=xs item`) still leaves `xs` untouched
//      across all engines (the alias-fix from `74668bb` must hold).
//   3. The accumulator produces correct output (length + final-element check)
//      on all engines, so the perf optimisation didn't trade off correctness.
//
// The size (50k) is calibrated so the pre-fix O(n²) JIT path takes long
// enough to be unambiguously distinguishable from the linear path without
// burning excessive CI time. Pre-fix at 50k: roughly tens of seconds and
// gigabytes of RSS. Post-fix: under a second.

use std::process::Command;
use std::time::{Duration, Instant};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// ── Correctness: 50k-element rebind accumulator returns the right list ──────
//
// Numeric and text variants exercise both the immediate-NaN-tagged item path
// and the heap-RC item path through the JIT helper.

const REBIND_50K_NUMERIC_LEN: &str =
    "build n:n>n;xs=[];@i 0..n{xs=+=xs i};len xs\ndemo>n;build 50000";

const REBIND_50K_NUMERIC_LAST: &str =
    "build n:n>n;xs=[];@i 0..n{xs=+=xs i};at xs (-n 1) ?? -1\ndemo>n;build 50000";

#[test]
fn rebind_50k_numeric_len_tree() {
    assert_eq!(run("--run-tree", REBIND_50K_NUMERIC_LEN, "demo"), "50000");
}

#[test]
fn rebind_50k_numeric_len_vm() {
    assert_eq!(run("--run-vm", REBIND_50K_NUMERIC_LEN, "demo"), "50000");
}

#[test]
#[cfg(feature = "cranelift")]
fn rebind_50k_numeric_len_cranelift() {
    assert_eq!(
        run("--run-cranelift", REBIND_50K_NUMERIC_LEN, "demo"),
        "50000"
    );
}

#[test]
fn rebind_50k_numeric_last_tree() {
    assert_eq!(run("--run-tree", REBIND_50K_NUMERIC_LAST, "demo"), "49999");
}

#[test]
fn rebind_50k_numeric_last_vm() {
    assert_eq!(run("--run-vm", REBIND_50K_NUMERIC_LAST, "demo"), "49999");
}

#[test]
#[cfg(feature = "cranelift")]
fn rebind_50k_numeric_last_cranelift() {
    assert_eq!(
        run("--run-cranelift", REBIND_50K_NUMERIC_LAST, "demo"),
        "49999"
    );
}

// ── Scale: 50k-element rebind accumulator must finish well under O(n²) ──────
//
// Pre-fix on the in-process Cranelift JIT this took tens of seconds with
// multi-GB peak RSS. Post-fix it's well under a second. 15s ceiling is
// generous enough for slow CI runners while catching any regression back to
// the cloning path (which on 50k would always exceed it on the JIT).

#[test]
fn rebind_50k_accumulator_under_15s_tree() {
    let start = Instant::now();
    let out = run("--run-tree", REBIND_50K_NUMERIC_LEN, "demo");
    let elapsed = start.elapsed();
    assert_eq!(out, "50000");
    assert!(
        elapsed < Duration::from_secs(15),
        "tree 50k rebind accumulator took {elapsed:?} — expected <15s"
    );
}

#[test]
fn rebind_50k_accumulator_under_15s_vm() {
    let start = Instant::now();
    let out = run("--run-vm", REBIND_50K_NUMERIC_LEN, "demo");
    let elapsed = start.elapsed();
    assert_eq!(out, "50000");
    assert!(
        elapsed < Duration::from_secs(15),
        "vm 50k rebind accumulator took {elapsed:?} — expected <15s"
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn rebind_50k_accumulator_under_15s_cranelift() {
    // This is the test that the original v0.11.3 regression failed: the
    // in-process JIT cloned the list on every iteration, so 50k iterations
    // ran in O(n²) and took roughly a minute with multi-GB RSS. With the
    // a_idx==b_idx peephole ported into jit_cranelift.rs, runtime drops to
    // sub-second.
    let start = Instant::now();
    let out = run("--run-cranelift", REBIND_50K_NUMERIC_LEN, "demo");
    let elapsed = start.elapsed();
    assert_eq!(out, "50000");
    assert!(
        elapsed < Duration::from_secs(15),
        "cranelift 50k rebind accumulator took {elapsed:?} — expected <15s. \
         The OP_LISTAPPEND a_idx==b_idx peephole in jit_cranelift.rs may have \
         regressed back to the cloning helper."
    );
}

// ── Alias fix from 74668bb still holds: distinct dest preserves source ──────
//
// `ys = +=xs item` must NOT mutate `xs` on any engine. The cloning helper is
// the only correct path here; the fix this file pins is purely the perf
// fast path on the rebind shape, not a semantics change.

const NON_REBIND_DISTINCT_PRESERVES_XS: &str = "f>L n;xs=[1,2,3];ys=+=xs 99;xs";

#[test]
fn non_rebind_distinct_preserves_xs_tree() {
    assert_eq!(
        run("--run-tree", NON_REBIND_DISTINCT_PRESERVES_XS, "f"),
        "[1, 2, 3]"
    );
}

#[test]
fn non_rebind_distinct_preserves_xs_vm() {
    assert_eq!(
        run("--run-vm", NON_REBIND_DISTINCT_PRESERVES_XS, "f"),
        "[1, 2, 3]"
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn non_rebind_distinct_preserves_xs_cranelift() {
    assert_eq!(
        run("--run-cranelift", NON_REBIND_DISTINCT_PRESERVES_XS, "f"),
        "[1, 2, 3]"
    );
}

// ── Text-item rebind: heap-RC item path through the in-place helper ─────────
//
// Numbers are immediate NaN-tagged so they never exercise the item-side RC
// path. Text items live on the heap, so a clone_rc/drop_rc imbalance in the
// in-place helper would surface as a wrong value or a leak. 1k iterations is
// enough to amplify any per-iter RC mismatch.

const REBIND_TEXT_1K_LAST: &str =
    "build n:n>t;xs=[];@i 0..n{xs=+=xs \"x\"};at xs (-n 1) ?? \"miss\"\ndemo>t;build 1000";

#[test]
fn rebind_text_1k_last_tree() {
    assert_eq!(run("--run-tree", REBIND_TEXT_1K_LAST, "demo"), "x");
}

#[test]
fn rebind_text_1k_last_vm() {
    assert_eq!(run("--run-vm", REBIND_TEXT_1K_LAST, "demo"), "x");
}

#[test]
#[cfg(feature = "cranelift")]
fn rebind_text_1k_last_cranelift() {
    assert_eq!(run("--run-cranelift", REBIND_TEXT_1K_LAST, "demo"), "x");
}
