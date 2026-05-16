// Regression: `ilo file.ilo` with no func name used to dump raw AST JSON,
// which was a long-running first-touch surprise documented repeatedly in
// the assessment log (entries at lines 527, 623, 816, 839, 936, 1045).
//
// New behaviour:
//   * `ilo file.ilo`           with exactly one fn → runs that fn
//   * `ilo file.ilo`           with `main` defined → runs main
//   * `ilo file.ilo`           multi-fn without main → friendly listing,
//                                                      exits 1
//   * `ilo file.ilo func args` keeps working unchanged
//   * `ilo --ast file.ilo`     dumps the AST as JSON (explicit flag,
//                              works before or after the source)
//   * `ilo '<code>'`           inline auto-runs main or single fn;
//                              falls back to AST-dump only when there's
//                              no runnable target (zero fns, or multi-fn
//                              snippets without main). Soundness fix:
//                              `ilo 'f>n;42'` used to silently AST-dump
//                              to stdout with exit 0, breaking piped
//                              consumers expecting the program result.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(args: &[&str]) -> (bool, String, String) {
    let out = ilo()
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn ilo: {e}"));
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn write_temp(content: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("prog.ilo");
    std::fs::write(&path, content).expect("write temp ilo");
    (dir, path)
}

// ── single-fn file: auto-run ───────────────────────────────────────────────────

#[test]
fn single_fn_file_runs_with_no_func_arg() {
    let (_dir, path) = write_temp("f>n;42\n");
    let (ok, stdout, stderr) = run(&[path.to_str().unwrap()]);
    assert!(ok, "expected success; stderr: {stderr}");
    assert_eq!(stdout.trim(), "42");
    assert!(
        !stdout.contains("\"declarations\""),
        "no AST dump expected; got: {stdout}"
    );
}

#[test]
fn single_fn_file_runs_with_positional_args() {
    let (_dir, path) = write_temp("double x:n>n;*x 2\n");
    let (ok, stdout, stderr) = run(&[path.to_str().unwrap(), "7"]);
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "14");
}

// ── multi-fn file: friendly listing, non-zero exit ────────────────────────────

#[test]
fn multi_fn_file_with_no_func_arg_lists_and_exits_nonzero() {
    let (_dir, path) = write_temp("foo>n;1\nbar>n;2\nbaz>n;3\n");
    let (ok, stdout, stderr) = run(&[path.to_str().unwrap()]);
    assert!(!ok, "expected non-zero exit; stdout: {stdout}");
    assert!(
        stderr.contains("defines multiple functions"),
        "expected listing header; stderr: {stderr}"
    );
    assert!(
        stderr.contains("foo"),
        "expected foo listed; stderr: {stderr}"
    );
    assert!(
        stderr.contains("bar"),
        "expected bar listed; stderr: {stderr}"
    );
    assert!(
        stderr.contains("baz"),
        "expected baz listed; stderr: {stderr}"
    );
    assert!(
        stderr.contains("--ast"),
        "expected --ast hint; stderr: {stderr}"
    );
    assert!(
        !stdout.contains("\"declarations\""),
        "no AST dump expected; got: {stdout}"
    );
}

#[test]
fn multi_fn_file_with_func_name_runs_unchanged() {
    let (_dir, path) = write_temp("foo>n;1\nbar>n;2\nbaz>n;3\n");
    let (ok, stdout, stderr) = run(&[path.to_str().unwrap(), "bar"]);
    assert!(ok, "expected success; stderr: {stderr}");
    assert_eq!(stdout.trim(), "2");
}

#[test]
fn multi_fn_file_with_func_name_and_args() {
    let (_dir, path) = write_temp("add a:n b:n>n;+a b\nmul a:n b:n>n;*a b\n");
    let (ok, stdout, stderr) = run(&[path.to_str().unwrap(), "mul", "6", "7"]);
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "42");
}

// ── --ast flag: explicit AST dump ─────────────────────────────────────────────

#[test]
fn ast_flag_dumps_single_fn_file() {
    let (_dir, path) = write_temp("f>n;42\n");
    let (ok, stdout, _stderr) = run(&["--ast", path.to_str().unwrap()]);
    assert!(ok);
    assert!(
        stdout.contains("\"declarations\""),
        "expected JSON AST; got: {stdout}"
    );
    // Crucially does NOT execute the fn.
    assert!(
        !stdout.trim().starts_with("42"),
        "AST dump must not execute; got: {stdout}"
    );
}

