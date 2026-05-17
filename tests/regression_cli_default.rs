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

// ── engine-flag auto-pick-main (rerun6 P1) ────────────────────────────────────
//
// PR #307 fixed the Default-engine branch to auto-run `main` on a
// multi-fn file when no func arg is given. The explicit-engine paths
// (`--run-tree`, `--run-vm`, `--run-cranelift`) kept the pre-307
// behaviour of treating the first declared fn as the entry, which
// surfaced as:
//   * Tree     → misleading arity error (`helper: expected 1 args, got 0`)
//   * VM       → silent `nil` result, exit 0
//   * Cranelift→ bare "Cranelift JIT: compilation failed" string
// Reported across four rerun6 personas (ml-engineer, routing-tsp,
// devops-sre, html-scraper). Fix mirrors the #307 heuristic in
// `resolve_engine_func_name`, exercised here across all four engines.

fn run_engine_picks_main(engine_flag: &str) {
    let (_dir, path) = write_temp("helper a:n>n;+a 1\nmain>n;helper 41\n");
    let (ok, stdout, stderr) = run(&[engine_flag, path.to_str().unwrap()]);
    assert!(
        ok,
        "{engine_flag}: expected main to auto-run; stderr: {stderr}"
    );
    assert_eq!(
        stdout.trim(),
        "42",
        "{engine_flag}: expected main result 42; stdout: {stdout}"
    );
    assert!(
        !stderr.contains("expected 1 args, got 0"),
        "{engine_flag}: must not leak helper arity error; stderr: {stderr}"
    );
    assert!(
        !stderr.contains("Cranelift JIT: compilation failed"),
        "{engine_flag}: must not leak bare JIT-failed string; stderr: {stderr}"
    );
}

#[test]
fn run_tree_flag_auto_picks_main_on_multi_fn_file() {
    run_engine_picks_main("--run-tree");
}

