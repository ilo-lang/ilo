// Regression tests for the non-rebind aliasing fix on string concat (`+`).
//
// Background:
//
// PR #232 shipped the RC=1 in-place fast path for `OP_ADD_SS` (VM) and
// `jit_concat` (Cranelift). PR #249 shipped the same shape for `OP_MSET`,
// PR #250 for `OP_LISTAPPEND`. Both #249 and #250 also fixed an aliasing hole
// where the in-place fast path fired whenever RC == 1, regardless of whether
// the destination register was the same SSA variable as the source. String
// concat (`+`) still had that hole.
//
// The bug:
//
// Code shaped like
//
//     b = +a suffix      -- non-rebind: distinct LHS source and dest
//
// where `a` happens to be RC=1 (e.g. the result of `fmt`, `cat`, a function
// call, or a prior concat) would:
//
//   * VM `OP_ADD_SS` (typed-string path): nullify slot `a` after consuming
//     the Rc, then store the concatenated result in slot `b`. The caller
//     observes `a` as nil — silent value loss.
//   * VM `OP_ADD` (untyped fallback when one side isn't statically marked
//     `reg_is_str`): same shape, same nullification.
//   * Cranelift `jit_concat` / `jit_add`: mutate the source string in place
//     via `Rc::get_mut`, return the same pointer. Both slot `a` and slot `b`
//     now point at the mutated string — silent corruption.
//
// Tree-walker is unaffected (it always clones; same property that makes
// Phase 2b a separate item).
//
// The fix mirrors #249 and #250: gate the in-place path on
// `a == b && rc_count == 1`. The compiler peephole `name = +name suffix`
// emits `a == b` so the accumulator perf path is preserved. Non-rebind
// distinct-register shapes go through the cloning branch and `a` is
// preserved.
//
// All tests cross-engine (tree, VM, Cranelift) so a divergence between
// backends shows up in CI.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str, arg: &str) -> String {
    let out = ilo()
        .args([src, engine, entry, arg])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// ── OP_ADD_SS non-rebind: typed-string param + literal suffix ───────────────
//
// `a:t` is statically known to be string, so the compiler emits OP_ADD_SS with
// `a_idx != b_idx` (fresh result register). At runtime `a`'s string was
// produced by `cat` so its RC is 1 — pre-fix this triggered the in-place
// mutation hole.

const ADDSS_NON_REBIND_PRESERVES_A: &str =
    "mks n:n>t;fmt \"k{}\" n\ngo n:n>t;a=mks n;b=+a \"_x\";a\n";

#[test]
fn addss_non_rebind_preserves_a_tree() {
    assert_eq!(
        run("--run-tree", ADDSS_NON_REBIND_PRESERVES_A, "go", "1"),
        "k1"
    );
}

