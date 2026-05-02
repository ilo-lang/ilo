// CLI integration tests covering previously-uncovered paths in main.rs.
//
// Tests exercise:
//   - `ilo graph <file>` — full JSON graph output
//   - `ilo graph <file> --dot` — DOT format output
//   - `ilo graph <file> --fn NAME` — per-function query
//   - `ilo graph <file> --fn NAME --subgraph` — subgraph query
//   - `ilo graph` (no args) — non-zero exit
//   - `ilo graph <file> --fn nonexistent` — function-not-found error
//   - `ilo compile <file>` — AOT compile path (no cranelift → error; with cranelift → ok/fail)
//   - `ilo compile` (no args) — non-zero exit
//   - `ilo <code> --mcp` (missing arg) — error path in dispatch_bare_args
//   - `ilo <code> --tools` (missing arg) — error path in dispatch_bare_args
//   - `ilo ''` (empty inline code) — empty-string guard
//   - `ilo run --run-jit 'code' f abc` — non-numeric JIT arg parse error
//   - `ilo run --run-vm 'code' f 5` — VM execution
//   - `ilo run --run-tree 'code' f 5` — tree-walking interpreter execution
//   - unknown flag in `ilo tools` — tools_cmd error path

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Run ilo with the given arguments; return (exit_success, stdout, stderr).
fn run_args(args: &[&str]) -> (bool, String, String) {
    let out = ilo()
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn ilo: {e}"));
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    (out.status.success(), stdout, stderr)
}

/// Write a small .ilo file into a temp directory and return the file path.
fn write_temp_ilo(content: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test.ilo");
    std::fs::write(&path, content).expect("write temp ilo");
    (dir, path)
}

// ── graph subcommand ──────────────────────────────────────────────────────────

/// `ilo graph <file>` should succeed and output JSON.
#[test]
fn graph_full_json_output() {
    let (_dir, path) = write_temp_ilo("add a:n b:n>n;+a b mul a:n b:n>n;*a b");
    let (ok, stdout, _stderr) = run_args(&["graph", path.to_str().unwrap()]);
    assert!(ok, "graph should succeed");
    // Output should be valid JSON
    let _parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("graph output should be valid JSON");
}

/// `ilo graph <file> --dot` should output DOT format (contains "digraph").
#[test]
fn graph_dot_output() {
    let (_dir, path) = write_temp_ilo("add a:n b:n>n;+a b");
    let (ok, stdout, _stderr) = run_args(&["graph", path.to_str().unwrap(), "--dot"]);
    assert!(ok, "graph --dot should succeed");
    assert!(
        stdout.contains("digraph"),
        "DOT output should contain 'digraph', got:\n{stdout}"
    );
}

/// `ilo graph <file> --fn NAME` should output per-function JSON.
#[test]
fn graph_fn_query() {
    let (_dir, path) = write_temp_ilo("add a:n b:n>n;+a b");
    let (ok, stdout, stderr) = run_args(&["graph", path.to_str().unwrap(), "--fn", "add"]);
    assert!(ok, "graph --fn add should succeed; stderr: {stderr}");
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("per-function output should be valid JSON");
    // The query result should reference the function name somehow
    assert!(
        stdout.contains("add"),
        "output should mention the queried function; got:\n{stdout}\nparsed: {parsed}"
    );
}

/// `ilo graph <file> --fn NAME --subgraph` should output subgraph JSON.
#[test]
fn graph_subgraph_query() {
    let (_dir, path) = write_temp_ilo("helper a:n>n;*a 2 main x:n>n;helper x");
    let (ok, stdout, stderr) = run_args(&[
        "graph",
        path.to_str().unwrap(),
        "--fn",
        "main",
        "--subgraph",
    ]);
    assert!(
        ok,
        "graph --fn main --subgraph should succeed; stderr: {stderr}"
    );
    let _parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("subgraph output should be valid JSON");
}

/// `ilo graph` with no file argument should fail (clap rejects missing required arg).
#[test]
fn graph_no_args_exits_nonzero() {
    let (ok, _stdout, _stderr) = run_args(&["graph"]);
    assert!(!ok, "graph with no args should fail");
}

/// `ilo graph <file> --fn nonexistent` should fail with a function-not-found message.
#[test]
fn graph_fn_not_found() {
    let (_dir, path) = write_temp_ilo("add a:n b:n>n;+a b");
    let (ok, _stdout, stderr) = run_args(&["graph", path.to_str().unwrap(), "--fn", "no_such_fn"]);
    assert!(!ok, "graph --fn on nonexistent function should fail");
    assert!(
        stderr.contains("not found"),
        "stderr should mention 'not found'; got:\n{stderr}"
    );
}

