// Regression: parse errors that fire when EOF terminates a prefix
// operator must anchor on the dangling operator, not at line 1 col 1.
//
// Background: scientific-researcher rerun3 (assessment doc line 2803)
// reported "Long unbalanced-brace lines silently mis-report parse error
// position" — concretely, the persona's first cut had a stray binary `-`
// inside a long prefix expression. The error they saw landed ~200 chars
// downstream rather than at the actual `-` problem, costing several
// iterations to localise. The parked entry at line 124 (filed during
// persona-diagnostic-batch-2 investigation) noted no concrete repro
// surfaced from the persona log alone — bare `-x` and `y= -5` both
// produced well-located errors on `main`.
//
// On the rerun bisect (2026-05-17) the underlying cause finally
// reproduced cleanly: any parse that runs out of tokens while looking
// for an operand to a prefix operator hits the EOF arm of
// `Parser::parse_atom` (`src/parser/mod.rs`). That arm built its
// `ILO-P010 expected expression, got EOF` error with `peek_span()`,
// which at EOF returns `Span::UNKNOWN` and renders as line 1 col 1 —
// regardless of where the dangling operator actually sits.
//
// `parser/mod.rs:118-122` already calls out this drift as an
// "infra-wide limitation" and routes around it for function headers via
// `check_fn_header_boundary` (which falls back to `prev_span()`). The
// fix in this branch extracts that fallback as a reusable helper
// (`here_or_prev_span`) and applies it to the EOF arm of `parse_atom`,
// so every prefix-operator-at-EOF error now lands on the dangling
// operator's line/column.
//
// This file pins:
//  1. every prefix-binop family (`+`, `-`, `*`, `/`, `<`, `>`, `<=`, `>=`)
//     reports an EOF-time error whose span is NOT line 1 col 1.
//  2. the dangling-prefix-op case sits past column 1 in a single-line
//     program (so a fixed line 1 col 1 anchor would not coincidentally
//     match the real position).
//  3. multi-line programs where the dangling prefix op sits on a
//     non-first line report on that line.
//
// Cross-engine isn't relevant — these are parser-time errors that the
// VM / Cranelift backends never see.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(name: &str, src: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "ilo_prefix_eof_span_{name}_{}_{n}.ilo",
        std::process::id()
    ));
    std::fs::write(&path, src).expect("write src");
    path
}