#[test]
fn ast_flag_dumps_multi_fn_file() {
    let (_dir, path) = write_temp("foo>n;1\nbar>n;2\n");
    let (ok, stdout, stderr) = run(&["--ast", path.to_str().unwrap()]);
    assert!(ok, "stderr: {stderr}");
    assert!(
        stdout.contains("\"declarations\""),
        "expected JSON AST; got: {stdout}"
    );
    assert!(
        !stderr.contains("defines multiple functions"),
        "no listing should fire when --ast is explicit; stderr: {stderr}"
    );
}

#[test]
fn ast_flag_trailing_position() {
    let (_dir, path) = write_temp("foo>n;1\nbar>n;2\n");
    let (ok, stdout, _stderr) = run(&[path.to_str().unwrap(), "--ast"]);
    assert!(ok);
    assert!(stdout.contains("\"declarations\""));
}

#[test]
fn ast_flag_on_inline_code() {
    let (ok, stdout, _stderr) = run(&["--ast", "f>n;5"]);
    assert!(ok);
    assert!(stdout.contains("\"declarations\""));
}

// ── multi-fn file with `main` auto-runs main ──────────────────────────────────

#[test]
fn multi_fn_file_with_main_auto_runs_main() {
    // SKILL.md documents: "Multi-function files require either a
    // function name argument or a function called `main`." Before this
    // fix the CLI errored with 'defines multiple functions' even when
    // main was the obvious entry.
    let (_dir, path) = write_temp("helper a:n>n;+a 1\nmain>n;helper 41\n");
    let (ok, stdout, stderr) = run(&[path.to_str().unwrap()]);
    assert!(ok, "expected success; stderr: {stderr}");
    assert_eq!(stdout.trim(), "42");
    assert!(
        !stderr.contains("defines multiple functions"),
        "no listing should fire when main is defined; stderr: {stderr}"
    );
}

#[test]
fn multi_fn_file_explicit_func_arg_overrides_main() {
    // Explicit func arg still wins, even when main is defined.
    let (_dir, path) = write_temp("helper>n;7\nmain>n;42\n");
    let (ok, stdout, stderr) = run(&[path.to_str().unwrap(), "helper"]);
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "7");
}

// ── inline code auto-runs runnable entries ────────────────────────────────────

#[test]
fn inline_single_fn_auto_runs() {
    // Soundness fix: `ilo 'f>n;42'` used to AST-dump to stdout with
    // exit 0. Piped consumers got valid-looking JSON that was not the
    // program result. Now auto-runs the entry function per SKILL.md.
    let (ok, stdout, stderr) = run(&["f>n;42"]);
    assert!(ok, "expected success; stderr: {stderr}");
    assert_eq!(stdout.trim(), "42");
    assert!(
        !stdout.contains("\"declarations\""),
        "no AST dump expected for runnable inline snippet; got: {stdout}"
    );
}

#[test]
fn inline_multi_fn_with_main_auto_runs_main() {
    let (ok, stdout, stderr) = run(&["helper a:n>n;+a 1\nmain>n;helper 41"]);
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "42");
}

#[test]
fn inline_multi_fn_without_main_still_dumps_ast() {
    // Legacy contract from PR #178 preserved for non-runnable snippets:
    // when there is no `main` and no single entry, the CLI can't pick
    // one without guessing, so AST-dump remains the fallback. `--ast`
    // is the explicit form for pinning this behaviour.
    let (ok, stdout, _stderr) = run(&["foo>n;1\nbar>n;2"]);
    assert!(ok);
    assert!(
        stdout.contains("\"declarations\""),
        "non-runnable inline snippet should AST-dump; got: {stdout}"
    );
}

#[test]
fn inline_zero_fn_still_dumps_ast() {
    // Comment-only inline snippets have zero declarations and nothing
    // to run, so AST-dump is still the right fallback.
    let (ok, stdout, _stderr) = run(&["-- comment only"]);
    assert!(ok);
    assert!(
        stdout.contains("\"declarations\""),
        "zero-fn inline snippet should AST-dump; got: {stdout}"
    );
}