#[test]
fn addss_non_rebind_preserves_a_vm() {
    assert_eq!(
        run("--run-vm", ADDSS_NON_REBIND_PRESERVES_A, "go", "1"),
        "k1"
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn addss_non_rebind_preserves_a_cranelift() {
    assert_eq!(
        run("--run-cranelift", ADDSS_NON_REBIND_PRESERVES_A, "go", "1"),
        "k1"
    );
}

// And the same shape returning `b` rather than `a`, to confirm the
// concatenated result is correct (i.e. we didn't break the happy path).

const ADDSS_NON_REBIND_B_GETS_RESULT: &str =
    "mks n:n>t;fmt \"k{}\" n\ngo n:n>t;a=mks n;b=+a \"_x\";b\n";

#[test]
fn addss_non_rebind_b_gets_result_tree() {
    assert_eq!(
        run("--run-tree", ADDSS_NON_REBIND_B_GETS_RESULT, "go", "1"),
        "k1_x"
    );
}

#[test]
fn addss_non_rebind_b_gets_result_vm() {
    assert_eq!(
        run("--run-vm", ADDSS_NON_REBIND_B_GETS_RESULT, "go", "1"),
        "k1_x"
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn addss_non_rebind_b_gets_result_cranelift() {
    assert_eq!(
        run("--run-cranelift", ADDSS_NON_REBIND_B_GETS_RESULT, "go", "1"),
        "k1_x"
    );
}

// Both source and dest visible in the same return — exercises the alias
// observation in one shot. Format: "a|b".

const ADDSS_NON_REBIND_BOTH_VISIBLE: &str =
    "mks n:n>t;fmt \"k{}\" n\ngo n:n>t;a=mks n;b=+a \"_x\";fmt \"{}|{}\" a b\n";

#[test]
fn addss_non_rebind_both_visible_tree() {
    assert_eq!(
        run("--run-tree", ADDSS_NON_REBIND_BOTH_VISIBLE, "go", "1"),
        "k1|k1_x"
    );
}

#[test]
fn addss_non_rebind_both_visible_vm() {
    assert_eq!(
        run("--run-vm", ADDSS_NON_REBIND_BOTH_VISIBLE, "go", "1"),
        "k1|k1_x"
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn addss_non_rebind_both_visible_cranelift() {
    assert_eq!(
        run("--run-cranelift", ADDSS_NON_REBIND_BOTH_VISIBLE, "go", "1"),
        "k1|k1_x"
    );
}

// ── OP_ADD untyped-string path: same shape, no static reg_is_str ────────────
//
// When the LHS register doesn't carry static `reg_is_str` info (e.g. `fmt`'s
// return register isn't marked string-typed), the compiler emits plain OP_ADD
// instead of OP_ADD_SS. OP_ADD's string branch had the same in-place hole;
// the fix lives in jit_add / jit_add_inplace and the VM OP_ADD dispatch.

const ADD_UNTYPED_NON_REBIND_PRESERVES_A: &str = "go n:n>t;a=fmt \"k{}\" n;b=+a \"_x\";a";

#[test]
fn add_untyped_non_rebind_preserves_a_tree() {
    assert_eq!(
        run("--run-tree", ADD_UNTYPED_NON_REBIND_PRESERVES_A, "go", "1"),
        "k1"
    );
}

#[test]
fn add_untyped_non_rebind_preserves_a_vm() {
    assert_eq!(
        run("--run-vm", ADD_UNTYPED_NON_REBIND_PRESERVES_A, "go", "1"),
        "k1"
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn add_untyped_non_rebind_preserves_a_cranelift() {
    assert_eq!(
        run(
            "--run-cranelift",
            ADD_UNTYPED_NON_REBIND_PRESERVES_A,
            "go",
            "1"
        ),
        "k1"
    );
}

// ── Rebind shape still in-place (no perf regression) ────────────────────────
//
// `name = +name suffix` is the accumulator shape the compiler peephole emits
// with `a == b`. It MUST still take the in-place path so building up a string
// in a loop is O(n) amortised, not O(n²). We can't directly observe "ran the
// fast path" from the language, but we can confirm the result is correct.
// (The existing #232 perf budget tests guard the wall-clock side.)

const ADD_REBIND_ACCUMULATOR: &str = "go n:n>t;s=\"\";@i 0..n{s=+s \"x\"};s";

#[test]
fn add_rebind_accumulator_tree() {
    assert_eq!(
        run("--run-tree", ADD_REBIND_ACCUMULATOR, "go", "100").len(),
        100
    );
}

#[test]
fn add_rebind_accumulator_vm() {
    assert_eq!(
        run("--run-vm", ADD_REBIND_ACCUMULATOR, "go", "100").len(),
        100
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn add_rebind_accumulator_cranelift() {
    assert_eq!(
        run("--run-cranelift", ADD_REBIND_ACCUMULATOR, "go", "100").len(),
        100
    );
}

// ── RC > 1 case (function-call boundary) ────────────────────────────────────
//
// Pass `a` through a function so the caller's binding bumps its RC to >= 2.
// The cloning path was always correct for RC > 1; this pin just makes sure
// the new helper split didn't accidentally drop the RC > 1 branch.

const ADDSS_NON_REBIND_RC_GT_1: &str =
    "ident s:t>t;s\nmks n:n>t;fmt \"k{}\" n\ngo n:n>t;a=mks n;keep=ident a;b=+a \"_x\";a\n";

#[test]
fn addss_non_rebind_rc_gt_1_tree() {
    assert_eq!(run("--run-tree", ADDSS_NON_REBIND_RC_GT_1, "go", "1"), "k1");
}

#[test]
fn addss_non_rebind_rc_gt_1_vm() {
    assert_eq!(run("--run-vm", ADDSS_NON_REBIND_RC_GT_1, "go", "1"), "k1");
}

#[test]
#[cfg(feature = "cranelift")]
fn addss_non_rebind_rc_gt_1_cranelift() {
    assert_eq!(
        run("--run-cranelift", ADDSS_NON_REBIND_RC_GT_1, "go", "1"),
        "k1"
    );
}

// ── Self-concat (`s = +s s`): in-place path must not self-alias ─────────────
//
// The rebind peephole `name = +name suffix` emits `a == b`. When RHS is also
// the same variable (`s = +s s`), the compiler resolves all three operand
// registers to the same SSA slot. The in-place path's `push_str(other)` would
// then read from the same String buffer it's growing — UB if the buffer
// reallocates. Both VM dispatch and Cranelift codegen guard against this by
// also requiring `b != c` before taking the in-place branch.

const ADD_SELF_CONCAT: &str = "go n:n>t;s=fmt \"k{}\" n;s=+s s;s";

#[test]
fn add_self_concat_tree() {
    assert_eq!(run("--run-tree", ADD_SELF_CONCAT, "go", "1"), "k1k1");
}

#[test]
fn add_self_concat_vm() {
    assert_eq!(run("--run-vm", ADD_SELF_CONCAT, "go", "1"), "k1k1");
}

#[test]
#[cfg(feature = "cranelift")]
fn add_self_concat_cranelift() {
    assert_eq!(run("--run-cranelift", ADD_SELF_CONCAT, "go", "1"), "k1k1");
}

// Self-concat with statically-typed LHS so the compiler emits OP_ADD_SS rather
// than OP_ADD. Same self-aliasing risk, fixed at the OP_ADD_SS dispatch site.

const ADDSS_SELF_CONCAT: &str = "go s:t>t;s=+s s;s";

#[test]
fn addss_self_concat_tree() {
    assert_eq!(run("--run-tree", ADDSS_SELF_CONCAT, "go", "ab"), "abab");
}

#[test]
fn addss_self_concat_vm() {
    assert_eq!(run("--run-vm", ADDSS_SELF_CONCAT, "go", "ab"), "abab");
}

#[test]
#[cfg(feature = "cranelift")]
fn addss_self_concat_cranelift() {
    assert_eq!(
        run("--run-cranelift", ADDSS_SELF_CONCAT, "go", "ab"),
        "abab"
    );
}
