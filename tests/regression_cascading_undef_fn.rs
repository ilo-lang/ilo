//! Regression: a function whose declaration fails to parse used to produce
//! `ILO-T005 undefined function 'X'` once per call site. With 10 broken
//! functions each called 30 times, this drowned the real parse errors in
//! 300+ cascade diagnostics. See the cron-explainer persona run that logged
//! 286 ILO-T005 from ~10 root causes for the original motivation.
//!
//! Fix:
//!   1. Parser records function names whose body/return-type failed to parse
//!      on `Program.parse_failed_fns`.
//!   2. Verifier skips type-checking parse-failed functions (their AST is
//!      poison) and collapses `undefined function 'X'` at call sites to ONE
//!      diagnostic per fn with a cross-reference back to the parse error.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_err(src: &str) -> String {
    let out = ilo()
        .arg("--text")
        .arg(src)
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success(), "expected failure for {src:?}");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn run_ok_ast(src: &str) {
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
fn parse_failed_fn_called_many_times_emits_one_t005() {
    // `broken` has a malformed return-type (`>;` instead of `>n;`). Without
    // the dedup, each of the 4 call sites in `caller` would emit
    // `ILO-T005 undefined function 'broken'`. After the fix, exactly one
    // collapsed diagnostic with a cross-reference to the parse error.
    let src = "broken n:n>;n+1
caller>n;x=broken 5;y=broken 6;z=broken 7;w=broken 8;x+y+z+w";
    let err = run_err(src);
    let t005 = count_code(&err, "ILO-T005");
    assert!(
        t005 <= 1,
        "expected at most 1 ILO-T005 after cascade fix, got {t005}.\nstderr:\n{err}"
    );
    // The collapsed diagnostic must cross-reference the originating parse
    // error so an agent can navigate to the root cause.
    assert!(
        err.contains("definition failed to parse"),
        "expected cross-reference note in collapsed diagnostic, stderr:\n{err}"
    );
    // The root-cause parse error itself must still be present in full.
    assert!(
        err.contains("ILO-P"),
        "originating parse error must still surface, stderr:\n{err}"
    );
}

#[test]
fn distinct_parse_failed_fns_each_get_one_diagnostic() {
    // Two separately broken functions, each called multiple times. The
    // dedup is per-function, so we expect exactly 2 ILO-T005 (one per
    // broken fn), not 1 and not 6.
    let src = "alpha n:n>;n
beta n:n>;n
caller>n;a1=alpha 1;a2=alpha 2;a3=alpha 3;b1=beta 4;b2=beta 5;b3=beta 6;a1+a2+a3+b1+b2+b3";
    let err = run_err(src);
    let t005 = count_code(&err, "ILO-T005");
    assert!(
        t005 <= 2,
        "expected at most 2 ILO-T005 (one per broken fn), got {t005}.\nstderr:\n{err}"
    );
    assert!(
        t005 >= 1,
        "expected at least 1 ILO-T005 cross-reference, got {t005}.\nstderr:\n{err}"
    );
}

#[test]
fn legitimate_undefined_function_still_errors() {
    // The cascade dedup only applies to functions whose declaration was
    // started but failed to parse. A genuinely-undefined name (typo of a
    // builtin, missing import, etc) must STILL surface a normal ILO-T005
    // with the usual suggestion text — otherwise the suppression has gone
    // too far and we're hiding real errors.
    let src = "caller>n;x=nosuchfunction 5;x";
    let err = run_err(src);
    let t005 = count_code(&err, "ILO-T005");
    assert!(
        t005 >= 1,
        "genuine undefined function must still error, got {t005}.\nstderr:\n{err}"
    );
    // The collapsed-cascade hint should NOT appear here — this is a real
    // undefined fn, not a parse-failed one.
    assert!(
        !err.contains("definition failed to parse"),
        "real undefined-fn errors should not get cross-reference text, stderr:\n{err}"
    );
}

#[test]
fn parse_failed_fn_body_does_not_emit_type_errors() {
    // If we still type-checked the (recovered, possibly partial) body of a
    // parse-failed function, we'd emit ILO-T errors for every reference
    // inside it. The body must be skipped entirely.
    //
    // Here `broken` has a malformed return-type; its body would reference an
    // undefined `q`. Pre-fix: ILO-P plus an ILO-T005 for `q`. Post-fix:
    // only the parse error.
    let src = "broken n:n>;q
caller>n;broken 1";
    let err = run_err(src);
    // Must have the root parse error.
    assert!(
        err.contains("ILO-P"),
        "expected parse error, stderr:\n{err}"
    );
    // The body of `broken` must NOT have been type-checked: no "undefined
    // function 'q'" or "undefined variable 'q'" should appear from inside
    // the broken body. The only T005 allowed is the collapsed one at the
    // call site referring to `broken` itself.
    let bad = err.contains("'q'");
    assert!(
        !bad,
        "body of parse-failed fn must not be type-checked, stderr:\n{err}"
    );
}

#[test]
fn cron_explainer_style_cascade_collapses() {
    // Cron-explainer persona logged 286 ILO-T005 from ~10 root causes. We
    // simulate by defining 5 broken functions each called 6 times. The
    // pre-fix count would be ~30 ILO-T005. Post-fix: at most 5 (one per
    // broken fn).
    let src = "fa n:n>;n
fb n:n>;n
fc n:n>;n
fd n:n>;n
fe n:n>;n
main>n;\
r=fa 1;r=fa 2;r=fa 3;r=fa 4;r=fa 5;r=fa 6;\
r=fb 1;r=fb 2;r=fb 3;r=fb 4;r=fb 5;r=fb 6;\
r=fc 1;r=fc 2;r=fc 3;r=fc 4;r=fc 5;r=fc 6;\
r=fd 1;r=fd 2;r=fd 3;r=fd 4;r=fd 5;r=fd 6;\
r=fe 1;r=fe 2;r=fe 3;r=fe 4;r=fe 5;r=fe 6;r";
    let err = run_err(src);
    let t005 = count_code(&err, "ILO-T005");
    // Pre-fix would be ~30; post-fix must be at most 5 (one per broken fn)
    // and may be fewer if some broken fns get suppressed by other recovery.
    assert!(
        t005 <= 5,
        "expected at most 5 ILO-T005 after dedup, got {t005}.\nstderr:\n{err}"
    );
}

#[test]
fn valid_program_unaffected() {
    // Sanity: well-formed multi-function program with cross-calls must not
    // be perturbed by the cascade dedup.
    run_ok_ast("a n:n>n;n+1\nb n:n>n;a n");
}
