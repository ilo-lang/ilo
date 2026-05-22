// Regression: ILO-378 — prefix-binop EOF span drift causes misleading
// ILO-P003 diagnostic.
//
// Background: a scientific-researcher persona run (2026-05-21, pair 23)
// wrote `*/dt 1 6 var` at the end of a statement.  The parser parsed it
// as `*(/ dt 1) 6`, leaving the trailing identifier orphaned at top-level.
// `parse_decl` then tried to parse the orphaned identifier as a new
// function declaration and emitted:
//
//   ILO-P003: expected '>', got ';'
//
// anchored at the `;` that immediately follows the orphaned identifier —
// far from the actual problem.  The agent spent three iterations chasing
// the wrong column before working around it by binding an intermediate.
//
// Fix (ILO-378): `parse_decl` now detects the pattern "plain identifier
// immediately followed by `;`/`}`/EOF at top-level" and emits a targeted
// ILO-P003 anchored on the orphaned identifier itself, with a hint that
// names the prefix-binop bind-first workaround.
//
// This file pins:
//   1. The diagnostic code is ILO-P003 (not the former spurious
//      "expected '>', got ';'" cascading from parse_fn_decl).
//   2. The diagnostic span does NOT land on the `;` (the old anchor).
//      Concretely: the column of the orphaned identifier is before the
//      column of the `;` that caused the previous false anchor.
//   3. The hint text mentions the prefix-binop bind-first pattern.
//   4. Well-formed prefix-binop expressions still parse correctly.

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
        "ilo_span_eof_drift_{name}_{}_{n}.ilo",
        std::process::id()
    ));
    std::fs::write(&path, src).expect("write src");
    path
}

fn run_expect_parse_err(name: &str, src: &str) -> String {
    let path = write_src(name, src);
    let out = ilo().arg(&path).arg("main").output().expect("run ilo");
    let _ = std::fs::remove_file(&path);
    assert!(
        !out.status.success(),
        "expected parse failure for `{src}`, but ilo succeeded"
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

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

// ── Core reproducer ───────────────────────────────────────────────────────

/// `*/dt 1 6 s1` — the scientific-researcher shape from the A/B run.
/// The parser reads `*(/ dt 1) 6`, leaving `s1` orphaned.
/// The diagnostic must be ILO-P003, anchored on `s1` (not on `;`).
#[test]
fn prefix_binop_orphaned_operand_anchors_on_ident_not_semicolon() {
    // Source: `main>n;dt=1.0;s1=5.0;dth=*/dt 1 6 s1;prnt dth`
    // Columns (1-based): `;` after `s1` is at col 32 in `dth=*/dt 1 6 s1;`
    // The `s1` identifier is at col 29.  Old code anchored on `;` (col 32);
    // new code must anchor on `s1` (col 29) — strictly before col 32.
    let src = "main>n;dt=1.0;s1=5.0;dth=*/dt 1 6 s1;prnt dth";
    let payload = run_expect_parse_err("core_reproducer", src);
    assert!(
        payload.contains("ILO-P003"),
        "expected ILO-P003, got: {payload}"
    );
    // The orphaned `s1` starts at byte offset 35 (0-based) in `src`, which
    // is col 36 (1-based) in the full string.  The `;` that follows is one
    // further.  Verify the reported col is less than the `;` position.
    let col = parse_field_int(&payload, "col");
    // Semicolon after s1: count from start of src.
    let semi_col = src.find(";prnt").map(|i| i + 1).unwrap_or(usize::MAX) as i64;
    assert!(
        col < semi_col,
        "expected diagnostic col ({col}) to be before `;` col ({semi_col}). Payload: {payload}"
    );
}

/// Hint text must mention "prefix-binop" and "bind" (the key guidance).
#[test]
fn prefix_binop_orphaned_operand_hint_mentions_bind_first() {
    let src = "main>n;dt=1.0;s1=5.0;dth=*/dt 1 6 s1;prnt dth";
    let payload = run_expect_parse_err("hint_text", src);
    assert!(
        payload.contains("prefix-binop") || payload.contains("prefix"),
        "hint should mention prefix-binop: {payload}"
    );
    assert!(
        payload.to_lowercase().contains("bind"),
        "hint should mention bind-first pattern: {payload}"
    );
}

/// Single-operator variant: `*a b c` — the `*` consumes `a` and `b`,
/// leaving `c` orphaned.  Must emit ILO-P003 anchored on the orphaned `c`
/// (not on the `;` one position further right).
#[test]
fn single_prefix_binop_orphaned_third_arg() {
    // Source: `main>n;a=1.0;b=2.0;cv=3.0;r=*a b cv;r`
    // (using `cv` so the orphaned ident is unambiguous in the payload)
    // `*a b` consumes `a` and `b`; `cv` is orphaned before `;`.
    // The `;` directly follows `cv` — orphaned ident col < `;` col.
    let src = "main>n;a=1.0;b=2.0;cv=3.0;r=*a b cv;r";
    let payload = run_expect_parse_err("single_op_orphan", src);
    assert!(
        payload.contains("ILO-P003"),
        "expected ILO-P003, got: {payload}"
    );
    // `cv` is at position 33 (0-based) → col 34 (1-based).
    // `;` after `cv` is at position 35 → col 36.
    // Diagnostic must land on `cv`, not on `;` or beyond.
    let col = parse_field_int(&payload, "col");
    // Byte offset of `cv` in `src` (0-based) + 1 = 1-based col.
    let cv_col = src.find("cv;r").map(|i| i + 1).unwrap_or(usize::MAX) as i64;
    let semi_after_cv = cv_col + 2; // `cv` is 2 chars, then `;`
    assert!(
        col < semi_after_cv,
        "expected diagnostic col ({col}) to be on `cv` not past the `;` (col {semi_after_cv}). Payload: {payload}"
    );
}

// ── Negative controls: well-formed expressions must still parse ───────────

/// `*/a b` is a valid expression: `*(/ a) b`... actually `/a` is prefix
/// divide with a single operand, which would fail on its own, so test
/// `*+a b c d` which is `*(+a b) c d` — wait, that orphans `c d`.
/// Use a well-formed nested form: `++a b c` = `+(+a b) c`.  Must parse.
#[test]
fn nested_prefix_binop_still_parses() {
    // `++a b c` = Add(Add(a, b), c) — valid, 3 atoms for 2 operators.
    let path = write_src("nested_happy", "main>n;a=1.0;b=2.0;c=3.0;r=++a b c;r");
    let out = ilo().arg(&path).arg("main").output().expect("run ilo");
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "++a b c should parse and run: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "6",
        "++a b c with a=1 b=2 c=3 should yield 6"
    );
}

/// `*a b` (two operands, correct) must continue to work.
#[test]
fn simple_prefix_binop_still_parses() {
    let path = write_src("simple_happy", "main>n;a=3.0;b=4.0;r=*a b;r");
    let out = ilo().arg(&path).arg("main").output().expect("run ilo");
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "*a b should parse and run: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "12");
}

/// `*2 - tcr2 tcr` — the happy-path from the existing regression suite
/// that mixes prefix-`*` with prefix-`-`.  Must still parse.
#[test]
fn mixed_prefix_still_parses() {
    let path = write_src(
        "mixed_happy",
        "main>n;tcr=1.0;tcr2=2.0;per=*2 - tcr2 tcr;per",
    );
    let out = ilo().arg(&path).arg("main").output().expect("run ilo");
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "*2 - tcr2 tcr should parse and run: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "2");
}
