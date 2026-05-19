// Regression coverage for the v0.11.7 rerun8 unknown-flag silent-consume trap.
//
// Reproduces: `ilo main.ilo --engine tree` and `ilo main.ilo --foo` used to
// silently consume the unknown long flag as a positional arg (because
// `Cli::args` and `RunArgs::rest` use `trailing_var_arg = true,
// allow_hyphen_values = true` so clap collects every unrecognised
// hyphen-prefixed token into the positional vec). The program then ran
// with the wrong arity and surfaced as misleading `ILO-R012 no functions
// defined` or `ILO-R004 main: expected N args, got N+1`. Six rerun8 personas
// (ab-tester, routing-tsp, content-mod, qa-tester, interactive-cli,
// security-researcher) independently burned minutes on this trap.
//
// The contract:
//   1. Any clean long-flag shape (`--word`, `--word-with-dashes`, or the
//      `--word=value` equals form) that isn't a recognised flag is rejected
//      upfront with a clear "unrecognised flag" message and exit 1.
//   2. To pass a hyphen-prefixed token as a literal arg, the user inserts
//      `--` first: `ilo main.ilo -- --foo` or `ilo main.ilo -- --foo=bar`.
//   3. All recognised long flags (`--vm`, `--bench`, etc.) still work.
//   4. Holds across every engine (default, --vm, --jit), the
//      bare-positional dispatcher AND the `run` subcommand path.

use std::io::Write;
use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_args(args: &[&str]) -> (i32, String, String) {
    let out = ilo()
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn ilo: {e}"));
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    (code, stdout, stderr)
}

fn assert_unrecognised(out: (i32, String, String), flag: &str) {
    let (code, stdout, stderr) = out;
    assert_eq!(
        code, 1,
        "expected exit 1 for unrecognised flag `{flag}`; stdout=\n{stdout}\nstderr=\n{stderr}"
    );
    let combined = format!("{stdout}\n{stderr}");
    assert!(
        combined.contains("unrecognised flag") && combined.contains(flag),
        "expected `unrecognised flag` + `{flag}` in output; got:\n{combined}"
    );
    assert!(
        combined.contains("--"),
        "expected `--` separator hint in output; got:\n{combined}"
    );
}

// Write a temp .ilo file with `main:n>n;42` (no required args). Returns the
// path. We use a unique-per-test path so parallel test runs don't collide.
fn temp_main(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir();
    let p = dir.join(format!("ilo_unknown_flag_guard_{tag}.ilo"));
    let mut f = std::fs::File::create(&p).expect("create temp .ilo");
    f.write_all(b"main>n;42\n").expect("write temp .ilo");
    p
}

// ── (a) `--engine tree` rejected on bare positional path ─────────────────────

#[test]
fn bare_unknown_engine_flag_rejected() {
    let p = temp_main("bare_engine");
    let path_str = p.to_str().unwrap();
    assert_unrecognised(run_args(&[path_str, "--engine", "tree"]), "--engine");
}

#[test]
fn bare_unknown_foo_flag_rejected() {
    let p = temp_main("bare_foo");
    let path_str = p.to_str().unwrap();
    assert_unrecognised(run_args(&[path_str, "--foo"]), "--foo");
}

#[test]
fn bare_unknown_hyphenated_flag_rejected() {
    let p = temp_main("bare_hyph");
    let path_str = p.to_str().unwrap();
    assert_unrecognised(run_args(&[path_str, "--some-flag"]), "--some-flag");
}

#[test]
fn bare_unknown_flag_in_source_position_rejected() {
    // `ilo --engine main.ilo` — flag in args[1] (the source slot). Without
    // the guard this would be treated as inline code and surface as a lex
    // error rather than a clear flag-shape diagnostic.
    assert_unrecognised(run_args(&["--engine", "tree"]), "--engine");
}

// ── (b) `--engine tree` rejected on `run` subcommand path ────────────────────

#[test]
fn run_subcmd_unknown_engine_flag_rejected() {
    let p = temp_main("run_engine");
    let path_str = p.to_str().unwrap();
    assert_unrecognised(run_args(&["run", path_str, "--engine", "tree"]), "--engine");
}

#[test]
fn run_subcmd_unknown_foo_flag_rejected() {
    let p = temp_main("run_foo");
    let path_str = p.to_str().unwrap();
    assert_unrecognised(run_args(&["run", path_str, "--foo"]), "--foo");
}

// ── (c) `--` separator escape works ──────────────────────────────────────────

#[test]
fn dash_dash_separator_passes_through_hyphen_args_bare() {
    // Inline program that takes a single text arg and returns it. With `--`
    // the `--foo` token should reach the program as literal data — not be
    // rejected as an unknown flag.
    let (code, stdout, stderr) = run_args(&["f x:t>t;x", "f", "--", "--foo"]);
    assert_eq!(
        code, 0,
        "expected exit 0 with `--` separator; stdout=\n{stdout}\nstderr=\n{stderr}"
    );
    let combined = format!("{stdout}\n{stderr}");
    assert!(
        !combined.contains("unrecognised flag"),
        "should not error on flags after `--`; got:\n{combined}"
    );
}

#[test]
fn dash_dash_separator_passes_through_hyphen_args_run_subcmd() {
    let (code, stdout, stderr) = run_args(&["run", "f x:t>t;x", "f", "--", "--foo"]);
    assert_eq!(
        code, 0,
        "expected exit 0 with `--` separator on run subcommand; stdout=\n{stdout}\nstderr=\n{stderr}"
    );
    let combined = format!("{stdout}\n{stderr}");
    assert!(!combined.contains("unrecognised flag"), "got:\n{combined}");
}

