// Regression tests for the non-rebind aliasing fix on `OP_LISTAPPEND` and the
// parity contract pin on `OP_MSET`.
//
// Background:
//
// PR #232 shipped the RC=1 in-place fast path for `OP_LISTAPPEND` (VM) and
// `jit_listappend` (Cranelift). The fast path mutates the source list's Vec
// when `strong_count == 1`, which makes `xs = += xs item` accumulator loops
// run in amortised O(1) per append instead of O(n).
//
// What #232 missed: the fast path fired unconditionally when RC=1, regardless
// of whether the destination register was the same SSA variable as the
// source. Code shaped like
//
//     ys = += xs item
//
// where `xs` happens to be RC=1 (e.g. a freshly built list) would:
//   * mutate `xs`'s Vec in place via the &mut cast, AND
//   * alias the same pointer into `ys`'s slot.
//
// Both `xs` and `ys` then observed the mutated list. Silent data corruption
// with no diagnostic. This file pins the fix: the in-place path now requires
// `a == b` (the rebind shape the compiler peephole emits) AND `rc_count == 1`,
// mirroring the `OP_MSET` / `jit_mset_inplace` split landed in PR #249.
//
// Companion contract pin: `m2 = mset m k v` was already guarded correctly on
// both VM and Cranelift after #249, but had no cross-engine regression test.
// This file adds one so a future refactor that drops the guard can't ship.
//
// All tests cross-engine (tree, VM, Cranelift) so a divergence between
// backends shows up in CI.

use std::process::Command;

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

// ── Issue 2: `ys = += xs item` must NOT mutate `xs` ─────────────────────────
//
// `xs` is a fresh list literal, so it is RC=1 at the point of the append. Pre
// fix, both VM and Cranelift would mutate it in place and alias the pointer
// into `ys`, giving the caller a wrong result for `xs`.
//
// The function returns `xs` after the append so the test directly observes
// whether the source was mutated.

const APPEND_NON_REBIND_PRESERVES_XS: &str = "f>L n;xs=[1,2,3];ys=+=xs 99;xs";

#[test]
fn append_non_rebind_preserves_xs_tree() {
    assert_eq!(
        run("--run-tree", APPEND_NON_REBIND_PRESERVES_XS, "f"),
        "[1, 2, 3]"
    );
}

