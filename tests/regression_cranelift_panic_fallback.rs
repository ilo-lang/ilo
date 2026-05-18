//! Regression: a panic inside the Cranelift JIT (most notably the AArch64
//! near-call relocation assertion in cranelift-jit 0.116,
//! `compiled_blob.rs:90` — `(diff >> 26 == -1) || (diff >> 26 == 0)`) must
//! be caught and surfaced as a stderr breadcrumb + engine fallback, not a
//! process crash.
//!
//! Hard repro of the AArch64 bug is non-deterministic and platform-specific
//! (depends on JIT code-cache vs runtime memory layout), so these tests use
//! the debug-build env-var hook `ILO_FORCE_JIT_PANIC=1` which raises a
//! synthetic panic at the same call site. The release binary does not have
//! the hook — the `cfg(debug_assertions)` guard trips it out — so this
//! cannot affect production users.
//!
//! Coverage: explicit `--jit` falls back to the bytecode VM, since the
//! user opted into a JIT engine and VM is the closest non-JIT tier.
//! (Post-#390 the default engine is the bytecode VM, so the default path
//! never invokes the JIT and the synthetic-panic hook cannot fire on it.)
//!
//! Gated on `cfg(debug_assertions)`: the env-var hook in
//! `vm::jit_cranelift::check_force_panic_env` is only compiled in debug
//! builds. In a release-mode test the hook is absent, so the synthetic
//! panic never fires and the assertions would misfire.

#![cfg(debug_assertions)]

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

// Note: pre-#390 there was a `cranelift_panic_default_falls_back_to_interpreter`
// test here. Post-#390 the default engine is the bytecode VM and never invokes
// the JIT, so the synthetic-panic hook cannot fire on the default path. The
// `--jit` opt-in path is covered by
// `cranelift_panic_explicit_engine_falls_back_to_vm` below.

#[test]
fn cranelift_panic_explicit_engine_falls_back_to_vm() {
    let out = ilo()
        .args(["--jit", "f x:n>n;*x 2", "f", "5"])
        .env("ILO_FORCE_JIT_PANIC", "1")
        .output()
        .expect("failed to run ilo");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        out.status.success(),
        "--jit should fall back to VM after JIT panic. \
         stdout={stdout:?} stderr={stderr:?} status={:?}",
        out.status.code()
    );
    assert!(
        stdout.trim() == "10",
        "VM fallback should produce f(5)=10, got stdout={stdout:?}"
    );
    assert!(
        stderr.contains("Cranelift JIT panicked"),
        "stderr breadcrumb missing, got {stderr:?}"
    );
    assert!(
        stderr.contains("falling back to bytecode VM"),
        "explicit-cranelift breadcrumb should mention VM fallback, got {stderr:?}"
    );
}

/// The breadcrumb must include the panic payload so the upstream issue
/// (AArch64 relocation assertion, etc.) is searchable in production logs
/// rather than being collapsed into a generic message. Exercised via the
/// `--jit` opt-in path since the default engine no longer invokes the JIT.
#[test]
fn cranelift_panic_breadcrumb_includes_payload() {
    let out = ilo()
        .args(["--jit", "f x:n>n;*x 2", "f", "5"])
        .env("ILO_FORCE_JIT_PANIC", "1")
        .output()
        .expect("failed to run ilo");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "fallback should succeed");
    assert!(
        stderr.contains("ILO_FORCE_JIT_PANIC") || stderr.contains("synthetic cranelift panic"),
        "breadcrumb should include the panic payload, got {stderr:?}"
    );
}
