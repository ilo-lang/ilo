// Regression: `prod xs` must execute on the Cranelift JIT without falling
// back to the bytecode VM.
//
// Before this fix, `register_helpers` in `src/vm/jit_cranelift.rs` was missing
// the `("jit_prod", jit_prod as *const u8)` entry. The JIT codegen emitted a
// call to the `jit_prod` symbol (the dispatch arm + `declare_helper` for
// `jit_prod` were both wired up), but the JITBuilder had no symbol resolver
// for it, so the module emitted
//
//   [ilo:jit-fallback] Cranelift JIT panicked (can't resolve symbol jit_prod);
//   falling back to bytecode VM.
//
// Output was still correct (the VM has its own working `prod`), but the JIT
// silently bailed and acceleration was defeated. The cross-engine tests in
// `regression_prod_cprod.rs` already pinned the numeric result on `--jit`;
// they passed because the fallback masked the bug. This file adds the
// missing stderr-aware assertion to catch the regression directly.
//
// Other math-list reducers (`sum`, `avg`, `min`, `max`) already have their
// symbol entries, so this test pattern can be generalised if the same gap
// reappears elsewhere.

#![cfg(feature = "cranelift")]

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

/// Run `ilo <src> <engine> <entry>` and return (stdout, stderr). Asserts the
/// process exits successfully so callers can focus on the streams themselves.
fn run(engine: &str, src: &str, entry: &str) -> (String, String) {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to spawn ilo");
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "ilo {engine} {src:?} failed: stdout={stdout} stderr={stderr}"
    );
    (stdout, stderr)
}

fn assert_no_jit_fallback(stderr: &str, ctx: &str) {
    assert!(
        !stderr.contains("[ilo:jit-fallback]"),
        "JIT unexpectedly fell back to VM ({ctx}); stderr was:\n{stderr}"
    );
    // Catch the underlying cause too, in case the fallback marker is ever
    // renamed but the symbol-resolution panic remains.
    assert!(
        !stderr.contains("can't resolve symbol jit_prod"),
        "JIT failed to resolve jit_prod ({ctx}); stderr was:\n{stderr}"
    );
}

#[test]
fn prod_runs_on_jit_without_fallback() {
    let src = "f>n;prod [2.0 3.0 4.0]";
    let (stdout, stderr) = run("--jit", src, "f");
    assert_eq!(stdout, "24", "prod [2,3,4] should equal 24 on JIT");
    assert_no_jit_fallback(&stderr, "prod basic floats");
}

#[test]
fn prod_integers_on_jit_without_fallback() {
    let src = "f>n;prod [1, 2, 3, 4]";
    let (stdout, stderr) = run("--jit", src, "f");
    assert_eq!(stdout, "24", "prod [1..4] should equal 24 on JIT");
    assert_no_jit_fallback(&stderr, "prod integers");
}

#[test]
fn prod_empty_on_jit_without_fallback() {
    // Multiplicative identity: prod [] = 1.
    let src = "f>n;prod []";
    let (stdout, stderr) = run("--jit", src, "f");
    assert_eq!(stdout, "1", "prod [] should equal 1 on JIT");
    assert_no_jit_fallback(&stderr, "prod empty");
}

#[test]
fn prod_cross_engine_parity_with_no_fallback() {
    // Pin parity across the default engine, the explicit VM, and the JIT
    // for the same expression, and confirm the JIT path stays on the JIT.
    let src = "f>n;prod [2.0 3.0 4.0]";

    // Default engine (no flag): exercises the entry-pick path agents hit
    // most often.
    let default_out = ilo()
        .args([src, "f"])
        .output()
        .expect("failed to spawn ilo");
    assert!(default_out.status.success(), "default engine failed");
    let default_stdout = String::from_utf8_lossy(&default_out.stdout)
        .trim()
        .to_string();

    let (vm_stdout, _) = run("--run-vm", src, "f");
    let (jit_stdout, jit_stderr) = run("--jit", src, "f");

    assert_eq!(default_stdout, "24", "default-engine result");
    assert_eq!(vm_stdout, "24", "vm result");
    assert_eq!(jit_stdout, "24", "jit result");
    assert_no_jit_fallback(&jit_stderr, "cross-engine parity");
}

#[test]
fn prod_composed_with_sum_on_jit_without_fallback() {
    // Exercises both helper-call numeric opcodes in one compiled chunk so
    // we'd catch a regression where adding more symbols breaks the table.
    let src = "f>n;+(prod [2, 3, 4]) (sum [1, 2, 3])";
    let (stdout, stderr) = run("--jit", src, "f");
    assert_eq!(stdout, "30", "prod + sum should equal 30 on JIT");
    assert_no_jit_fallback(&stderr, "prod + sum composed");
}
