// CLI integration tests for `ilo trace` (ILO-72).
//
// Tests verify:
//   - Each output line is valid JSON.
//   - Each line has the required keys: schemaVersion, line, stmt, bindings, result.
//   - The schemaVersion is 1.
//   - The line numbers are positive integers.
//   - `ilo trace` with no args exits non-zero with usage hint.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_args(args: &[&str]) -> (bool, String, String) {
    let out = ilo().args(args).output().expect("failed to run ilo");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (out.status.success(), stdout, stderr)
}

// ── trace command: basic JSONL shape ─────────────────────────────────────────

#[test]
fn trace_demo_emits_valid_jsonl() {
    let (ok, stdout, _stderr) = run_args(&["trace", "examples/trace-demo.ilo", "add", "3", "4"]);
    assert!(ok, "expected exit 0");

    // Must have at least one line.
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(!lines.is_empty(), "expected at least one trace line");

    // Every line must be valid JSON with required keys.
    for line in &lines {
        let v: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("invalid JSON on line {line:?}: {e}"));

        // Required keys.
        assert!(
            v.get("schemaVersion").is_some(),
            "missing schemaVersion in {line}"
        );
        assert!(v.get("line").is_some(), "missing line in {line}");
        assert!(v.get("stmt").is_some(), "missing stmt in {line}");
        assert!(v.get("bindings").is_some(), "missing bindings in {line}");
        assert!(v.get("result").is_some(), "missing result in {line}");

        // schemaVersion must be 1.
        assert_eq!(v["schemaVersion"], 1, "schemaVersion must be 1 in {line}");

        // line must be a positive integer.
        let ln = v["line"]
            .as_u64()
            .expect("line must be a non-negative integer");
        assert!(ln > 0, "line number must be > 0, got {ln}");

        // bindings must be an object.
        assert!(
            v["bindings"].is_object(),
            "bindings must be an object in {line}"
        );
    }
}

#[test]
fn trace_demo_bindings_contain_expected_vars() {
    let (ok, stdout, _stderr) = run_args(&["trace", "examples/trace-demo.ilo", "add", "3", "4"]);
    assert!(ok, "expected exit 0");

    // The last line should have bindings for x, y, a, b and result = 14.
    let last = stdout.lines().last().expect("at least one line");
    let v: serde_json::Value = serde_json::from_str(last).unwrap();
    let bindings = &v["bindings"];
    assert_eq!(bindings["x"], 3, "x should be 3");
    assert_eq!(bindings["y"], 4, "y should be 4");
    assert_eq!(bindings["a"], 7, "a should be 7 (+3 4)");
    assert_eq!(bindings["b"], 14, "b should be 14 (*a 2)");
    assert_eq!(v["result"], 14, "final result should be 14");
}

// ── trace command: no-args usage ──────────────────────────────────────────────

#[test]
fn trace_no_args_exits_nonzero() {
    let (ok, _stdout, stderr) = run_args(&["trace"]);
    assert!(!ok, "expected non-zero exit when no args");
    assert!(
        stderr.contains("Usage") || stderr.contains("usage") || stderr.contains("trace"),
        "expected usage hint in stderr, got: {stderr}"
    );
}

// ── trace command: missing file ───────────────────────────────────────────────

#[test]
fn trace_missing_file_exits_nonzero() {
    let (ok, _stdout, stderr) = run_args(&["trace", "nonexistent_file_xyz.ilo"]);
    assert!(!ok, "expected non-zero exit for missing file");
    assert!(
        stderr.contains("cannot read") || stderr.contains("No such file") || !stderr.is_empty(),
        "expected error message, got: {stderr}"
    );
}

// ── trace command: --depth expr ───────────────────────────────────────────────

#[test]
fn trace_depth_expr_emits_expr_kind_events() {
    let (ok, stdout, _stderr) = run_args(&[
        "trace",
        "--depth",
        "expr",
        "examples/trace-demo.ilo",
        "add",
        "3",
        "4",
    ]);
    assert!(ok, "expected exit 0");

    let lines: Vec<&str> = stdout.lines().collect();
    assert!(!lines.is_empty(), "expected at least one trace line");

    // At least one event must have kind=expr.
    let has_expr = lines.iter().any(|line| {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        v["kind"] == "expr"
    });
    assert!(has_expr, "expected at least one kind=expr event");

    // All lines must be valid JSON with schemaVersion=1.
    for line in &lines {
        let v: serde_json::Value =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("invalid JSON: {e}"));
        assert_eq!(v["schemaVersion"], 1);
        assert!(v.get("kind").is_some(), "missing kind in {line}");
        let kind = v["kind"].as_str().unwrap();
        match kind {
            "stmt" => {
                assert!(v.get("stmt").is_some(), "stmt event missing stmt key");
                assert!(
                    v.get("bindings").is_some(),
                    "stmt event missing bindings key"
                );
            }
            "expr" => {
                assert!(v.get("expr").is_some(), "expr event missing expr key");
                assert!(v.get("refs").is_some(), "expr event missing refs key");
                assert!(v.get("result").is_some(), "expr event missing result key");
            }
            other => panic!("unexpected kind: {other}"),
        }
    }
}

// ── trace command: --watch ────────────────────────────────────────────────────

#[test]
fn trace_watch_filters_to_relevant_bindings() {
    let (ok, stdout, _stderr) = run_args(&[
        "trace",
        "--watch",
        "a",
        "examples/trace-demo.ilo",
        "add",
        "3",
        "4",
    ]);
    assert!(ok, "expected exit 0");

    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        !lines.is_empty(),
        "expected at least one line after --watch a"
    );

    // Every emitted stmt event must have 'a' in its bindings.
    for line in &lines {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        if v["kind"] == "stmt" {
            assert!(
                v["bindings"].get("a").is_some(),
                "--watch a: stmt event without 'a' in bindings: {line}"
            );
        }
    }
}

#[test]
fn trace_watch_unknown_var_emits_nothing() {
    let (ok, stdout, _stderr) = run_args(&[
        "trace",
        "--watch",
        "__no_such_var__",
        "examples/trace-demo.ilo",
        "add",
        "3",
        "4",
    ]);
    assert!(ok, "expected exit 0");
    assert!(
        stdout.trim().is_empty(),
        "expected no output for unknown watch var, got: {stdout}"
    );
}

#[test]
fn trace_depth_expr_watch_filters_expr_events() {
    let (ok, stdout, _stderr) = run_args(&[
        "trace",
        "--depth",
        "expr",
        "--watch",
        "a",
        "examples/trace-demo.ilo",
        "add",
        "3",
        "4",
    ]);
    assert!(ok, "expected exit 0");

    // All expr events must reference 'a' in their refs.
    for line in stdout.lines() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        if v["kind"] == "expr" {
            let refs: Vec<&str> = v["refs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r.as_str().unwrap())
                .collect();
            assert!(
                refs.contains(&"a"),
                "--watch a: expr event without 'a' in refs: {line}"
            );
        }
    }
}
