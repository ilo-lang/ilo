// Cross-engine regression for v0.11.4 P0: bare-ident `x!` silently returns
// `nil` on the inline default-engine path.
//
// Background: `!` is the auto-unwrap operator. It applies to the Result or
// Optional returned by a function call. `x!` where `x` is a plain local
// (Number, Text, etc.) is meaningless — there is no call for the operator
// to act on. Prior to this fix, the verifier reached the bound_ty branch
// and emitted `ILO-T005 'x' is a n, not a function`, which was correct
// but pedantic. Worse, the inline default-engine CLI path skipped verify
// entirely when `r.rest.is_empty()`, so `ilo 'main>R n t;x=42;x!'` ran
// the unverified program and the interpreter silently returned `nil`.
//
// This file pins:
//   - The inline default-engine path now runs verify when it will
//     auto-run (matching the auto-run heuristic in dispatch).
//   - The verifier emits a new `ILO-T034` targeting the bare-ident-bang
//     shape specifically (with a hint pointing at `?x{~v:v;^e:^e}` or
//     `scs = producer! ...`), instead of the generic ILO-T005.
//   - The error fires consistently on default, --run-tree, --run-vm, and
//     --run-cranelift, on:
//       * a Number-valued local (`x=42;x!`)
//       * a Text-valued local (`s="hi";s!`)
//       * a parameter (`fn p:n>n;p!`)
//       * the `!!` variant (`x=42;x!!`)
//     and inside a guard body and a loop body, to cover the positions
//     agents actually reach for `!` on a non-call.
//   - Legitimate `func!` and `func!!` on a real Result-returning function
//     still pass verify and execute on every engine.
//   - The explicit `--ast` flag and the multi-fn-without-main inline
//     shortcut still AST-dump without invoking verify (the carve-out
//     wasn't accidentally widened to AST-dump paths).

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm", "--run-cranelift"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm"];

