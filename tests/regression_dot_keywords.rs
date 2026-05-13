// Regression tests for reserved-keyword field names at dot-access.
//
// Real-world JSON keys are frequently named after keywords (`type`, `if`,
// `use`, `with`, `true`, `false`, `nil`, ...). The strict identifier rule
// makes `parsed.type` trip ILO-P005 (`expected identifier, got Type`) at the
// parser, forcing the verbose `jpth! resp "type"` workaround.
//
// The fix: a lexer post-pass rewrites any keyword token sitting flush
// against a preceding `Dot`/`DotQuestion` into a `Token::Ident` using its
// original source slice. Bindings still emit their friendly ILO-P011 error.
// The pass runs before the snake_case merge so `.type_id` stitches.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine).arg(entry);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_err(src: &str) -> String {
    let out = ilo()
        .args([src, "--run-tree", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success(), "expected failure for `{src}`");
    String::from_utf8_lossy(&out.stderr).to_string()
}

// Build a one-shot `f j:t>R n t;r=jpar! j;r.<field>` program for the given
// reserved-keyword field name.
fn prog(field: &str) -> String {
    format!("f j:t>R n t;r=jpar! j;r.{field}")
}

fn check_keyword(engine: &str, field: &str, value: i32) {
    let src = prog(field);
    let json = format!("{{\"{field}\":{value}}}");
    assert_eq!(
        run(engine, &src, "f", &[&json]),
        value.to_string(),
        "engine={engine} field={field}"
    );
}

// Every reserved keyword that the lexer emits as a non-Ident token, and that
// could plausibly appear as a JSON field name. Type sigils (R/L/F/O/M/S) are
// included because real-world JSON keys can be a single uppercase letter.
const KEYWORDS: &[&str] = &[
    "type", "tool", "use", "with", "timeout", "retry", "if", "return", "let", "fn", "def", "var",
    "const", "true", "false", "nil", "R", "L", "F", "O", "M", "S",
];

fn check_all_keywords(engine: &str) {
    for (i, kw) in KEYWORDS.iter().enumerate() {
        check_keyword(engine, kw, i as i32 + 1);
    }
}

#[test]
fn dot_keywords_all_tree() {
    check_all_keywords("--run-tree");
}

#[test]
fn dot_keywords_all_vm() {
    check_all_keywords("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn dot_keywords_all_cranelift() {
    check_all_keywords("--run-cranelift");
}

// `.?keyword` (safe field access) still parses. We only assert the parse
// succeeds; runtime semantics on missing keys are unchanged by this fix.
const SAFE: &str = "f j:t>R n t;r=jpar! j;r.?type";

fn check_safe(engine: &str) {
    assert_eq!(run(engine, SAFE, "f", &[r#"{"type":17}"#]), "17");
}

#[test]
fn dot_keywords_safe_tree() {
    check_safe("--run-tree");
}

#[test]
fn dot_keywords_safe_vm() {
    check_safe("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn dot_keywords_safe_cranelift() {
    check_safe("--run-cranelift");
}

// `record.type_id` — keyword followed by snake_case suffix. The
// keyword-rewrite pass must run before the snake_case stitch so the
// trailing `_id` segment gets merged into the same Ident.
const SNAKE_AFTER_KW: &str = "f j:t>R n t;r=jpar! j;r.type_id";

fn check_snake_after_kw(engine: &str) {
    assert_eq!(
        run(engine, SNAKE_AFTER_KW, "f", &[r#"{"type_id":1234}"#]),
        "1234",
        "engine={engine}"
    );
}

#[test]
fn dot_keywords_snake_after_kw_tree() {
    check_snake_after_kw("--run-tree");
}

#[test]
fn dot_keywords_snake_after_kw_vm() {
    check_snake_after_kw("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn dot_keywords_snake_after_kw_cranelift() {
    check_snake_after_kw("--run-cranelift");
}

// `record.type_kind_id` — alternating keyword + snake segments.
const SNAKE_LONG: &str = "f j:t>R n t;r=jpar! j;r.type_kind_id";

fn check_snake_long(engine: &str) {
    assert_eq!(
        run(engine, SNAKE_LONG, "f", &[r#"{"type_kind_id":42}"#]),
        "42",
        "engine={engine}"
    );
}

#[test]
fn dot_keywords_snake_long_tree() {
    check_snake_long("--run-tree");
}

#[test]
fn dot_keywords_snake_long_vm() {
    check_snake_long("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn dot_keywords_snake_long_cranelift() {
    check_snake_long("--run-cranelift");
}

// --- Negative regressions: reserved words in binding position still error. ---
//
// The lexer rewrite is gated on `Dot`/`DotQuestion` being the immediately
// preceding token with no whitespace, so plain `type=5` still hits the
// existing ILO-P011 friendly-error path via `reserved_keyword_message`.

#[test]
fn type_as_binding_still_errors() {
    let err = run_err("f j:t>n;type=5;type");
    assert!(
        err.contains("reserved") || err.contains("Type") || err.contains("got Type"),
        "expected reserved-word error, got: {err}"
    );
}

#[test]
fn if_as_binding_still_errors() {
    let err = run_err("f j:t>n;if=5;if");
    assert!(
        err.contains("reserved") || err.contains("KwIf") || err.contains("got KwIf"),
        "expected reserved-word error, got: {err}"
    );
}

// `.<space>type` (whitespace between dot and keyword) still errors — the
// rewrite requires span-contiguity so it doesn't accidentally swallow
// keywords that aren't actually in field-access position.
#[test]
fn dot_with_whitespace_still_errors() {
    let err = run_err("f j:t>R n t;r=jpar! j;r. type");
    assert!(
        err.contains("ILO-P") || err.contains("expected"),
        "expected parse error, got: {err}"
    );
}
