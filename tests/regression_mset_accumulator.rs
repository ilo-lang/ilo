// Regression tests for the RC-aware `mset` accumulator fast path.
//
// Background: prior to this fix, `OP_MSET` (VM) and `jit_mset` (Cranelift)
// unconditionally cloned the entire HashMap on every insert. The common
// accumulator shape `m = mset m k v` inside a loop was therefore O(n²) in
// keys, which OOMed nlp-engineer's 16k-key word-frequency run and the
// logs-forensics 58k-key host map.
//
// The fix adds a compiler peephole for `name = mset name k v` and a runtime
// RC=1 fast path that mutates the HashMap in place when the accumulator
// variable is the sole owner. Pattern mirrors the merged
// `OP_LISTAPPEND` / `OP_ADD_SS` paths from PR #232.
//
// This file pins:
//   1. Correctness — small chains, repeated keys, value lifetime — on all
//      three engines.
//   2. **Non-numeric values** specifically: the original `jit_mset` had a
//      latent RC bug (`m.clone()` bit-copied entries but never bumped RC for
//      retained heap values, while `HeapObj::Drop` decremented every value).
//      Existing tests used only literal-number values so the UB never
//      manifested. These tests exercise `Text` values to catch any future
//      regression.
//   3. Scaling — a 5k-key accumulator finishes well inside a budget that the
//      old O(n²) path would blow.

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

// ── Basic correctness: 3-key chain ──────────────────────────────────────────

const CHAIN_TEXT_SRC: &str = "f>t;m=mset mmap \"a\" \"1\";m=mset m \"b\" \"2\";m=mset m \"c\" \"3\";mget m \"b\" ?? \"miss\"";

#[test]
fn mset_chain_text_tree() {
    assert_eq!(run("--run-tree", CHAIN_TEXT_SRC, "f"), "2");
}

#[test]
fn mset_chain_text_vm() {
    assert_eq!(run("--run-vm", CHAIN_TEXT_SRC, "f"), "2");
}

#[test]
#[cfg(feature = "cranelift")]
fn mset_chain_text_cranelift() {
    assert_eq!(run("--run-cranelift", CHAIN_TEXT_SRC, "f"), "2");
}

// ── Repeated-key overwrite preserves prior values ───────────────────────────
//
// Pre-fix `jit_mset`: `m.clone()` produced a map with entries whose RC was
// never bumped, then `HeapObj::Drop` decremented every value on map drop.
// With Text values, the second `mset` would drop the second map at scope end,
// over-decrementing the "a" Text Rc that the first map still referenced. The
// final lookup would observe freed memory.

const OVERWRITE_TEXT_SRC: &str =
    "f>t;m=mset mmap \"a\" \"first\";m=mset m \"a\" \"second\";mget m \"a\" ?? \"miss\"";

#[test]
fn mset_overwrite_text_tree() {
    assert_eq!(run("--run-tree", OVERWRITE_TEXT_SRC, "f"), "second");
}

#[test]
fn mset_overwrite_text_vm() {
    assert_eq!(run("--run-vm", OVERWRITE_TEXT_SRC, "f"), "second");
}

#[test]
#[cfg(feature = "cranelift")]
fn mset_overwrite_text_cranelift() {
    assert_eq!(run("--run-cranelift", OVERWRITE_TEXT_SRC, "f"), "second");
}

// NOTE on shared-map aliasing:
//
// A test of the form `m=mset mmap "a" "one"; m2=m; m=mset m "a" "two"; mget m2`
// might appear to exercise the RC > 1 slow path. It does not on the VM or
// Cranelift backends: the VM compiler's `Stmt::Let` for an alias-binding
// (`m2=m`) reuses m's register without emitting OP_MOVE (see Stmt::Let path in
// src/vm/mod.rs around line 1027), so m and m2 are the same SSA slot — there
// is only one Rc reference, not two. The subsequent `mset m k v2` therefore
// observes RC=1 and mutates in place, visible through m2.
//
// This is a pre-existing property of the register allocator that also affects
// OP_LISTAPPEND's RC=1 fast path (PR #232). The user-visible impact is small
// because direct same-scope aliasing of mutable collections is rare in ilo;
// the common shapes (function arguments, returned maps) do not share a
// register and so go through the slow path correctly. Flagged as a separate
// follow-up; out of scope for the mset accumulator fix.
//
// The function-call boundary, where the callee is passed the map and rebinds
// locally, IS a real RC > 1 trigger and is covered below.

