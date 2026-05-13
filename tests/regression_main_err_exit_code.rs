// Regression test: a program whose entry function returns `Value::Err(_)`
// from the plain `~`/`^` arm (no `!!` panic-unwrap) must exit non-zero, with
// the error formatted to stderr in plain mode or wrapped as
// `{"error": {...}}` on stdout in JSON mode.
//
// Background:
//
// PR #248 fixed the half of "errors print and exit nonzero" where the user
// opts into crash semantics via `!!` (panic-unwrap). The other half — a plain
// `^"reason"` returned from `main` — was still printed and the process exited
// 0, which silently broke CI / shell pipelines that try to detect program
// failure by exit code.
//
// The fix is in `src/main.rs`: each of the four CLI exec paths
// (`run_vm_with_provider`, `run_interp_with_provider`, `run_default`,
// `run_cranelift_engine`) now inspects the returned `Value` and exits 1 if it
// is `Value::Err(_)`. `print_value` also routes plain-mode err output to
// stderr (matching `report_diagnostic`'s stream convention); JSON output
// stays on stdout so machine consumers can parse uniformly and discriminate
// on the top-level `error` / `ok` key.
//
// All tests cross-engine (tree, VM, Cranelift) so a divergence between
// backends shows up in CI.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

// A `main` that returns `Value::Err`. Signature `>R n t` = Result<n, t>, body
// returns `^"oh no"` — the Err variant.
const ERR_SRC: &str = "m>R n t;^\"oh no\"";
const OK_SRC: &str = "m>R n t;~7";

// ── Plain mode: Value::Err exits 1 ─────────────────────────────────────────

fn assert_err_exit_plain(engine: &str) {
    let out = ilo()
        .args([ERR_SRC, engine])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "{engine}: expected non-zero exit for Value::Err from main, got success. stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "{engine}: expected exit code 1, got {:?}",
        out.status.code(),
    );
    // Plain-mode err is routed to stderr so stdout-piping callers don't see
    // an err value mixed in with a successful run's output.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.trim().is_empty(),
        "{engine}: expected empty stdout for Value::Err, got {stdout:?}",
    );
    assert!(
        stderr.contains("oh no"),
        "{engine}: expected err text on stderr, got {stderr:?}",
    );
}

#[test]
fn main_err_exits_one_tree() {
    assert_err_exit_plain("--run-tree");
}

#[test]
fn main_err_exits_one_vm() {
    assert_err_exit_plain("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn main_err_exits_one_cranelift() {
    assert_err_exit_plain("--run-cranelift");
}

// ── JSON mode: Value::Err exits 1 with structured envelope on stdout ───────

fn assert_err_exit_json(engine: &str) {
    let out = ilo()
        .args([ERR_SRC, engine, "-j"])
        .output()
        .expect("failed to run ilo");
    assert_eq!(
        out.status.code(),
        Some(1),
        "{engine} -j: expected exit 1, got {:?}. stdout={:?} stderr={:?}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // JSON consumers parse stdout — the envelope must arrive there even on
    // failure, so they can discriminate on the top-level key.
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("{engine} -j: stdout was not valid JSON ({e}): {stdout:?}"));
    let err_obj = parsed
        .get("error")
        .unwrap_or_else(|| panic!("{engine} -j: expected `error` key in {parsed:?}"));
    assert_eq!(
        err_obj.get("phase").and_then(|v| v.as_str()),
        Some("program")
    );
    assert_eq!(err_obj.get("value").and_then(|v| v.as_str()), Some("oh no"));
}

#[test]
fn main_err_exits_one_json_tree() {
    assert_err_exit_json("--run-tree");
}

#[test]
fn main_err_exits_one_json_vm() {
    assert_err_exit_json("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn main_err_exits_one_json_cranelift() {
    assert_err_exit_json("--run-cranelift");
}

// ── OK return still exits 0 (no regression on the happy path) ──────────────

fn assert_ok_exit(engine: &str) {
    let out = ilo()
        .args([OK_SRC, engine])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "{engine}: expected success exit for `~7`, got {:?}. stdout={:?} stderr={:?}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Top-level Value::Ok prints bare (no `~` wrapper) — symmetric with the
    // `^e`-to-stderr split this test file already pins. See
    // regression_main_ok_stdout_bare.rs for the full contract.
    assert!(
        stdout.contains('7') && !stdout.contains('~'),
        "{engine}: expected bare `7` on stdout (no `~` prefix), got {stdout:?}",
    );
}

#[test]
fn main_ok_exits_zero_tree() {
    assert_ok_exit("--run-tree");
}

#[test]
fn main_ok_exits_zero_vm() {
    assert_ok_exit("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn main_ok_exits_zero_cranelift() {
    assert_ok_exit("--run-cranelift");
}

// ── Default engine path (no --run-* flag) also surfaces the err exit code ──
//
// `dispatch_run`'s default branch routes through `run_default`, which tries
// Cranelift JIT first and falls back to the interpreter. Both fallback paths
// must observe the program_exit_code rule.

#[test]
fn main_err_exits_one_default_engine() {
    // The default engine path (no --run-* flag) only runs a program when an
    // entry function can be resolved — for inline source with no rest-arg
    // that falls through to the legacy AST-dump branch. Write to a temp file
    // and pass the function name so we exercise `run_default` proper, which
    // routes through Cranelift JIT with an interpreter fallback.
    // Unique per-pid temp file so concurrent `cargo test` runs of different
    // test binaries don't race on the same path. (Within this binary the
    // test is single-instance, but the harness can run multiple binaries in
    // parallel.)
    let path = std::env::temp_dir().join(format!(
        "ilo_regression_main_err_default_{}.ilo",
        std::process::id()
    ));
    std::fs::write(&path, ERR_SRC).expect("write temp ilo file");
    let out = ilo()
        .arg(&path)
        .arg("m")
        .output()
        .expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        out.status.code(),
        Some(1),
        "default engine: expected exit 1 for Value::Err from main, got {:?}. stdout={:?} stderr={:?}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}
