// Regression tests for the tree-walker RC-aware `mset` accumulator fast path.
//
// Background: PR #249 fixed the O(n²) accumulator on VM and Cranelift by
// adding a compiler peephole for `m = mset m k v` plus an RC=1 `HeapObj::Map`
// fast path. The tree walker stayed O(n²) because `Value::Map` held
// `HashMap<String, Value>` directly — every `mset` cloned the whole map.
//
// Phase 2b.1 of the RC-aware mutation rollout switches `Value::Map` to
// `Arc<HashMap<String, Value>>` and adds an `eval_stmt` peephole that:
//   1. Detects `Stmt::Let { name, value: Call("mset", [Ref(name), k, v]) }`.
//   2. Takes the env's binding out before evaluating the RHS (leaving Nil).
//   3. Runs `mset` on the moved Arc (refcount=1) so `Arc::make_mut` mutates
//      the HashMap in place.
//
// On the 16k-key Moby Dick reproducer this drops tree wall-clock from ~70s to
// ~0.1s. This file pins:
//   1. Correctness on the tree engine for the self-rebind shape with text,
//      number, and result-typed values.
//   2. Non-rebind shapes still produce fresh maps (caller's binding is not
//      mutated when the accumulator is held under a different name).
//   3. Scaling: a 5k-key accumulator finishes in well under a second on tree,
//      where the old O(n²) path took 30+ seconds.

use std::process::Command;
use std::time::{Duration, Instant};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_tree(src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, "--run-tree", entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo --run-tree failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// ── Self-rebind: text values ───────────────────────────────────────────────

#[test]
fn tree_mset_self_rebind_text_chain() {
    // Three-key chain via the self-rebind shape. Exercises the eval_stmt
    // peephole on every iteration of the assignment.
    let src = r#"f>t;m=mset mmap "a" "1";m=mset m "b" "2";m=mset m "c" "3";mget m "b" ?? "miss""#;
    assert_eq!(run_tree(src, "f"), "2");
}

#[test]
fn tree_mset_self_rebind_overwrite() {
    // Same key written twice. After the peephole drops the env binding for
    // the second mset, the inner Arc::make_mut must overwrite, not preserve
    // both. Confirms the in-place HashMap::insert is correct.
    let src = r#"f>t;m=mset mmap "a" "first";m=mset m "a" "second";mget m "a" ?? "miss""#;
    assert_eq!(run_tree(src, "f"), "second");
}

// ── Self-rebind: numeric values ────────────────────────────────────────────

#[test]
fn tree_mset_self_rebind_number_chain() {
    let src = r#"f>n;m=mset mmap "x" 1;m=mset m "y" 2;m=mset m "z" 3;mget m "z" ?? -1"#;
    assert_eq!(run_tree(src, "f"), "3");
}

// ── Non-rebind: caller's map is preserved ──────────────────────────────────

#[test]
fn tree_mset_non_rebind_preserves_caller() {
    // The accumulator is bound to a fresh name (`m2`), so the peephole must
    // NOT fire — env still holds `m` afterwards and `m` must observe the
    // pre-mset state. If the peephole were over-eager (taking from `m`
    // because `m` appears as the first arg of mset), the second `mget m "a"`
    // would return nil.
    let src = r#"f>t;m=mset mmap "a" "kept";m2=mset m "b" "added";mget m "a" ?? "lost""#;
    assert_eq!(run_tree(src, "f"), "kept");
}

#[test]
fn tree_mset_non_rebind_original_lacks_new_key() {
    // The non-rebind variant must leave the original map unchanged — the new
    // key must NOT leak back into `m`.
    let src = r#"f>t;m=mset mmap "a" "kept";m2=mset m "b" "added";mget m "b" ?? "absent""#;
    assert_eq!(run_tree(src, "f"), "absent");
}

// ── Self-rebind inside a foreach loop ──────────────────────────────────────

#[test]
fn tree_mset_self_rebind_in_foreach() {
    // The classic word-count accumulator shape. Exercises the peephole on
    // every iteration of the loop body.
    let src = r#"count-words ws:L t>n;m=mmap;@w ws{c=mget m w ?? 0;m=mset m w +c 1};len (mkeys m)
demo>n;count-words ["the","cat","the","sat","the","cat"]"#;
    assert_eq!(run_tree(src, "demo"), "3");
}

// ── Error semantics: RHS evaluation failure ─────────────────────────────────
//
// When the key or value expression of a self-rebind mset errors out, the
// peephole leaves Value::Nil in env's slot. ilo has no catch/recover form,
// so the error propagates to the function boundary and user code cannot
// observe the intermediate Nil. This test pins that the error path produces
// a visible error rather than silently corrupting state.

fn run_tree_err(src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, "--run-tree", entry])
        .output()
        .expect("failed to run ilo");
    // ILO programs that error during execution exit non-zero or emit the
    // error to stderr; either way, we want to see "num" / type-mismatch text.
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    combined.trim().to_string()
}

#[test]
fn tree_mset_self_rebind_key_eval_error_propagates() {
    // The key expression `at xs 99` is out-of-bounds on a 1-element list,
    // which surfaces as a runtime error. With the self-rebind peephole,
    // env's `m` is taken out, the key eval fails, and the error must
    // propagate cleanly rather than silently completing the assignment with
    // junk state — exercising the documented Nil-in-env-on-error semantics.
    let src = r#"f>n;m=mmap;xs=["k"];m=mset m (at xs 99) 1;len (mkeys m)"#;
    let out = run_tree_err(src, "f");
    assert!(
        !out.is_empty(),
        "expected runtime error output (got empty stdout/stderr)"
    );
    assert!(
        out.contains("at")
            || out.contains("bounds")
            || out.contains("index")
            || out.contains("error")
            || out.contains("Err")
            || out.contains("ILO-"),
        "expected at-OOB error to surface, got: {out}"
    );
}

// ── Scale: 5k-key build under 5s ───────────────────────────────────────────
//
// The old O(n²) tree path was ~30s for 5k keys on a recent MacBook; the new
// path completes in milliseconds. We pick a generous 5s ceiling so the test
// is not flaky on slow CI runners but still catches a regression to O(n²).

#[test]
fn tree_mset_scale_5k_keys_under_5s() {
    let src = r#"build n:n>n;m=mmap;@i 0..n{k=fmt "k{}" i;m=mset m k i};len (mkeys m)"#;
    let start = Instant::now();
    let out = ilo()
        .args([src, "--run-tree", "build", "5000"])
        .output()
        .expect("failed to run ilo");
    let elapsed = start.elapsed();
    assert!(
        out.status.success(),
        "ilo --run-tree build 5000 failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.trim(), "5000", "wrong key count: {stdout}");
    assert!(
        elapsed < Duration::from_secs(5),
        "tree mset 5k keys took {elapsed:?} — expected <5s; the O(n²) path \
         used to take ~30s. The eval_stmt peephole may have regressed."
    );
}