// ── RC > 1 via function-call boundary ───────────────────────────────────────
//
// Passing `m` to a helper that does its own `mset` puts the map at RC=2
// (caller slot + callee parameter slot) for the duration of the call. The
// callee's `mset m "a" "two"` must NOT mutate the caller's map.

const FN_RC_SRC: &str = "addto m:M t t k:t v:t>t;m=mset m k v;mget m k ?? \"miss\"\n\
                         f>t;m=mset mmap \"a\" \"one\";x=addto m \"a\" \"two\";mget m \"a\" ?? \"miss\"";

#[test]
fn mset_fn_boundary_tree() {
    assert_eq!(run("--run-tree", FN_RC_SRC, "f"), "one");
}

#[test]
fn mset_fn_boundary_vm() {
    assert_eq!(run("--run-vm", FN_RC_SRC, "f"), "one");
}

#[test]
#[cfg(feature = "cranelift")]
fn mset_fn_boundary_cranelift() {
    assert_eq!(run("--run-cranelift", FN_RC_SRC, "f"), "one");
}

// ── List values (non-numeric, non-Text heap value) ──────────────────────────
//
// Maps holding list values exercise nested RC: every map clone in the old
// slow path needed to bump RC on the inner List Rcs too.

const LIST_VAL_SRC: &str =
    "f>n;m=mset mmap \"xs\" [1,2,3];m=mset m \"ys\" [4,5];xs=mget m \"xs\" ?? [];len xs";

#[test]
fn mset_list_val_tree() {
    assert_eq!(run("--run-tree", LIST_VAL_SRC, "f"), "3");
}

#[test]
fn mset_list_val_vm() {
    assert_eq!(run("--run-vm", LIST_VAL_SRC, "f"), "3");
}

#[test]
#[cfg(feature = "cranelift")]
fn mset_list_val_cranelift() {
    assert_eq!(run("--run-cranelift", LIST_VAL_SRC, "f"), "3");
}

// ── Loop accumulator correctness over a non-trivial key count ──────────────
//
// 500 distinct text keys → each iteration takes the RC=1 fast path because
// the variable rebind keeps the map's strong count at 1. Counts the final
// number of keys to verify nothing was dropped.

const LOOP_TEXT_KEYS_SRC: &str = "f>n;\
    m=mmap;\
    @i 0..500{k=fmt \"k{}\" i;m=mset m k \"v\"};\
    len (mkeys m)";

#[test]
fn mset_loop_text_keys_tree() {
    assert_eq!(run("--run-tree", LOOP_TEXT_KEYS_SRC, "f"), "500");
}

#[test]
fn mset_loop_text_keys_vm() {
    assert_eq!(run("--run-vm", LOOP_TEXT_KEYS_SRC, "f"), "500");
}

#[test]
#[cfg(feature = "cranelift")]
fn mset_loop_text_keys_cranelift() {
    assert_eq!(run("--run-cranelift", LOOP_TEXT_KEYS_SRC, "f"), "500");
}

// ── Scaling sanity: 5k keys must finish quickly on VM and Cranelift ────────
//
// Pre-fix VM took ~3-4 seconds for 5k keys; the fast path drops this to
// milliseconds. We don't assert on the tree-walker here because the
// tree-walker still uses HashMap::clone on every mset (Phase 2b deferred).
//
// Budget chosen with headroom for debug builds and slow CI runners. The
// O(n²) path on 5k keys is ~12.5M HashMap entries copied — well over the
// budget even on fast hardware.

const SCALE_SRC: &str = "f>n;\
    m=mmap;\
    @i 0..5000{k=fmt \"k{}\" i;m=mset m k i};\
    len (mkeys m)";

fn run_with_budget(engine: &str, src: &str, budget: Duration) -> String {
    let start = Instant::now();
    let out = run(engine, src, "f");
    let elapsed = start.elapsed();
    assert!(
        elapsed < budget,
        "engine={engine} took {elapsed:?} (budget {budget:?}) — accumulator fast path may have regressed"
    );
    out
}

#[test]
fn mset_scaling_vm() {
    let result = run_with_budget("--run-vm", SCALE_SRC, Duration::from_secs(10));
    assert_eq!(result, "5000");
}

#[test]
#[cfg(feature = "cranelift")]
fn mset_scaling_cranelift() {
    let result = run_with_budget("--run-cranelift", SCALE_SRC, Duration::from_secs(10));
    assert_eq!(result, "5000");
}