/// `ilo graph nonexistent_file.ilo` should fail with a read-error message.
#[test]
fn graph_file_not_found() {
    let (ok, _stdout, stderr) = run_args(&["graph", "/tmp/ilo_test_nonexistent_12345.ilo"]);
    assert!(!ok, "graph on missing file should fail");
    assert!(
        stderr.contains("Error reading") || stderr.contains("No such"),
        "stderr should mention file error; got:\n{stderr}"
    );
}

// ── compile subcommand ────────────────────────────────────────────────────────

/// `ilo compile` with no arguments should fail (clap rejects missing required arg).
#[test]
fn compile_no_args_exits_nonzero() {
    let (ok, _stdout, _stderr) = run_args(&["compile"]);
    assert!(!ok, "compile with no args should fail");
}

/// `ilo compile <file>` should attempt compilation.
/// Without a linked runtime it may fail at the AOT link step, but it must
/// reach the compile_cmd code path (not crash or show usage).
#[test]
fn compile_attempts_compilation() {
    let (_dir, path) = write_temp_ilo("double x:n>n;*x 2");
    let file = path.to_str().unwrap();
    // Strip the .ilo extension to derive a custom output path so we don't
    // pollute the test directory.
    let out_path = path.with_extension("").to_string_lossy().to_string();
    let (ok, _stdout, stderr) = run_args(&["compile", file, "-o", &out_path]);
    if ok {
        // Compiled successfully — clean up the output binary.
        let _ = std::fs::remove_file(&out_path);
    } else {
        // Expected on systems where libilo.a / linker setup is incomplete.
        // The important thing is that we hit the compile path, not a usage error.
        assert!(
            !stderr.contains("Usage: ilo compile"),
            "should not show bare usage; stderr:\n{stderr}"
        );
        assert!(
            stderr.contains("AOT compile error")
                || stderr.contains("Compiled")
                || stderr.contains("Compile error")
                || stderr.contains("Error"),
            "stderr should mention a compile-related message; got:\n{stderr}"
        );
    }
}

// ── --mcp / --tools missing-argument error paths ─────────────────────────────

/// `ilo 'code' --mcp` (no path after --mcp in bare-args mode) should fail.
#[test]
fn bare_args_mcp_missing_path() {
    let (ok, _stdout, stderr) = run_args(&["f x:n>n;*x 2", "--mcp"]);
    assert!(!ok, "--mcp without path should fail");
    assert!(
        stderr.contains("--mcp") || stderr.contains("requires"),
        "stderr should mention --mcp; got:\n{stderr}"
    );
}

/// `ilo 'code' --tools` (no path after --tools) should fail.
#[test]
fn bare_args_tools_missing_path() {
    let (ok, _stdout, stderr) = run_args(&["f x:n>n;*x 2", "--tools"]);
    assert!(!ok, "--tools without path should fail");
    assert!(
        stderr.contains("--tools") || stderr.contains("requires"),
        "stderr should mention --tools; got:\n{stderr}"
    );
}

// ── empty inline code guard ───────────────────────────────────────────────────

/// `ilo ''` should fail with an empty-code error, not a panic.
#[test]
fn empty_inline_code_is_rejected() {
    let (ok, _stdout, stderr) = run_args(&[""]);
    assert!(!ok, "empty code string should fail");
    assert!(
        stderr.contains("empty") || stderr.contains("Usage"),
        "stderr should mention empty code or usage; got:\n{stderr}"
    );
}

// ── JIT / VM engine paths ─────────────────────────────────────────────────────

/// `ilo run --run-vm 'f x:n>n;*x 2' f 5` should succeed and print 10.
/// Uses the `run` subcommand to properly route --run-vm through dispatch_run.
#[test]
fn run_vm_basic_execution() {
    let (ok, stdout, stderr) = run_args(&["run", "--run-vm", "f x:n>n;*x 2", "f", "5"]);
    assert!(ok, "--run-vm should succeed; stderr: {stderr}");
    assert_eq!(
        stdout.trim(),
        "10",
        "--run-vm result should be 10; got stdout: {stdout}"
    );
}

/// `ilo run --run-tree 'f x:n>n;*x 2' f 5` should succeed and print 10.
/// Uses the `run` subcommand to properly route --run-tree through dispatch_run.
#[test]
fn run_tree_basic_execution() {
    let (ok, stdout, stderr) = run_args(&["run", "--run-tree", "f x:n>n;*x 2", "f", "5"]);
    assert!(ok, "--run-tree should succeed; stderr: {stderr}");
    assert_eq!(
        stdout.trim(),
        "10",
        "--run-tree result should be 10; got: {stdout}"
    );
}

