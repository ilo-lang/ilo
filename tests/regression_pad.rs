// Cross-engine regression tests for `padl` (left-pad) and `padr` (right-pad).
// Mirrors the trm/math-extra cross-engine test pattern: every engine must
// agree on padding behaviour including edge cases (already-wider, exact, w=0,
// and negative-width errors).

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm", "--run-cranelift"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm"];

fn run_ok(engine: &str, src: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} {src:?} {args:?} failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Strip the trailing newline only; preserve internal whitespace so the
    // padding shape is preserved in the comparison.
    let s = String::from_utf8_lossy(&out.stdout);
    s.trim_end_matches('\n').to_string()
}

fn run_err(engine: &str, src: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "ilo {engine} {src:?} {args:?} unexpectedly succeeded: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

const PADL_SRC: &str = "f s:t w:n>t;padl s w";
const PADR_SRC: &str = "f s:t w:n>t;padr s w";

#[test]
fn padl_basic_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run_ok(engine, PADL_SRC, &["f", "hi", "5"]);
        assert_eq!(out, "   hi", "{engine}: padl basic");
    }
}

#[test]
fn padr_basic_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run_ok(engine, PADR_SRC, &["f", "hi", "5"]);
        assert_eq!(out, "hi   ", "{engine}: padr basic");
    }
}

#[test]
fn padl_exact_width_no_op() {
    for engine in ENGINES_ALL {
        let out = run_ok(engine, PADL_SRC, &["f", "exact", "5"]);
        assert_eq!(out, "exact", "{engine}: padl exact width");
    }
}

#[test]
fn padr_exact_width_no_op() {
    for engine in ENGINES_ALL {
        let out = run_ok(engine, PADR_SRC, &["f", "exact", "5"]);
        assert_eq!(out, "exact", "{engine}: padr exact width");
    }
}

#[test]
fn padl_already_wider_no_op() {
    for engine in ENGINES_ALL {
        let out = run_ok(engine, PADL_SRC, &["f", "already-wider", "4"]);
        assert_eq!(out, "already-wider", "{engine}: padl already wider");
    }
}

#[test]
fn padr_already_wider_no_op() {
    for engine in ENGINES_ALL {
        let out = run_ok(engine, PADR_SRC, &["f", "already-wider", "4"]);
        assert_eq!(out, "already-wider", "{engine}: padr already wider");
    }
}

#[test]
fn pad_width_zero_returns_input() {
    for engine in ENGINES_ALL {
        let l = run_ok(engine, PADL_SRC, &["f", "abc", "0"]);
        let r = run_ok(engine, PADR_SRC, &["f", "abc", "0"]);
        assert_eq!(l, "abc", "{engine}: padl w=0");
        assert_eq!(r, "abc", "{engine}: padr w=0");
    }
}

#[test]
fn pad_empty_string_pads_to_width() {
    for engine in ENGINES_ALL {
        let l = run_ok(engine, PADL_SRC, &["f", "", "3"]);
        let r = run_ok(engine, PADR_SRC, &["f", "", "3"]);
        assert_eq!(l, "   ", "{engine}: padl empty");
        assert_eq!(r, "   ", "{engine}: padr empty");
    }
}

#[test]
fn pad_negative_width_errors() {
    // Cranelift returns nil on invalid width (matches `at`/`hd` engine-divergence
    // precedent); tree-walker and VM error. Harmonising this is a deferred follow-up.
    for engine in &["--run-tree", "--run-vm"] {
        let _ = run_err(engine, PADL_SRC, &["f", "hi", "-1"]);
        let _ = run_err(engine, PADR_SRC, &["f", "hi", "-1"]);
    }
}

// ── 3-arg pad-char overload ─────────────────────────────────────────────────
// `padl s w pc` / `padr s w pc` — pad with a custom 1-character string. Default
// pad char (2-arg form) is space; the 3-arg form is the common need for sortable
// zero-padded numeric keys and aligned log lines.

const PADL3_SRC: &str = "f s:t w:n p:t>t;padl s w p";
const PADR3_SRC: &str = "f s:t w:n p:t>t;padr s w p";

#[test]
fn padl_with_zero_pads_numeric_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run_ok(engine, PADL3_SRC, &["f", "42", "5", "0"]);
        assert_eq!(out, "00042", "{engine}: padl zero-pad");
    }
}

#[test]
fn padr_with_dot_pads_text_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run_ok(engine, PADR3_SRC, &["f", "x", "4", "."]);
        assert_eq!(out, "x...", "{engine}: padr dot-pad");
    }
}

