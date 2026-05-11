// Regression: parameter names that share a prefix with a builtin alias
// must not trigger a false-positive "did you mean '<builtin>'?" suggestion.
//
// Original report: a parameter named `sm` (a Number) was used correctly
// inside its function, but the verifier produced
//   "undefined function 'sm' ... did you mean 'sum'?"
// when `sm` appeared in a call position. Since `sm` is in scope, the
// suggestion against the builtin table is a false positive — the name
// DOES resolve, it's just not a function.
//
// Run cross-engine so we cover both the tree-walker and VM paths.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn engines() -> &'static [&'static str] {
    &["--run-tree", "--run-vm"]
}

fn run_ok_all(src: &str, args: &[&str], expected: &str) {
    for engine in engines() {
        let mut cmd = ilo();
        cmd.arg(src).arg(engine);
        for a in args {
            cmd.arg(a);
        }
        let out = cmd.output().expect("failed to spawn ilo");
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(
            out.status.success(),
            "engine {engine} failed for src={src:?}\nstdout: {stdout}\nstderr: {stderr}"
        );
        assert_eq!(
            stdout.trim(),
            expected,
            "engine {engine} produced wrong output for src={src:?}"
        );
        // The whole point: no false-positive fuzzy suggestion in stderr.
        assert!(
            !stderr.contains("did you mean"),
            "engine {engine}: unexpected suggestion in stderr for src={src:?}\nstderr: {stderr}"
        );
    }
}

fn run_err(src: &str) -> String {
    // Pass `f` as the function arg so we hit the execution path; inline-no-func
    // form is AST-dump mode (per PR #178) which skips verify errors.
    let out = ilo()
        .args([src, "f"])
        .output()
        .expect("failed to spawn ilo");
    assert!(
        !out.status.success(),
        "expected failure for {src:?}, stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).into_owned()
}

// ---- The false-positive cases. Each function uses a short param name
// that is within Levenshtein distance 3 of a builtin alias. The body
// uses the param correctly, so the verifier must not emit a suggestion.

#[test]
fn sm_as_param_no_sum_suggestion() {
    // sm vs sum (dist 1) — used as a value via prefix `+`
    run_ok_all("f sm:n>n;+sm 1", &["f", "5"], "6");
}

#[test]
fn sx_as_param_no_str_suggestion() {
    // sx vs str (dist 2) — used as identity
    run_ok_all("f sx:t>t;sx", &["f", "hello"], "hello");
}

#[test]
fn ga_as_param_no_max_suggestion() {
    // ga vs max (dist 2) — used as identity
    run_ok_all("f ga:n>n;ga", &["f", "7"], "7");
}

#[test]
fn multi_param_prefix_collision() {
    // ab and bc both clash with builtin prefixes (`abs`, etc.).
    // Verifier must accept and add them cleanly.
    run_ok_all("f ab:n bc:n>n;+ab bc", &["f", "2", "3"], "5");
}

// ---- The Ref path: using a param as a value (not in a call position).

#[test]
fn sm_as_param_in_ref_position_no_suggestion() {
    // Direct reference (no call) — must resolve cleanly.
    run_ok_all("f sm:n>n;sm", &["f", "42"], "42");
}

// ---- Negative cases: a genuinely undefined `sm` should still produce
// a helpful suggestion. We do NOT require the suggestion to mention
// `sum` specifically — only that *some* suggestion is offered, proving
// the friendly-error path still works.

#[test]
fn genuinely_undefined_name_still_gets_suggestion() {
    // `sm` is not a param, not a local, not a function — the fuzzy
    // matcher should still help out.
    let err = run_err("f q:n>n;sm");
    assert!(
        err.contains("undefined variable 'sm'"),
        "expected ILO-T004 for undefined ref, got: {err}"
    );
    assert!(
        err.contains("did you mean"),
        "expected a suggestion when name is truly undefined, got: {err}"
    );
}

// ---- The call-position case: calling a non-function param. This was
// the original false-positive: `sm 1` produced "did you mean 'sum'?".
// Now it should produce a targeted error saying `sm` is a value, not a
// function — and NOT suggest a builtin.

#[test]
fn calling_param_as_function_no_builtin_suggestion() {
    let err = run_err("f sm:n>n;sm 1");
    assert!(
        !err.contains("did you mean 'sum'"),
        "false-positive builtin suggestion leaked: {err}"
    );
    // The error should mention that sm is not a function.
    assert!(
        err.contains("not a function") || err.contains("'sm'"),
        "expected targeted error about sm not being callable, got: {err}"
    );
}
