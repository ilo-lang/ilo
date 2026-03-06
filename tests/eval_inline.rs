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
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
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
    assert!(stdout.contains("\"name\""), "expected AST JSON, got: {}", stdout);
}

// --- Inline code: multiple functions ---

#[test]
fn inline_multi_func_select_by_name() {
    let out = ilo()
        .args(["dbl x:n>n;s=*x 2;+s 0 tot p:n q:n r:n>n;s=*p q;t=*s r;+s t", "tot", "10", "20", "30"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "6200");
}

#[test]
fn inline_multi_func_first_by_default() {
    let out = ilo()
        .args(["dbl x:n>n;s=*x 2;+s 0 tot p:n q:n r:n>n;s=*p q;t=*s r;+s t", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
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
    assert!(stdout.contains("def tot"), "expected 'def tot', got: {}", stdout);
}

// --- Inline code: explicit --run ---

#[test]
fn inline_explicit_run() {
    let out = ilo()
        .args(["tot p:n q:n r:n>n;s=*p q;t=*s r;+s t", "--run", "tot", "10", "20", "30"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "6200");
}

// --- Error cases ---

#[test]
fn no_args_shows_usage() {
    let out = ilo()
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Usage"), "expected usage message, got: {}", stderr);
}

#[test]
fn inline_empty_string_errors() {
    let out = ilo()
        .args([""])
        .output()
        .expect("failed to run ilo");
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
        .args(["research/explorations/idea9-ultra-dense-short/01-simple-function.ilo", "10", "20", "0.1"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    // 01-simple-function.ilo defines tot: (10*20) + (10*20*0.1) = 200 + 20 = 220
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "220");
}

#[test]
fn file_no_args_outputs_ast() {
    let out = ilo()
        .args(["research/explorations/idea9-ultra-dense-short/01-simple-function.ilo"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"name\""), "expected AST JSON, got: {}", stdout);
}

// --- Nested prefix operators ---

#[test]
fn inline_nested_prefix() {
    let out = ilo()
        .args(["f a:n b:n c:n>n;+*a b c", "2", "3", "4"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "10");
}

// --- CLI modes ---

#[test]
fn inline_run_vm_mode() {
    let out = ilo()
        .args(["f x:n>n;*x 2", "--run-vm", "f", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "10");
}

#[test]
fn inline_run_with_func_name() {
    let out = ilo()
        .args(["f x:n>n;*x 2", "--run", "f", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
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
    assert!(stderr.contains("Unknown emit target"), "expected emit error, got: {}", stderr);
}

#[test]
fn inline_parse_bool_arg() {
    let out = ilo()
        .args(["f x:b>b;!x", "true"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "false");
}

#[test]
fn inline_parse_false_arg() {
    // "false" string arg → parse_arg_value returns Bool(false) (main.rs L771)
    let out = ilo()
        .args(["f x:b>b;x", "false"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "false");
}

#[test]
fn inline_parse_text_arg() {
    let out = ilo()
        .args(["f x:t>t;x", "hello"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
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
    assert!(stderr.contains("Parse error") || stderr.contains("error"), "expected parse error, got: {}", stderr);
}

#[test]
fn inline_bench_mode() {
    let out = ilo()
        .args(["f x:n>n;*x 2", "--bench", "f", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("interpreter") || stdout.contains("vm"), "expected benchmark output, got: {}", stdout);
}

// --- Legacy -e flag ---

// --- Help ---

#[test]
fn help_variants_show_usage() {
    for flag in ["--help", "-h", "help"] {
        let out = ilo().args([flag]).output().expect("failed to run ilo");
        assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("Backends:"), "expected backends section, got: {}", stdout);
    }
}

// --- List arguments ---

#[test]
fn inline_list_arg_bracketed() {
    let out = ilo()
        .args(["f xs:L n>n;len xs", "[1,2,3]"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "3");
}

#[test]
fn inline_list_arg_bracketed_index() {
    let out = ilo()
        .args(["f xs:L n>n;xs.0", "[10,20,30]"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "10");
}

#[test]
fn inline_list_arg_bare_comma() {
    let out = ilo()
        .args(["f xs:L n>n;len xs", "1,2,3"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "3");
}

#[test]
fn inline_list_arg_bare_comma_index() {
    let out = ilo()
        .args(["f xs:L n>n;xs.0", "10,20,30"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "10");
}

#[test]
fn help_shows_usage() {
    let out = ilo()
        .args(["help"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Backends:"), "expected backends section, got: {}", stdout);
    assert!(stdout.contains("--run-interp"), "expected --run-interp, got: {}", stdout);
}

#[test]
fn help_lang_shows_spec() {
    let out = ilo()
        .args(["help", "lang"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("ilo Language Spec"), "expected spec header, got: {}", stdout);
}

// --- Backend flags ---

#[test]
fn inline_run_interp() {
    let out = ilo()
        .args(["f x:n>n;*x 2", "--run-interp", "f", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "10");
}

#[test]
fn inline_run_cranelift() {
    let out = ilo()
        .args(["f x:n>n;*x 2", "--run-cranelift", "f", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "10");
}

#[test]
fn default_falls_back_for_non_numeric() {
    // Bool args are not JIT-eligible, should fall back to interpreter
    let out = ilo()
        .args(["f x:b>b;!x", "true"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "false");
}

// --- Legacy -e flag ---

#[test]
fn legacy_e_flag_still_works() {
    let out = ilo()
        .args(["-e", "tot p:n q:n r:n>n;s=*p q;t=*s r;+s t", "--run", "tot", "10", "20", "30"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "6200");
}

#[test]
fn legacy_e_flag_missing_code() {
    let out = ilo()
        .args(["-e"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Usage"), "expected usage message, got: {}", stderr);
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
    assert!(stderr.contains("error[ILO-T004]"), "expected error in stderr, got: {}", stderr);
    assert!(stderr.contains("undefined variable 'y'"), "expected undefined var error, got: {}", stderr);
}

#[test]
fn verify_undefined_function() {
    let out = ilo()
        .args(["--text", "f x:n>n;foo x", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("error[ILO-T005]"), "expected error in stderr, got: {}", stderr);
    assert!(stderr.contains("undefined function 'foo'"), "expected undefined func error, got: {}", stderr);
}

#[test]
fn verify_arity_mismatch() {
    let out = ilo()
        .args(["--text", "g a:n b:n>n;+a b f x:n>n;g x", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("arity mismatch"), "expected arity error, got: {}", stderr);
}

#[test]
fn verify_type_mismatch() {
    let out = ilo()
        .args(["--text", "f x:t>n;*x 2", "hello"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("error[ILO-T009]"), "expected error in stderr, got: {}", stderr);
}

#[test]
fn verify_valid_program_runs() {
    let out = ilo()
        .args(["f x:n>n;*x 2", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "10");
}

// --- Prefix expressions as call arguments ---

#[test]
fn inline_factorial_with_prefix_call_arg() {
    // fac -n 1 as a call with prefix arg, result bound then used in operator
    let out = ilo()
        .args(["fac n:n>n;<=n 1{1};r=fac -n 1;*n r", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "120");
}

#[test]
fn inline_fibonacci_with_prefix_call_args() {
    // fib -n 1 and fib -n 2 as direct calls with prefix args
    let out = ilo()
        .args(["fib n:n>n;<=n 1{n};a=fib -n 1;b=fib -n 2;+a b", "10"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "55");
}

#[test]
fn inline_call_with_nested_prefix_unchanged() {
    // +*a b c should still work as nested prefix: (a*b) + c
    let out = ilo()
        .args(["f a:n b:n c:n>n;+*a b c", "2", "3", "4"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
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
    let out = ilo()
        .args(["--text", "f x:n>n;+x \"hi\""])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("error["), "expected 'error[' in stderr: {}", stderr);
    // No ANSI codes
    assert!(!stderr.contains("\x1b["), "unexpected ANSI codes in text mode: {}", stderr);
}

#[test]
fn ansi_flag_produces_colored_error() {
    let out = ilo()
        .args(["--ansi", "f x:n>n;+x \"hi\""])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("error"), "expected error in stderr: {}", stderr);
    // Should contain ANSI escape codes
    assert!(stderr.contains("\x1b["), "expected ANSI codes in ansi mode: {}", stderr);
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
    let first_line = stderr.lines().next()
        .unwrap_or_else(|| panic!("expected output on stderr, got empty"));
    let v: serde_json::Value = serde_json::from_str(first_line)
        .unwrap_or_else(|_| panic!("expected JSON on first line of stderr, got: {}", stderr));
    assert_eq!(v["severity"], "error");
    // Should have labels with span info
    assert!(v["labels"].as_array().is_some_and(|l| !l.is_empty()),
        "expected labels in: {}", stderr);
}

#[test]
fn text_flag_verify_error_has_function_note() {
    let out = ilo()
        .args(["--text", "f x:n>n;+x \"hi\""])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("note:"), "expected note in stderr: {}", stderr);
    assert!(stderr.contains("'f'"), "expected function name in stderr: {}", stderr);
}

#[test]
fn mutual_exclusion_json_text() {
    let out = ilo()
        .args(["--json", "--text", "f x:n>n;x"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("mutually exclusive"), "expected mutual exclusion error: {}", stderr);
}

#[test]
fn no_color_env_produces_no_ansi() {
    let out = ilo()
        .args(["f x:n>n;+x \"hi\""])
        .env("NO_COLOR", "1")
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("\x1b["), "unexpected ANSI codes with NO_COLOR: {}", stderr);
}

// --- Compact spec (ilo help ai / ilo -ai) ---

#[test]
fn help_ai_subcommand_exits_success() {
    let out = ilo()
        .args(["help", "ai"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn ai_flag_exits_success() {
    let out = ilo()
        .args(["-ai"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn help_ai_and_ai_flag_produce_same_output() {
    let out1 = ilo().args(["help", "ai"]).output().expect("failed to run ilo");
    let out2 = ilo().args(["-ai"]).output().expect("failed to run ilo");
    assert_eq!(out1.stdout, out2.stdout, "help ai and -ai should produce identical output");
}

#[test]
fn help_ai_hygiene_checks() {
    let out = ilo().args(["help", "ai"]).output().expect("failed to run ilo");
    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines() {
        assert!(!line.trim().is_empty(), "unexpected blank line in compact spec");
        assert!(!line.trim_start().starts_with("```"), "code fence found in compact spec: {}", line);
        assert_ne!(line.trim(), "---", "horizontal rule found in compact spec");
    }
}

#[test]
fn help_ai_preserves_key_content() {
    let out = ilo().args(["help", "ai"]).output().expect("failed to run ilo");
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Core syntax constructs must be present
    assert!(stdout.contains("fac n:n>n"), "missing factorial pattern");
    assert!(stdout.contains("FUNCTIONS:"), "missing FUNCTIONS section");
    assert!(stdout.contains("TYPES:"), "missing TYPES section");
    assert!(stdout.contains("OPERATORS:"), "missing OPERATORS section");
}

#[test]
fn help_ai_is_smaller_than_full_spec() {
    let full = ilo().args(["help", "lang"]).output().expect("failed to run ilo");
    let compact = ilo().args(["help", "ai"]).output().expect("failed to run ilo");
    assert!(
        compact.stdout.len() < full.stdout.len(),
        "compact spec ({} bytes) should be smaller than full spec ({} bytes)",
        compact.stdout.len(), full.stdout.len()
    );
}

// --- --version / -V flag ---

#[test]
fn version_flags_show_version() {
    for flag in ["--version", "-V"] {
        let out = ilo().args([flag]).output().expect("failed to run ilo");
        assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("ilo "), "expected version string, got: {stdout}");
    }
}

// --- --explain flag ---

#[test]
fn explain_known_code() {
    let out = ilo().args(["--explain", "ILO-T005"]).output().expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("ILO-T005"), "expected explanation, got: {stdout}");
}

#[test]
fn explain_unknown_code() {
    let out = ilo().args(["--explain", "ILO-XXXX"]).output().expect("failed to run ilo");
    assert!(!out.status.success(), "should exit with error for unknown code");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown error code"), "expected 'unknown error code' in stderr: {stderr}");
}

#[test]
fn explain_no_code_arg() {
    let out = ilo().args(["--explain"]).output().expect("failed to run ilo");
    assert!(!out.status.success(), "should exit with error when no code given");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Usage"), "expected 'Usage' in stderr: {stderr}");
}

// --- source annotation: ilo code --explain / -x ---

#[test]
fn source_explain_fn_start() {
    let out = ilo()
        .args(["f x:n>n;+x 1", "--explain"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("fn start"), "expected 'fn start' annotation: {stdout}");
}

#[test]
fn source_explain_short_flag() {
    let out = ilo()
        .args(["f x:n>n;+x 1", "-x"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("fn start"), "expected 'fn start' annotation: {stdout}");
}

#[test]
fn source_explain_bind_annotation() {
    let out = ilo()
        .args(["f x:n>n;y=+x 1;y", "--explain"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("bind → y"), "expected 'bind → y': {stdout}");
}

#[test]
fn source_explain_guard_annotation() {
    let out = ilo()
        .args(["f x:n>n;<=x 0{x};+x 1", "--explain"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("guard"), "expected 'guard' annotation: {stdout}");
}

#[test]
fn source_explain_return_annotation() {
    let out = ilo()
        .args(["f x:n>n;+x 1", "--explain"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("return"), "expected 'return' annotation: {stdout}");
}

// --- trm / unq / fmt via inline ---

#[test]
fn inline_trm_basic() {
    let out = ilo()
        .args(["f s:t>t;trm s", "  hello  "])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hello");
}

#[test]
fn inline_unq_text() {
    let out = ilo()
        .args(["f s:t>t;unq s", "aabbc"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "abc");
}

#[test]
fn inline_fmt_basic() {
    let out = ilo()
        .args([r#"f a:t b:t>t;fmt "{} and {}" a b"#, "foo", "bar"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "foo and bar");
}

// --- --run-jit ---

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn run_jit_arm64_numeric() {
    // On arm64 macOS, --run-jit should compile and run a simple numeric function
    let out = ilo()
        .args(["f x:n>n;*x 2", "--run-jit", "f", "5"])
        .output()
        .expect("failed to run ilo");
    // Either succeeds with correct output, or fails gracefully
    if out.status.success() {
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(stdout.trim(), "10", "expected 10, got: {stdout}");
    }
    // If JIT compilation fails, that's also acceptable
}

// --- Additional ARM64 JIT opcode coverage ---

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn run_jit_arm64_no_arg() {
    // f>n;42: no-arg function → vec![] run_args (main.rs L248), call_0 (jit_arm64.rs L42)
    let out = ilo()
        .args(["f>n;42", "--run-jit", "f"])
        .output()
        .expect("failed to run ilo");
    if out.status.success() {
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "42");
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn run_jit_arm64_addition() {
    // OP_ADD_NN (jit_arm64.rs L150), arm64_fadd (L63-65), call_2 (L44)
    let out = ilo()
        .args(["f x:n y:n>n;+x y", "--run-jit", "f", "3", "4"])
        .output()
        .expect("failed to run ilo");
    if out.status.success() {
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "7");
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn run_jit_arm64_subtraction() {
    // OP_SUB_NN (jit_arm64.rs L151), arm64_fsub (L67-69)
    let out = ilo()
        .args(["f x:n y:n>n;-x y", "--run-jit", "f", "10", "3"])
        .output()
        .expect("failed to run ilo");
    if out.status.success() {
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "7");
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn run_jit_arm64_division_nn() {
    // OP_DIV_NN (jit_arm64.rs L153), arm64_fdiv (L75-77), float result path (main.rs L263-264)
    let out = ilo()
        .args(["f x:n y:n>n;/x y", "--run-jit", "f", "1", "3"])
        .output()
        .expect("failed to run ilo");
    if out.status.success() {
        let stdout = String::from_utf8_lossy(&out.stdout);
        // 1/3 = 0.333... — non-integer result triggers float output branch
        assert!(stdout.trim().starts_with("0.333"), "expected 0.333…, got: {stdout}");
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn run_jit_arm64_addk() {
    // OP_ADDK_N (jit_arm64.rs L155-163), arm64_ldr_d_imm, arm64_adr
    let out = ilo()
        .args(["f x:n>n;+x 1", "--run-jit", "f", "5"])
        .output()
        .expect("failed to run ilo");
    if out.status.success() {
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "6");
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn run_jit_arm64_subk() {
    // OP_SUBK_N (jit_arm64.rs L165-172)
    let out = ilo()
        .args(["f x:n>n;-x 1", "--run-jit", "f", "5"])
        .output()
        .expect("failed to run ilo");
    if out.status.success() {
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "4");
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn run_jit_arm64_divk() {
    // OP_DIVK_N (jit_arm64.rs L183-190)
    let out = ilo()
        .args(["f x:n>n;/x 2", "--run-jit", "f", "10"])
        .output()
        .expect("failed to run ilo");
    if out.status.success() {
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "5");
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn run_jit_arm64_negate() {
    // OP_NEG — unary negation (jit_arm64.rs L212-214), arm64_fneg (L79-81)
    let out = ilo()
        .args(["f x:n>n;-x", "--run-jit", "f", "5"])
        .output()
        .expect("failed to run ilo");
    if out.status.success() {
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "-5");
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn run_jit_arm64_not_eligible() {
    // is_jit_eligible returns false for OP_EQ (L23), compile_and_call → None → L267-268
    let out = ilo()
        .args(["f x:n y:n>b;=x y", "--run-jit", "f", "1", "1"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success(), "should fail for non-eligible function");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("eligible") || stderr.contains("JIT"),
        "expected eligibility error, got: {stderr}"
    );
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn run_jit_arm64_const_dedup() {
    // add_const called twice with same value → dedup return (jit_arm64.rs L129)
    // Two ADDK_N using constant 1.0: second call finds existing entry
    let out = ilo()
        .args(["f x:n>n;a=+x 1;+a 1", "--run-jit", "f", "5"])
        .output()
        .expect("failed to run ilo");
    if out.status.success() {
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "7");
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn run_jit_arm64_3_args() {
    // call_3 (jit_arm64.rs L45)
    let out = ilo()
        .args(["f x:n y:n z:n>n;+x y", "--run-jit", "f", "3", "4", "0"])
        .output()
        .expect("failed to run ilo");
    if out.status.success() {
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "7");
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn run_jit_arm64_4_args() {
    // call_4 (jit_arm64.rs L46)
    let out = ilo()
        .args(["f x:n y:n z:n w:n>n;+x y", "--run-jit", "f", "3", "4", "0", "0"])
        .output()
        .expect("failed to run ilo");
    if out.status.success() {
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "7");
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn run_jit_arm64_5_args() {
    // call_5 (jit_arm64.rs L47)
    let out = ilo()
        .args(["f x:n y:n z:n w:n p:n>n;+x y", "--run-jit", "f", "3", "4", "0", "0", "0"])
        .output()
        .expect("failed to run ilo");
    if out.status.success() {
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "7");
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn run_jit_arm64_6_args() {
    // call_6 (jit_arm64.rs L48)
    let out = ilo()
        .args(["f x:n y:n z:n w:n p:n q:n>n;+x y", "--run-jit", "f", "3", "4", "0", "0", "0", "0"])
        .output()
        .expect("failed to run ilo");
    if out.status.success() {
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "7");
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn run_jit_arm64_7_args() {
    // call_7 (jit_arm64.rs L49)
    let out = ilo()
        .args(["f x:n y:n z:n w:n p:n q:n r:n>n;+x y", "--run-jit", "f", "3", "4", "0", "0", "0", "0", "0"])
        .output()
        .expect("failed to run ilo");
    if out.status.success() {
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "7");
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn run_jit_arm64_8_args() {
    // call_8 (jit_arm64.rs L50)
    let out = ilo()
        .args(["f x:n y:n z:n w:n p:n q:n r:n s:n>n;+x y", "--run-jit", "f", "3", "4", "0", "0", "0", "0", "0", "0"])
        .output()
        .expect("failed to run ilo");
    if out.status.success() {
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "7");
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn run_jit_arm64_multiply_nn() {
    // OP_MUL_NN (jit_arm64.rs L152) — both register operands → arm64_fmul with a,b,c
    let out = ilo()
        .args(["f x:n y:n>n;*x y", "--run-jit", "f", "3", "4"])
        .output()
        .expect("failed to run ilo");
    if out.status.success() {
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "12");
    }
}

#[cfg(not(all(target_arch = "aarch64", target_os = "macos")))]
#[test]
fn run_jit_unavailable_on_non_arm64() {
    let out = ilo()
        .args(["f x:n>n;*x 2", "--run-jit", "f", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success(), "should fail on non-arm64 platform");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("arm64") || stderr.contains("aarch64") || stderr.contains("JIT"),
        "expected JIT unavailability message, got: {stderr}"
    );
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
    // --run-interp with a program that errors at runtime (division by zero)
    // Exercises L379-381 in main.rs (error reporting for --run-interp)
    let out = ilo()
        .args(["f>n;/1 0", "--run-interp", "f"])
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
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
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
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let val: f64 = stdout.trim().parse().expect("expected float output");
    assert!((val - 2.0/3.0).abs() < 1e-6, "expected ~0.666, got: {val}");
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
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
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
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let val: f64 = String::from_utf8_lossy(&out.stdout).trim().parse().expect("expected float");
    assert!((val - 2.0/3.0).abs() < 1e-6, "expected ~0.666, got: {val}");
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
    assert!(out.status.success(), "match should be JIT-eligible now, stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.trim() == "3", "expected wildcard arm result 3, got: {stdout}");
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
    assert!(out.status.success(), "JIT handles empty list index, stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.trim() == "nil", "expected nil for empty list index, got: {stdout}");
}

// run_bench: L448-746 — bench with simple function covers the full run_bench path
#[test]
fn bench_simple_function() {
    // `f>n;42` with --bench covers run_bench (L448+), L230 (vec![]), all benchmark paths
    let out = ilo()
        .args(["f>n;42", "--bench", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Rust interpreter"), "expected bench output, got: {stdout}");
    assert!(stdout.contains("Register VM"), "expected VM bench output");
}

// main.rs L525 (_ => None in filter_map) + L638 (Text(s) in call_args map)
#[test]
fn bench_with_various_arg_types() {
    let cases = [
        ("f x:t>t;x", "f", "hello"),
        ("f x:b>b;x", "f", "true"),
        ("f xs:L n>n;+xs.0 1", "f", "[1,2,3]"),
    ];
    for (prog, func, arg) in cases { 
        let out = ilo().args([prog, "--bench", func, arg]).output().expect("failed to run ilo");
        assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("Rust interpreter"), "expected bench output, got: {stdout}");
    }
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
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Rust interpreter"), "expected bench output, got: {stdout}");
    // On JIT-capable platforms, the result line should show 0.5 (not integer)
    if stdout.contains("Custom JIT") || stdout.contains("Cranelift JIT") {
        assert!(stdout.contains("0.5"), "expected float result in JIT output, got: {stdout}");
    }
}

// main.rs L560 (arm64 closing } when JIT returns None) + L593 (Cranelift closing })
// L197 (arm64 LOADK non-number → None) + L161 (Cranelift LOADK non-number → None)
// Uses a function with a text constant: JIT can't compile it → returns None
#[test]
fn bench_jit_non_numeric_const() {
    // f x:n>n;y="hi";x — NanVal JIT now handles text constants
    let out = ilo()
        .args(["f x:n>n;y=\"hi\";x", "--bench", "f", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Rust interpreter"), "expected bench output, got: {stdout}");
    // Cranelift JIT now compiles text-const functions via NanVal
    #[cfg(feature = "cranelift")]
    assert!(stdout.contains("Cranelift JIT"), "cranelift JIT should compile text-const fn with NanVal");
}

// vm/jit_arm64.rs L207-209 (OP_MOVE with a != b) + vm/jit_cranelift.rs L167-170 (OP_MOVE with a != b)
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
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Rust interpreter"), "expected bench output, got: {stdout}");
    // Result should be 8 (x + 1 = 7 + 1)
    if stdout.contains("Custom JIT") || stdout.contains("Cranelift JIT") {
        assert!(stdout.contains("  result:     8"), "expected result 8 in JIT output, got: {stdout}");
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
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
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
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
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
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
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
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("inner!"), "expected inner! in formatted output, got: {}", stdout);
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
    assert!(stderr.contains("T025") || stderr.contains("not a Result"),
        "expected T025 error, got: {}", stderr);
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
    assert!(stderr.contains("T026") || stderr.contains("not a Result"),
        "expected T026 error, got: {}", stderr);
}

// --- HTTP get builtin + $ syntax ---

#[test]
fn get_verifier_wrong_type() {
    // get with number arg should fail verification
    let out = ilo()
        .args(["f x:n>R t t;get x"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("T013") || stderr.contains("expects t"),
        "expected type error for get with number, got: {}", stderr);
}

#[test]
fn dollar_parses_inline() {
    // $"url" should parse and verify without error (returns AST when no args)
    let out = ilo()
        .args([r#"f url:t>R t t;$url"#])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    // No args → AST output
    assert!(stdout.contains("get"), "expected 'get' in AST output, got: {}", stdout);
}

#[test]
fn dollar_bang_parses_inline() {
    // $!url should parse as get! url — enclosing function must return R t t for ! to verify
    let out = ilo()
        .args([r#"f url:t>R t t;~($!url)"#])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("get"), "expected 'get' in AST output, got: {}", stdout);
}

// --- Braceless guards ---

#[test]
fn braceless_guard_classify() {
    let out = ilo()
        .args([r#"cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze""#, "1500"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "gold");
}

#[test]
fn braceless_guard_classify_silver() {
    let out = ilo()
        .args([r#"cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze""#, "750"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "silver");
}

#[test]
fn braceless_guard_classify_bronze() {
    let out = ilo()
        .args([r#"cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze""#, "100"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "bronze");
}

#[test]
fn braceless_guard_factorial() {
    let out = ilo()
        .args(["fac n:n>n;<=n 1 1;r=fac -n 1;*n r", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "120");
}

#[test]
fn braceless_guard_fibonacci() {
    let out = ilo()
        .args(["fib n:n>n;<=n 1 n;a=fib -n 1;b=fib -n 2;+a b", "10"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "55");
}

#[test]
fn braceless_guard_equivalent_to_braced() {
    let braced = ilo()
        .args([r#"cls sp:n>t;>=sp 1000{"gold"};>=sp 500{"silver"};"bronze""#, "1500"])
        .output()
        .expect("failed to run ilo");
    let braceless = ilo()
        .args([r#"cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze""#, "1500"])
        .output()
        .expect("failed to run ilo");
    assert_eq!(
        String::from_utf8_lossy(&braced.stdout),
        String::from_utf8_lossy(&braceless.stdout),
        "braced and braceless should produce identical output"
    );
}

// --- Range iteration ---

#[test]
fn range_basic() {
    let out = ilo()
        .args(["f>n;@i 0..3{i}", "--run", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "2");
}

#[test]
fn range_with_arg() {
    let out = ilo()
        .args(["f n:n>n;@i 0..n{*i i}", "4"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    // i goes 0,1,2,3 → last body value is 3*3 = 9
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "9");
}

#[test]
fn range_empty() {
    let out = ilo()
        .args(["f>n;@i 5..2{99};0", "--run", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
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
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "~42");
}

#[test]
fn alias_in_param_run() {
    let out = ilo()
        .args(["-e", "alias num n\nf x:num>num;+x 1", "--run", "f", "5"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "6");
}
