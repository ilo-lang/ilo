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