#[test]
fn run_vm_flag_auto_picks_main_on_multi_fn_file() {
    run_engine_picks_main("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn run_cranelift_flag_auto_picks_main_on_multi_fn_file() {
    run_engine_picks_main("--run-cranelift");
}

#[test]
fn run_default_flag_auto_picks_main_on_multi_fn_file() {
    // Pin the pre-existing #307 behaviour here too, so all four engines
    // are exercised in one place. If the default-branch heuristic ever
    // drifts, this test will catch it alongside the engine-flag set.
    let (_dir, path) = write_temp("helper a:n>n;+a 1\nmain>n;helper 41\n");
    let (ok, stdout, stderr) = run(&[path.to_str().unwrap()]);
    assert!(ok, "default: expected main to auto-run; stderr: {stderr}");
    assert_eq!(stdout.trim(), "42");
}

fn run_engine_explicit_func_overrides_main(engine_flag: &str) {
    let (_dir, path) = write_temp("helper>n;7\nmain>n;42\n");
    let (ok, stdout, stderr) = run(&[engine_flag, path.to_str().unwrap(), "helper"]);
    assert!(
        ok,
        "{engine_flag}: expected explicit helper to run; stderr: {stderr}"
    );
    assert_eq!(stdout.trim(), "7");
}

#[test]
fn run_tree_flag_explicit_func_overrides_main() {
    run_engine_explicit_func_overrides_main("--run-tree");
}

#[test]
fn run_vm_flag_explicit_func_overrides_main() {
    run_engine_explicit_func_overrides_main("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn run_cranelift_flag_explicit_func_overrides_main() {
    run_engine_explicit_func_overrides_main("--run-cranelift");
}

fn run_engine_undefined_func_arg_still_errors(engine_flag: &str) {
    // Companion regression: when the caller passes an explicit
    // positional that doesn't match any function, the engine must
    // still surface `undefined function: <name>` (or an equivalent
    // diagnostic). This pins the pre-existing contract that the
    // narrow auto-pick-main fix did NOT touch — we only auto-pick
    // `main` when `rest` is empty, so positional pass-through (and
    // its typo-detection) is preserved verbatim.
    let (_dir, path) = write_temp("helper a:n>n;+a 1\nmain>n;helper 41\n");
    let (ok, _stdout, stderr) = run(&[engine_flag, path.to_str().unwrap(), "wibble"]);
    assert!(!ok, "{engine_flag}: undefined fn arg must fail");
    assert!(
        stderr.contains("undefined")
            || stderr.contains("no such function")
            || stderr.contains("wibble"),
        "{engine_flag}: expected fn-not-found-style error; stderr: {stderr}"
    );
}

#[test]
fn run_tree_flag_undefined_func_arg_still_errors() {
    run_engine_undefined_func_arg_still_errors("--run-tree");
}

#[test]
fn run_vm_flag_undefined_func_arg_still_errors() {
    run_engine_undefined_func_arg_still_errors("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn run_cranelift_flag_undefined_func_arg_still_errors() {
    run_engine_undefined_func_arg_still_errors("--run-cranelift");
}

fn run_engine_single_fn_no_args_still_runs(engine_flag: &str) {
    // Auto-run contract for single-fn files with no positional args
    // must keep working under every engine flag — the single-fn
    // dispatch path already handles `func_name = None` by running
    // the sole declared fn. The fix preserves that path unchanged.
    // (Single-fn + positional-args on engine flags is a pre-existing
    // limitation: positional args are still parsed as the func name
    // first, so `--run-tree file.ilo 21` errors with `undefined
    // function: 21` on main. Out of scope for this fix.)
    let (_dir, path) = write_temp("entry>n;42\n");
    let (ok, stdout, stderr) = run(&[engine_flag, path.to_str().unwrap()]);
    assert!(ok, "{engine_flag}: expected success; stderr: {stderr}");
    assert_eq!(stdout.trim(), "42");
}

#[test]
fn run_tree_flag_single_fn_no_args_still_runs() {
    run_engine_single_fn_no_args_still_runs("--run-tree");
}

#[test]
fn run_vm_flag_single_fn_no_args_still_runs() {
    run_engine_single_fn_no_args_still_runs("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn run_cranelift_flag_single_fn_no_args_still_runs() {
    run_engine_single_fn_no_args_still_runs("--run-cranelift");
}

// ── hyphenated unknown subcommand: friendly error (PR #320 follow-up) ─────────
//
// Originating bug (interactive-cli rerun6 P1): `ilo file.ilo list-orders`
// on a multi-fn file silently routed to the FIRST declared function with
// `["list-orders"]` as positional args, producing a misleading
// `load: expected 0 args` error. PR #320 added the unknown-subcommand
// error but its `looks_like_subcommand_name` helper rejected anything
// containing `-`, so hyphenated idents fell through to the old greedy
// dispatch.
//
// Per SPEC the canonical ilo ident shape is
// `[a-z][a-z0-9]*(-[a-z0-9]+)*` so `-` is a legal mid-ident character
// and `list-orders` / `wibble-x` / `parse-csv` are all plausible
// subcommand names.

#[test]
fn hyphenated_unknown_subcommand_errors_with_listing() {
    // Pre-fix: routed to `load` (first declared) with `["list-orders"]`
    // as args, producing a misleading type / arity error. Post-fix: the
    // helper accepts `-` as a tail char so `list-orders` is recognised
    // as an ident shape, falls into the unknown-subcommand branch, and
    // emits the friendly "no such function" listing.
    let (_dir, path) = write_temp("load x:n>n;*x 2\nmain>n;load 21\n");
    let (ok, _stdout, stderr) = run(&[path.to_str().unwrap(), "list-orders"]);
    assert!(!ok, "unknown hyphenated subcommand should exit non-zero");
    assert!(
        stderr.contains("no such function 'list-orders'"),
        "expected 'no such function' error for hyphenated name; stderr: {stderr}"
    );
    assert!(
        stderr.contains("available functions"),
        "expected available-functions listing; stderr: {stderr}"
    );
    assert!(
        stderr.contains("load") && stderr.contains("main"),
        "expected both fn names listed; stderr: {stderr}"
    );
    // The dispatch must NOT have invoked the first-declared `load` with
    // the typo as a positional. If it had, we'd see a `load` arity/type
    // error rather than the listing.
    assert!(
        !stderr.contains("'load' arg"),
        "must not leak first-fn dispatch; stderr: {stderr}"
    );
}

#[test]
fn hyphenated_unknown_subcommand_wibble_x() {
    // Tail-hyphenated form (`wibble-x`) from the bug report. Same
    // diagnostic shape as the `list-orders` case.
    let (_dir, path) = write_temp("helper a:n>n;+a 1\nmain>n;helper 41\n");
    let (ok, _stdout, stderr) = run(&[path.to_str().unwrap(), "wibble-x"]);
    assert!(!ok);
    assert!(
        stderr.contains("no such function 'wibble-x'"),
        "expected no-such-function for `wibble-x`; stderr: {stderr}"
    );
}

#[test]
fn trailing_dash_falls_through_as_data() {
    // `-` is legal mid-ident but not at the end. `foo-` is not a valid
    // ident shape, so it must NOT trip the unknown-subcommand error;
    // it routes to `main` if defined, otherwise passes through to the
    // first declared function.
    let (_dir, path) = write_temp("first s:t>t;s\nother s:t>t;s\n");
    let (ok, stdout, stderr) = run(&[path.to_str().unwrap(), "foo-"]);
    assert!(
        ok,
        "trailing-dash positional should pass through, not error; stderr: {stderr}"
    );
    assert_eq!(stdout.trim().trim_matches('"'), "foo-");
}

// ── non-ident leading arg with `main` defined: route to `main` ─────────────────
//
// Originating bug (gis-analyst rerun6): `ilo main_v5.ilo top200.csv` on
// a multi-fn file used to silently route the non-ident-shaped arg
// `top200.csv` to the FIRST declared function (e.g. `hav`) rather than
// to `main`. The presence of `main` is a strong intent signal that the
// user wants `main` as the entry point; positional args after the file
// path should flow into `main`'s arg list, not into whichever function
// happens to be declared first.

#[test]
fn non_ident_arg_routes_to_main_when_main_exists() {
    // First-declared function is `hav` (3 args), `main` is the real
    // entry. Pre-fix: `top200.csv` routed to `hav` and produced an
    // arity error (`hav: expected 3 args, got 1`). Post-fix: `main`
    // receives `top200.csv` as its sole arg and echoes it back.
    let (_dir, path) = write_temp("hav lat:n lon:n r:n>n;+lat lon\nmain path:t>t;path\n");
    let (ok, stdout, stderr) = run(&[path.to_str().unwrap(), "top200.csv"]);
    assert!(ok, "non-ident arg should route to main; stderr: {stderr}");
    assert_eq!(stdout.trim().trim_matches('"'), "top200.csv");
    // The pre-fix arity error from the first declared function must
    // not appear.
    assert!(
        !stderr.contains("hav:"),
        "must not invoke `hav` (first declared); stderr: {stderr}"
    );
}

#[test]
fn non_ident_arg_routes_to_main_with_multiple_positionals() {
    // Same routing rule must extend to multi-positional invocations:
    // every positional flows into `main`'s arg list, in order.
    let (_dir, path) = write_temp("hav lat:n lon:n>n;+lat lon\nmain a:t b:t>L t;[a,b]\n");
    let (ok, stdout, stderr) = run(&[path.to_str().unwrap(), "data.csv", "out.txt"]);
    assert!(ok, "expected success; stderr: {stderr}");
    // Both positionals routed to main, in order.
    assert!(stdout.contains("data.csv"), "stdout: {stdout}");
    assert!(stdout.contains("out.txt"), "stdout: {stdout}");
}

#[test]
fn non_ident_arg_routes_to_main_numeric() {
    // A numeric leading positional is also non-ident-shaped (`42`
    // starts with a digit). When `main` exists it should route to
    // `main`, not to whichever function happens to be declared first.
    let (_dir, path) = write_temp("hav x:n>n;+x 1\nmain x:n>n;*x 2\n");
    let (ok, stdout, stderr) = run(&[path.to_str().unwrap(), "21"]);
    assert!(ok, "expected success; stderr: {stderr}");
    assert_eq!(stdout.trim(), "42");
}

#[test]
fn non_ident_arg_without_main_keeps_legacy_passthrough() {
    // Companion check: when there is NO `main`, the legacy
    // pass-through-to-first-fn behaviour is preserved. This is the
    // contract `multi_fn_file_numeric_leading_arg_passes_through_to_entry_fn`
    // already pins; restated here as an explicit guard against the
    // new bug-2 branch over-firing.
    let (_dir, path) = write_temp("outer x:n>n;+x 1\ninner x:n>n;*x 2\n");
    let (ok, stdout, stderr) = run(&[path.to_str().unwrap(), "41"]);
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "42");
}

#[test]
fn non_ident_path_arg_routes_to_main_devops_sre_shape() {
    // devops-sre rerun6: same root cause as gis-analyst rerun6,
    // independently surfaced via a different persona workload. A
    // multi-fn file with a named-helper field-access (`gs i:_>...`
    // taking a struct/record and reading `i.field`) and a `main` taking
    // a JSON path. Pre-fix: `ilo probe.ilo /tmp/inc.json` routed the
    // path positional (`/` + `.` make it non-ident-shaped) to the
    // first-declared `gs`, hitting a field-access type mismatch on a
    // raw text arg. Post-fix: the path flows into `main` as intended.
    let (_dir, path) = write_temp("gs i:_>t;i.host\nmain p:t>t;p\n");
    let (ok, stdout, stderr) = run(&[path.to_str().unwrap(), "/tmp/inc.json"]);
    assert!(
        ok,
        "non-ident path arg should route to main, not gs; stderr: {stderr}"
    );
    assert_eq!(stdout.trim().trim_matches('"'), "/tmp/inc.json");
    // Pre-fix symptom: invoking `gs` on the text arg would surface a
    // field-access type error referencing `gs` or `i.host`.
    assert!(
        !stderr.contains("'gs'"),
        "must not invoke first-declared `gs`; stderr: {stderr}"
    );
    assert!(
        !stderr.contains("i.host"),
        "must not surface gs's field-access path; stderr: {stderr}"
    );
}

#[test]
fn known_func_name_overrides_main_routing() {
    // Bug-2 fix must NOT shadow explicit function selection: when the
    // leading positional matches a declared function name, that
    // function still wins even if `main` is defined.
    let (_dir, path) = write_temp("hav x:n>n;+x 1\nmain x:n>n;*x 2\n");
    let (ok, stdout, stderr) = run(&[path.to_str().unwrap(), "hav", "41"]);
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "42");
}
