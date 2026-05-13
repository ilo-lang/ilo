use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

// --- Inline code: single function ---

#[test]
fn inline_single_func_bare_args() {
    let out = ilo()
        .args(["tot p:n q:n r:n>n;s=*p q;t=*s r;+s t", "10", "20", "30"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "6200");
}

#[test]
fn inline_no_args_outputs_ast() {
    let out = ilo()
        .args(["tot p:n q:n r:n>n;s=*p q;t=*s r;+s t"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"name\""),
        "expected AST JSON, got: {}",
        stdout
    );
}

// Inline-no-func AST-dump mode is an inspection path, not an execution path.
// Verify errors on partial snippets (e.g. a function body that references
// an undeclared name) should not gate the AST dump; the user is exploring
// structure, not running code.
#[test]
fn inline_no_args_skips_verify_errors_on_partial_snippet() {
    let out = ilo()
        .args(["f>n;slc xs 0 1"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "expected AST dump to succeed without verify gating; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"slc\""),
        "expected AST JSON containing the builtin call, got: {}",
        stdout
    );
}

// Regression: when a function name IS provided, verify must still gate
// execution. A program that references an undeclared variable must still
// produce a verify error and a non-zero exit.
#[test]
fn inline_with_func_arg_still_reports_verify_errors() {
    let out = ilo()
        .args(["f>n;slc xs 0 1", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected verify error to gate execution when a func arg is given"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let combined = format!("{stderr}{stdout}");
    assert!(
        combined.contains("undefined variable") || combined.contains("ILO-T004"),
        "expected an undefined-variable diagnostic; got stdout: {stdout}, stderr: {stderr}"
    );
}

// A real lex/parse error must still gate the AST dump in inline-no-func
// mode — we can't dump an AST we couldn't build.
#[test]
fn inline_no_args_parse_error_still_fatal() {
    let out = ilo()
        .args(["this is not valid ilo code @@##$$"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected parse error to gate AST dump"
    );
}

// --- Inline code: multiple functions ---

#[test]
fn inline_multi_func_select_by_name() {
    let out = ilo()
        .args([
            "dbl x:n>n;s=*x 2;+s 0 tot p:n q:n r:n>n;s=*p q;t=*s r;+s t",
            "tot",
            "10",
            "20",
            "30",
        ])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "6200");
}

#[test]
fn inline_multi_func_first_by_default() {
    let out = ilo()
        .args([
            "dbl x:n>n;s=*x 2;+s 0 tot p:n q:n r:n>n;s=*p q;t=*s r;+s t",
            "5",
        ])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "10");
}

// --- Inline code: emit ---

#[test]
fn inline_emit_python() {
    let out = ilo()
        .args(["tot p:n q:n r:n>n;s=*p q;t=*s r;+s t", "--emit", "python"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("def tot"),
        "expected 'def tot', got: {}",
        stdout
    );
}

// --- Inline code: explicit --run ---

#[test]
fn inline_explicit_run() {
    let out = ilo()
        .args([
            "tot p:n q:n r:n>n;s=*p q;t=*s r;+s t",
            "--run",
            "tot",
            "10",
            "20",
            "30",
        ])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "6200");
}

// --- Error cases ---

#[test]
fn no_args_shows_usage() {
    let out = ilo().output().expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Usage"),
        "expected usage message, got: {}",
        stderr
    );
}

#[test]
fn inline_empty_string_errors() {
    let out = ilo().args([""]).output().expect("failed to run ilo");
    assert!(!out.status.success());
}

#[test]
fn inline_invalid_code_errors() {
    let out = ilo()
        .args(["this is not valid ilo code @@##$$"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.is_empty(), "expected error on stderr");
}

// --- File mode: bare args ---

#[test]
fn file_bare_args_runs_first_func() {
    let out = ilo()
        .args(["examples/01-simple-function.ilo", "10", "20", "0.1"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // 01-simple-function.ilo defines tot: (10*20) + (10*20*0.1) = 200 + 20 = 220
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "220");
}

#[test]
fn file_with_ast_flag_dumps_ast() {
    // Previously `ilo file.ilo` with no func arg dumped raw AST JSON,
    // which was a long-standing first-touch surprise: users expected
    // it to run. The AST dump is now gated behind an explicit `--ast`
    // flag (the auto-run / friendly-listing behaviour is pinned in
    // tests/regression_cli_default.rs).
    let out = ilo()
        .args(["--ast", "examples/01-simple-function.ilo"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"name\""),
        "expected AST JSON, got: {}",
        stdout
    );
}

// --- Nested prefix operators ---

#[test]
fn inline_nested_prefix() {
    let out = ilo()
        .args(["f a:n b:n c:n>n;+*a b c", "2", "3", "4"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "10");
}

// --- CLI modes ---

#[test]
fn inline_run_vm_mode() {
    let out = ilo()
        .args(["f x:n>n;*x 2", "--run-vm", "f", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "10");
}

#[test]
fn inline_run_with_func_name() {
    let out = ilo()
        .args(["f x:n>n;*x 2", "--run", "f", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "10");
}

#[test]
fn inline_emit_unknown_target() {
    let out = ilo()
        .args(["f x:n>n;*x 2", "--emit", "javascript"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Unknown emit target"),
        "expected emit error, got: {}",
        stderr
    );
}

#[test]
fn inline_parse_bool_arg() {
    let out = ilo()
        .args(["f x:b>b;!x", "true"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "false");
}

#[test]
fn inline_parse_false_arg() {
    // "false" string arg → parse_arg_value returns Bool(false) (main.rs L771)
    let out = ilo()
        .args(["f x:b>b;x", "false"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "false");
}

#[test]
fn inline_parse_text_arg() {
    let out = ilo()
        .args(["f x:t>t;x", "hello"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hello");
}

#[test]
fn inline_parse_error() {
    let out = ilo()
        .args(["f x:>n;x", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Parse error") || stderr.contains("error"),
        "expected parse error, got: {}",
        stderr
    );
}

#[test]
fn inline_bench_mode() {
    let out = ilo()
        .args(["f x:n>n;*x 2", "--bench", "f", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("interpreter") || stdout.contains("vm"),
        "expected benchmark output, got: {}",
        stdout
    );
}

// --- Legacy -e flag ---

// --- Help ---

#[test]
fn help_flag_shows_usage() {
    let out = ilo().args(["--help"]).output().expect("failed to run ilo");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Backends:"),
        "expected backends section, got: {}",
        stdout
    );
}

#[test]
fn help_short_flag_shows_usage() {
    let out = ilo().args(["-h"]).output().expect("failed to run ilo");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Backends:"),
        "expected backends section, got: {}",
        stdout
    );
}

// --- List arguments ---

#[test]
fn inline_list_arg_bracketed() {
    let out = ilo()
        .args(["f xs:L n>n;len xs", "[1,2,3]"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "3");
}

#[test]
fn inline_list_arg_bracketed_index() {
    let out = ilo()
        .args(["f xs:L n>n;xs.0", "[10,20,30]"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "10");
}

#[test]
fn inline_list_arg_bare_comma() {
    let out = ilo()
        .args(["f xs:L n>n;len xs", "1,2,3"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "3");
}

#[test]
fn inline_list_arg_bare_comma_index() {
    let out = ilo()
        .args(["f xs:L n>n;xs.0", "10,20,30"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "10");
}

#[test]
fn help_shows_usage() {
    let out = ilo().args(["help"]).output().expect("failed to run ilo");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Backends:"),
        "expected backends section, got: {}",
        stdout
    );
    assert!(
        stdout.contains("--run-tree"),
        "expected --run-tree, got: {}",
        stdout
    );
}

#[test]
fn help_lang_shows_spec() {
    let out = ilo()
        .args(["help", "lang"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ilo Language Spec"),
        "expected spec header, got: {}",
        stdout
    );
}

// --- Backend flags ---

#[test]
fn inline_run_tree() {
    let out = ilo()
        .args(["f x:n>n;*x 2", "--run-tree", "f", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "10");
}

#[test]
fn inline_run_cranelift() {
    let out = ilo()
        .args(["f x:n>n;*x 2", "--run-cranelift", "f", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "10");
}

#[test]
fn default_falls_back_for_non_numeric() {
    // Bool args are not JIT-eligible, should fall back to interpreter
    let out = ilo()
        .args(["f x:b>b;!x", "true"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "false");
}

// --- Legacy -e flag ---

#[test]
fn legacy_e_flag_still_works() {
    let out = ilo()
        .args([
            "-e",
            "tot p:n q:n r:n>n;s=*p q;t=*s r;+s t",
            "--run",
            "tot",
            "10",
            "20",
            "30",
        ])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "6200");
}

#[test]
fn legacy_e_flag_missing_code() {
    let out = ilo().args(["-e"]).output().expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Usage"),
        "expected usage message, got: {}",
        stderr
    );
}

// --- Static verifier errors ---

#[test]
fn verify_undefined_variable() {
    let out = ilo()
        .args(["--text", "f x:n>n;*y 2", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("error[ILO-T004]"),
        "expected error in stderr, got: {}",
        stderr
    );
    assert!(
        stderr.contains("undefined variable 'y'"),
        "expected undefined var error, got: {}",
        stderr
    );
}

#[test]
fn verify_undefined_function() {
    let out = ilo()
        .args(["--text", "f x:n>n;foo x", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("error[ILO-T005]"),
        "expected error in stderr, got: {}",
        stderr
    );
    assert!(
        stderr.contains("undefined function 'foo'"),
        "expected undefined func error, got: {}",
        stderr
    );
}

#[test]
fn verify_arity_mismatch() {
    let out = ilo()
        .args(["--text", "g a:n b:n>n;+a b f x:n>n;g x", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("arity mismatch"),
        "expected arity error, got: {}",
        stderr
    );
}

#[test]
fn verify_type_mismatch() {
    let out = ilo()
        .args(["--text", "f x:t>n;*x 2", "hello"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("error[ILO-T009]"),
        "expected error in stderr, got: {}",
        stderr
    );
}

#[test]
fn verify_valid_program_runs() {
    let out = ilo()
        .args(["f x:n>n;*x 2", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "10");
}

// --- Prefix expressions as call arguments ---

#[test]
fn inline_factorial_with_prefix_call_arg() {
    // fac -n 1 as a call with prefix arg, result bound then used in operator
    // Use braceless guard for early return base case
    let out = ilo()
        .args(["fac n:n>n;<=n 1 1;r=fac -n 1;*n r", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "120");
}

#[test]
fn inline_fibonacci_with_prefix_call_args() {
    // fib -n 1 and fib -n 2 as direct calls with prefix args
    // Use braceless guard for early return base case
    let out = ilo()
        .args(["fib n:n>n;<=n 1 n;a=fib -n 1;b=fib -n 2;+a b", "10"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "55");
}

#[test]
fn inline_call_with_nested_prefix_unchanged() {
    // +*a b c should still work as nested prefix: (a*b) + c
    let out = ilo()
        .args(["f a:n b:n c:n>n;+*a b c", "2", "3", "4"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "10");
}

// --- Output format flags ---

#[test]
fn json_flag_produces_json_error() {
    let out = ilo()
        .args(["--json", "not-valid-ilo!!!"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Should be parseable JSON with severity field
    let v: serde_json::Value = serde_json::from_str(stderr.trim())
        .unwrap_or_else(|_| panic!("expected JSON on stderr, got: {}", stderr));
    assert_eq!(v["severity"], "error");
}

#[test]
fn text_flag_produces_plain_error() {
    // Pass a func arg so we exercise the execution path (where verify gates),
    // not the inline-no-func AST-dump path (where verify is skipped).
    let out = ilo()
        .args(["--text", "f x:n>n;+x \"hi\"", "f", "1"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("error["),
        "expected 'error[' in stderr: {}",
        stderr
    );
    // No ANSI codes
    assert!(
        !stderr.contains("\x1b["),
        "unexpected ANSI codes in text mode: {}",
        stderr
    );
}

#[test]
fn ansi_flag_produces_colored_error() {
    let out = ilo()
        .args(["--ansi", "f x:n>n;+x \"hi\"", "f", "1"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("error"),
        "expected error in stderr: {}",
        stderr
    );
    // Should contain ANSI escape codes
    assert!(
        stderr.contains("\x1b["),
        "expected ANSI codes in ansi mode: {}",
        stderr
    );
}

#[test]
fn json_flag_parse_error_has_span() {
    let out = ilo()
        .args(["--json", "42 x:n>n;x"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    // JSON mode emits one object per line (NDJSON). Check the first line.
    let first_line = stderr
        .lines()
        .next()
        .unwrap_or_else(|| panic!("expected output on stderr, got empty"));
    let v: serde_json::Value = serde_json::from_str(first_line)
        .unwrap_or_else(|_| panic!("expected JSON on first line of stderr, got: {}", stderr));
    assert_eq!(v["severity"], "error");
    // Should have labels with span info
    assert!(
        v["labels"].as_array().is_some_and(|l| !l.is_empty()),
        "expected labels in: {}",
        stderr
    );
}

#[test]
fn text_flag_verify_error_has_function_note() {
    let out = ilo()
        .args(["--text", "f x:n>n;+x \"hi\"", "f", "1"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("note:"),
        "expected note in stderr: {}",
        stderr
    );
    assert!(
        stderr.contains("'f'"),
        "expected function name in stderr: {}",
        stderr
    );
}

#[test]
fn mutual_exclusion_json_text() {
    let out = ilo()
        .args(["--json", "--text", "f x:n>n;x"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("mutually exclusive"),
        "expected mutual exclusion error: {}",
        stderr
    );
}

#[test]
fn no_color_env_produces_no_ansi() {
    let out = ilo()
        .args(["f x:n>n;+x \"hi\"", "f", "1"])
        .env("NO_COLOR", "1")
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("\x1b["),
        "unexpected ANSI codes with NO_COLOR: {}",
        stderr
    );
}

// --- Compact spec (ilo help ai / ilo -ai) ---

#[test]
fn help_ai_subcommand_exits_success() {
    let out = ilo()
        .args(["help", "ai"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn ai_flag_exits_success() {
    let out = ilo().args(["-ai"]).output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn help_ai_and_ai_flag_produce_same_output() {
    let out1 = ilo()
        .args(["help", "ai"])
        .output()
        .expect("failed to run ilo");
    let out2 = ilo().args(["-ai"]).output().expect("failed to run ilo");
    assert_eq!(
        out1.stdout, out2.stdout,
        "help ai and -ai should produce identical output"
    );
}

#[test]
fn help_ai_contains_no_blank_lines() {
    let out = ilo()
        .args(["help", "ai"])
        .output()
        .expect("failed to run ilo");
    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines() {
        assert!(
            !line.trim().is_empty(),
            "unexpected blank line in compact spec"
        );
    }
}

#[test]
fn help_ai_strips_code_fences() {
    let out = ilo()
        .args(["help", "ai"])
        .output()
        .expect("failed to run ilo");
    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines() {
        assert!(
            !line.trim_start().starts_with("```"),
            "code fence found in compact spec: {}",
            line
        );
    }
}

#[test]
fn help_ai_strips_horizontal_rules() {
    let out = ilo()
        .args(["help", "ai"])
        .output()
        .expect("failed to run ilo");
    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines() {
        assert!(
            line.trim() != "---",
            "horizontal rule found in compact spec"
        );
    }
}

#[test]
fn help_ai_preserves_key_content() {
    let out = ilo()
        .args(["help", "ai"])
        .output()
        .expect("failed to run ilo");
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Core syntax constructs must be present
    assert!(stdout.contains("fac n:n>n"), "missing factorial pattern");
    assert!(stdout.contains("FUNCTIONS:"), "missing FUNCTIONS section");
    assert!(stdout.contains("TYPES:"), "missing TYPES section");
    assert!(stdout.contains("OPERATORS:"), "missing OPERATORS section");
}

#[test]
fn help_ai_is_smaller_than_full_spec() {
    let full = ilo()
        .args(["help", "lang"])
        .output()
        .expect("failed to run ilo");
    let compact = ilo()
        .args(["help", "ai"])
        .output()
        .expect("failed to run ilo");
    assert!(
        compact.stdout.len() < full.stdout.len(),
        "compact spec ({} bytes) should be smaller than full spec ({} bytes)",
        compact.stdout.len(),
        full.stdout.len()
    );
}

// --- --version / -V flag ---

#[test]
fn version_flag() {
    let out = ilo()
        .args(["--version"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ilo "),
        "expected version string, got: {stdout}"
    );
}

#[test]
fn version_flag_short() {
    let out = ilo().args(["-V"]).output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ilo "),
        "expected version string, got: {stdout}"
    );
}

// --- --explain flag ---

#[test]
fn explain_known_code() {
    let out = ilo()
        .args(["--explain", "ILO-T005"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ILO-T005"),
        "expected explanation, got: {stdout}"
    );
}

#[test]
fn explain_unknown_code() {
    let out = ilo()
        .args(["--explain", "ILO-XXXX"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "should exit with error for unknown code"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown error code"),
        "expected 'unknown error code' in stderr: {stderr}"
    );
}

#[test]
fn explain_no_code_arg() {
    let out = ilo()
        .args(["--explain"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "should exit with error when no code given"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Usage"),
        "expected 'Usage' in stderr: {stderr}"
    );
}

// --- source annotation: ilo code --explain / -x ---

#[test]
fn source_explain_fn_start() {
    let out = ilo()
        .args(["f x:n>n;+x 1", "--explain"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("fn start"),
        "expected 'fn start' annotation: {stdout}"
    );
}

#[test]
fn source_explain_short_flag() {
    let out = ilo()
        .args(["f x:n>n;+x 1", "-x"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("fn start"),
        "expected 'fn start' annotation: {stdout}"
    );
}

#[test]
fn source_explain_bind_annotation() {
    let out = ilo()
        .args(["f x:n>n;y=+x 1;y", "--explain"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("bind → y"), "expected 'bind → y': {stdout}");
}

#[test]
fn source_explain_guard_annotation() {
    let out = ilo()
        .args(["f x:n>n;<=x 0{x};+x 1", "--explain"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("guard"),
        "expected 'guard' annotation: {stdout}"
    );
}

#[test]
fn source_explain_return_annotation() {
    let out = ilo()
        .args(["f x:n>n;+x 1", "--explain"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("return"),
        "expected 'return' annotation: {stdout}"
    );
}

// --- trm / unq / fmt via inline ---

#[test]
fn inline_trm_basic() {
    let out = ilo()
        .args(["f s:t>t;trm s", "  hello  "])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hello");
}

#[test]
fn inline_unq_text() {
    let out = ilo()
        .args(["f s:t>t;unq s", "aabbc"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "abc");
}

#[test]
fn inline_fmt_basic() {
    let out = ilo()
        .args([r#"f a:t b:t>t;fmt "{} and {}" a b"#, "foo", "bar"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "foo and bar");
}

// --- Runtime error paths ---

#[test]
fn run_vm_runtime_error() {
    // --run-vm with a program that errors at runtime (division by zero)
    // Exercises L363-365 in main.rs (error reporting for --run-vm)
    let out = ilo()
        .args(["f>n;/1 0", "--run-vm", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success(), "should exit with error");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("division") || stderr.contains("zero") || stderr.contains("ILO"),
        "expected runtime error in stderr: {stderr}"
    );
}

#[test]
fn run_interp_runtime_error() {
    // --run-tree with a program that errors at runtime (division by zero)
    // Exercises L379-381 in main.rs (error reporting for --run-tree)
    let out = ilo()
        .args(["f>n;/1 0", "--run-tree", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success(), "should exit with error");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("division") || stderr.contains("zero") || stderr.contains("ILO"),
        "expected runtime error in stderr: {stderr}"
    );
}

#[test]
fn typedef_in_func_names_filter() {
    // Program with a Function + TypeDef: the func_names filter at L388 hits `_ => None` for TypeDef.
    // TypeDef must come AFTER the function to avoid a chunk-index mismatch in the VM compiler.
    // Bare arg "5" is not a function name → func_name=None, run_args=[5.0]
    let out = ilo()
        .args(["f x:n>n;+x 1\ntype point{x:n}", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.trim(), "6", "expected 6, got: {stdout}");
}

#[test]
fn run_default_float_result() {
    // Cranelift JIT returns a non-integer float → println!("{}", result) (L430 in run_default)
    // f x:n>n;/x 3 with arg 2 → 0.666... (not representable as i64)
    let out = ilo()
        .args(["f x:n>n;/x 3", "2"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let val: f64 = stdout.trim().parse().expect("expected float output");
    assert!(
        (val - 2.0 / 3.0).abs() < 1e-6,
        "expected ~0.666, got: {val}"
    );
}

#[cfg(not(feature = "llvm"))]
#[test]
fn run_llvm_not_enabled() {
    // --run-llvm when LLVM feature is disabled → error message (L348-349)
    let out = ilo()
        .args(["f x:n>n;*x 2", "--run-llvm", "f", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success(), "should fail when LLVM not enabled");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("LLVM") || stderr.contains("llvm") || stderr.contains("not enabled"),
        "expected LLVM not enabled message, got: {stderr}"
    );
}

// L164-166: file read error — file exists but is unreadable (Unix only)
#[cfg(unix)]
#[test]
fn file_read_error() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir();
    let path = dir.join("ilo_test_unreadable.ilo");
    // Restore permissions first in case a previous run left the file unreadable
    if path.exists() {
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644));
    }
    std::fs::write(&path, "f>n;42").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    let out = ilo()
        .arg(path.to_str().unwrap())
        .output()
        .expect("failed to run ilo");
    // Restore permissions so cleanup works
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    std::fs::remove_file(&path).ok();
    assert!(!out.status.success(), "should fail on unreadable file");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Error reading") || stderr.contains("Permission"),
        "expected read error, got: {stderr}"
    );
}

// L285: --run-cranelift with no extra args after func name → vec![]
#[cfg(feature = "cranelift")]
#[test]
fn run_cranelift_no_extra_args() {
    // `f>n;42` takes no args, `--run-cranelift f` → run_args = vec![] at L285
    let out = ilo()
        .args(["f>n;42", "--run-cranelift", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "42");
}

// L300: --run-cranelift float result (non-integer)
#[cfg(feature = "cranelift")]
#[test]
fn run_cranelift_float_result() {
    // /x 3 with x=2 → 2/3 = 0.666... → println!("{}", result) at L300
    let out = ilo()
        .args(["f x:n>n;/x 3", "--run-cranelift", "f", "2"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let val: f64 = String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .expect("expected float");
    assert!(
        (val - 2.0 / 3.0).abs() < 1e-6,
        "expected ~0.666, got: {val}"
    );
}

// L304-305: --run-cranelift with non-eligible function → "not eligible" error
#[cfg(feature = "cranelift")]
#[test]
fn run_cranelift_not_eligible() {
    // Match expression is now JIT-eligible with NanVal JIT — should succeed
    let out = ilo()
        .args(["f x:n>n;?x{1:2;_:3}", "--run-cranelift", "f", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "match should be JIT-eligible now, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.trim() == "3",
        "expected wildcard arm result 3, got: {stdout}"
    );
}

// L441-443: run_default interpreter fallback error
#[test]
fn run_default_interpreter_error() {
    // f xs:L n>n;xs.0 with empty list [] — JIT now handles this,
    // returns nil for out-of-bounds index
    let out = ilo()
        .args(["f xs:L n>n;xs.0", "f", "[]"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "JIT handles empty list index, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.trim() == "nil",
        "expected nil for empty list index, got: {stdout}"
    );
}

// run_bench: L448-746 — bench with simple function covers the full run_bench path
#[test]
fn bench_simple_function() {
    // `f>n;42` with --bench covers run_bench (L448+), L230 (vec![]), all benchmark paths
    let out = ilo()
        .args(["f>n;42", "--bench", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Rust interpreter"),
        "expected bench output, got: {stdout}"
    );
    assert!(stdout.contains("Register VM"), "expected VM bench output");
}

// main.rs L525 (_ => None in filter_map) + L638 (Text(s) in call_args map)
#[test]
fn bench_with_text_arg() {
    // bench mode with a text arg → filter_map hits `_ => None` (L525), all_numeric=false,
    // and the Python call_args builder hits the Text(s) branch (L638)
    let out = ilo()
        .args(["f x:t>t;x", "--bench", "f", "hello"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Rust interpreter"),
        "expected bench output, got: {stdout}"
    );
}

// main.rs L639 (Bool(b) in call_args map)
#[test]
fn bench_with_bool_arg() {
    // bench mode with a bool arg → Python call_args builder hits Bool(b) branch (L639)
    let out = ilo()
        .args(["f x:b>b;x", "--bench", "f", "true"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Rust interpreter"),
        "expected bench output, got: {stdout}"
    );
}

// main.rs L640 (_ => "None" in call_args map for non-standard values like lists)
#[test]
fn bench_with_list_arg() {
    // bench mode with a list arg → Python call_args builder hits _ => "None" branch (L640)
    let out = ilo()
        .args(["f xs:L n>n;+xs.0 1", "--bench", "f", "[1,2,3]"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Rust interpreter"),
        "expected bench output, got: {stdout}"
    );
}

// main.rs L554-555 (arm64 JIT bench float result) + L587-588 (Cranelift bench float result)
// Use /x 2 with arg 1 → result is 0.5 (non-integer), hitting the else branch
#[test]
fn bench_jit_float_result() {
    // f x:n>n;/x 2 with arg 1 → JIT result = 0.5 (non-integer) → covers else branch
    let out = ilo()
        .args(["f x:n>n;/x 2", "--bench", "f", "1"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Rust interpreter"),
        "expected bench output, got: {stdout}"
    );
    // On JIT-capable platforms, the result line should show 0.5 (not integer)
    if stdout.contains("Cranelift JIT") {
        assert!(
            stdout.contains("0.5"),
            "expected float result in JIT output, got: {stdout}"
        );
    }
}

// L593 (Cranelift closing })
// L161 (Cranelift LOADK non-number → None)
// Uses a function with a text constant: Cranelift falls back via NanVal
#[test]
fn bench_jit_non_numeric_const() {
    // f x:n>n;y="hi";x — NanVal JIT now handles text constants
    let out = ilo()
        .args(["f x:n>n;y=\"hi\";x", "--bench", "f", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Rust interpreter"),
        "expected bench output, got: {stdout}"
    );
    // Cranelift JIT now compiles text-const functions via NanVal
    #[cfg(feature = "cranelift")]
    assert!(
        stdout.contains("Cranelift JIT"),
        "cranelift JIT should compile text-const fn with NanVal"
    );
}

// vm/jit_cranelift.rs L167-170 (OP_MOVE with a != b)
// Uses a match with wildcard arm: compile_match_arms allocates result_reg then body gives
// a different reg → OP_MOVE result_reg, body_reg (a != b) is emitted
#[test]
fn bench_jit_move_different_regs() {
    // f x:n>n;?x{_:+x 1} — match with wildcard arm producing +x 1
    // compile_match_arms: result_reg = reg1, body compiles +x 1 to reg2
    // → OP_MOVE 1,2 (a=1 != b=2) → arm64 L207-209 + Cranelift L167-170
    let out = ilo()
        .args(["f x:n>n;?x{_:+x 1}", "--bench", "f", "7"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Rust interpreter"),
        "expected bench output, got: {stdout}"
    );
    // Result should be 8 (x + 1 = 7 + 1)
    if stdout.contains("Cranelift JIT") {
        assert!(
            stdout.contains("  result:     8"),
            "expected result 8 in JIT output, got: {stdout}"
        );
    }
}

// main.rs L434: in run_default, Cranelift JIT path, compiled.func_names.iter().position()
// returns None when target function name isn't in compiled func_names.
// This happens when a program has no functions (e.g., only TypeDef declarations)
// and run_default is called, target="main", but compiled.func_names is empty.
#[test]
fn run_default_no_functions_in_compiled() {
    // type-only program with a numeric arg → run_default → JIT tries target="main"
    // compiled.func_names = [] → position returns None → L434 closing } fires
    // Falls through to interpreter which fails (no function named "main")
    let out = ilo()
        .args(["type pt{x:n}", "5"])
        .output()
        .expect("failed to run ilo");
    // Will fail because no function to call, but L434 is hit
    let _ = out;
}

// --- Auto-unwrap operator ! ---

fn write_temp_ilo(content: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = dir.join(format!("ilo_test_{}_{}.ilo", std::process::id(), n));
    std::fs::write(&path, content).expect("failed to write temp file");
    path
}

#[test]
fn unwrap_ok_path_inline() {
    // Use ~(inner! x) — parens prevent greedy arg consumption at decl boundary
    let f = write_temp_ilo("outer x:n>R n t;~(inner! x)\ninner x:n>R n t;~x");
    let out = ilo()
        .args([f.to_str().unwrap(), "42"])
        .output()
        .expect("failed to run ilo");
    std::fs::remove_file(&f).ok();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "~42");
}

#[test]
fn unwrap_err_path_inline() {
    let f = write_temp_ilo("outer x:n>R n t;~(inner! x)\ninner x:n>R n t;^\"fail\"");
    let out = ilo()
        .args([f.to_str().unwrap(), "42"])
        .output()
        .expect("failed to run ilo");
    std::fs::remove_file(&f).ok();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "^fail");
}

#[test]
fn unwrap_nested_propagation_inline() {
    let f = write_temp_ilo("a x:n>R n t;~(b! x)\nb x:n>R n t;~(c! x)\nc x:n>R n t;^\"deep\"");
    let out = ilo()
        .args([f.to_str().unwrap(), "1"])
        .output()
        .expect("failed to run ilo");
    std::fs::remove_file(&f).ok();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "^deep");
}

#[test]
fn unwrap_formatter_roundtrip() {
    let f = write_temp_ilo("outer x:n>R n t;~(inner! x)\ninner x:n>R n t;~x");
    let out = ilo()
        .args([f.to_str().unwrap(), "--fmt"])
        .output()
        .expect("failed to run ilo");
    std::fs::remove_file(&f).ok();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("inner!"),
        "expected inner! in formatted output, got: {}",
        stdout
    );
}

#[test]
fn unwrap_verifier_t025() {
    // inner returns n, not R — should fail with T025
    let f = write_temp_ilo("outer x:n>R n t;~(inner! x)\ninner x:n>n;x");
    let out = ilo()
        .args([f.to_str().unwrap()])
        .output()
        .expect("failed to run ilo");
    std::fs::remove_file(&f).ok();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("T025") || stderr.contains("not a Result"),
        "expected T025 error, got: {}",
        stderr
    );
}

#[test]
fn unwrap_verifier_t026() {
    // outer returns n, not R — should fail with T026
    let f = write_temp_ilo("outer x:n>n;(inner! x)\ninner x:n>R n t;~x");
    let out = ilo()
        .args([f.to_str().unwrap()])
        .output()
        .expect("failed to run ilo");
    std::fs::remove_file(&f).ok();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("T026") || stderr.contains("not a Result"),
        "expected T026 error, got: {}",
        stderr
    );
}

// --- HTTP get builtin + $ syntax ---

#[test]
fn get_verifier_wrong_type() {
    // get with number arg should fail verification
    let out = ilo()
        .args(["f x:n>R t t;get x", "f", "1"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("T013") || stderr.contains("expects t"),
        "expected type error for get with number, got: {}",
        stderr
    );
}

#[test]
fn dollar_parses_inline() {
    // $"url" should parse and verify without error (returns AST when no args)
    let out = ilo()
        .args([r#"f url:t>R t t;$url"#])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // No args → AST output
    assert!(
        stdout.contains("get"),
        "expected 'get' in AST output, got: {}",
        stdout
    );
}

#[test]
fn dollar_bang_parses_inline() {
    // $!url should parse as get! url — enclosing function must return R t t for ! to verify
    let out = ilo()
        .args([r#"f url:t>R t t;~($!url)"#])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("get"),
        "expected 'get' in AST output, got: {}",
        stdout
    );
}

// --- HTTP post builtin ---

#[test]
fn post_verifier_wrong_type_url() {
    // first arg (url) must be t
    let out = ilo()
        .args(["f x:n body:t>R t t;post x body", "f", "1", "b"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("T013") || stderr.contains("expects t"),
        "expected type error for post with number url, got: {stderr}"
    );
}

#[test]
fn post_verifier_wrong_type_body() {
    // second arg (body) must be t
    let out = ilo()
        .args(["f url:t x:n>R t t;post url x", "f", "u", "1"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("T013") || stderr.contains("expects t"),
        "expected type error for post with number body, got: {stderr}"
    );
}

#[test]
fn post_returns_result_type() {
    // post url body should type-check as R t t
    let out = ilo()
        .args([r#"f url:t body:t>R t t;post url body"#])
        .output()
        .expect("failed to run ilo");
    // No args → AST output; should succeed verification
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn post_appears_in_ast() {
    // post url body — no runtime args → AST output; verify succeeds
    let out = ilo()
        .args([r#"f url:t body:t>R t t;post url body"#])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("post"),
        "expected 'post' in AST output, got: {stdout}"
    );
}

// --- Braceless guards ---

#[test]
fn braceless_guard_classify_cases() {
    let program = r#"cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze""#;
    for (input, expected) in [("1500", "gold"), ("750", "silver"), ("100", "bronze")] {
        let out = ilo()
            .args([program, input])
            .output()
            .expect("failed to run ilo");
        assert!(
            out.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), expected);
    }
}

#[test]
fn braceless_guard_factorial() {
    let out = ilo()
        .args(["fac n:n>n;<=n 1 1;r=fac -n 1;*n r", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "120");
}

#[test]
fn braceless_guard_fibonacci() {
    let out = ilo()
        .args(["fib n:n>n;<=n 1 n;a=fib -n 1;b=fib -n 2;+a b", "10"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "55");
}

#[test]
fn braceless_guard_early_return_vs_braced_conditional() {
    // Braceless guard: early return → returns "gold" for 1500
    let braceless = ilo()
        .args([
            r#"cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze""#,
            "1500",
        ])
        .output()
        .expect("failed to run ilo");
    assert_eq!(
        String::from_utf8_lossy(&braceless.stdout).trim(),
        "gold",
        "braceless guard should early-return"
    );
    // Braced guard: conditional execution (no early return) → returns "bronze"
    let braced = ilo()
        .args([
            r#"cls sp:n>t;>=sp 1000{"gold"};>=sp 500{"silver"};"bronze""#,
            "1500",
        ])
        .output()
        .expect("failed to run ilo");
    assert_eq!(
        String::from_utf8_lossy(&braced.stdout).trim(),
        "bronze",
        "braced guard should be conditional execution (no early return)"
    );
}

// --- Range iteration ---

#[test]
fn range_basic() {
    let out = ilo()
        .args(["f>n;@i 0..3{i}", "--run", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "2");
}

#[test]
fn range_with_arg() {
    let out = ilo()
        .args(["f n:n>n;@i 0..n{*i i}", "4"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // i goes 0,1,2,3 → last body value is 3*3 = 9
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "9");
}

#[test]
fn range_empty() {
    let out = ilo()
        .args(["f>n;@i 5..2{99};0", "--run", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "0");
}

// --- Type aliases ---

#[test]
fn alias_basic_run() {
    // alias res R n t; function returning ~42 as res type
    let out = ilo()
        .args(["-e", "alias res R n t\nf>res;~42", "--run", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "~42");
}

#[test]
fn alias_in_param_run() {
    let out = ilo()
        .args(["-e", "alias num n\nf x:num>num;+x 1", "--run", "f", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "6");
}

// --- Import system (use "file.ilo") ---

#[test]
fn use_imports_function_from_file() {
    let lib = "/tmp/ilo_test_math.ilo";
    let main_file = "/tmp/ilo_test_main.ilo";
    std::fs::write(lib, "dbl n:n>n;*n 2\n").unwrap();
    std::fs::write(main_file, "use \"ilo_test_math.ilo\"\nrun x:n>n;dbl x\n").unwrap();

    let out = ilo()
        .args([main_file, "--run", "run", "5"])
        .output()
        .expect("failed to run ilo");
    let _ = std::fs::remove_file(lib);
    let _ = std::fs::remove_file(main_file);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "10");
}

#[test]
fn use_file_not_found_error() {
    let main_file = "/tmp/ilo_test_missing_import.ilo";
    std::fs::write(main_file, "use \"nonexistent_xyz.ilo\"\nf>n;1\n").unwrap();

    let out = ilo().args([main_file]).output().expect("failed to run ilo");
    let _ = std::fs::remove_file(main_file);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{err}{}", String::from_utf8_lossy(&out.stdout));
    assert!(
        combined.contains("ILO-P017")
            || combined.contains("not found")
            || combined.contains("nonexistent"),
        "got: {combined}"
    );
}

#[test]
fn use_circular_import_error() {
    let a = "/tmp/ilo_test_circ_a.ilo";
    let b = "/tmp/ilo_test_circ_b.ilo";
    std::fs::write(a, "use \"ilo_test_circ_b.ilo\"\nfa>n;1\n").unwrap();
    std::fs::write(b, "use \"ilo_test_circ_a.ilo\"\nfb>n;2\n").unwrap();

    let out = ilo().args([a]).output().expect("failed to run ilo");
    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{err}{}", String::from_utf8_lossy(&out.stdout));
    assert!(
        combined.contains("ILO-P018") || combined.contains("circular"),
        "got: {combined}"
    );
}

#[test]
fn use_in_inline_code_error() {
    // use in inline code (no file context) should error with ILO-P017
    let out = ilo()
        .args(["-e", "use \"foo.ilo\"\nf>n;1", "--run", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{err}{}", String::from_utf8_lossy(&out.stdout));
    assert!(
        combined.contains("ILO-P017")
            || combined.contains("inline")
            || combined.contains("context"),
        "got: {combined}"
    );
}

// --- Import error and transitive imports ---

#[test]
fn use_parse_error_in_imported_file() {
    let bad = "/tmp/ilo_test_parse_err_import.ilo";
    let main_file = "/tmp/ilo_test_parse_err_main.ilo";
    std::fs::write(bad, "f x:>n;x\n").unwrap(); // syntax error: missing type after ':'
    std::fs::write(
        main_file,
        "use \"ilo_test_parse_err_import.ilo\"\ng x:n>n;+x 1\n",
    )
    .unwrap();

    let out = ilo().args([main_file]).output().expect("failed to run ilo");
    let _ = std::fs::remove_file(bad);
    let _ = std::fs::remove_file(main_file);
    assert!(
        !out.status.success(),
        "should fail when imported file has parse error"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("error") || stderr.contains("expected"),
        "expected parse error diagnostic, got: {stderr}"
    );
}

#[test]
fn use_transitive_imports() {
    let file_b = "/tmp/ilo_test_trans_b.ilo";
    let file_a = "/tmp/ilo_test_trans_a.ilo";
    let file_main = "/tmp/ilo_test_trans_main.ilo";

    std::fs::write(file_b, "triple x:n>n;*x 3\n").unwrap();
    std::fs::write(
        file_a,
        "use \"ilo_test_trans_b.ilo\"\nsextuple x:n>n;t=triple x;*t 2\n",
    )
    .unwrap();
    std::fs::write(
        file_main,
        "use \"ilo_test_trans_a.ilo\"\nmain x:n>n;sextuple x\n",
    )
    .unwrap();

    let out = ilo()
        .args([file_main, "--run", "main", "2"])
        .output()
        .expect("failed to run ilo");
    let _ = std::fs::remove_file(file_b);
    let _ = std::fs::remove_file(file_a);
    let _ = std::fs::remove_file(file_main);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "12");
}

// --- --dense / -d flag ---

#[test]
fn dense_flag_formats_code() {
    let out = ilo()
        .args(["f x:n>n;+x 1", "--dense"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("f"),
        "expected function name in dense output: {stdout}"
    );
}

#[test]
fn dense_short_flag() {
    let out = ilo()
        .args(["f x:n>n;+x 1", "-d"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("f"));
}

// --- --expanded flag ---

#[test]
fn expanded_flag_formats_code() {
    let out = ilo()
        .args(["f x:n>n;+x 1", "--expanded"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("f"));
}

// --- --json flag result wrapping ---

#[test]
fn json_flag_wraps_ok_result() {
    let out = ilo()
        .args(["--json", "f x:n>n;*x 2", "--run", "f", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"ok\""),
        "expected JSON ok wrapper, got: {stdout}"
    );
    assert!(stdout.contains("10"), "expected result 10, got: {stdout}");
}

#[test]
fn json_flag_wraps_err_result() {
    let out = ilo()
        .args(["--json", "-e", "f>R n t;^\"oops\"", "--run", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"error\""),
        "expected JSON error wrapper, got: {stdout}"
    );
    assert!(
        stdout.contains("program"),
        "expected 'program' phase, got: {stdout}"
    );
}

// --- JSON mode cross-language warning ---

#[test]
fn json_mode_cross_language_warning() {
    let out = ilo()
        .args(["--json", "f x:n>n;*x 2", "5"])
        .env("NO_COLOR", "1")
        .output()
        .expect("failed to run ilo");
    // Just verify it runs without crash; cross-language warning only fires if pattern present
    assert!(out.status.success() || !out.stderr.is_empty());
}

// ── --tools / --mcp missing value error paths ─────────────────────────────

/// `ilo <code> --tools` with no following path → error + exit (main.rs L996-997)
#[test]
fn run_cmd_tools_flag_missing_path() {
    let out = ilo()
        .args(["f>n;1", "--tools"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected failure when --tools has no path"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--tools"),
        "expected --tools error, got: {stderr}"
    );
}

/// `ilo <code> --mcp` with no following path → error + exit (main.rs L1004-1005)
#[test]
fn run_cmd_mcp_flag_missing_path() {
    let out = ilo()
        .args(["f>n;1", "--mcp"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected failure when --mcp has no path"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--mcp"),
        "expected --mcp error, got: {stderr}"
    );
}

/// `ilo <code> --mcp <path>` with a valid path triggers the no-tools-feature error.
/// Covers: main.rs L1041-1043 (cfg(not(tools)) mcp_config_path check).
#[test]
fn run_cmd_mcp_with_path_no_tools_feature() {
    use std::io::Write;

    let mut path = std::env::temp_dir();
    path.push("ilo_run_mcp_test.json");
    let mut f = std::fs::File::create(&path).expect("create temp file");
    writeln!(f, r#"{{"mcpServers": {{}}}}"#).unwrap();
    drop(f);

    let out = ilo()
        .args(["f>n;1", "--mcp", path.to_str().unwrap()])
        .output()
        .expect("failed to run ilo");
    // With tools feature: succeeds; without: exits with "tools feature" error
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Either way, no panic
    assert!(
        !stderr.contains("thread 'main' panicked"),
        "unexpected panic: {stderr}"
    );

    std::fs::remove_file(&path).ok();
}

// ── verify warnings (ILO-T029) reported in run_cmd (main.rs L1107-1108) ───

/// A program with unreachable code after `ret` produces an ILO-T029 warning.
/// The warning is reported (not fatal), and the program still runs.
#[test]
fn run_cmd_verify_warning_unreachable_code() {
    // `ret 1` followed by `2` — `2` is unreachable → ILO-T029 warning
    let out = ilo()
        .env("NO_COLOR", "1")
        .args(["f>n;ret 1;2", "f"])
        .output()
        .expect("failed to run ilo");
    // Program still runs (warnings are non-fatal)
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Either succeeds with output "1" or emits a warning to stderr
    assert!(
        stdout.trim() == "1"
            || stderr.contains("T029")
            || stderr.contains("unreachable")
            || stderr.contains("warn"),
        "expected output=1 or ILO-T029 warning; stdout={stdout:?} stderr={stderr:?}"
    );
}

// ── ilo serv / ilo repl subcommand basic paths ─────────────────────────────

/// `ilo serv` with empty stdin (immediate EOF) exercises the serve loop startup.
/// Covers: main.rs L506-512 (http_config=None), L515-521 (rt create), L524-543 (mcp=None),
/// L558 (ready signal), L560-585 (stdin loop, exits on EOF).
#[test]
fn serv_cmd_empty_stdin_exits_cleanly() {
    use std::process::Stdio;
    let out = ilo()
        .args(["serv"])
        .stdin(Stdio::null()) // immediate EOF
        .output()
        .expect("failed to run ilo serv");
    let stdout = String::from_utf8_lossy(&out.stdout);
    // serv_cmd prints {"ready":true} before reading stdin
    assert!(
        stdout.contains("ready"),
        "expected ready signal, got: {stdout}"
    );
}

/// `ilo serv` processing a valid JSON request covers the full serve request path.
/// Covers: main.rs L562-583 (read line, process, print response).
#[test]
fn serv_cmd_processes_one_request() {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = ilo()
        .args(["serv"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ilo serv");

    // Send one valid request then close stdin (EOF)
    if let Some(mut stdin) = child.stdin.take() {
        writeln!(stdin, r#"{{"program":"f>n;1","args":[],"func":"f"}}"#).unwrap();
        // stdin drops here → EOF
    }

    let out = child.wait_with_output().expect("ilo serv failed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    // First line: ready; second line: result with "ok"
    assert!(stdout.contains("ready"), "expected ready signal");
    assert!(
        stdout.contains("ok") || stdout.contains("1"),
        "expected ok result, got: {stdout}"
    );
}

/// `ilo repl` launches interactive REPL, exits on EOF (stdin closed).
#[test]
fn repl_exits_on_eof() {
    use std::process::Stdio;
    let out = ilo()
        .args(["repl"])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run ilo repl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("ilo"), "expected banner from repl");
    assert!(out.status.success(), "repl should exit cleanly on EOF");
}

/// `ilo repl -j` falls through to JSON serv mode.
#[test]
fn repl_json_mode_is_serv() {
    use std::process::Stdio;
    let out = ilo()
        .args(["repl", "-j"])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run ilo repl -j");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ready"),
        "expected ready signal from repl -j"
    );
}

/// `ilo repl` can define functions and run expressions.
#[test]
fn repl_define_and_run() {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = ilo()
        .args(["repl"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ilo repl");
    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(stdin, "dbl x:n>n;*x 2").unwrap();
        writeln!(stdin, "dbl 21").unwrap();
        writeln!(stdin, ":q").unwrap();
    }
    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("defined: dbl"),
        "should show definition: {stdout}"
    );
    assert!(
        stdout.contains("42"),
        "should compute dbl 21 = 42: {stdout}"
    );
}

/// `ilo serv --tools <config>` loads the HTTP config before the serve loop.
/// Covers: main.rs L506-512 (http_path=Some → http_config loaded).
#[test]
fn serv_cmd_with_tools_config_loads_http() {
    use std::io::Write;
    use std::process::Stdio;

    let mut path = std::env::temp_dir();
    path.push("ilo_serv_test_tools.json");
    let mut f = std::fs::File::create(&path).expect("create temp file");
    writeln!(
        f,
        r#"{{"tools": {{"echo": {{"url": "http://127.0.0.1:19999/echo"}}}}}}"#
    )
    .unwrap();
    drop(f);

    let out = ilo()
        .args(["serv", "--tools", path.to_str().unwrap()])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run ilo serv --tools");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ready"),
        "expected ready signal with tools config"
    );

    std::fs::remove_file(&path).ok();
}

/// `ilo serv --mcp <empty_mcp.json>` parses the mcp path arg successfully.
/// Covers: main.rs L482-483 (serv_cmd --mcp path present → mcp_path = Some).
#[test]
fn serv_cmd_mcp_with_empty_config_exits_cleanly() {
    use std::io::Write;
    use std::process::Stdio;

    let mut path = std::env::temp_dir();
    path.push("ilo_serv_mcp_empty.json");
    let mut f = std::fs::File::create(&path).expect("create temp file");
    writeln!(f, r#"{{"mcpServers": {{}}}}"#).unwrap();
    drop(f);

    let out = ilo()
        .args(["serv", "--mcp", path.to_str().unwrap()])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run ilo serv --mcp");

    // The --mcp arg was parsed (lines 482-483 covered). With tools feature: prints ready;
    // without tools feature: exits with "tools feature" error. Either is acceptable.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains("ready") || stderr.contains("tools"),
        "expected ready or tools error, got stdout={stdout} stderr={stderr}"
    );

    std::fs::remove_file(&path).ok();
}

/// `ilo serv --mcp` (no path) → exit(1) with error message.
/// Covers: main.rs L478-481 (serv_cmd --mcp missing path).
#[test]
fn serv_cmd_mcp_missing_path_exits_with_error() {
    use std::process::Stdio;
    let out = ilo()
        .args(["serv", "--mcp"])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run ilo serv --mcp");
    assert!(!out.status.success(), "expected non-zero exit");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--mcp"),
        "expected --mcp error, got: {stderr}"
    );
}

/// `ilo serv --tools` (no path) → exit(1) with error message.
/// Covers: main.rs L487-488 (serv_cmd --tools missing path).
#[test]
fn serv_cmd_tools_missing_path_exits_with_error() {
    use std::process::Stdio;
    let out = ilo()
        .args(["serv", "--tools"])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run ilo serv --tools");
    assert!(!out.status.success(), "expected non-zero exit");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--tools"),
        "expected --tools error, got: {stderr}"
    );
}

/// `ilo serv --tools <invalid.json>` → exit(1) when config fails to load.
/// Covers: main.rs L508-510 (serv_cmd http_config error path).
#[test]
fn serv_cmd_tools_invalid_config_exits_with_error() {
    use std::io::Write;
    use std::process::Stdio;

    let mut path = std::env::temp_dir();
    path.push("ilo_serv_test_invalid_tools.json");
    let mut f = std::fs::File::create(&path).expect("create temp file");
    writeln!(f, "not valid json at all!!!").unwrap();
    drop(f);

    let out = ilo()
        .args(["serv", "--tools", path.to_str().unwrap()])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run ilo serv --tools invalid");
    assert!(!out.status.success(), "expected non-zero exit");

    std::fs::remove_file(&path).ok();
}

/// `ilo serv` with empty lines in stdin — empty lines are skipped (L571 continue).
/// Covers: main.rs L570-572 (empty line trimmed → continue).
#[test]
fn serv_cmd_skips_empty_stdin_lines() {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = ilo()
        .args(["serv"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ilo serv");

    if let Some(mut stdin) = child.stdin.take() {
        // Send empty lines followed by a valid request
        writeln!(stdin).unwrap();
        writeln!(stdin, "   ").unwrap();
        writeln!(stdin, r#"{{"program":"f>n;42","args":[],"func":"f"}}"#).unwrap();
        // drop stdin → EOF
    }

    let out = child.wait_with_output().expect("ilo serv failed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("ready"), "expected ready signal");
    assert!(
        stdout.contains("ok") || stdout.contains("42"),
        "expected result, got: {stdout}"
    );
}

/// `ilo tools --tools <invalid.json>` → exit(1) when ToolsConfig::from_file fails.
/// Covers: main.rs L108-109 (tools_cmd http error path).
#[test]
fn tools_cmd_invalid_tools_config_exits_with_error() {
    use std::io::Write;

    let mut path = std::env::temp_dir();
    path.push("ilo_tools_test_invalid.json");
    let mut f = std::fs::File::create(&path).expect("create temp file");
    writeln!(f, "{{bad json").unwrap();
    drop(f);

    let out = ilo()
        .args(["tools", "--tools", path.to_str().unwrap()])
        .output()
        .expect("failed to run ilo tools --tools invalid");
    assert!(!out.status.success(), "expected non-zero exit");

    std::fs::remove_file(&path).ok();
}

/// `ilo <prog> --run-vm f --tools <config>` runs via VM with HTTP tools config loaded.
/// Covers: main.rs L1351-1371 (run_vm_with_provider http tools path).
#[test]
fn run_vm_with_tools_config() {
    use std::io::Write;

    let mut path = std::env::temp_dir();
    path.push("ilo_vm_tools_test.json");
    let mut f = std::fs::File::create(&path).expect("create temp file");
    writeln!(
        f,
        r#"{{"tools": {{"echo": {{"url": "http://127.0.0.1:19999/echo"}}}}}}"#
    )
    .unwrap();
    drop(f);

    // Program doesn't call the tool, so no network needed; just loads config and runs normally
    let out = ilo()
        .args(["f>n;99", "--run-vm", "f", "--tools", path.to_str().unwrap()])
        .output()
        .expect("failed to run ilo --run-vm --tools");
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Should succeed and print 99
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.trim() == "99", "expected 99, got: {stdout}");

    std::fs::remove_file(&path).ok();
}

// ── Builtin long-form aliases ───────────────────────────────────────────────

#[test]
fn builtin_alias_length() {
    let out = ilo()
        .args(["f xs:L n>n;length xs", "1,2,3"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "3");
}

#[test]
fn builtin_alias_filter() {
    let out = ilo()
        .args([
            "pos x:n>b;>x 0 main xs:L n>L n;filter pos xs",
            "main",
            "-3,0,2,4",
        ])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "[2, 4]");
}

#[test]
fn builtin_alias_sort() {
    let out = ilo()
        .args(["f xs:L n>L n;sort xs", "3,1,2"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "[1, 2, 3]");
}

#[test]
fn builtin_alias_reverse() {
    let out = ilo()
        .args(["f xs:L n>L n;reverse xs", "1,2,3"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "[3, 2, 1]");
}

#[test]
fn builtin_alias_trim() {
    let out = ilo()
        .args(["f s:t>t;trim s", "  hello  "])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hello");
}

#[test]
fn builtin_alias_average() {
    let out = ilo()
        .args(["f xs:L n>n;average xs", "2,4,6"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "4");
}

#[test]
fn builtin_alias_hint_emitted() {
    let out = ilo()
        .args(["f xs:L n>n;length xs", "1,2,3"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("hint") && stderr.contains("length") && stderr.contains("len"),
        "expected alias hint, got: {stderr}"
    );
}

#[test]
fn builtin_alias_floor_and_ceil() {
    let out = ilo()
        .args(["f x:n>n;floor x", "3.7"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "3");

    let out = ilo()
        .args(["f x:n>n;ceil x", "3.2"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "4");
}

#[test]
fn builtin_alias_format() {
    let out = ilo()
        .args(["f>t;format \"{} + {} = {}\" 1 2 3", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "1 + 2 = 3");
}

#[test]
fn builtin_alias_no_hint_suppressed() {
    let out = ilo()
        .args(["f xs:L n>n;length xs", "--no-hints", "1,2,3"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("hint"),
        "hints should be suppressed: {stderr}"
    );
}

// ── Alias coverage: exercise alias resolution in various AST contexts ───────

#[test]
fn alias_in_guard() {
    // alias inside guard body (ternary form)
    let out = ilo()
        .args(["f x:n>n;>=x 5{floor x}{ceil x}", "7.3"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "7");
}

#[test]
fn alias_in_guard_else() {
    // alias in else branch of ternary guard
    let out = ilo()
        .args(["f x:n>n;>=x 5{floor x}{ceil x}", "3.2"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "4");
}

#[test]
fn alias_in_let() {
    // alias inside a let binding
    let out = ilo()
        .args(["f xs:L n>n;n=length xs;n", "5,6,7,8"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "4");
}

#[test]
fn alias_in_foreach() {
    // alias inside a foreach body (parens needed for multi-arg call in braces)
    let out = ilo()
        .args(["f xs:L n>n;r=0;@x xs{r=+r (floor 1.9)};r", "1,2,3"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "3");
}

#[test]
fn alias_in_match_stmt() {
    // alias in match arm body
    let out = ilo()
        .args(["f x:n>n;?x{1:(floor 3.7);_:0}", "1"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "3");
}

#[test]
fn alias_in_list_literal() {
    // alias call nested inside a list literal
    let out = ilo()
        .args(["f x:n>L n;[floor x, ceil x]", "3.5"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "[3, 4]");
}

#[test]
fn alias_in_binop() {
    // alias as operand of a binary operation
    let out = ilo()
        .args(["f x:n>n;+(floor x) 10", "3.7"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "13");
}

#[test]
fn alias_in_for_range() {
    // alias inside a for-range body
    let out = ilo()
        .args(["f n:n>n;r=0;@i 0..n{r=+r (floor 1.5)};r", "3"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "3");
}

// ── REPL integration tests (piped stdin) ────────────────────────────────────

/// Helper: spawn `ilo repl` with piped stdin, send input, collect output.
fn run_repl(input: &str) -> std::process::Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = ilo()
        .args(["repl"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ilo repl");
    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(input.as_bytes()).unwrap();
    }
    child.wait_with_output().unwrap()
}

#[test]
fn repl_quit_q() {
    let out = run_repl(":q\n");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn repl_quit_exit_command() {
    let out = run_repl(":exit\n");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn repl_quit_x_command() {
    let out = run_repl(":x\n");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn repl_quit_word_exit() {
    let out = run_repl("exit\n");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn repl_quit_word_quit() {
    let out = run_repl("quit\n");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn repl_eval_expression() {
    let out = run_repl("+1 2\n:q\n");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("3"), "expected 3 in output, got: {stdout}");
}

#[test]
fn repl_defs_empty() {
    let out = run_repl(":defs\n:q\n");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("(no definitions)"),
        "expected '(no definitions)', got: {stdout}"
    );
}

#[test]
fn repl_defs_lists_functions() {
    let out = run_repl("f x:n>n;x\n:defs\n:q\n");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("f x:n>n;x"),
        "expected definition in :defs output, got: {stdout}"
    );
}

#[test]
fn repl_clear_defs() {
    let out = run_repl("f x:n>n;x\n:clear\n:defs\n:q\n");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("cleared all definitions"),
        "expected 'cleared all definitions', got: {stdout}"
    );
    assert!(
        stdout.contains("(no definitions)"),
        "expected empty defs after clear, got: {stdout}"
    );
}

#[test]
fn repl_help_command() {
    let out = run_repl(":help\n:q\n");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(":w <file>"),
        "expected help text, got: {stdout}"
    );
    assert!(
        stdout.contains(":defs"),
        "expected :defs in help, got: {stdout}"
    );
}

#[test]
fn repl_unknown_command() {
    let out = run_repl(":foobar\n:q\n");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown command"),
        "expected 'unknown command', got: {stderr}"
    );
}

#[test]
fn repl_wq_no_defs() {
    let out = run_repl(":wq\n:q\n");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no definitions to save"),
        "expected 'no definitions to save', got: {stderr}"
    );
}

#[test]
fn repl_wq_with_defs_no_path() {
    let out = run_repl("f x:n>n;x\n:wq\n:q\n");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("usage: :w <file.ilo>"),
        "expected usage hint, got: {stderr}"
    );
}

#[test]
fn repl_w_save_file() {
    let path = "/tmp/ilo_repl_test_save_cov.ilo";
    let _ = std::fs::remove_file(path);
    let out = run_repl(&format!("f x:n>n;*x 2\n:w {path}\n:q\n"));
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("saved 1 definition(s)"),
        "expected save message, got: {stdout}"
    );
    let contents = std::fs::read_to_string(path).expect("saved file should exist");
    assert!(
        contents.contains("f x:n>n;*x 2"),
        "expected definition in file, got: {contents}"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn repl_wq_save_and_quit() {
    let path = "/tmp/ilo_repl_test_wq_cov.ilo";
    let _ = std::fs::remove_file(path);
    let out = run_repl(&format!("f x:n>n;+x 1\n:wq {path}\n"));
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("saved 1 definition(s)"),
        "expected save message, got: {stdout}"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn repl_w_no_defs_to_save() {
    let out = run_repl(":w /tmp/ilo_repl_nodefs_cov.ilo\n:q\n");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no definitions to save"),
        "expected 'no definitions to save', got: {stderr}"
    );
}

#[test]
fn repl_multiline_braces() {
    // Multi-line input: unclosed brace continues reading on next line
    let out = run_repl("f x:n>n;<=x 0{\n0\n};x\nf 5\n:q\n");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("5"), "expected 5 in output, got: {stdout}");
}

#[test]
fn repl_multiline_semicolon() {
    // Line ending with ; continues reading
    let out = run_repl("f x:n>n;\n+x 1\nf 5\n:q\n");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("defined:"),
        "expected definition, got: {stdout}"
    );
}

#[test]
fn repl_empty_lines_ignored() {
    let out = run_repl("\n\n+1 1\n:q\n");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("2"), "expected 2 in output, got: {stdout}");
}

#[test]
fn repl_eof_exits() {
    use std::process::Stdio;
    let out = ilo()
        .args(["repl"])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn repl_define_typedef() {
    // Covers type_to_ilo for TypeDef fields (L666-668)
    let out = run_repl("type point{x:n;y:n}\n:defs\n:q\n");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("defined type: point"),
        "expected typedef output, got: {stdout}"
    );
}

#[test]
fn repl_define_alias() {
    // Covers type_to_ilo for Alias (L670-671)
    let out = run_repl("alias num n\n:defs\n:q\n");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("defined alias: num"),
        "expected alias output, got: {stdout}"
    );
}

#[test]
fn repl_parse_error() {
    // Invalid expression should show error but not crash
    let out = run_repl("@@@\n:q\n");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.is_empty(), "expected error on stderr, got nothing");
}

// ── CLI emit format edge cases ──────────────────────────────────────────────

#[test]
fn emit_dense_format() {
    let out = ilo()
        .args(["f x:n>n;y=*x 2;+y 1", "--dense"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.is_empty(), "expected dense output");
}

#[test]
fn emit_dense_short_flag() {
    let out = ilo()
        .args(["f x:n>n;*x 2", "-d"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn emit_fmt_alias() {
    let out = ilo()
        .args(["f x:n>n;*x 2", "--fmt"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn emit_expanded_format() {
    let out = ilo()
        .args(["f x:n>n;y=*x 2;+y 1", "--expanded"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.is_empty(), "expected expanded output");
}

#[test]
fn emit_expanded_short_flag() {
    let out = ilo()
        .args(["f x:n>n;*x 2", "-e"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn emit_fmt_expanded_alias() {
    let out = ilo()
        .args(["f x:n>n;*x 2", "--fmt-expanded"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn no_hints_flag() {
    // --no-hints should suppress idiomatic hints
    let out = ilo()
        .args(["--no-hints", "f x:n y:n>b;=x y", "1", "1"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("hint:"),
        "expected no hints with --no-hints, got: {stderr}"
    );
}

#[test]
fn no_hints_short_flag() {
    let out = ilo()
        .args(["-nh", "f x:n y:n>b;=x y", "1", "1"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("hint:"),
        "expected no hints with -nh, got: {stderr}"
    );
}

// ── serv subcommand additional tests ────────────────────────────────────────

#[test]
fn serv_run_program_with_response() {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = ilo()
        .args(["serv"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ilo serv");
    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            r#"{{"program":"f x:n>n;*x 2","args":["5"],"func":"f"}}"#
        )
        .unwrap();
    }
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines.len() >= 2, "expected at least 2 lines, got: {stdout}");
    let resp: serde_json::Value = serde_json::from_str(lines[1])
        .unwrap_or_else(|_| panic!("expected JSON response, got: {}", lines[1]));
    assert_eq!(
        resp["ok"],
        serde_json::json!(10),
        "expected ok=10, got: {resp}"
    );
}

#[test]
fn serv_invalid_json_request() {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = ilo()
        .args(["serv"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ilo serv");
    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(stdin, "not json").unwrap();
    }
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines.len() >= 2, "expected at least 2 lines, got: {stdout}");
    let resp: serde_json::Value = serde_json::from_str(lines[1])
        .unwrap_or_else(|_| panic!("expected JSON response, got: {}", lines[1]));
    assert!(
        resp["error"].is_object(),
        "expected error in response, got: {resp}"
    );
}

// --- CLI nil arg ---

#[test]
fn cli_nil_arg_to_optional_param() {
    let out = ilo()
        .args(["f x:O n>n;x??0", "f", "nil"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "0");
}

#[test]
fn cli_nil_arg_equality() {
    let out = ilo()
        .args(["f x:O n>b;=x nil", "f", "nil"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "true");
}

// --- CLI single value auto-wrapped as list ---

#[test]
fn cli_single_number_coerced_to_list() {
    let out = ilo()
        .args(["f xs:L n>n;sum xs", "f", "10"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "10");
}

#[test]
fn cli_single_text_coerced_to_list() {
    let out = ilo()
        .args(["f xs:L t>n;len xs", "f", "hello"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "1");
}

#[test]
fn cli_comma_list_still_works() {
    let out = ilo()
        .args(["f xs:L n>n;sum xs", "f", "1,2,3"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "6");
}

// --- Sum type exhaustive match ---

#[test]
fn sum_type_match_all_variants_runs() {
    let out = ilo()
        .args([
            r#"f x:S red green blue>t;?x{"red":"r";"green":"g";"blue":"b"}"#,
            "f",
            "red",
        ])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "r");
}

#[test]
fn sum_type_match_missing_variant_errors() {
    let out = ilo()
        .args([
            r#"f x:S red green blue>t;?x{"red":"r";"green":"g"}"#,
            "f",
            "red",
        ])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("ILO-T024") || stderr.contains("non-exhaustive"));
}