// ── (d) recognised flags still work ──────────────────────────────────────────

#[test]
fn recognised_run_vm_flag_still_works() {
    let p = temp_main("known_run_vm");
    let path_str = p.to_str().unwrap();
    let (code, stdout, stderr) = run_args(&["--vm", path_str]);
    assert_eq!(
        code, 0,
        "--vm should still parse and run; stdout=\n{stdout}\nstderr=\n{stderr}"
    );
}

#[test]
fn removed_run_tree_flag_hits_unknown_flag_guard() {
    // --run-tree / --run were removed from the public CLI surface as part
    // of the tree-walker soft-deprecation. They must now hit the unknown-flag
    // guard with a clean error rather than being silently accepted as a
    // positional argument.
    let p = temp_main("removed_run_tree");
    let path_str = p.to_str().unwrap();
    let (code, _, stderr) = run_args(&["--run-tree", path_str]);
    assert_ne!(code, 0, "--run-tree should be rejected; stderr={stderr}");
    assert!(
        stderr.contains("--run-tree") || stderr.contains("unknown"),
        "stderr should name the removed flag; got: {stderr}"
    );
}

#[test]
fn recognised_bench_flag_still_works() {
    // `--bench` triggers benchmark mode; we just need it to NOT be rejected.
    let p = temp_main("known_bench");
    let path_str = p.to_str().unwrap();
    let (_, _, stderr) = run_args(&[path_str, "--bench"]);
    assert!(
        !stderr.contains("unrecognised flag"),
        "`--bench` should not be rejected as unknown; stderr={stderr}"
    );
}

// ── (e) all engines respect the guard ────────────────────────────────────────

#[test]
fn unknown_flag_rejected_under_run_tree() {
    let p = temp_main("eng_tree");
    let path_str = p.to_str().unwrap();
    assert_unrecognised(run_args(&["--vm", path_str, "--foo"]), "--foo");
}

#[test]
fn unknown_flag_rejected_under_run_vm() {
    let p = temp_main("eng_vm");
    let path_str = p.to_str().unwrap();
    assert_unrecognised(run_args(&["--vm", path_str, "--foo"]), "--foo");
}

#[cfg(feature = "cranelift")]
#[test]
fn unknown_flag_rejected_under_run_cranelift() {
    let p = temp_main("eng_cl");
    let path_str = p.to_str().unwrap();
    assert_unrecognised(run_args(&["--jit", path_str, "--foo"]), "--foo");
}

#[test]
fn unknown_flag_rejected_under_run_subcmd_run_vm() {
    let p = temp_main("subcmd_vm");
    let path_str = p.to_str().unwrap();
    assert_unrecognised(run_args(&["run", path_str, "--vm", "--foo"]), "--foo");
}

// ── (f) inline source path also guarded ──────────────────────────────────────

#[test]
fn inline_source_unknown_flag_in_tail_rejected() {
    // Inline program `main:n>n;42` followed by stray `--engine`.
    assert_unrecognised(run_args(&["main>n;42", "--engine", "tree"]), "--engine");
}

// ── data shapes that look hyphen-ish but are NOT flags must pass through ────

#[test]
fn negative_number_arg_not_treated_as_flag() {
    // `-3` is a negative number arg, not a flag. Must pass through.
    let (code, stdout, stderr) = run_args(&["neg x:n>n;-0 x", "neg", "-3"]);
    let combined = format!("{stdout}\n{stderr}");
    assert!(
        !combined.contains("unrecognised flag"),
        "negative number should not be flagged as unknown flag; got:\n{combined}"
    );
    assert_eq!(code, 0, "exit 0 expected; got combined:\n{combined}");
}

#[test]
fn equals_form_unknown_flag_rejected_bare() {
    // `--key=value` used to slip past the guard (the shape check excluded
    // tokens containing `=`) and got silently consumed as a positional,
    // surfacing as a misleading ILO-R012/R004 downstream. The guard now
    // splits on `=` and checks the `--key` head, so this form is rejected
    // the same way as the space-separated form.
    let p = temp_main("bare_engine_eq");
    let path_str = p.to_str().unwrap();
    assert_unrecognised(run_args(&[path_str, "--engine=tree"]), "--engine=tree");
}

#[test]
fn equals_form_unknown_foo_flag_rejected_bare() {
    let p = temp_main("bare_foo_eq");
    let path_str = p.to_str().unwrap();
    assert_unrecognised(run_args(&[path_str, "--foo=bar"]), "--foo=bar");
}

#[test]
fn equals_form_unknown_flag_rejected_run_subcmd() {
    let p = temp_main("run_engine_eq");
    let path_str = p.to_str().unwrap();
    assert_unrecognised(
        run_args(&["run", path_str, "--engine=tree"]),
        "--engine=tree",
    );
}

#[test]
fn equals_form_after_dash_dash_passes_through() {
    // `--` separator still escapes everything that follows, including the
    // equals form.
    let (code, stdout, stderr) = run_args(&["f x:t>t;x", "f", "--", "--key=val"]);
    assert_eq!(
        code, 0,
        "expected exit 0 with `--` separator on equals form; stdout=\n{stdout}\nstderr=\n{stderr}"
    );
    assert!(
        !stderr.contains("unrecognised flag"),
        "should not error on equals form after `--`; stderr={stderr}"
    );
}

#[test]
fn non_flag_equals_data_passes_through() {
    // `key=value` (no leading `--`) is data, not a flag. Must pass through.
    let (_, _, stderr) = run_args(&["f x:t>t;x", "f", "key=value"]);
    assert!(
        !stderr.contains("unrecognised flag"),
        "`key=value` shape should pass through as data; stderr={stderr}"
    );
}
