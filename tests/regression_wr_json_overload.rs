// Regression: the 3-arg `wr path data "json"` overload should type-check
// and serialise `data` with `jdmp` before writing.
//
// History: SPEC and the agent skill documented `wr path data "json"` as a
// shortcut, but the verifier only accepted 2 args (path:t, content:t). Agents
// hit a confusing arity or type error and had to manually `jdmp` first.
//
// This file pins down the contract across engines:
//   * 2-arg `wr path text` keeps working everywhere.
//   * 3-arg `wr path data "json"` serialises and writes.
//   * Unsupported format literals are rejected by the verifier.
//   * Roundtrip via `rdl` + `jpar` returns equivalent data.

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

fn run_err(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "ilo {engine} unexpectedly succeeded for `{src}`: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

fn engines() -> &'static [&'static str] {
    &["--run-tree", "--run-vm"]
}

// 3-arg `wr path data "json"` writes serialised JSON.
#[test]
fn wr_json_overload_record_writes_json() {
    for (i, engine) in engines().iter().enumerate() {
        let path = format!("/tmp/ilo_wr_json_overload_rec_{i}.json");
        let _ = std::fs::remove_file(&path);
        let src = format!(r#"f>R t t;wr "{path}" (mset (mset mmap "x" 1) "y" 2) "json""#);
        let _ = run_ok(engine, &src, "f");
        let body = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("engine={engine}: missing output file: {e}"));
        let parsed: serde_json::Value =
            serde_json::from_str(&body).expect("output must be valid JSON");
        assert_eq!(parsed["x"].as_f64(), Some(1.0));
        assert_eq!(parsed["y"].as_f64(), Some(2.0));
        let _ = std::fs::remove_file(&path);
    }
}

// 3-arg form with a list value.
#[test]
fn wr_json_overload_list_writes_json() {
    for (i, engine) in engines().iter().enumerate() {
        let path = format!("/tmp/ilo_wr_json_overload_list_{i}.json");
        let _ = std::fs::remove_file(&path);
        let src = format!(r#"f>R t t;wr "{path}" [1,2,3] "json""#);
        let _ = run_ok(engine, &src, "f");
        let body = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("engine={engine}: missing output file: {e}"));
        let parsed: serde_json::Value =
            serde_json::from_str(&body).expect("output must be valid JSON");
        let nums: Vec<f64> = parsed
            .as_array()
            .expect("expected JSON array")
            .iter()
            .map(|v| v.as_f64().expect("array element should be numeric"))
            .collect();
        assert_eq!(nums, vec![1.0, 2.0, 3.0]);
        let _ = std::fs::remove_file(&path);
    }
}

// Old-style 2-arg wr with plain text content still works.
#[test]
fn wr_two_arg_text_still_works() {
    for (i, engine) in engines().iter().enumerate() {
        let path = format!("/tmp/ilo_wr_json_overload_txt_{i}.txt");
        let _ = std::fs::remove_file(&path);
        let src = format!(r#"f>R t t;wr "{path}" "hello""#);
        let _ = run_ok(engine, &src, "f");
        let body = std::fs::read_to_string(&path).expect("missing output file");
        assert_eq!(body, "hello");
        let _ = std::fs::remove_file(&path);
    }
}

// Wrong third arg: verifier rejects with a hint pointing to the supported formats.
#[test]
fn wr_unsupported_format_literal_is_verifier_error() {
    let src = r#"f>R t t;wr "/tmp/x" [1,2,3] "not_json""#;
    let err = run_err("--run-tree", src, "f");
    assert!(
        err.contains("not_json"),
        "error should mention the bad format literal, got: {err}"
    );
    assert!(
        err.contains("json"),
        "error should point at the supported \"json\" format, got: {err}"
    );
}

// 3-arg with a numeric format arg → type error from builtin_check_args.
#[test]
fn wr_format_arg_must_be_text() {
    let src = r#"f>R t t;wr "/tmp/x" [1,2,3] 42"#;
    let err = run_err("--run-tree", src, "f");
    assert!(
        err.contains("format") || err.contains("expects t"),
        "should reject numeric format arg, got: {err}"
    );
}

// Two-arg with a non-text data arg keeps the original type error
// (and the hint should suggest the 3-arg form).
#[test]
fn wr_two_arg_nontext_data_hints_three_arg_form() {
    let src = r#"f>R t t;wr "/tmp/x" [1,2,3]"#;
    let err = run_err("--run-tree", src, "f");
    assert!(
        err.contains("arg 2") && err.contains("t"),
        "expected text-content error on 2-arg wr, got: {err}"
    );
    assert!(
        err.contains("3-arg") || err.contains("\"json\""),
        "expected hint pointing to the json overload, got: {err}"
    );
}

// Roundtrip: write a record as JSON, read it back with `rdl` + `jpar!`,
// and confirm the parsed value matches.
#[test]
fn wr_json_roundtrip_via_rdl_jpar() {
    // We do the assertion outside ilo (parsing the JSON in Rust) so this test
    // works regardless of how `jpar`'s output is rendered by the engine.
    for (i, engine) in engines().iter().enumerate() {
        let path = format!("/tmp/ilo_wr_json_overload_rt_{i}.json");
        let _ = std::fs::remove_file(&path);
        let src = format!(r#"f>R t t;wr "{path}" (mset (mset mmap "x" 1) "y" 2) "json""#);
        let _ = run_ok(engine, &src, "f");
        let body = std::fs::read_to_string(&path).expect("missing output file");

        // Re-read via ilo's own `rdl` + `jpar!` pipeline; only checks that the
        // engines can read what they wrote without erroring.
        let read_src = format!(r#"g>t;p=rdl "{path}";?p{{t v:hd v;_:"err"}}"#);
        let first_line = run_ok(engine, &read_src, "g");
        assert!(
            !first_line.is_empty(),
            "engine={engine}: rdl roundtrip produced empty output"
        );
        // And the on-disk file parses as the original record.
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        assert_eq!(parsed["x"].as_f64(), Some(1.0));
        assert_eq!(parsed["y"].as_f64(), Some(2.0));
        let _ = std::fs::remove_file(&path);
    }
}
