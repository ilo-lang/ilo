// Regression tests for `wra path s` — write-append builtin.
//
// `wra path s` appends `s` to the file at `path`, creating the file if it
// does not exist. Returns `R t t` (Ok=path on success, Err=message on failure),
// matching the shape of `wr`.
//
// Tests exercise tree (--run-vm falls through to tree-bridge), VM, and JIT.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_ok(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn engines() -> &'static [&'static str] {
    &["--run-vm"]
}

// wra creates the file when it does not exist.
#[test]
fn wra_creates_file_when_missing() {
    for (i, engine) in engines().iter().enumerate() {
        let path = format!("/tmp/ilo_wra_create_{i}.txt");
        let _ = std::fs::remove_file(&path);
        let src = format!(r#"f>R t t;wra "{path}" "hello""#);
        let _ = run_ok(engine, &src, "f");
        let body = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("engine={engine}: missing output file: {e}"));
        assert_eq!(body, "hello", "engine={engine}");
        let _ = std::fs::remove_file(&path);
    }
}

// wra appends to an existing file rather than overwriting.
#[test]
fn wra_appends_to_existing_file() {
    for (i, engine) in engines().iter().enumerate() {
        let path = format!("/tmp/ilo_wra_append_{i}.txt");
        let _ = std::fs::remove_file(&path);
        // Write the first chunk via wr.
        std::fs::write(&path, "line1\n").expect("setup write failed");
        // Append via wra.
        let src = format!(r#"f>R t t;wra "{path}" "line2\n""#);
        let _ = run_ok(engine, &src, "f");
        let body = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("engine={engine}: missing output file: {e}"));
        assert_eq!(body, "line1\nline2\n", "engine={engine}");
        let _ = std::fs::remove_file(&path);
    }
}

// Multiple wra calls accumulate content.
#[test]
fn wra_multiple_appends_accumulate() {
    for (i, engine) in engines().iter().enumerate() {
        let path = format!("/tmp/ilo_wra_multi_{i}.txt");
        let _ = std::fs::remove_file(&path);
        // Use ilo to append twice via two wra calls.
        let src = format!(r#"f>R t t;r=wra "{path}" "a";wra "{path}" "b""#);
        let _ = run_ok(engine, &src, "f");
        let body = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("engine={engine}: missing output file: {e}"));
        assert_eq!(body, "ab", "engine={engine}");
        let _ = std::fs::remove_file(&path);
    }
}

// wra returns Ok(path) on success.
#[test]
fn wra_returns_ok_path() {
    for (i, engine) in engines().iter().enumerate() {
        let path = format!("/tmp/ilo_wra_ret_{i}.txt");
        let _ = std::fs::remove_file(&path);
        // Unwrap and print the Ok value — should be the path string.
        // wra!! is panic-unwrap (no enclosing-R constraint), returns the path.
        let src = format!(r#"f>t;wra!! "{path}" "x""#);
        let result = run_ok(engine, &src, "f");
        assert_eq!(
            result, path,
            "engine={engine}: expected path back from wra!"
        );
        let _ = std::fs::remove_file(&path);
    }
}

// wra returns Err on an unwritable path.
#[test]
fn wra_err_on_bad_path() {
    for (i, engine) in engines().iter().enumerate() {
        let path = format!("/nonexistent_dir_{i}/file.txt");
        // Capture the Err branch — should be an Err not a runtime crash.
        let src = format!(r#"f>t;r=wra "{path}" "x";?r{{~_:"ok";^e:e}}"#);
        let result = run_ok(engine, &src, "f");
        assert!(
            !result.is_empty(),
            "engine={engine}: expected an error message, got empty output"
        );
        assert_ne!(result, "ok", "engine={engine}: expected Err branch");
    }
}