// ── no-fn file: AST dump (nothing to run) ─────────────────────────────────────

#[test]
fn empty_file_dumps_ast() {
    // A file with zero functions has nothing to run, so the AST-dump
    // path is still the right fallback. Empty file is the cleanest
    // example we can write portably.
    let (_dir, path) = write_temp("");
    let (ok, stdout, stderr) = run(&[path.to_str().unwrap()]);
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("\"declarations\""));
}

// ── synthetic inline-lambda decls hidden from listing ─────────────────────────

#[test]
fn synthetic_lambda_decls_hidden_from_multi_fn_listing() {
    // Inline lambdas lift to synthetic `__lit_N` top-level decls. These
    // are an implementation detail and must not surface in the
    // "available functions" listing the CLI prints for a multi-fn file
    // with no func arg and no main.
    let (_dir, path) =
        write_temp("sq xs:L n>L n;map (x:n>n;*x x) xs\nother xs:L n>L n;flt (x:n>b;>x 0) xs\n");
    let (ok, _stdout, stderr) = run(&[path.to_str().unwrap()]);
    assert!(!ok, "expected non-zero exit");
    assert!(
        !stderr.contains("__lit"),
        "synthetic __lit_N names must not leak to user listing; stderr: {stderr}"
    );
    assert!(
        stderr.contains("sq") && stderr.contains("other"),
        "real fn names should still be listed; stderr: {stderr}"
    );
}

// ── unknown subcommand: friendly error, not silent first-fn dispatch ──────────
//
// Originating bug: `ilo file.ilo wibble x` on a multi-fn file used to
// silently route to the FIRST declared function with `["wibble", "x"]`
// as positional args. The user saw a misleading arity error
// (`helper: expected 1 args, got 2`) far from the cause. Reported as
// interactive-cli rerun5 P1.
//
// New behaviour: when the leading positional doesn't match any user
// function in a multi-fn file, emit `no such function '<name>' in
// <file>` plus the available-function listing, exit 1. Single-fn
// files and inline programs keep their existing pass-through
// semantics so the unknown leading token is treated as a literal arg.

#[test]
fn unknown_subcommand_errors_with_available_listing() {
    // Two user functions, neither matches `wibble`. Pre-fix this
    // produced `helper: expected 1 args, got 2` from the first
    // declared function. Now it must produce a "no such function"
    // diagnostic listing both `helper` and `main`.
    let (_dir, path) = write_temp("helper a:n>n;+a 1\nmain>n;helper 41\n");
    let (ok, _stdout, stderr) = run(&[path.to_str().unwrap(), "wibble", "x"]);
    assert!(!ok, "unknown subcommand should exit non-zero");
    assert!(
        stderr.contains("no such function 'wibble'"),
        "expected 'no such function' error; stderr: {stderr}"
    );
    assert!(
        stderr.contains("available functions"),
        "expected available-functions listing; stderr: {stderr}"
    );
    assert!(
        stderr.contains("helper") && stderr.contains("main"),
        "expected both fn names listed; stderr: {stderr}"
    );
    // The pre-fix arity error must not appear: that was the misleading
    // symptom the fix is replacing.
    assert!(
        !stderr.contains("expected 1 args, got 2"),
        "must not leak first-fn arity error; stderr: {stderr}"
    );
}

#[test]
fn known_subcommand_still_routes_correctly_in_multi_fn_file() {
    // Companion check: the fix must not break the happy path. With a
    // valid function name as the leading positional, it should still
    // dispatch to that function with the remaining args.
    let (_dir, path) = write_temp("helper a:n>n;+a 1\nmain>n;helper 41\n");
    let (ok, stdout, stderr) = run(&[path.to_str().unwrap(), "helper", "5"]);
    assert!(ok, "known subcommand should succeed; stderr: {stderr}");
    assert_eq!(stdout.trim(), "6");
}