#[test]
fn append_non_rebind_preserves_xs_vm() {
    assert_eq!(
        run("--run-vm", APPEND_NON_REBIND_PRESERVES_XS, "f"),
        "[1, 2, 3]"
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn append_non_rebind_preserves_xs_cranelift() {
    assert_eq!(
        run("--run-cranelift", APPEND_NON_REBIND_PRESERVES_XS, "f"),
        "[1, 2, 3]"
    );
}

// And the same shape returning `ys` rather than `xs`, to confirm the new
// list contains the appended item (i.e. we didn't break the happy path).

const APPEND_NON_REBIND_YS_GETS_NEW_ITEM: &str = "f>L n;xs=[1,2,3];ys=+=xs 99;ys";

#[test]
fn append_non_rebind_ys_gets_new_item_tree() {
    assert_eq!(
        run("--run-tree", APPEND_NON_REBIND_YS_GETS_NEW_ITEM, "f"),
        "[1, 2, 3, 99]"
    );
}

#[test]
fn append_non_rebind_ys_gets_new_item_vm() {
    assert_eq!(
        run("--run-vm", APPEND_NON_REBIND_YS_GETS_NEW_ITEM, "f"),
        "[1, 2, 3, 99]"
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn append_non_rebind_ys_gets_new_item_cranelift() {
    assert_eq!(
        run("--run-cranelift", APPEND_NON_REBIND_YS_GETS_NEW_ITEM, "f"),
        "[1, 2, 3, 99]"
    );
}

// Both source and dest visible in the same return — exercises the alias
// observation in one shot. Format: "xs_len;ys_len".

const APPEND_NON_REBIND_BOTH_VISIBLE: &str =
    "f>t;xs=[1,2,3];ys=+=xs 99;fmt \"{};{}\" (len xs) (len ys)";

#[test]
fn append_non_rebind_both_visible_tree() {
    assert_eq!(
        run("--run-tree", APPEND_NON_REBIND_BOTH_VISIBLE, "f"),
        "3;4"
    );
}

#[test]
fn append_non_rebind_both_visible_vm() {
    assert_eq!(run("--run-vm", APPEND_NON_REBIND_BOTH_VISIBLE, "f"), "3;4");
}

#[test]
#[cfg(feature = "cranelift")]
fn append_non_rebind_both_visible_cranelift() {
    assert_eq!(
        run("--run-cranelift", APPEND_NON_REBIND_BOTH_VISIBLE, "f"),
        "3;4"
    );
}

// ── Issue 2: `xs = += xs item` rebind shape still in-place (no regression) ──
//
// The rebind shape (`name = += name item`, the compiler peephole) MUST still
// take the fast path. The point of #232 was to make this O(1); the alias fix
// must not regress that. We can't directly observe "ran the fast path" from
// the language, but we can confirm the result is correct and the build path
// still mutates the same pointer (covered by the existing #232 budget test
// in regression_range_expr.rs and the foreach-build perf in CI).

const APPEND_REBIND_ACCUMULATOR: &str = "f>n;xs=[];@i 0..100{xs=+=xs i};len xs";

#[test]
fn append_rebind_accumulator_tree() {
    assert_eq!(run("--run-tree", APPEND_REBIND_ACCUMULATOR, "f"), "100");
}

#[test]
fn append_rebind_accumulator_vm() {
    assert_eq!(run("--run-vm", APPEND_REBIND_ACCUMULATOR, "f"), "100");
}

#[test]
#[cfg(feature = "cranelift")]
fn append_rebind_accumulator_cranelift() {
    assert_eq!(
        run("--run-cranelift", APPEND_REBIND_ACCUMULATOR, "f"),
        "100"
    );
}

// ── Issue 2 with non-numeric items (Text) ───────────────────────────────────
//
// Mirrors the mset Text-value coverage from #249. Numeric items are
// NaN-tag-immediate so they never exercise the heap-RC path on the item side.
// Text items DO live on the heap, so an over-/under-decrement here would show
// up as a use-after-free or leak in CI under address sanitiser, and as a
// wrong result (or crash) on plain debug.

const APPEND_NON_REBIND_TEXT: &str =
    "f>t;xs=[\"a\",\"b\"];ys=+=xs \"c\";fmt \"{}|{}\" (cat xs \"\") (cat ys \"\")";

#[test]
fn append_non_rebind_text_tree() {
    assert_eq!(run("--run-tree", APPEND_NON_REBIND_TEXT, "f"), "ab|abc");
}

#[test]
fn append_non_rebind_text_vm() {
    assert_eq!(run("--run-vm", APPEND_NON_REBIND_TEXT, "f"), "ab|abc");
}

#[test]
#[cfg(feature = "cranelift")]
fn append_non_rebind_text_cranelift() {
    assert_eq!(
        run("--run-cranelift", APPEND_NON_REBIND_TEXT, "f"),
        "ab|abc"
    );
}

// ── Issue 2 at RC > 1 (function-call boundary) ──────────────────────────────
//
// Pass `xs` through a function so the caller's binding bumps its RC to >= 2.
// The cloning path was always correct for RC>1; this pin just makes sure the
// new helper split didn't accidentally drop the RC>1 branch.

const APPEND_NON_REBIND_RC_GT_1: &str = "\
ident xs:L n>L n;xs\n\
f>L n;xs=[1,2,3];keep=ident xs;ys=+=xs 99;xs\n\
";

#[test]
fn append_non_rebind_rc_gt_1_tree() {
    assert_eq!(
        run("--run-tree", APPEND_NON_REBIND_RC_GT_1, "f"),
        "[1, 2, 3]"
    );
}

#[test]
fn append_non_rebind_rc_gt_1_vm() {
    assert_eq!(run("--run-vm", APPEND_NON_REBIND_RC_GT_1, "f"), "[1, 2, 3]");
}

#[test]
#[cfg(feature = "cranelift")]
fn append_non_rebind_rc_gt_1_cranelift() {
    assert_eq!(
        run("--run-cranelift", APPEND_NON_REBIND_RC_GT_1, "f"),
        "[1, 2, 3]"
    );
}

// ── Issue 1 contract pin: `m2 = mset m k v` must NOT mutate `m` ─────────────
//
// mset's guard was correct on both backends after PR #249, but the contract
// had no cross-engine regression test. The repro returns `m`'s key list after
// `mset m "b" 2` into a distinct binding `m2`; the count must stay at 1.

const MSET_NON_REBIND_PRESERVES_M: &str = "f>n;m=mset mmap \"a\" 1;m2=mset m \"b\" 2;len (mkeys m)";

#[test]
fn mset_non_rebind_preserves_m_tree() {
    assert_eq!(run("--run-tree", MSET_NON_REBIND_PRESERVES_M, "f"), "1");
}

#[test]
fn mset_non_rebind_preserves_m_vm() {
    assert_eq!(run("--run-vm", MSET_NON_REBIND_PRESERVES_M, "f"), "1");
}

#[test]
#[cfg(feature = "cranelift")]
fn mset_non_rebind_preserves_m_cranelift() {
    assert_eq!(
        run("--run-cranelift", MSET_NON_REBIND_PRESERVES_M, "f"),
        "1"
    );
}

// And confirm m2 received the new entry.

const MSET_NON_REBIND_M2_GETS_NEW_ENTRY: &str =
    "f>n;m=mset mmap \"a\" 1;m2=mset m \"b\" 2;len (mkeys m2)";

#[test]
fn mset_non_rebind_m2_gets_new_entry_tree() {
    assert_eq!(
        run("--run-tree", MSET_NON_REBIND_M2_GETS_NEW_ENTRY, "f"),
        "2"
    );
}

#[test]
fn mset_non_rebind_m2_gets_new_entry_vm() {
    assert_eq!(run("--run-vm", MSET_NON_REBIND_M2_GETS_NEW_ENTRY, "f"), "2");
}

#[test]
#[cfg(feature = "cranelift")]
fn mset_non_rebind_m2_gets_new_entry_cranelift() {
    assert_eq!(
        run("--run-cranelift", MSET_NON_REBIND_M2_GETS_NEW_ENTRY, "f"),
        "2"
    );
}

// mset non-rebind with Text values — exercises the cloning helper's
// RC-bump-on-retained-entries path that #249 introduced.

const MSET_NON_REBIND_TEXT_PRESERVES_M: &str = "\
f>t;m=mset mmap \"a\" \"first\";m2=mset m \"b\" \"second\";mget m \"a\" ?? \"miss\"\n\
";

#[test]
fn mset_non_rebind_text_preserves_m_tree() {
    assert_eq!(
        run("--run-tree", MSET_NON_REBIND_TEXT_PRESERVES_M, "f"),
        "first"
    );
}

#[test]
fn mset_non_rebind_text_preserves_m_vm() {
    assert_eq!(
        run("--run-vm", MSET_NON_REBIND_TEXT_PRESERVES_M, "f"),
        "first"
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn mset_non_rebind_text_preserves_m_cranelift() {
    assert_eq!(
        run("--run-cranelift", MSET_NON_REBIND_TEXT_PRESERVES_M, "f"),
        "first"
    );
}
