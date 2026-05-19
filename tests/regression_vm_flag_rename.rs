// Regression tests for the `--run-vm` -> `--vm` rename.
//
// `--vm` is the new canonical spelling: symmetric in shape with `--jit` and
// `--run-llvm`, where the flag names the engine, not the action. The old
// `--run-vm` spelling stays for one release behind clap's `visible_alias`
// plus a pre-parse scan in `extract_run_engine_flag`, and every invocation
// emits a one-shot stderr deprecation hint. Hard removal lands in 0.13.0.
//
// What this test pins:
//   1. `--vm` runs the program and prints the expected result.
//   2. `--run-vm` still runs the program and prints the same result
//      (backwards-compat guard).
//   3. `--run-vm` emits the deprecation hint on stderr.
//   4. `--vm` does NOT emit the deprecation hint.
//   5. The hint mentions both the old and new spelling and the 0.13.0
//      removal cutoff so the message is greppable by carry-forward tooling.
//   6. `--vm` and `--run-vm` produce byte-identical stdout for the same
//      program (the alias has no behavioural divergence).

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

const SRC: &str = "main x:n>n;y=*x 2;+y 1";

#[test]
fn vm_flag_canonical_runs_and_silent() {
    let (stdout, stderr, code) = run_args(&["--vm", SRC, "5"]);
    assert_eq!(code, 0, "exit code; stderr={stderr}");
    assert_eq!(stdout, "11");
    assert!(
        !stderr.contains("--run-vm"),
        "canonical --vm must not emit the deprecation hint; got stderr={stderr}"
    );
}

#[test]
fn run_vm_alias_still_runs() {
    let (stdout, stderr, code) = run_args(&["--run-vm", SRC, "5"]);
    assert_eq!(code, 0, "exit code; stderr={stderr}");
    assert_eq!(stdout, "11");
}

#[test]
fn run_vm_alias_emits_deprecation_hint() {
    let (_, stderr, _) = run_args(&["--run-vm", SRC, "5"]);
    assert!(
        stderr.contains("--run-vm") && stderr.contains("--vm"),
        "hint should reference both spellings; got: {stderr}"
    );
    assert!(
        stderr.contains("0.13.0"),
        "hint should name the removal cutoff so tooling can grep for it; got: {stderr}"
    );
}

#[test]
fn vm_and_run_vm_produce_identical_stdout() {
    let (stdout_vm, _, code_vm) = run_args(&["--vm", SRC, "5"]);
    let (stdout_alias, _, code_alias) = run_args(&["--run-vm", SRC, "5"]);
    assert_eq!(code_vm, 0);
    assert_eq!(code_alias, 0);
    assert_eq!(
        stdout_vm, stdout_alias,
        "alias must be behaviourally identical to canonical"
    );
}

#[test]
fn run_vm_subcommand_path_also_emits_hint() {
    // `ilo run <file> --run-vm` exercises the clap-driven `run` subcommand
    // dispatch path rather than the bare-arg dispatcher. The hint must fire
    // on this path too — otherwise scripts that use `ilo run` would silently
    // miss the migration nudge.
    let (stdout, stderr, code) = run_args(&["run", "--run-vm", SRC, "main", "5"]);
    assert_eq!(code, 0, "exit code; stderr={stderr}");
    assert_eq!(stdout, "11");
    assert!(
        stderr.contains("--run-vm") && stderr.contains("--vm"),
        "subcommand path must also emit the hint; got: {stderr}"
    );
}