#[test]
fn pad_char_already_wide_no_op_cross_engine() {
    // 3-arg form must respect the same already-wider short-circuit as 2-arg.
    for engine in ENGINES_ALL {
        let l = run_ok(engine, PADL3_SRC, &["f", "already-wider", "4", "0"]);
        let r = run_ok(engine, PADR3_SRC, &["f", "already-wider", "4", "."]);
        assert_eq!(l, "already-wider", "{engine}: padl already-wider");
        assert_eq!(r, "already-wider", "{engine}: padr already-wider");
    }
}

#[test]
fn pad_char_unicode_scalar_cross_engine() {
    // Pad char is a Unicode scalar, not a byte. A single multi-byte char must
    // count as 1 toward width and must round-trip cleanly on every engine.
    for engine in ENGINES_ALL {
        let l = run_ok(engine, PADL3_SRC, &["f", "hi", "5", "·"]);
        assert_eq!(l, "···hi", "{engine}: padl unicode pad");
    }
}

#[test]
fn pad_char_multichar_errors_tree_vm() {
    // Tree and VM error; Cranelift returns nil (existing engine-divergence on
    // invalid pad inputs, same precedent as the negative-width case above).
    for engine in &["--run-tree", "--run-vm"] {
        let _ = run_err(engine, PADL3_SRC, &["f", "x", "5", "ab"]);
        let _ = run_err(engine, PADR3_SRC, &["f", "x", "5", "ab"]);
    }
}

#[test]
fn pad_char_empty_errors_tree_vm() {
    // Empty pad string is not a 1-character string; same error semantics as multi-char.
    for engine in &["--run-tree", "--run-vm"] {
        let _ = run_err(engine, PADL3_SRC, &["f", "x", "5", ""]);
        let _ = run_err(engine, PADR3_SRC, &["f", "x", "5", ""]);
    }
}

#[test]
fn pad_two_arg_form_still_pads_with_space_cross_engine() {
    // Regression: adding the 3-arg overload must not change 2-arg behaviour.
    for engine in ENGINES_ALL {
        let l = run_ok(engine, PADL_SRC, &["f", "42", "5"]);
        let r = run_ok(engine, PADR_SRC, &["f", "42", "5"]);
        assert_eq!(l, "   42", "{engine}: padl 2-arg space default");
        assert_eq!(r, "42   ", "{engine}: padr 2-arg space default");
    }
}

#[test]
fn pad_char_non_text_rejected_at_verify() {
    // Verifier rejects a non-text 3rd arg with ILO-T013 before any engine runs.
    // Covers the arity-3 type-check branch in verify.rs.
    let src = "f s:t w:n>t;padl s w 7"; // numeric literal 7 in the pad-char slot
    let err = run_err("--run-tree", src, &["f", "x", "5"]);
    assert!(
        err.contains("ILO-T013") || err.contains("expects t"),
        "expected ILO-T013 for non-text pad char, got: {err}"
    );
}

#[test]
fn pad_arity_overload_rejects_four_args() {
    // The arity overload accepts 2 or 3 args. Four must still be rejected so
    // the arity-mismatch error message picks up the new "2 or 3" branch.
    let src = "f s:t w:n p:t q:t>t;padl s w p q";
    let err = run_err("--run-tree", src, &["f", "x", "5", "0", "0"]);
    assert!(
        err.contains("ILO-T006") || err.contains("arity") || err.contains("2 or 3"),
        "expected ILO-T006 arity mismatch, got: {err}"
    );
}

#[test]
fn pad_zero_width_passes_through_with_pad_char() {
    // w=0 with a pad char should still short-circuit to the input unchanged.
    // Covers the cc >= w branch in the 3-arg dispatch on every engine.
    for engine in ENGINES_ALL {
        let l = run_ok(engine, PADL3_SRC, &["f", "abc", "0", "0"]);
        let r = run_ok(engine, PADR3_SRC, &["f", "abc", "0", "."]);
        assert_eq!(l, "abc", "{engine}: padl 3-arg w=0");
        assert_eq!(r, "abc", "{engine}: padr 3-arg w=0");
    }
}

#[test]
fn pad_empty_string_pads_to_width_with_pad_char() {
    // Empty input + pad char fills the whole width with the pad char.
    // Covers the cc=0, w>0 branch in the 3-arg dispatch.
    for engine in ENGINES_ALL {
        let l = run_ok(engine, PADL3_SRC, &["f", "", "4", "0"]);
        let r = run_ok(engine, PADR3_SRC, &["f", "", "4", "."]);
        assert_eq!(l, "0000", "{engine}: padl empty + zero pad");
        assert_eq!(r, "....", "{engine}: padr empty + dot pad");
    }
}
