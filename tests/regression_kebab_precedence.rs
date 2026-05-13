// Regression tests pinning kebab-case identifier behaviour and the
// diagnostic-layer hint that fires when an undefined kebab-case ident's
// halves are both bound in scope.
//
// The lexer rule `[a-z][a-z0-9]*(-[a-z0-9]+)*` (logos, priority 1) makes
// kebab-case atomic: `best-d` is always one `Ident` token, never `best`
// `-` `d`. These tests lock that guarantee in across tree/VM/Cranelift
// engines so a future lexer change cannot silently re-introduce the
// persona-reported confusion. The diagnostic test then covers the
// secondary issue: when both halves resolve but the kebab ident does
// not, the error should explicitly tell the model the ident is atomic.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_ok(engine: &str, src: &str, func_argv: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine);
    for a in func_argv {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}` argv={func_argv:?}: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn check_all(src: &str, func: &str, expected: &str) {
    // Split `func` on whitespace so multi-arg invocations like "f 10 3"
    // are passed as separate argv entries (not a single quoted string).
    let argv: Vec<&str> = func.split_whitespace().collect();
    for engine in ["--run-tree", "--run-vm"] {
        let actual = run_ok(engine, src, &argv);
        assert_eq!(
            actual, expected,
            "engine={engine} src=`{src}` func=`{func}`"
        );
    }
    #[cfg(feature = "cranelift")]
    {
        let engine = "--run-cranelift";
        let actual = run_ok(engine, src, &argv);
        assert_eq!(
            actual, expected,
            "engine={engine} src=`{src}` func=`{func}`"
        );
    }
}

// ---- Lexer / parser guarantee: kebab-case is always one identifier ----

#[test]
fn kebab_ident_in_str_call_arg() {
    // The persona-reported case: `str best-d` must look up the kebab-case
    // ident, not compute `str(best) - d`. If the parser ever split the
    // ident this would either error (type mismatch) or print 7.
    check_all("f>t;best=10;d=3;best-d=99;str best-d", "f", "99");
}

#[test]
fn kebab_ident_distinct_from_halves_in_list() {
    // All three idents coexist; each resolves independently. Catches any
    // scope leakage where `best-d` would alias `best` or `d`.
    check_all(
        "f>L t;best=10;d=3;best-d=99;[str best-d, str best, str d]",
        "f",
        "[99, 10, 3]",
    );
}

#[test]
fn explicit_subtraction_still_works() {
    // The escape hatch the diagnostic recommends: `- best d` with spaces.
    check_all("f best:n d:n>n;- best d", "f 10 3", "7");
}

#[test]
fn multi_segment_kebab_ident() {
    // `a-b-c` is one ident even with three segments. Locks the regex's
    // `(-[a-z0-9]+)*` repetition.
    check_all("f>n;a-b-c=42;a-b-c", "f", "42");
}

#[test]
fn kebab_with_digit_segment() {
    // The regex permits digits in segments after the first letter; this
    // pins that case so a tightened regex doesn't break real-world names
    // like `v2-config` or `item-1`.
    check_all("f>n;v2-config=7;v2-config", "f", "7");
}

// ---- Diagnostic-layer: helpful hint when halves are bound but kebab is not ----

fn run_err(src: &str, func: &str) -> String {
    // Bare `ilo "<src>"` dumps AST and exits 0; verify only runs when a
    // function is named. Pass a func so the verifier path executes.
    let out = ilo().args([src, func]).output().expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected failure for `{src}` func=`{func}`, stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn diagnostic_hint_when_kebab_unbound_but_halves_bound() {
    // Originating persona report: `str best-d` errors as
    // "undefined variable 'best-d'" with the default suggestion
    // "did you mean 'best'?" — which reads like the parser split the
    // ident. The new hint should instead tell the reader the ident is
    // atomic and show the explicit subtraction form.
    let err = run_err("f>t;best=10;d=3;str best-d", "f");
    assert!(err.contains("ILO-T004"), "stderr: {err}");
    assert!(err.contains("undefined variable 'best-d'"), "stderr: {err}");
    assert!(
        err.contains("single identifier"),
        "expected kebab clarification, stderr: {err}"
    );
    assert!(
        err.contains("- best d"),
        "expected explicit subtraction form, stderr: {err}"
    );
    // The misleading default "did you mean 'best'?" must not appear.
    assert!(
        !err.contains("did you mean 'best'?"),
        "old suggestion leaked: {err}"
    );
}

#[test]
fn diagnostic_hint_multi_segment_no_subtract_form() {
    // For 3+ segments there's no single binary-subtract spelling to
    // recommend, so the hint just clarifies atomicity without a form.
    let err = run_err("f>n;a=1;b=2;c=3;+a-b-c 0", "f");
    assert!(err.contains("ILO-T004"), "stderr: {err}");
    assert!(
        err.contains("single identifier"),
        "expected kebab clarification, stderr: {err}"
    );
    // No 2-arg subtract suggestion for 3-segment names.
    assert!(
        !err.contains("'- a b'"),
        "should not suggest 2-arg subtract for 3-segment name: {err}"
    );
}

#[test]
fn diagnostic_falls_back_to_closest_match_when_half_unbound() {
    // If only one half resolves, the kebab-confusion theory doesn't
    // apply — fall back to the standard closest-match suggestion. Locks
    // that the new hint is targeted, not a blanket override.
    let err = run_err("f>t;best=10;str best-d", "f");
    assert!(err.contains("ILO-T004"), "stderr: {err}");
    assert!(
        !err.contains("single identifier"),
        "kebab hint should not fire when 'd' is unbound: {err}"
    );
    // Standard suggestion still appears.
    assert!(
        err.contains("did you mean"),
        "expected closest-match fallback, stderr: {err}"
    );
}
