//! Regression: a single parse error inside an `@`-loop body (or any block
//! body) used to cascade into 15-20 spurious ILO-P001 errors all pointing at
//! the same stray token. The first error has the actionable hint; the rest
//! are noise that hides the real issue and wastes agent retries.
//!
//! Fix: in `parse_program`, once we've emitted one P001-class error, drop
//! subsequent P001-class errors until we successfully consume another
//! declaration. Also: `sync_to_decl_boundary` now consumes a stray top-level
//! `}` so the resync loop always makes forward progress.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

/// Run `ilo <src>` and return stderr. Asserts the run failed.
fn run_err(src: &str) -> String {
    let out = ilo()
        .arg("--text")
        .arg(src)
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success(), "expected failure for {src:?}");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn run_ok(src: &str) {
    // Use --ast so inline single-fn snippets with required params don't
    // trip the new auto-run contract on a sanity-check (we only want
    // parser/verifier acceptance, not runtime).
    let out = ilo()
        .args(["--ast", src])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "expected success for {src:?}, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn count_code(stderr: &str, code: &str) -> usize {
    stderr.matches(&format!("error[{}]", code)).count()
}

#[test]
fn single_p001_in_at_body_does_not_cascade() {
    // `let` inside `@` body is a P009 (expected expression). Previously the
    // recovery path produced ~20 spurious ILO-P001 errors all pointing at the
    // stray `}` left over after the failed body parse. The first error should
    // be the only real one; total P001 count must be small (cascade gone).
    let src = "f n:n>n;s=0;@i 0..n{let x=5;s=+s i;return s};s";
    let err = run_err(src);
    let p001 = count_code(&err, "ILO-P001");
    assert!(
        p001 <= 2,
        "expected at most 2 ILO-P001 (cascade fix), got {p001}.\nstderr:\n{err}"
    );
    // Total error count should be small too.
    let total = err.matches("error[").count();
    assert!(
        total <= 3,
        "expected at most 3 total errors after cascade fix, got {total}.\nstderr:\n{err}"
    );
}

#[test]
fn parse_error_does_not_trigger_type_cascade() {
    // A parse error followed by a valid statement still surfaces only the
    // parse error, not consequent type errors. Here the `let` keyword fails
    // to parse as a statement inside the function body; the rest is dropped
    // and we should NOT see a "type" or "verify" cascade.
    let src = "f>n;let x=5;7";
    let err = run_err(src);
    // Real parse error must be present.
    assert!(
        err.contains("ILO-P"),
        "expected a parse error code, stderr:\n{err}"
    );
    // No "ILO-T" (type) cascade should appear from the broken body.
    assert!(
        !err.contains("ILO-T"),
        "type errors should not cascade from a parse error, stderr:\n{err}"
    );
}

#[test]
fn distinct_parse_errors_in_separate_blocks_each_surface() {
    // Two separate broken function bodies should each surface their own
    // real (non-P001) error. The cascade suppression must only collapse
    // P001 noise, not silence different real errors in different bodies.
    let src = "a>n;let x=5\nb>n;let y=6\nc>n;let z=7\nd>n;let w=8";
    let err = run_err(src);
    let p009 = count_code(&err, "ILO-P009");
    assert!(
        p009 >= 2,
        "each broken body should surface its own real error, got {p009} ILO-P009.\nstderr:\n{err}"
    );
}

#[test]
fn valid_program_produces_no_errors() {
    // Pure sanity check that the cascade fix does not introduce false
    // positives on well-formed input.
    run_ok("f n:n>n;s=0;@i 0..n{s=+s i};s");
}

#[test]
fn stray_top_level_close_brace_does_not_loop() {
    // A bare `}` at the top level used to be reported once per iteration of
    // the resync loop until MAX_ERRORS (20) was hit. After the fix,
    // `sync_to_decl_boundary` consumes the stray `}` and the cascade is
    // bounded.
    let src = "}}}}}}}}";
    let err = run_err(src);
    let p001 = count_code(&err, "ILO-P001");
    assert!(
        p001 <= 2,
        "stray `}}` should not cascade, got {p001} ILO-P001 errors.\nstderr:\n{err}"
    );
}
