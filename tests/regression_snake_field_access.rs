// Regression tests for snake_case field access on records.
//
// Real-world JSON (consumed via `jpar!`) is overwhelmingly snake_case
// (`stargazers_count`, `change_1d`, `people_vaccinated_per_hundred`).
// The strict identifier rule (lowercase + hyphens) makes `r.snake_field`
// trip ILO-L002 at the lexer. The fix special-cases the lexer post-pass:
// after `Dot` / `DotQuestion`, contiguous `Ident (_ (Ident|int))*` runs are
// merged back into a single `Ident` so the parser sees one field name.
// Bindings (`my_var=5`) still emit ILO-L002.

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

// `r.stargazers_count` returns 42 across engines.
const SIMPLE: &str = "f j:t>R n t;rec=jpar! j;rec.stargazers_count";

fn check_simple(engine: &str) {
    assert_eq!(
        run(engine, SIMPLE, "f", &[r#"{"stargazers_count":42}"#]),
        "42",
        "engine={engine}"
    );
}

#[test]
fn snake_field_simple_tree() {
    check_simple("--run-tree");
}

#[test]
fn snake_field_simple_vm() {
    check_simple("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn snake_field_simple_cranelift() {
    check_simple("--run-cranelift");
}

// Multi-underscore field name.
const MULTI: &str = "f j:t>R n t;r=jpar! j;r.people_vaccinated_per_hundred";

fn check_multi(engine: &str) {
    assert_eq!(
        run(
            engine,
            MULTI,
            "f",
            &[r#"{"people_vaccinated_per_hundred":12.5}"#]
        ),
        "12.5",
        "engine={engine}"
    );
}

#[test]
fn snake_field_multi_tree() {
    check_multi("--run-tree");
}

#[test]
fn snake_field_multi_vm() {
    check_multi("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn snake_field_multi_cranelift() {
    check_multi("--run-cranelift");
}

// Field name with a digit segment (`change_1d`).
const DIGIT: &str = "f j:t>R n t;r=jpar! j;r.change_1d";

fn check_digit(engine: &str) {
    assert_eq!(
        run(engine, DIGIT, "f", &[r#"{"change_1d":0.07}"#]),
        "0.07",
        "engine={engine}"
    );
}

#[test]
fn snake_field_digit_tree() {
    check_digit("--run-tree");
}

#[test]
fn snake_field_digit_vm() {
    check_digit("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn snake_field_digit_cranelift() {
    check_digit("--run-cranelift");
}

// Safe access (`.?`) on a snake_case field.
#[test]
fn snake_field_safe_access_tree() {
    let out = run(
        "--run-tree",
        "f j:t>R n t;r=jpar! j;r.?stargazers_count",
        "f",
        &[r#"{"stargazers_count":7}"#],
    );
    assert_eq!(out, "7");
}

// ---- Negative regressions: PR #154 behaviour preserved ----

#[test]
fn bare_underscore_binding_still_errors() {
    // `my_var=5` in a binding position must still emit ILO-L002 with the
    // hyphen-suggestion friendly message.
    let err = run_err("f>n;rev_ps=5;rev_ps");
    assert!(err.contains("ILO-L002"), "stderr: {err}");
    assert!(err.contains("underscores are not allowed"), "stderr: {err}");
    assert!(err.contains("rev-ps"), "stderr: {err}");
}

#[test]
fn dot_then_plain_ident_unchanged() {
    // `r.foo` (no underscore) must still parse as a plain field access; the
    // following identifier is a separate token.  Sanity that we didn't
    // accidentally consume tokens beyond the field name.
    let out = run(
        "--run-tree",
        "f j:t>R n t;r=jpar! j;r.foo",
        "f",
        &[r#"{"foo":3}"#],
    );
    assert_eq!(out, "3");
}

#[test]
fn dot_then_ident_space_ident_keeps_tokens_separate() {
    // `r.foo bar` must NOT merge `bar` into the field access. `bar` is a
    // separate token and (since it isn't bound) the program should fail
    // rather than evaluate `r.foo` successfully.
    let err = run_err("f j:t>R n t;r=jpar! j;r.foo bar");
    // We don't pin the exact error code; only that it didn't silently
    // succeed treating `bar` as part of the field name.
    assert!(!err.is_empty(), "expected an error, got empty stderr");
}

// Field name ending in a bare digit (`x_1`, no trailing letter).
const BARE_DIGIT: &str = "f j:t>R n t;r=jpar! j;r.x_1";

fn check_bare_digit(engine: &str) {
    assert_eq!(
        run(engine, BARE_DIGIT, "f", &[r#"{"x_1":11}"#]),
        "11",
        "engine={engine}"
    );
}

#[test]
fn snake_field_bare_digit_tree() {
    check_bare_digit("--run-tree");
}

#[test]
fn snake_field_bare_digit_vm() {
    check_bare_digit("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn snake_field_bare_digit_cranelift() {
    check_bare_digit("--run-cranelift");
}

// Alternating Ident/Number/Ident segments (`x_2y_3z`).
const ALTERNATING: &str = "f j:t>R n t;r=jpar! j;r.x_2y_3z";

fn check_alternating(engine: &str) {
    assert_eq!(
        run(engine, ALTERNATING, "f", &[r#"{"x_2y_3z":99}"#]),
        "99",
        "engine={engine}"
    );
}

#[test]
fn snake_field_alternating_tree() {
    check_alternating("--run-tree");
}

#[test]
fn snake_field_alternating_vm() {
    check_alternating("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn snake_field_alternating_cranelift() {
    check_alternating("--run-cranelift");
}

// Real-world shape: `ema_20d_change_5d` (two `_Number Ident` groups).
const REAL_WORLD: &str = "f j:t>R n t;r=jpar! j;r.ema_20d_change_5d";

fn check_real_world(engine: &str) {
    assert_eq!(
        run(engine, REAL_WORLD, "f", &[r#"{"ema_20d_change_5d":0.42}"#]),
        "0.42",
        "engine={engine}"
    );
}

#[test]
fn snake_field_real_world_tree() {
    check_real_world("--run-tree");
}

#[test]
fn snake_field_real_world_vm() {
    check_real_world("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn snake_field_real_world_cranelift() {
    check_real_world("--run-cranelift");
}
