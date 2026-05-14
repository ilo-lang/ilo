// Cross-engine regression tests for the `!!` panic-unwrap operator.
//
// Background: `!` is the propagate-unwrap operator. `R ~v` becomes `v`,
// `R ^e` becomes an early-return of `^e` to the enclosing function, and
// `O some` / `O nil` works the same way for Optional. The enclosing
// function's return type must carry the propagation: an R-returning
// function for `!` on a Result, or an O / Nil / Unknown function for `!`
// on an Optional.
//
// `!!` is symmetric in shape but aborts the program on the failure path
// instead of propagating. That lets persona code in a `main>t` (or any
// non-Result function) call `rdl!! "path"`, `num!! "abc"`, `mget!! m "k"`
// and get clean diagnostics + exit 1 on failure, without the viral
// `>R t t` + `~v` ceremony.
//
// This file pins:
//   - `!!` exits 1 with a stderr diagnostic on Err / nil, on tree, VM, and
//     Cranelift (when the feature is enabled).
//   - `!!` returns the inner Ok / non-nil value on the happy path, on every
//     engine.
//   - `!!` works without an enclosing-return-type constraint — the bug fix
//     is specifically that an R-returning callee can be `!!`-unwrapped
//     from a `>n` / `>t` / `main>t` function.
//   - `!!` still compiles inside an R-returning function (no constraint
//     was added in the other direction).
//   - The verifier still rejects `!!` on a callee that returns neither
//     Result nor Optional, with ILO-T025.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm", "--run-cranelift"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm"];

/// Run `ilo <engine> <src>` and return (stdout, stderr, exit code).
fn run_full(engine: &str, src: &str) -> (String, String, i32) {
    let out = ilo()
        .args([src, engine])
        .output()
        .expect("failed to run ilo");
    let code = out.status.code().unwrap_or(-1);
    (
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
        code,
    )
}

/// Same but with an explicit entry-fn name.
fn run_full_entry(engine: &str, src: &str, entry: &str) -> (String, String, i32) {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    let code = out.status.code().unwrap_or(-1);
    (
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
        code,
    )
}

// ─── Result-shaped failure: num!! "abc" from a >n function ──────────────────

#[test]
fn bangbang_num_err_aborts_cross_engine() {
    let src = "main >n;num!! \"abc\"";
    for engine in ENGINES_ALL {
        let (stdout, stderr, code) = run_full(engine, src);
        assert_eq!(code, 1, "{engine}: expected exit 1, got {code}");
        assert!(
            stdout.is_empty(),
            "{engine}: expected empty stdout, got {stdout:?}"
        );
        assert!(
            stderr.contains("panic-unwrap"),
            "{engine}: stderr missing 'panic-unwrap': {stderr}"
        );
        assert!(
            stderr.contains("abc"),
            "{engine}: stderr missing Err payload 'abc': {stderr}"
        );
    }
}

// ─── Result-shaped happy path: num!! "42" returns the inner value ───────────

#[test]
fn bangbang_num_ok_returns_inner_cross_engine() {
    let src = "main >n;num!! \"42\"";
    for engine in ENGINES_ALL {
        let (stdout, _, code) = run_full(engine, src);
        assert_eq!(code, 0, "{engine}: expected exit 0");
        assert_eq!(stdout, "42", "{engine}: expected '42', got {stdout:?}");
    }
}

// ─── Optional-shaped failure: mget!! on a missing key aborts ────────────────

#[test]
fn bangbang_mget_nil_aborts_cross_engine() {
    let src = "main >n;m=mset mmap \"k\" 7;mget!! m \"missing\"";
    for engine in ENGINES_ALL {
        let (stdout, stderr, code) = run_full(engine, src);
        assert_eq!(code, 1, "{engine}: expected exit 1");
        assert!(
            stdout.is_empty(),
            "{engine}: expected empty stdout, got {stdout:?}"
        );
        assert!(
            stderr.contains("panic-unwrap"),
            "{engine}: stderr missing 'panic-unwrap': {stderr}"
        );
        assert!(
            stderr.contains("nil"),
            "{engine}: stderr should mention nil: {stderr}"
        );
    }
}

// ─── Optional-shaped happy path: mget!! on a present key returns the value ──

