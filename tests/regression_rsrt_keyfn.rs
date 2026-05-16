// Regression tests for the `rsrt fn xs` and `rsrt fn ctx xs` overloads.
//
// `rsrt` mirrors `srt`'s 1/2/3-arg pattern. The 1-arg form (descending sort
// of a list-or-text by natural order) shipped in #196. The 2-arg key-fn form
// and 3-arg closure-bind form were a 4-release standing ask from
// qa-tester / security-researcher / logs-forensics / content-mod personas:
// descending-by-key previously required `rev (srt fn xs)` or a negating
// key fn (`-0 v`) wrapped around `srt`.
//
// Routing: like `srt 2`/`srt 3`, the new `rsrt 2`/`rsrt 3` paths go through
// the tree bridge (`is_tree_bridge_eligible`) so VM and Cranelift share the
// tree interpreter's user-fn callback dispatch. These tests pin that the
// three engines agree on output for every form.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn check_stdout(engine: &str, src: &str, expected: &str) {
    let out = ilo()
        .args([src, engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "engine={engine}: expected success for `{src}`, got stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        expected,
        "engine={engine}: stdout mismatch for `{src}`"
    );
}

fn check_all(src: &str, expected: &str) {
    check_stdout("--run-tree", src, expected);
    check_stdout("--run-vm", src, expected);
    #[cfg(feature = "cranelift")]
    check_stdout("--run-cranelift", src, expected);
}

// ── 2-arg rsrt fn xs ──────────────────────────────────────────────────────

#[test]
fn rsrt_numeric_key_desc_cross_engine() {
    // sort numbers descending by themselves (identity key)
    check_all(
        "idk x:n>n;x f>L n;rsrt idk [1,3,2,5,4]",
        "[5, 4, 3, 2, 1]",
    );
}

#[test]
fn rsrt_text_by_length_desc_cross_engine() {
    // sort strings by length, longest first — the canonical persona case
    check_all(
        "ln s:t>n;len s f>L t;rsrt ln [\"a\",\"bbb\",\"cc\",\"dddd\"]",
        "[dddd, bbb, cc, a]",
    );
}

#[test]
fn rsrt_negated_key_inverts_to_ascending_cross_engine() {
    // rsrt with a negating key fn is equivalent to ascending sort — pins
    // the comparator direction is genuinely reversed vs srt's.
    check_all(
        "neg x:n>n;-0 x f>L n;rsrt neg [1,3,2,5,4]",
        "[1, 2, 3, 4, 5]",
    );
}

#[test]
fn rsrt_preserves_element_type_via_key_cross_engine() {
    // The element type (text) is preserved; only the key (number) drives
    // ordering. Mirrors srt's contract — return list element type = input
    // list element type.
    check_all(
        "ln s:t>n;len s f>L t;rsrt ln [\"ab\",\"x\",\"qrst\",\"\"]",
        "[qrst, ab, x, ]",
    );
}

#[test]
fn rsrt_empty_list_cross_engine() {
    check_all("idk x:n>n;x f>L n;rsrt idk []", "[]");
}

// ── 3-arg rsrt fn ctx xs (closure-bind) ───────────────────────────────────

#[test]
fn rsrt_closure_bind_ctx_cross_engine() {
    // 3-arg form: fn receives (elem, ctx). The ctx is the same for every
    // element so it doesn't change ordering, just confirms the bridge
    // threads the extra arg through without dropping it.
    check_all(
        "addk x:n c:n>n;+x c f>L n;rsrt addk 10 [1,3,2,5,4]",
        "[5, 4, 3, 2, 1]",
    );
}

#[test]
fn rsrt_closure_bind_ctx_inverting_cross_engine() {
    // Negate-with-ctx: the ctx supplies the scale. Identical result-shape
    // to `rsrt neg xs` — exercises the closure-bind dispatch path on each
    // engine.
    check_all(
        "scl x:n c:n>n;*x c f>L n;rsrt scl -1 [1,3,2,5,4]",
        "[1, 2, 3, 4, 5]",
    );
}

// ── 1-arg rsrt xs still works (regression guard) ──────────────────────────

#[test]
fn rsrt_one_arg_descending_unchanged_cross_engine() {
    check_all("f>L n;rsrt [1,3,2,5,4]", "[5, 4, 3, 2, 1]");
}

#[test]
fn rsrt_one_arg_text_unchanged_cross_engine() {
    check_all("f>t;rsrt \"cab\"", "cba");
}
