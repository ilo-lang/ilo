// Regression tests pinning trailing-semicolon semantics (ILO-405).
//
// A trailing `;` — a `;` after the last statement before a structural
// boundary (EOF, `}`, `)`) — is always silently consumed in ilo. It is
// never required and never an error. These tests pin that behavior across
// the three body contexts: top-level function bodies, inline-lambda bodies,
// and match-arm bodies.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(label: &str, src: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "ilo_tsemi_{label}_{}_{n}.ilo",
        std::process::id()
    ));
    std::fs::write(&path, src).expect("write src");
    path
}

/// Run `src` with the given entry point and args, assert success, return stdout (trimmed).
fn run_ok(label: &str, src: &str, entry: &str, args: &[&str]) -> String {
    let path = write_src(label, src);
    let mut cmd = ilo();
    cmd.arg(&path).arg("--vm").arg(entry);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "ilo failed for `{src}` (entry={entry}): stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Run `src` inline (no file), assert failure, return stderr.
fn run_fail(src: &str, entry: &str) -> String {
    let out = ilo()
        .arg(src)
        .arg(entry)
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected failure for `{src}`, but it succeeded"
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

// ── 1. Top-level function body: trailing `;` is silently consumed ──────────

/// `f>n;42` and `f>n;42;` must both return 42.
#[test]
fn fn_body_trailing_semi_ignored() {
    let without = run_ok("nosemi", "f>n;42", "f", &[]);
    let with = run_ok("withsemi", "f>n;42;", "f", &[]);
    assert_eq!(without, "42");
    assert_eq!(with, "42");
    assert_eq!(without, with, "trailing `;` must not change the result");
}

/// Multi-statement body: `f x:n>n;a=+x 1;*a 2` and with trailing `;` are identical.
#[test]
fn fn_body_multi_stmt_trailing_semi_ignored() {
    let without = run_ok("msnosemi", "f x:n>n;a=+x 1;*a 2", "f", &["3"]);
    let with = run_ok("mswithsemi", "f x:n>n;a=+x 1;*a 2;", "f", &["3"]);
    assert_eq!(without, "8");
    assert_eq!(with, "8");
    assert_eq!(without, with);
}

// ── 2. Inline lambda body: trailing `;` before `)` is silently consumed ────

/// `(x:n>n;+x 1)` and `(x:n>n;+x 1;)` must both produce the same HOF output.
#[test]
fn lambda_body_trailing_semi_ignored() {
    let without = run_ok(
        "lam_nosemi",
        "f xs:L n>L n;map (x:n>n;+x 1) xs",
        "f",
        &["[1,2,3]"],
    );
    let with = run_ok(
        "lam_withsemi",
        "f xs:L n>L n;map (x:n>n;+x 1;) xs",
        "f",
        &["[1,2,3]"],
    );
    assert_eq!(without, "[2, 3, 4]");
    assert_eq!(with, "[2, 3, 4]");
    assert_eq!(without, with);
}

// ── 3. Match arm body: trailing `;` before `}` is silently consumed ─────────

/// `?x{1:10;_:20}` and `?x{1:10;_:20;}` must both return the same value.
#[test]
fn match_arm_trailing_semi_ignored() {
    let without = run_ok("match_nosemi", "f x:n>n;?x{1:10;_:20}", "f", &["1"]);
    let with = run_ok("match_withsemi", "f x:n>n;?x{1:10;_:20;}", "f", &["1"]);
    assert_eq!(without, "10");
    assert_eq!(with, "10");
    assert_eq!(without, with);
}

// ── 4. A leading `;` (before any statement) is a parse error ───────────────

/// `f>n;;42` — double-semicolon at statement start must fail.
/// This ensures the "trailing `;` is consumed" rule only applies
/// *after* a valid statement, not as a general empty-statement rule.
#[test]
fn double_semi_at_body_start_is_error() {
    let stderr = run_fail("f>n;;42", "f");
    assert!(
        !stderr.is_empty(),
        "expected a parse error for `f>n;;42`, got empty stderr"
    );
}