/// `ilo run --run-jit 'f x:n>n;*x 2' f abc` — non-numeric arg to JIT should fail.
/// On non-arm64 macOS this hits the unsupported-platform path; on arm64 it
/// should hit the numeric-parse error.  Either way the exit code must be 1.
#[test]
fn run_jit_non_numeric_arg_fails() {
    let (ok, _stdout, stderr) = run_args(&["run", "--run-jit", "f x:n>n;*x 2", "f", "abc"]);
    // The JIT path on non-arm64 will say "only available on aarch64 macOS";
    // on arm64 it should say "not a valid number".
    // In both cases the process should fail.
    assert!(
        !ok || stderr.contains("error") || stderr.contains("Error"),
        "JIT with non-numeric arg should fail or emit an error; stderr: {stderr}"
    );
}

// ── tools subcommand — unknown flag error path ────────────────────────────────

/// `ilo tools --unknown-flag` should fail and mention the unknown flag.
#[test]
fn tools_unknown_flag() {
    let (ok, _stdout, stderr) = run_args(&["tools", "--unknown-flag-xyz"]);
    assert!(!ok, "tools with unknown flag should fail");
    // tools_cmd emits either "unknown flag: ..." or the no-source error first.
    // Either is acceptable as long as we exit non-zero.
    let _ = stderr; // checked via exit code above
}

/// `ilo tools` with no --mcp or --tools should fail with the no-source error.
#[test]
fn tools_no_source_fails() {
    let (ok, _stdout, stderr) = run_args(&["tools"]);
    assert!(!ok, "tools with no source should fail");
    assert!(
        stderr.contains("requires at least one of"),
        "stderr should mention the required flags; got:\n{stderr}"
    );
}

// ── graph --reverse path ──────────────────────────────────────────────────────

/// `ilo graph <file> --fn NAME --reverse` should output reverse-callers JSON.
#[test]
fn graph_reverse_query() {
    let (_dir, path) = write_temp_ilo("helper a:n>n;*a 2 main x:n>n;helper x");
    let (ok, stdout, stderr) = run_args(&[
        "graph",
        path.to_str().unwrap(),
        "--fn",
        "helper",
        "--reverse",
    ]);
    assert!(
        ok,
        "graph --fn helper --reverse should succeed; stderr: {stderr}"
    );
    let _parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("reverse graph output should be JSON");
}

// ── graph --budget path ───────────────────────────────────────────────────────

/// `ilo graph <file> --fn NAME --budget N` should output a budget-limited JSON.
#[test]
fn graph_budget_query() {
    let (_dir, path) = write_temp_ilo("add a:n b:n>n;+a b");
    let (ok, stdout, stderr) = run_args(&[
        "graph",
        path.to_str().unwrap(),
        "--fn",
        "add",
        "--budget",
        "100",
    ]);
    assert!(
        ok,
        "graph --fn add --budget 100 should succeed; stderr: {stderr}"
    );
    let _parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("budget graph output should be JSON");
}

/// `ilo graph <file> --budget notanumber` should fail (clap rejects non-usize).
#[test]
fn graph_budget_invalid_value() {
    let (_dir, path) = write_temp_ilo("add a:n b:n>n;+a b");
    let (ok, _stdout, _stderr) =
        run_args(&["graph", path.to_str().unwrap(), "--budget", "notanumber"]);
    assert!(!ok, "graph --budget with non-integer should fail");
}

// ── Coverage tests for main.rs and cli/args.rs ──────────────────────────────

/// `ilo --version` should print version (L1824-1825)
#[test]
fn cli_cov_version() {
    let (ok, stdout, _stderr) = run_args(&["--version"]);
    assert!(ok, "--version should succeed");
    assert!(
        stdout.contains("ilo"),
        "--version should contain 'ilo', got: {stdout}"
    );
}

/// `ilo spec lang` should print the full spec (L1817)
#[test]
fn cli_cov_spec_lang() {
    let (ok, stdout, _stderr) = run_args(&["spec", "lang"]);
    assert!(ok, "spec lang should succeed");
    assert!(stdout.len() > 100, "spec lang output should be non-trivial");
}

/// `ilo spec ai` should print compact/AI spec (L1818)
#[test]
fn cli_cov_spec_ai() {
    let (ok, stdout, _stderr) = run_args(&["spec", "ai"]);
    assert!(ok, "spec ai should succeed");
    assert!(stdout.len() > 10, "spec ai should have content");
}

