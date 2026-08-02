// Regression test for the 0.12.1 JSON envelope uniformity work.
//
// Before 0.12.1 five `--json` emitters predated the `schemaVersion`
// convention (`ilo run`, `ilo graph`, `ilo --ast`, `ilo serv`,
// `ilo tools --json`) and `ilo spec` had no JSON mode at all. After
// the fix every machine-readable envelope on the CLI carries
// `"schemaVersion": 1` at the top level, so agents can route generically
// without per-command special cases.
//
// This test exercises each of the six emitters end-to-end and asserts
// the field is present. It is intentionally separate from
// `json_output_contracts.rs` so that file can keep its narrow
// "documented top-level keys" assertions and this file can document
// the cross-cutting contract.

use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn parse_stdout(args: &[&str]) -> (bool, Value, String, String) {
    let out = ilo()
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn ilo: {e}"));
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let v: Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "stdout was not valid JSON for args {args:?}\nstdout:\n{stdout}\nstderr:\n{stderr}\nerr: {e}"
        )
    });
    (out.status.success(), v, stdout, stderr)
}

fn write_temp(name: &str, body: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(name);
    std::fs::write(&path, body).expect("write src");
    (dir, path)
}

// ── `ilo run` (success envelope) ─────────────────────────────────────────────

#[test]
fn run_success_envelope_has_schema_version() {
    let (_d, path) = write_temp("r.@", "main >n;42\n");
    let (ok, v, stdout, stderr) = parse_stdout(&["run", path.to_str().unwrap(), "--json"]);
    assert!(ok, "run should succeed\nstdout: {stdout}\nstderr: {stderr}");
    assert_eq!(
        v["schemaVersion"], 1,
        "schemaVersion missing on run success"
    );
    assert_eq!(v["ok"], 42, "expected ok: 42");
}

// ── bare-file run (success envelope, schemaVersion uniform with `run`) ──────

