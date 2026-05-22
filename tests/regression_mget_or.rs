// Regression tests for the `mget-or m k default` builtin (ILO-42).
//
// `mget-or` is the defaulted map-lookup counterpart to `mget`. It returns
// the value at key `k`, or `default` if the key is absent — never nil.
// The default's type must match the map's value type (verifier ILO-T013).
//
// Coverage:
//   - text-key map: hit and miss paths
//   - numeric-key map: hit and miss paths
//   - text-valued map with text default
//   - default-type mismatch rejected at verify time (ILO-T013)
//   - tree-bridge dispatch consistent with interpreter (--run-vm)

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_inline(engine: &str, src: &str, entry: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "ilo_mget_or_{}_{}.ilo",
        std::process::id(),
        seq
    ));
    std::fs::write(&path, src).unwrap();
    let out = ilo()
        .args([path.to_str().unwrap(), engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_inline_expect_err(engine: &str, src: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "ilo_mget_or_err_{}_{}.ilo",
        std::process::id(),
        seq
    ));
    std::fs::write(&path, src).unwrap();
    let out = ilo()
        .args([path.to_str().unwrap(), engine])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected failure but ilo {engine} succeeded for `{src}`"
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

fn check(engine: &str) {
    // Text-key numeric-value map: hit
    assert_eq!(
        run_inline(engine, r#"f>n;m=mset mmap "k" 5;mget-or m "k" 0"#, "f"),
        "5",
        "mget-or text-key hit [{engine}]"
    );
    // Text-key numeric-value map: miss
    assert_eq!(
        run_inline(engine, r#"f>n;m=mmap;mget-or m "missing" 99"#, "f"),
        "99",
        "mget-or text-key miss [{engine}]"
    );
    // Text-key text-value map: miss returns text default
    assert_eq!(
        run_inline(engine, r#"f>t;m=mmap;mget-or m "k" "fallback""#, "f"),
        "fallback",
        "mget-or text-val miss [{engine}]"
    );
    // Numeric-key map: hit
    assert_eq!(
        run_inline(engine, r#"f>t;m=mset mmap 7 "seven";mget-or m 7 "default""#, "f"),
        "seven",
        "mget-or num-key hit [{engine}]"
    );
    // Numeric-key map: miss
    assert_eq!(
        run_inline(engine, r#"f>t;m=mset mmap 7 "seven";mget-or m 8 "default""#, "f"),
        "default",
        "mget-or num-key miss [{engine}]"
    );
}

#[test]
fn mget_or_vm() {
    check("--run-vm");
}

#[cfg(feature = "cranelift")]
#[test]
fn mget_or_cranelift() {
    check("--jit");
}

// Default type must match map value type — verifier must reject with ILO-T013.
#[test]
fn mget_or_default_type_mismatch_rejected() {
    // Map is M t n (value=n); default "x" is text — mismatch.
    let src = r#"f>n;m=mset mmap "k" 5;mget-or m "k" "x""#;
    let stderr = run_inline_expect_err("--run-vm", src);
    assert!(
        stderr.contains("ILO-T013"),
        "expected ILO-T013 for mget-or default type mismatch, got: {stderr}"
    );
    assert!(
        stderr.contains("mget-or"),
        "expected message to mention mget-or, got: {stderr}"
    );
}