#[test]
fn bangbang_mget_present_returns_inner_cross_engine() {
    let src = "main >n;m=mset mmap \"k\" 7;mget!! m \"k\"";
    for engine in ENGINES_ALL {
        let (stdout, _, code) = run_full(engine, src);
        assert_eq!(code, 0, "{engine}: expected exit 0");
        assert_eq!(stdout, "7", "{engine}: expected '7', got {stdout:?}");
    }
}

// ─── No enclosing-return constraint: !! works from main>t ───────────────────
//
// The bug fix is specifically that `!!` does NOT require the enclosing
// function to return R / O. Persona writing `main >t;num!! "42";"ok"` (or
// equivalent) should compile and run cleanly.

#[test]
fn bangbang_no_enclosing_constraint_cross_engine() {
    // Result `!!` from a >t function — the prior `!` would have demanded
    // `main >R t t`, the whole point of `!!` is to lift that.
    let src = "main >t;v=num!! \"42\";str v";
    for engine in ENGINES_ALL {
        let (stdout, stderr, code) = run_full(engine, src);
        assert_eq!(
            code, 0,
            "{engine}: expected exit 0, got {code}, stderr={stderr}"
        );
        assert_eq!(stdout, "42", "{engine}: expected '42', got {stdout:?}");
    }
}

// ─── `!!` still compiles inside an R-returning function (no reverse rule) ───
//
// `!!` aborts on failure, so it works fine inside `>R _ _` too. Confirm we
// didn't add an inverse constraint forbidding `!!` in R-returning contexts.

#[test]
fn bangbang_inside_result_fn_cross_engine() {
    // `f >R n t` returns the result of `~(num!! "42")`. Reaches the Ok arm
    // on every engine and bubbles up as the wrapped value.
    let src = "f >R n t;v=num!! \"42\";~v";
    for engine in ENGINES_ALL {
        let (stdout, stderr, code) = run_full_entry(engine, src, "f");
        assert_eq!(
            code, 0,
            "{engine}: expected exit 0, got {code}, stderr={stderr}"
        );
        // Display of Value::Ok(42.0) is "~42" across engines.
        assert!(
            stdout.contains("42"),
            "{engine}: expected '42' in stdout, got {stdout:?}"
        );
    }
}

// ─── Verifier still rejects `!!` on a non-R / non-O callee ──────────────────

#[test]
fn bangbang_on_non_result_rejected_by_verifier() {
    // `len` returns `n`. Both engines route through the same verifier so
    // tree alone suffices for the type check coverage.
    let src = "main >n;len!! \"abc\"";
    let (_, stderr, code) = run_full("--run-tree", src);
    assert_eq!(code, 1, "expected exit 1");
    assert!(
        stderr.contains("ILO-T025"),
        "expected ILO-T025 in stderr, got: {stderr}"
    );
    assert!(
        stderr.contains("'!!'"),
        "expected '!!' in verifier message, got: {stderr}"
    );
}

// ─── Mixed `!` + `!!` in the same expression chain ──────────────────────────
//
// Pin that the two flavours coexist without parser confusion. `f >R n t`
// returns the Result; `main >n` uses `!!` to abort on Err, while `f` itself
// uses `!` to propagate from `num`.

#[test]
fn bangbang_and_bang_compose_cross_engine() {
    let src = "main >n;f!! \"42\";f s:t >R n t;v=num! s;~v";
    for engine in ENGINES_ALL {
        let (stdout, stderr, code) = run_full(engine, src);
        assert_eq!(
            code, 0,
            "{engine}: expected exit 0, got {code}, stderr={stderr}"
        );
        assert_eq!(stdout, "42", "{engine}: expected '42', got {stdout:?}");
    }
    // And the failure path: `f` propagates Err, `main` aborts on it.
    let src_fail = "main >n;f!! \"abc\";f s:t >R n t;v=num! s;~v";
    for engine in ENGINES_ALL {
        let (_, stderr, code) = run_full(engine, src_fail);
        assert_eq!(code, 1, "{engine}: expected exit 1");
        assert!(
            stderr.contains("panic-unwrap"),
            "{engine}: stderr should contain panic-unwrap: {stderr}"
        );
    }
}