#[test]
fn bare_file_run_success_envelope_has_schema_version() {
    let (_d, path) = write_temp("b.@", "main >n;7\n");
    let (ok, v, stdout, stderr) = parse_stdout(&[path.to_str().unwrap(), "main", "--json"]);
    assert!(
        ok,
        "bare run should succeed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert_eq!(v["schemaVersion"], 1);
    assert_eq!(v["ok"], 7);
}

// ── `ilo run` (error envelope) ──────────────────────────────────────────────

#[test]
fn run_error_envelope_has_schema_version() {
    // Function returns `Value::Err` — should land in the `program` phase
    // error envelope, which is the legacy `--json` shape we lifted to
    // schemaVersion: 1 in 0.12.1.
    let (_d, path) = write_temp("e.@", "main >R n t;^\"bad\"\n");
    let (_ok, v, stdout, stderr) = parse_stdout(&["run", path.to_str().unwrap(), "--json"]);
    assert_eq!(
        v["schemaVersion"], 1,
        "schemaVersion missing on run error\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert_eq!(v["error"]["phase"], "program");
}

// ── `ilo graph` (full graph) ────────────────────────────────────────────────

#[test]
fn graph_envelope_has_schema_version() {
    let (_d, path) = write_temp("g.@", "main >n;42\n");
    let (ok, v, _stdout, _stderr) = parse_stdout(&["graph", path.to_str().unwrap()]);
    assert!(ok, "graph should succeed");
    assert_eq!(v["schemaVersion"], 1);
}

// ── `ilo graph --fn` (function query) ───────────────────────────────────────

#[test]
fn graph_fn_query_has_schema_version() {
    let (_d, path) = write_temp("gf.@", "main >n;42\n");
    let (ok, v, stdout, stderr) = parse_stdout(&["graph", path.to_str().unwrap(), "--fn", "main"]);
    assert!(
        ok,
        "graph --fn should succeed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert_eq!(v["schemaVersion"], 1);
}

// ── `ilo --ast` (AST dump) ──────────────────────────────────────────────────

#[test]
fn ast_envelope_has_schema_version() {
    let (_d, path) = write_temp("a.@", "main >n;42\n");
    // Bare-file mode with --ast.
    let (ok, v, stdout, stderr) = parse_stdout(&[path.to_str().unwrap(), "--ast"]);
    assert!(
        ok,
        "ilo --ast should succeed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert_eq!(v["schemaVersion"], 1, "schemaVersion missing on ast");
    assert!(
        v["declarations"].is_array(),
        "AST should still carry declarations alongside schemaVersion"
    );
}

// ── `ilo tools --json` is feature-gated; only assert when the feature is on ──
//
// The integration suite's default build for this crate is `cargo test
// --release --features cranelift` per the workflow, so the `tools`
// feature is NOT always on. Guard the assertion behind a runtime probe
// instead of `#[cfg]` so a build without `tools` still passes.

#[test]
fn tools_envelope_has_schema_version_if_feature_enabled() {
    // Probe feature support: `ilo version --json` lists compiled features.
    let (_ok, v, _, _) = parse_stdout(&["version", "--json"]);
    let features = v["features"].as_array().cloned().unwrap_or_default();
    let has_tools = features.iter().any(|f| f.as_str() == Some("tools"));
    if !has_tools {
        eprintln!("skipping: ilo built without --features tools");
        return;
    }

    // Minimal HTTP tools config so `ilo tools --json` has something to emit.
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = dir.path().join("tools.json");
    std::fs::write(
        &cfg,
        r#"{
            "tools": {
                "echo": {
                    "url": "http://127.0.0.1:9/echo",
                    "method": "POST",
                    "params": [],
                    "return": "t"
                }
            }
        }"#,
    )
    .expect("write tools cfg");

    let out = ilo()
        .args(["tools", "--tools", cfg.to_str().unwrap(), "--json"])
        .output()
        .expect("spawn ilo tools");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let v: Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("tools --json not JSON: {e}\nstdout: {stdout}\nstderr: {stderr}")
    });
    assert_eq!(v["schemaVersion"], 1);
    assert!(v["tools"].is_array(), "tools envelope should wrap an array");
}

// ── `ilo serv` (JSONL stdio, every line carries schemaVersion) ──────────────

#[test]
fn serv_ready_line_and_response_have_schema_version() {
    let mut child = ilo()
        .arg("serv")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ilo serv");

    // Send a single request, then close stdin so the loop exits.
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        writeln!(
            stdin,
            "{{\"program\": \"main >n;42\\n\", \"func\": \"main\"}}"
        )
        .expect("write req");
    }
    // Dropping stdin via take():
    drop(child.stdin.take());

    let out = child.wait_with_output().expect("wait_with_output");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();

    let mut lines = stdout.lines();
    let ready_line = lines.next().expect("ready line");
    let ready: Value = serde_json::from_str(ready_line).expect("ready line should be JSON");
    assert_eq!(
        ready["schemaVersion"], 1,
        "serv ready handshake should carry schemaVersion"
    );
    assert_eq!(ready["ready"], true);

    let resp_line = lines.next().expect("response line");
    let resp: Value = serde_json::from_str(resp_line).expect("response should be JSON");
    assert_eq!(
        resp["schemaVersion"], 1,
        "serv response should carry schemaVersion"
    );
    assert_eq!(resp["ok"], 42);
}

#[test]
fn serv_error_response_has_schema_version() {
    let mut child = ilo()
        .arg("serv")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ilo serv");

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        // Not-JSON line triggers the `request` phase error envelope.
        writeln!(stdin, "this is not json").expect("write req");
    }
    drop(child.stdin.take());

    let out = child.wait_with_output().expect("wait_with_output");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();

    let mut lines = stdout.lines();
    let _ready = lines.next().expect("ready");
    let err_line = lines.next().expect("error response line");
    let err: Value = serde_json::from_str(err_line).expect("error response should be JSON");
    assert_eq!(err["schemaVersion"], 1);
    assert_eq!(err["error"]["phase"], "request");
}

// ── `ilo spec --json [lang|ai]` (new in 0.12.1) ─────────────────────────────

#[test]
fn spec_lang_json_has_schema_version_and_content() {
    let (ok, v, stdout, stderr) = parse_stdout(&["spec", "lang", "--json"]);
    assert!(
        ok,
        "spec lang --json should succeed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert_eq!(v["schemaVersion"], 1);
    assert_eq!(v["format"], "markdown");
    assert!(
        v["content"].is_string() && !v["content"].as_str().unwrap().is_empty(),
        "content should be a non-empty string"
    );
}

#[test]
fn spec_ai_json_has_schema_version_and_content() {
    let (ok, v, stdout, stderr) = parse_stdout(&["spec", "ai", "--json"]);
    assert!(
        ok,
        "spec ai --json should succeed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert_eq!(v["schemaVersion"], 1);
    assert_eq!(v["format"], "ai-txt");
    assert!(
        v["content"].is_string() && !v["content"].as_str().unwrap().is_empty(),
        "content should be a non-empty string"
    );
}