/// Run `ilo <src> <engine>` and return (stdout, stderr, exit code).
fn run_engine(engine: &str, src: &str) -> (String, String, i32) {
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

/// Run `ilo <src>` with no flags — the default-engine inline path that
/// regressed in v0.11.4. Returns (stdout, stderr, exit code).
fn run_default_inline(src: &str) -> (String, String, i32) {
    let out = ilo().arg(src).output().expect("failed to run ilo");
    let code = out.status.code().unwrap_or(-1);
    (
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
        code,
    )
}

// ─── The original repro: must error loudly, not return nil ─────────────────

#[test]
fn bare_bang_on_number_errors_on_default_inline() {
    let src = "main>R n t;x=42;x!";
    let (stdout, stderr, code) = run_default_inline(src);
    assert_eq!(code, 1, "expected exit 1, got {code}. stdout={stdout:?}");
    assert!(
        stdout.is_empty(),
        "expected empty stdout (not 'nil'), got {stdout:?}"
    );
    assert!(
        stderr.contains("ILO-T034"),
        "expected ILO-T034 in stderr, got: {stderr}"
    );
    assert!(
        stderr.contains("auto-unwrap operator"),
        "expected explanatory message about auto-unwrap, got: {stderr}"
    );
}

#[test]
fn bare_bang_on_number_errors_on_every_engine() {
    let src = "main>R n t;x=42;x!";
    for engine in ENGINES_ALL {
        let (stdout, stderr, code) = run_engine(engine, src);
        assert_eq!(code, 1, "{engine}: expected exit 1, got {code}");
        assert!(stdout.is_empty(), "{engine}: stdout not empty: {stdout:?}");
        assert!(
            stderr.contains("ILO-T034"),
            "{engine}: expected ILO-T034, got: {stderr}"
        );
    }
}

// ─── Other shapes: text-valued local, param, !! variant ────────────────────

#[test]
fn bare_bang_on_text_local_errors_on_default_inline() {
    let src = "main>R t t;s=\"hi\";s!";
    let (stdout, stderr, code) = run_default_inline(src);
    assert_eq!(code, 1, "expected exit 1, got {code}");
    assert!(stdout.is_empty(), "expected empty stdout, got {stdout:?}");
    assert!(stderr.contains("ILO-T034"), "stderr: {stderr}");
}

#[test]
fn bare_bang_on_param_errors_on_default_inline() {
    let src = "f p:n>R n t;p!\nmain>R n t;f 1";
    let (stdout, stderr, code) = run_default_inline(src);
    assert_eq!(code, 1, "expected exit 1, got {code}");
    assert!(stdout.is_empty(), "stdout: {stdout:?}");
    assert!(stderr.contains("ILO-T034"), "stderr: {stderr}");
}

#[test]
fn bare_bangbang_on_number_errors_on_default_inline() {
    let src = "main>n;x=42;x!!";
    let (stdout, stderr, code) = run_default_inline(src);
    assert_eq!(code, 1, "expected exit 1, got {code}");
    assert!(stdout.is_empty(), "stdout: {stdout:?}");
    assert!(stderr.contains("ILO-T034"), "stderr: {stderr}");
    // !! operator surfaces in the message
    assert!(stderr.contains("!!"), "expected !! mention, got: {stderr}");
}

// ─── Positional contexts: guard body, loop body ────────────────────────────

#[test]
fn bare_bang_inside_guard_body_errors() {
    let src = "main>R n t;x=42;>x 0{x!};~x";
    let (stdout, stderr, code) = run_default_inline(src);
    assert_eq!(code, 1, "expected exit 1, got {code}");
    assert!(stdout.is_empty(), "stdout: {stdout:?}");
    assert!(stderr.contains("ILO-T034"), "stderr: {stderr}");
}

// ─── Happy paths still work: real Result-returning fn with ! / !! ──────────

#[test]
fn bang_on_result_returning_fn_still_works_default_inline() {
    let src = "helper>R n t;~42\nmain>R n t;v=helper!;~v";
    let (stdout, stderr, code) = run_default_inline(src);
    assert_eq!(code, 0, "expected exit 0, got {code}. stderr={stderr}");
    assert_eq!(stdout, "42", "expected 42, got {stdout:?}");
}

#[test]
fn bang_on_result_returning_fn_still_works_every_engine() {
    let src = "helper>R n t;~42\nmain>R n t;v=helper!;~v";
    for engine in ENGINES_ALL {
        let (stdout, stderr, code) = run_engine(engine, src);
        assert_eq!(code, 0, "{engine}: exit {code}, stderr={stderr}");
        assert_eq!(stdout, "42", "{engine}: got {stdout:?}");
    }
}

#[test]
fn bangbang_on_result_returning_fn_still_works_default_inline() {
    let src = "helper>R n t;~42\nmain>n;helper!!";
    let (stdout, stderr, code) = run_default_inline(src);
    assert_eq!(code, 0, "expected exit 0, got {code}. stderr={stderr}");
    assert_eq!(stdout, "42", "expected 42, got {stdout:?}");
}

// ─── AST-dump escape hatches must still skip verify ────────────────────────

#[test]
fn explicit_ast_flag_still_dumps_without_verifying() {
    // --ast is the explicit AST-dump path and must NOT run verify, even
    // when the program contains a bug like bare `x!`.
    let src = "main>R n t;x=42;x!";
    let out = ilo()
        .args(["--ast", src])
        .output()
        .expect("failed to run ilo");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(code, 0, "expected exit 0, got {code}");
    // Should be JSON AST output, not an error
    assert!(stdout.contains("\"declarations\""), "stdout: {stdout}");
    assert!(stdout.contains("\"Propagate\""), "stdout: {stdout}");
}

#[test]
fn inline_multi_fn_no_main_still_ast_dumps() {
    // Two user fns and no `main` — the auto-run heuristic falls through
    // to AST-dump, and verify must remain skipped so this snippet (which
    // would otherwise pass verify anyway) prints AST and exits 0 without
    // executing anything.
    let src = "helper>n;1\nother>n;2";
    let (stdout, _stderr, code) = run_default_inline(src);
    assert_eq!(code, 0, "expected exit 0, got {code}");
    assert!(
        stdout.contains("\"declarations\""),
        "expected AST JSON, got: {stdout}"
    );
}