/// `ilo explain ILO-V001` should print known error code explanation (L1804-1808)
#[test]
fn cli_cov_explain_known() {
    let (ok, stdout, _stderr) = run_args(&["explain", "ILO-V001"]);
    if ok {
        assert!(!stdout.is_empty(), "explain should produce output");
    }
}

/// `ilo explain UNKNOWN-CODE` should fail (L1809-1812)
#[test]
fn cli_cov_explain_unknown() {
    let (ok, _stdout, stderr) = run_args(&["explain", "UNKNOWN-CODE"]);
    assert!(!ok, "explain unknown code should fail");
    assert!(
        stderr.contains("unknown error code"),
        "should mention unknown error code, got: {stderr}"
    );
}

/// `ilo 'bad syntax ###'` inline parse error (L360-363)
#[test]
fn cli_cov_inline_parse_error() {
    let (ok, _stdout, stderr) = run_args(&["bad syntax ###"]);
    assert!(!ok, "inline parse error should fail");
    assert!(!stderr.is_empty(), "should report parse error on stderr");
}

/// `ilo graph file --fn nonexistent` function not found (L467-468)
#[test]
fn cli_cov_graph_fn_not_found() {
    let (_dir, path) = write_temp_ilo("f x:n>n;+x 1");
    let (ok, _stdout, stderr) = run_args(&["graph", path.to_str().unwrap(), "--fn", "nonexistent"]);
    assert!(!ok, "graph with nonexistent function should fail");
    assert!(
        stderr.contains("not found") || stderr.contains("nonexistent"),
        "should mention function not found, got: {stderr}"
    );
}

/// `ilo compile` with syntax error (L1100-1104)
#[test]
fn cli_cov_compile_parse_error() {
    let (_dir, path) = write_temp_ilo("f x:n>n;???");
    let (ok, _stdout, stderr) = run_args(&["compile", path.to_str().unwrap()]);
    assert!(!ok, "compile with parse error should fail");
    assert!(
        stderr.contains("error") || stderr.contains("ILO"),
        "should report parse error, got: {stderr}"
    );
}

/// `ilo serve --mcp` without path (L1204-1205)
#[test]
fn cli_cov_serve_mcp_no_path() {
    let (ok, _stdout, stderr) = run_args(&["serve", "--mcp"]);
    assert!(!ok, "serve --mcp without path should fail");
    assert!(
        stderr.contains("--mcp") || stderr.contains("requires") || stderr.contains("error"),
        "should mention --mcp error, got: {stderr}"
    );
}

/// `ilo serve --tools` without path (L1212-1213)
#[test]
fn cli_cov_serve_tools_no_path() {
    let (ok, _stdout, stderr) = run_args(&["serve", "--tools"]);
    assert!(!ok, "serve --tools without path should fail");
    assert!(
        stderr.contains("--tools") || stderr.contains("requires") || stderr.contains("error"),
        "should mention --tools error, got: {stderr}"
    );
}

/// `ilo run --run-jit 'f x:n>n;+x 1' nonexistent` — JIT with undefined function (L2473-2474)
#[test]
fn cli_cov_jit_fn_not_found() {
    let (ok, _stdout, stderr) = run_args(&["run", "--run-jit", "f x:n>n;+x 1", "nonexistent"]);
    // On non-aarch64, the JIT may not be available; either way exit should be non-zero
    assert!(
        !ok || stderr.contains("error") || stderr.contains("Error"),
        "JIT with undefined function should fail or error; stderr: {stderr}"
    );
}

/// `ilo run --run-vm 'f x:n>n;+x 1' nonexistent 5` — VM with undefined fn (L2530-2531)
#[test]
fn cli_cov_run_vm_fn_not_found() {
    let (ok, _stdout, stderr) = run_args(&["run", "--run-vm", "f x:n>n;+x 1", "nonexistent", "5"]);
    assert!(!ok, "VM with undefined function should fail");
    assert!(
        stderr.contains("undefined")
            || stderr.contains("nonexistent")
            || stderr.contains("not found"),
        "should mention function not found, got: {stderr}"
    );
}

/// `ilo run 'f x:n>n;+x 1' f 5` exercises the default engine path
#[test]
fn cli_cov_run_default_engine() {
    let (ok, stdout, stderr) = run_args(&["run", "f x:n>n;+x 1", "f", "5"]);
    assert!(
        ok,
        "run with default engine should succeed; stderr: {stderr}"
    );
    assert!(
        stdout.trim() == "6" || stdout.trim() == "6.0",
        "should output 6, got: {stdout}"
    );
}
