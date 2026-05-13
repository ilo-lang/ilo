// Cross-engine regression tests for the per-char codepoint builtins:
// `ord c:t -> n` (first char's Unicode codepoint) and
// `chr n:n -> t` (codepoint to single-char string).
//
// Each builtin must produce identical output across the tree-walking
// interpreter, the bytecode VM, and (when enabled) the Cranelift JIT.

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
        "ilo {engine} {src:?} failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_expect_err(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "ilo {engine} {src:?} unexpectedly succeeded: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm", "--run-cranelift"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm"];

// ── ord ─────────────────────────────────────────────────────────────────

#[test]
fn ord_uppercase_h_cross_engine() {
    let src = "f>n;ord \"H\"";
    for engine in ENGINES_ALL {
        assert_eq!(run(engine, src, "f"), "72", "{engine}: ord(\"H\")");
    }
}

#[test]
fn ord_lowercase_a_cross_engine() {
    let src = "f>n;ord \"a\"";
    for engine in ENGINES_ALL {
        assert_eq!(run(engine, src, "f"), "97", "{engine}: ord(\"a\")");
    }
}

#[test]
fn ord_digit_zero_cross_engine() {
    let src = "f>n;ord \"0\"";
    for engine in ENGINES_ALL {
        assert_eq!(run(engine, src, "f"), "48", "{engine}: ord(\"0\")");
    }
}

#[test]
fn ord_takes_first_char_only_cross_engine() {
    // `ord` should return the codepoint of the first char, ignoring the rest.
    let src = "f>n;ord \"hello\"";
    for engine in ENGINES_ALL {
        assert_eq!(run(engine, src, "f"), "104", "{engine}: ord(\"hello\")");
    }
}

#[test]
fn ord_multibyte_utf8_cross_engine() {
    // U+00E9 = 233 (LATIN SMALL LETTER E WITH ACUTE)
    let src = "f>n;ord \"é\"";
    for engine in ENGINES_ALL {
        assert_eq!(run(engine, src, "f"), "233", "{engine}: ord(\"é\")");
    }
}

#[test]
fn ord_empty_string_errors_tree_and_vm() {
    // Tree and VM raise a runtime error on empty input. Cranelift returns
    // nil to match the existing `at`/`hd`/`padl` precedent for invalid args
    // (it cannot raise a typed runtime error from a JIT helper without
    // unwinding through Cranelift).
    let src = "f>n;ord \"\"";
    for engine in &["--run-tree", "--run-vm"] {
        run_expect_err(engine, src, "f");
    }
}

#[test]
#[cfg(feature = "cranelift")]
fn ord_empty_string_returns_nil_cranelift() {
    let src = "f>n;ord \"\"";
    let out = run("--run-cranelift", src, "f");
    assert_eq!(out, "nil", "cranelift: ord(\"\") returns nil");
}

// ── chr ─────────────────────────────────────────────────────────────────

#[test]
fn chr_uppercase_h_cross_engine() {
    let src = "f>t;chr 72";
    for engine in ENGINES_ALL {
        assert_eq!(run(engine, src, "f"), "H", "{engine}: chr 72");
    }
}

#[test]
fn chr_zero_cross_engine() {
    // U+0000 is NUL — a valid codepoint. The printed form has length 1 but
    // renders as an unprintable byte, so we round-trip via ord to assert.
    let src = "f>n;ord chr 0";
    for engine in ENGINES_ALL {
        assert_eq!(run(engine, src, "f"), "0", "{engine}: chr 0 round-trip");
    }
}

#[test]
fn chr_digit_zero_cross_engine() {
    let src = "f>t;chr 48";
    for engine in ENGINES_ALL {
        assert_eq!(run(engine, src, "f"), "0", "{engine}: chr 48");
    }
}

#[test]
fn chr_multibyte_codepoint_cross_engine() {
    // U+00E9 = 233 = 'é'
    let src = "f>t;chr 233";
    for engine in ENGINES_ALL {
        assert_eq!(run(engine, src, "f"), "é", "{engine}: chr 233");
    }
}

// ── round-trip ──────────────────────────────────────────────────────────

#[test]
fn ord_chr_roundtrip_ascii_cross_engine() {
    // `chr ord c == c` for any single-char ASCII string.
    let src = "f>t;chr ord \"z\"";
    for engine in ENGINES_ALL {
        assert_eq!(run(engine, src, "f"), "z", "{engine}: chr(ord(\"z\"))");
    }
}

#[test]
fn chr_ord_roundtrip_codepoint_cross_engine() {
    let src = "f>n;ord chr 65";
    for engine in ENGINES_ALL {
        assert_eq!(run(engine, src, "f"), "65", "{engine}: ord(chr(65))");
    }
}
