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
