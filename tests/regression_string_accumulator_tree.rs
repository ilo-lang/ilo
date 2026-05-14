// Regression tests for the tree-walker RC-aware string-concat fast path.
//
// Background: Phase 2b.1 (PR #261, Map) and Phase 2b.2 (PR #273, List) made
// `Value::Map` / `Value::List` RC-aware via `Arc<HashMap>` / `Arc<Vec>` and
// added eval_stmt peepholes for self-rebind accumulator shapes. Phase 2b.3
// (this PR) does the same for `Value::Text`:
//
//   1. `Value::Text(String)` becomes `Value::Text(Arc<String>)`, so cloning a
//      string is a refcount bump rather than a full byte copy.
//   2. The existing `xs = xs + ys` eval_stmt peephole gains a Text branch
//      alongside its List branch. When prev and rhs are both Text, it calls
//      `Arc::make_mut(&mut prev).push_str(&rhs)` and returns the same Arc.
//      With the env's binding taken to Nil before evaluating the rhs the Arc
//      arrives at refcount=1, so the mutation is in place and the
//      string-accumulator loop runs in O(n) instead of O(n²).
//
// Mirror of the VM `OP_ADD_SS` rebind-shape guard (PR #260) and the
// Cranelift `jit_concat` non-rebind split (PR #250) — same alias trap, same
// fix shape.
//
// On a 5k-iteration accumulator the tree wall-clock drops from 10+s to
// milliseconds. This file pins:
//   1. Correctness for `s = s + suffix` with literal and variable suffixes
//      on the tree engine.
//   2. The non-rebind shape (`b = +a c`) leaves `a` untouched (alias
//      contract).
//   3. Aliased rhs (`s = s + s` self-concat) still works correctly because
//      the peephole bails out when the rhs references the same binding.
//   4. Numeric `n = n + 1` is unaffected (falls through to apply_binop).
//   5. Scaling: a 5k-iteration accumulator finishes well under a second.

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

// ── Self-rebind: text-literal suffix ──────────────────────────────────────

#[test]
fn tree_text_concat_self_rebind_literal() {
    // Three concats via the self-rebind shape. Exercises the eval_stmt
    // Text branch on every assignment.
    let src = r#"f>t;s="";s=+s "a";s=+s "b";s=+s "c";s"#;
    assert_eq!(run_tree(src, "f"), "abc");
}

#[test]
fn tree_text_concat_self_rebind_starts_nonempty() {
    let src = r#"f>t;s="x";s=+s "y";s=+s "z";s"#;
    assert_eq!(run_tree(src, "f"), "xyz");
}

// ── Self-rebind: variable suffix ──────────────────────────────────────────

#[test]
fn tree_text_concat_self_rebind_variable() {
    let src = r#"f>t;s="hello ";name="world";s=+s name;s"#;
    assert_eq!(run_tree(src, "f"), "hello world");
}

// ── Self-rebind: concat inside foreach (the hot accumulator shape) ─────────

#[test]
fn tree_text_concat_self_rebind_in_foreach() {
    // Classic accumulator shape inside a loop. This is the pattern Phase
    // 2b.3 targets: every iteration goes through the peephole, so the loop
    // runs in O(n) instead of O(n²).
    let src = r#"f>n;s="";@i 0..5{s=+s "x"};len s"#;
    assert_eq!(run_tree(src, "f"), "5");
}

// ── Non-rebind: caller's text is preserved (alias contract) ────────────────

#[test]
fn tree_text_concat_non_rebind_preserves_source() {
    // `b = +a c` is the non-rebind shape: the peephole's name-match fails
    // and we go through apply_binop, which allocates a fresh String. `a`
    // must still observe its original value.
    let src = r#"f>t;a="k1";b=+a "_x";a"#;
    assert_eq!(run_tree(src, "f"), "k1");
}

#[test]
fn tree_text_concat_non_rebind_both_visible() {
    // Both source and dest visible in one return. Pins the aliasing
    // contract: `a` must be untouched, `b` must have the concat.
    let src = r#"f>t;a="k1";b=+a "_x";fmt "{}|{}" a b"#;
    assert_eq!(run_tree(src, "f"), "k1|k1_x");
}

// ── Self-rebind: RHS is a Call returning Text ─────────────────────────────

#[test]
fn tree_text_concat_self_rebind_rhs_is_call() {
    // `s = +s (fmt "...")` — the rhs is a Call expression, not a literal or
    // Ref. The peephole's expr_refers_to walks into Call args, sees no Ref
    // to `s`, and fires. eval_self_rebind_concat evaluates the Call, gets a
    // Value::Text back, and goes through the Arc::make_mut + push_str path.
    let src = r#"f>t;s="k";s=+s (fmt "_{}" 1);s"#;
    assert_eq!(run_tree(src, "f"), "k_1");
}

// ── Aliasing: rhs references same binding ──────────────────────────────────

#[test]
fn tree_text_concat_self_alias_doubles() {
    // `s = s + s` — the peephole MUST bail out because evaluating the rhs
    // after `env.take("s")` would observe Nil. The expr_refers_to guard in
    // match_self_rebind_concat is what catches this. Same trap that bit
    // OP_ADD_SS in #260.
    let src = r#"f>t;s="ab";s=+s s;s"#;
    assert_eq!(run_tree(src, "f"), "abab");
}

// ── Numeric add still works under the same operator ───────────────────────

#[test]
fn tree_numeric_add_self_rebind_distinct_rhs() {
    // `n = n + 1` — peephole fires because rhs is a literal (no self-ref).
    // eval_self_rebind_concat falls back to eval_binop because values
    // aren't both Text or both List. Result must be 4.
    let src = r#"f>n;n=3;n=+n 1;n"#;
    assert_eq!(run_tree(src, "f"), "4");
}

// ── Scale: 5k-iteration accumulator under 5s ──────────────────────────────
//
// The pre-Phase-2b.3 tree path was O(n²) — every `+s c` cloned the whole
// String. 5k iterations of "x" appended took 10+s on a recent MacBook. With
// Arc + the peephole it's a millisecond or two. The 5s ceiling catches an
// accidental regression to the quadratic path while being lenient on slow
// CI runners.

#[test]
fn tree_text_concat_scale_5k_under_5s() {
    let src = r#"build n:n>n;s="";@i 0..n{s=+s "x"};len s
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
    assert_eq!(stdout.trim(), "5000", "wrong string length: {stdout}");
    assert!(
        elapsed < Duration::from_secs(5),
        "tree text-concat 5k took {elapsed:?} - expected <5s; the O(n^2) \
         path used to take 10+s. The eval_stmt Text peephole may have regressed."
    );
}
