// Regression tests for `wro path s` — write-overwrite (truncating) builtin.
//
// `wro path s` truncates the file at `path` and writes `s`, creating the file
// if it does not exist. Returns `R t t` (Ok=path on success, Err=message on
// failure), matching the shape of `wr` and `wra`.
//
// Tests exercise tree (--run-vm falls through to tree-bridge) and VM engines.

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

// wro creates the file when it does not exist.
#[test]
fn wro_creates_file_when_missing() {
    for (i, engine) in engines().iter().enumerate() {
        let path = format!("/tmp/ilo_wro_create_{i}.txt");
        let _ = std::fs::remove_file(&path);
        let src = format!(r#"f>R t t;wro "{path}" "hello""#);
        let _ = run_ok(engine, &src, "f");
        let body = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("engine={engine}: missing output file: {e}"));
        assert_eq!(body, "hello", "engine={engine}");
        let _ = std::fs::remove_file(&path);
    }
}

// wro truncates an existing file rather than appending.
#[test]
fn wro_truncates_existing_file() {
    for (i, engine) in engines().iter().enumerate() {
        let path = format!("/tmp/ilo_wro_trunc_{i}.txt");
        let _ = std::fs::remove_file(&path);
        // Seed the file with initial content.
        std::fs::write(&path, "initial content that is longer\n").expect("setup write failed");
        // Overwrite via wro — result must be only the new content.
        let src = format!(r#"f>R t t;wro "{path}" "new""#);
        let _ = run_ok(engine, &src, "f");
        let body = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("engine={engine}: missing output file: {e}"));
        assert_eq!(body, "new", "engine={engine}: expected truncated content");
        let _ = std::fs::remove_file(&path);
    }
}

// Multiple wro calls keep only the last write.
#[test]
fn wro_multiple_writes_keep_last() {
    for (i, engine) in engines().iter().enumerate() {
        let path = format!("/tmp/ilo_wro_multi_{i}.txt");
        let _ = std::fs::remove_file(&path);
        let src = format!(r#"f>R t t;r=wro "{path}" "first";wro "{path}" "last""#);
        let _ = run_ok(engine, &src, "f");
        let body = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("engine={engine}: missing output file: {e}"));
        assert_eq!(body, "last", "engine={engine}");
        let _ = std::fs::remove_file(&path);
    }
}

// wro returns Ok(path) on success.
#[test]
fn wro_returns_ok_path() {
    for (i, engine) in engines().iter().enumerate() {
        let path = format!("/tmp/ilo_wro_ret_{i}.txt");
        let _ = std::fs::remove_file(&path);
        let src = format!(r#"f>t;wro!! "{path}" "x""#);
        let result = run_ok(engine, &src, "f");
        assert_eq!(
            result, path,
            "engine={engine}: expected path back from wro!"
        );
        let _ = std::fs::remove_file(&path);
    }
}

// wro returns Err on an unwritable path.
#[test]
fn wro_err_on_bad_path() {
    for (i, engine) in engines().iter().enumerate() {
        let path = format!("/nonexistent_dir_{i}/file.txt");
        let src = format!(r#"f>t;r=wro "{path}" "x";?r{{~_:"ok";^e:e}}"#);
        let result = run_ok(engine, &src, "f");
        assert!(
            !result.is_empty(),
            "engine={engine}: expected an error message, got empty output"
        );
        assert_ne!(result, "ok", "engine={engine}: expected Err branch");
    }
}