/// Run `ilo <path> main`, expect a parse failure, and return the JSON
/// error payload as a string.
fn run_expect_parse_err(name: &str, src: &str) -> String {
    let path = write_src(name, src);
    let out = ilo().arg(&path).arg("main").output().expect("run ilo");
    let _ = std::fs::remove_file(&path);
    assert!(
        !out.status.success(),
        "expected parse failure for `{src}`, but ilo succeeded with stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

/// Parse a `"line":N` integer out of the JSON error payload.
fn parse_field_int(payload: &str, field: &str) -> i64 {
    let needle = format!("\"{field}\":");
    let start = payload
        .find(&needle)
        .unwrap_or_else(|| panic!("field `{field}` not in payload: {payload}"));
    let rest = &payload[start + needle.len()..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit() && c != '-')
        .unwrap_or(rest.len());
    rest[..end]
        .parse()
        .unwrap_or_else(|_| panic!("could not parse `{field}` from `{rest}`"))
}

/// Assert the error payload's span does NOT collapse to the
/// `Span::UNKNOWN` rendering of line 1 col 1.
fn assert_not_line1_col1(name: &str, payload: &str) {
    let line = parse_field_int(payload, "line");
    let col = parse_field_int(payload, "col");
    assert!(
        !(line == 1 && col == 1),
        "{name}: expected span to anchor on dangling prefix op, got line=1 col=1. Full payload: {payload}"
    );
}

// ── Single-line: every prefix-binop family at EOF ─────────────────────

#[test]
fn prefix_plus_at_eof_spans_past_col1() {
    // `g=+ a` followed by EOF — second operand missing.
    let src = "main>n;a=1;b=2;c=3;d=4;e=5;f=6;g=+ a";
    let payload = run_expect_parse_err("plus", src);
    assert_not_line1_col1("plus", &payload);
    // ILO-P010 is the expected code, not P001 (which would mean the
    // parse_atom path wasn't even reached).
    assert!(
        payload.contains("ILO-P010"),
        "expected ILO-P010 for plus, got: {payload}"
    );
}

#[test]
fn prefix_star_at_eof_spans_past_col1() {
    let src = "main>n;a=1;b=2;c=3;d=4;e=5;f=6;g=* a";
    let payload = run_expect_parse_err("star", src);
    assert_not_line1_col1("star", &payload);
    assert!(payload.contains("ILO-P010"));
}

#[test]
fn prefix_slash_at_eof_spans_past_col1() {
    let src = "main>n;a=1;b=2;c=3;d=4;e=5;f=6;g=/ a";
    let payload = run_expect_parse_err("slash", src);
    assert_not_line1_col1("slash", &payload);
    assert!(payload.contains("ILO-P010"));
}

#[test]
fn prefix_less_at_eof_spans_past_col1() {
    let src = "main>n;a=1;b=2;c=3;d=4;e=5;f=6;g=< a";
    let payload = run_expect_parse_err("less", src);
    assert_not_line1_col1("less", &payload);
    assert!(payload.contains("ILO-P010"));
}

#[test]
fn prefix_greater_at_eof_spans_past_col1() {
    let src = "main>n;a=1;b=2;c=3;d=4;e=5;f=6;g=> a";
    let payload = run_expect_parse_err("greater", src);
    assert_not_line1_col1("greater", &payload);
    assert!(payload.contains("ILO-P010"));
}

#[test]
fn prefix_le_at_eof_spans_past_col1() {
    let src = "main>n;a=1;b=2;c=3;d=4;e=5;f=6;g=<= a";
    let payload = run_expect_parse_err("le", src);
    assert_not_line1_col1("le", &payload);
    assert!(payload.contains("ILO-P010"));
}

#[test]
fn prefix_ge_at_eof_spans_past_col1() {
    let src = "main>n;a=1;b=2;c=3;d=4;e=5;f=6;g=>= a";
    let payload = run_expect_parse_err("ge", src);
    assert_not_line1_col1("ge", &payload);
    assert!(payload.contains("ILO-P010"));
}

// ── Bare-operator-at-EOF (no operands at all) ─────────────────────────

#[test]
fn bare_minus_at_eof_spans_past_col1() {
    // The cleanest reproduction of the scientific-researcher symptom:
    // a long line ending in a bare `-` would previously report line 1
    // col 1 regardless of how far down the line the `-` actually sat.
    let src = "main>n;a=1;b=2;c=3;d=4;e=5;f=6;-";
    let payload = run_expect_parse_err("bare_minus", src);
    assert_not_line1_col1("bare_minus", &payload);
    assert!(payload.contains("ILO-P010"));
    // Belt-and-braces: the column should sit well past col 1 because
    // the prefix line is ~32 chars long before the `-`.
    let col = parse_field_int(&payload, "col");
    assert!(
        col > 20,
        "bare_minus: expected col past 20, got col={col} for payload {payload}"
    );
}

#[test]
fn bare_slash_at_eof_spans_past_col1() {
    let src = "main>n;a=1;b=2;c=3;d=4;e=5;f=6;/";
    let payload = run_expect_parse_err("bare_slash", src);
    assert_not_line1_col1("bare_slash", &payload);
    assert!(payload.contains("ILO-P010"));
}

// ── Multi-line: dangling op on a non-first line ───────────────────────

#[test]
fn dangling_prefix_op_on_line_three_lands_on_line_three() {
    // A whitespace-separated multi-statement layout (rare in idiomatic
    // ilo, but valid for top-level decls). The dangling `-` sits on
    // line 3; the diagnostic must land on line 3, not line 1.
    let src = "main>n;\na=1;\nb=-\n";
    let payload = run_expect_parse_err("multiline", src);
    let line = parse_field_int(&payload, "line");
    assert_eq!(
        line, 3,
        "multiline: expected error on line 3, got line={line} for payload {payload}"
    );
    assert!(payload.contains("ILO-P010"));
}

// ── Negative control: well-formed prefix expressions still parse ──────

#[test]
fn well_formed_prefix_minus_still_parses() {
    // `*2 - tcr2 tcr` was the persona's cited snippet. It parses as
    // `2 * (tcr2 - tcr)` and must continue to work — the fix only
    // touches the EOF error arm, not the happy path.
    let path = write_src(
        "happy_path",
        "main>n;tcr=1.0;tcr2=2.0;per=*2 - tcr2 tcr;per",
    );
    let out = ilo().arg(&path).arg("main").output().expect("run ilo");
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "happy path failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "2",
        "happy path stdout mismatch"
    );
}