#[test]
fn unknown_subcommand_does_not_list_synthetic_lit_decls() {
    // Same #307 guarantee that the multi-fn-no-main listing has,
    // applied to the unknown-subcommand path. Inline-lambda HOF use
    // emits `__lit_N` synthetic top-level decls; they must not appear
    // in the "available functions" listing the user sees.
    let (_dir, path) =
        write_temp("sq xs:L n>L n;map (x:n>n;*x x) xs\nother xs:L n>L n;flt (x:n>b;>x 0) xs\n");
    let (ok, _stdout, stderr) = run(&[path.to_str().unwrap(), "wibble"]);
    assert!(!ok, "unknown subcommand should exit non-zero");
    assert!(
        stderr.contains("no such function 'wibble'"),
        "expected no-such-function error; stderr: {stderr}"
    );
    assert!(
        !stderr.contains("__lit"),
        "synthetic __lit_N names must not leak to user listing; stderr: {stderr}"
    );
    assert!(
        stderr.contains("sq") && stderr.contains("other"),
        "real fn names should still be listed; stderr: {stderr}"
    );
}

#[test]
fn single_fn_file_treats_unknown_leading_token_as_arg() {
    // For single-fn files (one user function, no `main`), the
    // pre-existing convention is that any positional args are passed
    // through to that sole function. The unknown-subcommand check
    // must NOT fire here — otherwise `ilo dbl.ilo 21` would refuse to
    // run `dbl 21` and demand an explicit subcommand name. This is
    // the auto-run contract from #307 / the SKILL.md "Inline programs
    // and single-function files" rule.
    let (_dir, path) = write_temp("dbl x:n>n;*x 2\n");
    let (ok, stdout, stderr) = run(&[path.to_str().unwrap(), "21"]);
    assert!(ok, "single-fn auto-run should succeed; stderr: {stderr}");
    assert_eq!(stdout.trim(), "42");
}

#[test]
fn inline_program_treats_unknown_leading_token_as_arg() {
    // Inline programs (`ilo '<code>'`) also auto-run their entry fn
    // with remaining args, per the same SKILL.md convention. The
    // unknown-subcommand check is file-only — inline keeps its
    // existing pass-through.
    let (ok, stdout, stderr) = run(&["dbl x:n>n;*x 2", "21"]);
    assert!(ok, "inline auto-run should succeed; stderr: {stderr}");
    assert_eq!(stdout.trim(), "42");
}

#[test]
fn multi_fn_file_numeric_leading_arg_passes_through_to_entry_fn() {
    // The unknown-subcommand check is shape-aware: only ident-shaped
    // tokens (alphabetic / underscore start) trigger the error. A
    // numeric leading arg is clearly data, not a typoed subcommand, so
    // it must still pass through to the first declared function — the
    // long-standing contract that `tests/eval_inline.rs`
    // unwrap_*_inline pins (`ilo file.ilo 42` where the multi-fn file
    // defines an `outer x:n>R n t` entry routes `42` to `outer`).
    //
    // Without the shape guard, the unknown-subcommand error fired on
    // every numeric-arg invocation, breaking those existing tests.
    let (_dir, path) = write_temp("outer x:n>n;+x 1\ninner x:n>n;*x 2\n");
    let (ok, stdout, stderr) = run(&[path.to_str().unwrap(), "41"]);
    assert!(
        ok,
        "numeric leading arg should pass through to entry fn; stderr: {stderr}"
    );
    assert_eq!(stdout.trim(), "42");
}

#[test]
fn multi_fn_file_quoted_string_leading_arg_passes_through() {
    // Companion to the numeric pass-through: a quoted-string leading
    // arg is also clearly data. Same guard, same outcome — first
    // declared fn receives the literal.
    let (_dir, path) = write_temp("greet s:t>t;s\nother s:t>t;s\n");
    let (ok, stdout, stderr) = run(&[path.to_str().unwrap(), "\"hi\""]);
    assert!(
        ok,
        "quoted-string leading arg should pass through; stderr: {stderr}"
    );
    assert_eq!(stdout.trim().trim_matches('"'), "hi");
}

#[test]
fn multi_fn_file_bracketed_list_leading_arg_passes_through() {
    // A bracketed-list leading arg is also data (starts with `[`,
    // not a letter), so it must pass through to the entry fn rather
    // than tripping the unknown-subcommand error.
    let (_dir, path) = write_temp("first xs:L n>n;hd xs\nother xs:L n>n;hd xs\n");
    let (ok, stdout, stderr) = run(&[path.to_str().unwrap(), "[7,8,9]"]);
    assert!(
        ok,
        "bracketed-list leading arg should pass through; stderr: {stderr}"
    );
    assert_eq!(stdout.trim(), "7");
}
