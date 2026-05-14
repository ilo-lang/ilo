// Regression tests for the tree-walker RC-aware list-append fast path.
//
// Background: Phase 2b.1 (PR #261) made `Value::Map` RC-aware via
// `Arc<HashMap>` and added an eval_stmt peephole for `m = mset m k v`. Phase
// 2b.2 (this PR) does the same for `Value::List`:
//
//   1. `Value::List(Vec<Value>)` becomes `Value::List(Arc<Vec<Value>>)`, so
//      cloning a list is a refcount bump rather than a full Vec copy.
//   2. Two eval_stmt peepholes detect self-rebind accumulator shapes:
//        `xs = +=xs v`   (BinOp::Append)
//        `xs = xs + ys`  (BinOp::Add on two lists)
//      Both take the env's binding out before evaluating the rhs so the
//      Arc<Vec<Value>> reaches the fast path with refcount=1, and
//      `Arc::make_mut` mutates the inner Vec in place.
//
// On a 5k-element accumulator loop this drops tree wall-clock from O(n²) to
// O(n). This file pins:
//   1. Correctness for `xs = +=xs v` with numbers and text on the tree
//      engine.
//   2. Correctness for `xs = xs + ys` list-concat with empty / non-empty
//      sides.
//   3. The non-rebind shape (`ys = +=xs v`, different name) still produces a
//      fresh list, leaving the original `xs` unchanged.
//   4. Aliased rhs (`s = s + s` self-concat) still works correctly because
//      the peephole bails out when the rhs references the same binding.
//   5. Scaling: a 5k-element accumulator finishes well under a second.

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

// ── Self-rebind: append numbers ────────────────────────────────────────────

#[test]
fn tree_list_append_self_rebind_numbers() {
    // Three appends via the self-rebind shape. Exercises the eval_stmt
    // append peephole on every assignment.
    let src = r#"f>n;xs=[];xs=+=xs 1;xs=+=xs 2;xs=+=xs 3;len xs"#;
    assert_eq!(run_tree(src, "f"), "3");
}

#[test]
fn tree_list_append_self_rebind_text() {
    let src = r#"f>t;xs=[];xs=+=xs "a";xs=+=xs "b";xs=+=xs "c";at xs 2 ?? "miss""#;
    assert_eq!(run_tree(src, "f"), "c");
}

// ── Self-rebind: append inside foreach ─────────────────────────────────────

#[test]
fn tree_list_append_self_rebind_in_foreach() {
    // Classic accumulator shape inside a loop. This is the pattern Phase 2b.2
    // targets: every iteration goes through the peephole, so the loop runs
    // in O(n) instead of O(n²).
    let src = r#"f>n;xs=[];@i 0..5{xs=+=xs i};len xs"#;
    assert_eq!(run_tree(src, "f"), "5");
}

// ── Non-rebind: caller's list is preserved ─────────────────────────────────

#[test]
fn tree_list_append_non_rebind_preserves_caller() {
    // Append into a fresh name — the peephole must NOT fire. The original
    // `xs` is held by a second binding, so the Arc has refcount=2 and
    // `Arc::make_mut` must clone rather than mutate in place.
    let src = r#"f>n;xs=[];xs=+=xs 1;xs=+=xs 2;ys=+=xs 99;len xs"#;
    assert_eq!(run_tree(src, "f"), "2");
}

#[test]
fn tree_list_append_non_rebind_target_gets_extra() {
    // The non-rebind target receives the appended element.
    let src = r#"f>n;xs=[];xs=+=xs 1;ys=+=xs 99;len ys"#;
    assert_eq!(run_tree(src, "f"), "2");
}

// ── Self-rebind: list concat ───────────────────────────────────────────────

#[test]
fn tree_list_concat_self_rebind_two_singletons() {
    let src = r#"f>n;xs=[];xs=+xs [1];xs=+xs [2];len xs"#;
    assert_eq!(run_tree(src, "f"), "2");
}

#[test]
fn tree_list_concat_self_rebind_empty_rhs() {
    let src = r#"f>n;xs=[];xs=+xs [1];xs=+xs [];len xs"#;
    assert_eq!(run_tree(src, "f"), "1");
}

// ── Aliasing: rhs references same binding ──────────────────────────────────

#[test]
fn tree_list_concat_self_alias_doubles() {
    // `xs = xs + xs` — the peephole MUST bail out because evaluating the rhs
    // after `env.take("xs")` would observe Nil. Phase 2b.1 hit the same trap
    // on text-concat (`s = s + s`), fixed by the rhs-refers-to-name check.
    let src = r#"f>n;xs=[1,2];xs=+xs xs;len xs"#;
    assert_eq!(run_tree(src, "f"), "4");
}

// ── Numeric add still works under the same operator ────────────────────────

#[test]
fn tree_numeric_add_self_rebind_unaffected() {
    // `n = n + n` — the peephole's match_self_rebind_concat fires, but
    // eval_self_rebind_concat sees non-list values and falls back to
    // eval_binop. The semantics must match the general path exactly.
    //
    // Wait — `n = n + n` also has rhs referring to `n`, so the peephole
    // bails out via expr_refers_to. This test pins the general-path path
    // works correctly when peephole bails out on numeric self-rebind.
    let src = r#"f>n;n=3;n=+n n;n"#;
    assert_eq!(run_tree(src, "f"), "6");
}

#[test]
fn tree_numeric_add_self_rebind_distinct_rhs() {
    // `n = n + 1` — peephole fires because rhs is a literal (no self-ref).
    // eval_self_rebind_concat falls back to eval_binop because values
    // aren't both lists. Result must be 4.
    let src = r#"f>n;n=3;n=+n 1;n"#;
    assert_eq!(run_tree(src, "f"), "4");
}

// ── Scale: 5k-element accumulator under 5s ─────────────────────────────────
//
// The pre-Phase-2b.2 tree path was O(n²) — every `+=` cloned the whole Vec.
// On a recent MacBook 5k iterations took 10+ seconds. With Arc + the
// peephole it's milliseconds. 5s ceiling catches an accidental regression to
// the quadratic path while being lenient on slow CI runners.

#[test]
fn tree_list_append_scale_5k_under_5s() {
    let src = r#"build n:n>n;xs=[];@i 0..n{xs=+=xs i};len xs
demo>n;build 5000"#;
    let start = Instant::now();
    let out = ilo()
        .args([src, "--run-tree", "demo"])
        .output()
        .expect("failed to run ilo");
    let elapsed = start.elapsed();
    assert!(
        out.status.success(),
        "ilo --run-tree demo failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.trim(), "5000", "wrong list length: {stdout}");
    assert!(
        elapsed < Duration::from_secs(5),
        "tree list-append 5k took {elapsed:?} — expected <5s; the O(n²) \
         path used to take 10+s. The eval_stmt peephole may have regressed."
    );
}
