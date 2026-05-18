// Regression: `--jit` is the canonical CLI flag for opt-in Cranelift JIT.
// The old spellings `--run-cranelift` and `--cranelift` were removed in a
// clean break (no deprecation cycle). The unknown-flag guard (#366) must
// reject them loudly so agents who reach for the old names get an immediate
// error rather than silent fallthrough into mis-parsed positional args.
//
// We assert:
//   1. `ilo --jit <code> main <n>` runs the program on the JIT path and
//      produces the expected output.
//   2. `ilo --run-cranelift <code> ...` is rejected by the unknown-flag guard.
//   3. `ilo --cranelift <code> ...` is rejected the same way.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_args(args: &[&str]) -> (String, String, i32) {
    let out = ilo().args(args).output().expect("failed to run ilo");
    (
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.code().unwrap_or(-1),
    )
}

const NUMERIC_SRC: &str = "main x:n>n;y=*x 2;+y 1";

#[test]
fn jit_flag_runs_program() {
    let (out, stderr, code) = run_args(&["--jit", NUMERIC_SRC, "main", "7"]);
    assert_eq!(code, 0, "--jit exit (stderr={stderr})");
    assert_eq!(out, "15");
}

#[test]
fn run_cranelift_flag_rejected() {
    let (_, stderr, code) = run_args(&["--run-cranelift", NUMERIC_SRC, "main", "7"]);
    assert_eq!(code, 1, "--run-cranelift must be rejected, stderr={stderr}");
    assert!(
        stderr.contains("unrecognised flag") && stderr.contains("--run-cranelift"),
        "expected unknown-flag error mentioning '--run-cranelift', got: {stderr}",
    );
}

#[test]
fn cranelift_flag_rejected() {
    let (_, stderr, code) = run_args(&["--cranelift", NUMERIC_SRC, "main", "7"]);
    assert_eq!(code, 1, "--cranelift must be rejected, stderr={stderr}");
    assert!(
        stderr.contains("unrecognised flag") && stderr.contains("--cranelift"),
        "expected unknown-flag error mentioning '--cranelift', got: {stderr}",
    );
}
