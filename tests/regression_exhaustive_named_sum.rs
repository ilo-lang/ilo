// ILO-362: exhaustive match enforcement on named sum types (discriminated unions).
//
// Named sum types (`type Foo = A | B(n) | C(t)`) require every variant to be
// covered by a pattern-match arm, or a wildcard `_:` must be present.  The
// verifier emits ILO-T024 listing every missing variant by name and suggests
// the correct arm syntax.
//
// These tests pin the verifier behaviour for multi-variant named sum types so
// regressions are caught early.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

/// Run ilo in inline mode (source as first arg, entry as second) and return
/// (success, stderr).
fn run(code: &str, entry: &str) -> (bool, String) {
    let out = ilo()
        .arg(code)
        .arg(entry)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn ilo: {e}"));
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    (out.status.success(), stderr)
}

fn assert_ok(code: &str, entry: &str) {
    let (ok, stderr) = run(code, entry);
    assert!(ok, "expected success\nstderr: {stderr}");
}

fn assert_err_code(code: &str, entry: &str, diag: &str) -> String {
    let (ok, stderr) = run(code, entry);
    assert!(!ok, "expected failure (code={diag})\nstderr: {stderr}");
    assert!(
        stderr.contains(diag),
        "expected {diag} in stderr\nstderr: {stderr}"
    );
    stderr
}

// ── All three variants covered — no error ─────────────────────────────────

#[test]
fn named_sum_three_variants_all_covered_no_error() {
    // shape type: circle(n) | square(n) | point — all arms present, verify must pass.
    // Wrap in a main that passes a constructed value so we can run it zero-arg.
    assert_ok(
        "type shape = circle(n) | square(n) | point\narea s:shape>n;?s{circle(r):*3.14159 *r r;square(side):*side side;point:0}\nmain>n;area (circle 5)",
        "main",
    );
}

// ── Wildcard satisfies exhaustiveness ─────────────────────────────────────

#[test]
fn named_sum_wildcard_satisfies_exhaustiveness() {
    assert_ok(
        "type shape = circle(n) | square(n) | point\narea s:shape>n;?s{circle(r):*3.14159 *r r;_:0}\nmain>n;area (circle 5)",
        "main",
    );
}

// ── Single missing variant reported ───────────────────────────────────────

#[test]
fn named_sum_single_missing_variant_reported() {
    // Only circle and square covered — point missing.
    let stderr = assert_err_code(
        "type shape = circle(n) | square(n) | point\narea s:shape>n;?s{circle(r):*3.14159 *r r;square(side):*side side}",
        "area",
        "ILO-T024",
    );
    assert!(
        stderr.contains("point"),
        "expected missing variant 'point' in error\nstderr: {stderr}"
    );
}

// ── Multiple missing variants all listed in one diagnostic ────────────────

#[test]
fn named_sum_multiple_missing_variants_all_listed() {
    // Only circle covered — square and point both missing.
    let stderr = assert_err_code(
        "type shape = circle(n) | square(n) | point\narea s:shape>n;?s{circle(r):*3.14159 *r r}",
        "area",
        "ILO-T024",
    );
    assert!(
        stderr.contains("square"),
        "expected missing variant 'square' in error\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("point"),
        "expected missing variant 'point' in error\nstderr: {stderr}"
    );
}

// ── Hint names correct arm syntax for payload variant ─────────────────────

#[test]
fn named_sum_hint_payload_variant_arm_syntax() {
    // point covered but circle(n) and square(n) missing — hint must show `circle(v): <expr>`.
    let stderr = assert_err_code(
        "type shape = circle(n) | square(n) | point\narea s:shape>n;?s{point:0}",
        "area",
        "ILO-T024",
    );
    assert!(
        stderr.contains("circle(v)"),
        "expected hint 'circle(v): <expr>' for payload variant\nstderr: {stderr}"
    );
}

// ── Hint names correct arm syntax for payload-less variant ────────────────

#[test]
fn named_sum_hint_payload_less_variant_arm_syntax() {
    // Four-variant payload-less enum; west missing.
    let stderr = assert_err_code(
        "type dir = north | south | east | west\ngo d:dir>t;?d{north:\"N\";south:\"S\";east:\"E\"}",
        "go",
        "ILO-T024",
    );
    assert!(
        stderr.contains("west"),
        "expected 'west' in error\nstderr: {stderr}"
    );
    // Payload-less variant hint must NOT add parens.
    assert!(
        !stderr.contains("west("),
        "payload-less variant hint must not suggest 'west(v)'\nstderr: {stderr}"
    );
}

// ── Four-variant type, two missing ────────────────────────────────────────

#[test]
fn named_sum_four_variants_two_missing() {
    let stderr = assert_err_code(
        "type dir = north | south | east | west\ngo d:dir>t;?d{north:\"N\";south:\"S\"}",
        "go",
        "ILO-T024",
    );
    assert!(stderr.contains("east"), "expected 'east'\nstderr: {stderr}");
    assert!(stderr.contains("west"), "expected 'west'\nstderr: {stderr}");
}

// ── Mixed payload / no-payload type, all covered ──────────────────────────

#[test]
fn named_sum_mixed_payload_all_covered() {
    assert_ok(
        "type result2 = ok(n) | err(t) | pending\nshow r:result2>t;?r{ok(v):str v;err(msg):msg;pending:\"wait\"}\nmain>t;show (ok 42)",
        "main",
    );
}

// ── Mixed payload / no-payload type, payload variant missing ──────────────

#[test]
fn named_sum_mixed_missing_payload_variant() {
    let stderr = assert_err_code(
        "type result2 = ok(n) | err(t) | pending\nshow r:result2>t;?r{err(msg):msg;pending:\"wait\"}",
        "show",
        "ILO-T024",
    );
    assert!(
        stderr.contains("ok"),
        "expected 'ok' in error\nstderr: {stderr}"
    );
}

// ── Error message says 'missing variant(s)' ───────────────────────────────

#[test]
fn named_sum_error_message_says_missing_variants() {
    let stderr = assert_err_code(
        "type shape = circle(n) | square(n) | point\narea s:shape>n;?s{circle(r):*3.14159 *r r}",
        "area",
        "ILO-T024",
    );
    assert!(
        stderr.contains("missing variant"),
        "expected 'missing variant' in error message\nstderr: {stderr}"
    );
}
