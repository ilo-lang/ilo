// Regression: an indented `--` comment line above a paren-bound call
// silently corrupted parsing. `normalize_newlines` (src/lexer/mod.rs)
// rewrites `\n`+indent to `;` before the logos lexer's `--[^\n]*`
// comment-skip runs. On an indented comment line, the trailing `\n`
// also became `;`, so the comment-skip regex (which stops at `\n`)
// greedily ate the comment text plus every following statement up to
// the next non-indented newline. The function body ended up empty and
// the diagnostic pointed many lines past the actual cause (typically
// inside a format-string `{}` placeholder), costing ~15 minutes of
// bisection per occurrence.
//
// The fix detects `--` directly inside `normalize_newlines` and skips
// past comment content without emitting `;`, leaving the trailing `\n`
// intact for the loop's existing newline handling. Strings are passed
// through verbatim so `--` inside a string literal is not mistaken for
// a comment.
//
// Tests run the same repro across every engine to anchor the
// invariant that comments are free at every layer.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_file(engine: &str, src: &str, entry: &str) -> (bool, String, String) {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "ilo_comment_parse_{}_{}.ilo",
        std::process::id(),
        seq
    ));
    std::fs::write(&path, src).unwrap();
    let out = ilo()
        .args([path.to_str().unwrap(), engine, entry])
        .output()
        .expect("failed to run ilo");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// The original repro from the assessment doc: indented `--` comment
// immediately above a paren-bound `fmt` call.
const PAREN_BOUND_FMT: &str = "main>t\n  -- build the thing\n  m=(fmt \"k={}\" 1)\n  m\n";

// Comment between two bindings (no paren-bound RHS) — also broke the
// same way before the fix because the comment text was swallowed
// across the synthesised `;`.
const COMMENT_BETWEEN_BINDINGS: &str = "main>n\n  a=1\n  -- midline note\n  b=2\n  +a b\n";

// Multiple comment lines in a row.
const STACKED_COMMENTS: &str = "main>n\n  -- one\n  -- two\n  -- three\n  x=42\n  x\n";

// Comment containing characters that look like operators / punctuation
// (`{}`, `;`, `()`) — the comment-skip path must not be confused.
const COMMENT_WITH_PUNCT: &str = "main>n\n  -- format is {} ; (yes)\n  x=7\n  x\n";

// `--` appearing inside a string literal must NOT be treated as a
// comment by normalize_newlines.
const DASHES_IN_STRING: &str = "main>t\n  m=\"hello -- world\"\n  m\n";

fn check_paren_bound_fmt(engine: &str) {
    let (ok, stdout, stderr) = run_file(engine, PAREN_BOUND_FMT, "main");
    assert!(
        ok,
        "engine={engine}: paren-bound fmt with leading comment failed: stderr={stderr}"
    );
    assert_eq!(stdout, "k=1", "engine={engine}: wrong output");
}

fn check_comment_between_bindings(engine: &str) {
    let (ok, stdout, stderr) = run_file(engine, COMMENT_BETWEEN_BINDINGS, "main");
    assert!(
        ok,
        "engine={engine}: midline comment broke parsing: stderr={stderr}"
    );
    assert_eq!(stdout, "3", "engine={engine}: wrong output");
}

fn check_stacked_comments(engine: &str) {
    let (ok, stdout, stderr) = run_file(engine, STACKED_COMMENTS, "main");
    assert!(
        ok,
        "engine={engine}: stacked comments broke parsing: stderr={stderr}"
    );
    assert_eq!(stdout, "42", "engine={engine}: wrong output");
}

fn check_comment_with_punct(engine: &str) {
    let (ok, stdout, stderr) = run_file(engine, COMMENT_WITH_PUNCT, "main");
    assert!(
        ok,
        "engine={engine}: comment with punctuation broke parsing: stderr={stderr}"
    );
    assert_eq!(stdout, "7", "engine={engine}: wrong output");
}

fn check_dashes_in_string(engine: &str) {
    let (ok, stdout, stderr) = run_file(engine, DASHES_IN_STRING, "main");
    assert!(
        ok,
        "engine={engine}: dashes-in-string broke parsing: stderr={stderr}"
    );
    assert_eq!(stdout, "hello -- world", "engine={engine}: wrong output");
}

#[test]
fn paren_bound_fmt_tree() {
    check_paren_bound_fmt("--run-tree");
}

#[test]
fn paren_bound_fmt_vm() {
    check_paren_bound_fmt("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn paren_bound_fmt_cranelift() {
    check_paren_bound_fmt("--run-cranelift");
}

#[test]
fn comment_between_bindings_tree() {
    check_comment_between_bindings("--run-tree");
}

#[test]
fn comment_between_bindings_vm() {
    check_comment_between_bindings("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn comment_between_bindings_cranelift() {
    check_comment_between_bindings("--run-cranelift");
}

#[test]
fn stacked_comments_tree() {
    check_stacked_comments("--run-tree");
}

#[test]
fn stacked_comments_vm() {
    check_stacked_comments("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn stacked_comments_cranelift() {
    check_stacked_comments("--run-cranelift");
}

#[test]
fn comment_with_punct_tree() {
    check_comment_with_punct("--run-tree");
}

#[test]
fn comment_with_punct_vm() {
    check_comment_with_punct("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn comment_with_punct_cranelift() {
    check_comment_with_punct("--run-cranelift");
}

#[test]
fn dashes_in_string_tree() {
    check_dashes_in_string("--run-tree");
}

#[test]
fn dashes_in_string_vm() {
    check_dashes_in_string("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn dashes_in_string_cranelift() {
    check_dashes_in_string("--run-cranelift");
}
