// Regression test for the glued-negative-literal misparse diagnostic.
//
// Background: `a -1.5` lexes as `Number(a), Number(-1.5)` because the lexer
// packs a leading `-` (no space before) into the number token. This is
// load-bearing for call-arg forms (`mod n -2`, `sub 5 -3`) and list literals
// (`[1 -2 3]`), so we deliberately do NOT split in those contexts (see
// `regression_neg_literal_papercut.rs` for the contexts that DO split).
//
// The downside: when a user writes `0 -1.5` meaning "zero minus one point
// five" with an accidental missing space, the parser previously emitted a
// generic "expected declaration, got number `-1.5`" with no hint, leaving
// the user with no clue that the issue was the missing space.
//
// This test pins the tailored ILO-P001 hint that spells out the spacing rule
// and the parenthesised-negation workaround. It also pins the non-regressing
// shapes (spaces-both-sides subtraction, glued call-args, list literals,
// parenthesised negation) so a future lexer tweak that breaks any of those
// shows up here too. The hint emission is engine-independent (parser-level),
// and the positive shapes are exercised on every backend reachable from the
// public CLI: VM and Cranelift JIT (cranelift feature). The tree-walker was
// removed from the public CLI in 0.12.x but still runs in-process as the
// HOF-callback fallback, so the VM arm transitively covers it.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(src: &str, tag: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "ilo_neg_lit_diag_{}_{}_{}.ilo",
        std::process::id(),
        seq,
        tag,
    ));
    std::fs::write(&path, src).unwrap();
    path
}

fn run_ok(engine: &str, src: &str, args: &[&str]) -> String {
    let path = write_src(src, engine.trim_start_matches("--"));
    let mut cmd = ilo();
    cmd.arg(path.to_str().unwrap()).arg(engine);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for src=`{src}` args={args:?}: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn glued_neg_literal_at_stmt_position_emits_tailored_hint() {
    // `0 -1.5` inside a function body: lexer packs `-1.5`; parser sees
    // Number(0), Number(-1.5) and fails out of the body, re-enters
    // parse_decl which sees the Number(-1.5) at decl position.
    let src = "m > n\n  0 -1.5\nm\n";
    let path = write_src(src, "tailored_hint");
    let out = ilo()
        .arg(path.to_str().unwrap())
        .arg("--json")
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success(), "expected ilo to fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("ILO-P001"),
        "expected ILO-P001, got: {stderr}"
    );
    assert!(
        stderr.contains("got number `-1.5`"),
        "expected message to mention the offending number, got: {stderr}"
    );
    // The tailored hint should mention the spaces-both-sides rule AND the
    // parenthesised-negation workaround.
    assert!(
        stderr.contains("spaces both sides"),
        "expected hint to mention spaces both sides, got: {stderr}"
    );
    assert!(
        stderr.contains("(-1.5)"),
        "expected hint to mention `(-1.5)` parens workaround, got: {stderr}"
    );
    // And it should reference the canonical `0 - 1.5` form.
    assert!(
        stderr.contains("0 - 1.5"),
        "expected hint to suggest the canonical `0 - 1.5` form, got: {stderr}"
    );
}

#[test]
fn glued_neg_int_literal_at_stmt_position_emits_tailored_hint() {
    // Same shape but with an integer to confirm the suggestion uses an
    // int-style example (no spurious `.0`).
    let src = "m > n\n  4 -1\nm\n";
    let path = write_src(src, "tailored_int_hint");
    let out = ilo()
        .arg(path.to_str().unwrap())
        .arg("--json")
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success(), "expected ilo to fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("ILO-P001"),
        "expected ILO-P001, got: {stderr}"
    );
    assert!(
        stderr.contains("spaces both sides"),
        "expected hint to mention spaces both sides, got: {stderr}"
    );
}

fn check_positive_shapes(engine: &str) {
    // Spaces-both-sides subtraction: the canonical form returns -1.5.
    assert_eq!(
        run_ok(engine, "m > n\n  0 - 1.5\n", &["m"]),
        "-1.5",
        "0 - 1.5 (spaces both sides) engine={engine}"
    );

    // Glued negative literal as call argument (load-bearing form). `mod 7
    // -3` must still parse as `mod(7, -3)`.
    assert_eq!(
        run_ok(engine, "x > n\n  mod 7 -3\n", &["x"]),
        "1",
        "mod 7 -3 (glued call-arg) engine={engine}"
    );

    // Glued negative literal in middle of list literal.
    assert_eq!(
        run_ok(engine, "x > n\n  l=[1 -2 3]\n  at l 1\n", &["x"]),
        "-2",
        "[1 -2 3] (glued list element) engine={engine}"
    );

    // Parenthesised negation: the documented workaround for "I want a
    // negative value as an expression".
    assert_eq!(
        run_ok(engine, "x > n\n  (-1.5)\n", &["x"]),
        "-1.5",
        "(-1.5) (paren negation) engine={engine}"
    );
}

#[test]
fn neg_literal_positive_shapes_vm() {
    check_positive_shapes("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn neg_literal_positive_shapes_cranelift() {
    check_positive_shapes("--jit");
}
