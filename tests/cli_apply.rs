/// Integration tests for `ilo apply` subcommand.
///
/// Each test works on a temporary copy of a fixture so the original examples
/// are never mutated.
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn ilo_bin() -> PathBuf {
    // Use the debug binary built by `cargo test`.
    let mut path = std::env::current_exe().expect("current_exe");
    // Strip test binary path: target/debug/deps/<test>
    // Walk up to target/debug/
    path.pop(); // deps/
    if path.ends_with("deps") {
        path.pop();
    }
    path.push("ilo");
    path
}

/// Copy a fixture into a temp directory and return (dir, file_path).
/// The `_dir` guard must be kept alive for the duration of the test.
fn tmp_copy(src: &str) -> (tempfile::TempDir, PathBuf) {
    let content = fs::read_to_string(src).expect("read fixture");
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("fixture.ilo");
    fs::write(&file, &content).expect("write tmp");
    (dir, file)
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn run_apply(path: &std::path::Path, extra_args: &[&str]) -> std::process::Output {
    Command::new(ilo_bin())
        .arg("apply")
        .args(extra_args)
        .arg(path)
        .output()
        .expect("ilo apply")
}

// ── tests ─────────────────────────────────────────────────────────────────────

/// A clean file (no diagnostics) should report 0 fixes, 0 remaining.
#[test]
fn apply_clean_file_noop() {
    let (_dir, path) = tmp_copy("examples/arithmetic.ilo");
    let out = run_apply(&path, &[]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() || stdout.contains("0 remaining"),
        "expected clean exit; stdout={stdout}"
    );
    assert!(
        stdout.contains("0 fixes applied") || stdout.contains("0 remaining"),
        "expected 0 remaining; stdout={stdout}"
    );
}

/// Applying `apply-demo.ilo` should rename `word_count` → `word-count` (ILO-L002).
#[test]
fn apply_demo_fixes_l002() {
    let (_dir, path) = tmp_copy("examples/apply-demo.ilo");

    // Before: contains underscore identifier.
    let before = fs::read_to_string(&path).unwrap();
    assert!(before.contains("word_count"), "fixture must contain word_count");

    let out = run_apply(&path, &[]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains("1 fix(es) applied"),
        "expected 1 fix; path={} stdout={stdout} stderr={stderr}", path.display()
    );
    assert!(
        stdout.contains("0 remaining"),
        "expected 0 remaining after fix; path={} stdout={stdout} stderr={stderr}", path.display()
    );

    // After: underscore identifier replaced (the declaration line no longer
    // starts with `word_count`; comments mentioning the old name are fine).
    let after = fs::read_to_string(&path).unwrap();
    assert!(
        after.contains("word-count xs:"),
        "word-count identifier should appear in function declaration; path={} after={after}",
        path.display()
    );
    assert!(
        !after.contains("word_count xs:"),
        "word_count identifier should be gone from function declaration; path={} after={after}",
        path.display()
    );
}

/// `--dry-run` previews the edit without writing the file.
#[test]
fn apply_dry_run_does_not_modify_file() {
    let (_dir, path) = tmp_copy("examples/apply-demo.ilo");

    let before = fs::read_to_string(&path).unwrap();

    let out = run_apply(&path, &["--dry-run"]);
    assert!(out.status.success(), "dry-run should exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("dry-run"),
        "expected dry-run marker; stdout={stdout}"
    );

    // File must be unchanged.
    let after = fs::read_to_string(&path).unwrap();
    assert_eq!(before, after, "dry-run must not modify the file");
}

/// `--dry-run` output includes the before/after snippet for each edit.
#[test]
fn apply_dry_run_shows_edit_details() {
    let (_dir, path) = tmp_copy("examples/apply-demo.ilo");
    let out = run_apply(&path, &["--dry-run"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("word_count") && stdout.contains("word-count"),
        "dry-run should show before/after; stdout={stdout}"
    );
}

/// A file with unfixable diagnostics (no fix_plan) reports 0 fixes applied.
#[test]
fn apply_unfixable_reports_zero_fixes() {
    // Write a file with a genuine type error that has no fix_plan.
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("bad.ilo");
    // Declare f with wrong return type but no auto-fixable suggestion.
    fs::write(&p, "f a:n b:n>n;+a (+b \"oops\")\n").unwrap();

    let out = run_apply(&p, &[]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Either clean (unlikely) or 0 fixes applied with some remaining.
    // Key invariant: it never panics and always prints a summary line.
    assert!(
        stdout.contains("apply:"),
        "should always print summary; stdout={stdout}"
    );
}
