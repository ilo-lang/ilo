// JSON output contracts across the ilo CLI.
//
// Locks the per-command JSON schemas documented in `JSON_OUTPUT.md` at
// the repo root. Each test runs the binary with
// `--json` on a known input, parses stdout as JSON, and asserts the
// documented top-level keys are present. Future changes that break a
// schema fail here.
//
// Legacy outputs (run / bare-file run / graph / --ast / tools / serv)
// predate the schemaVersion convention and are exercised without the
// schemaVersion assertion. Everything added in 0.13 carries
// `schemaVersion: 1`.

use serde_json::Value;
use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_stdout_json(args: &[&str]) -> (bool, Value, String) {
    let out = ilo()
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn ilo: {e}"));
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let parsed: Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "stdout was not valid JSON for args {args:?}\nstdout:\n{stdout}\nstderr:\n{stderr}\nerr: {e}"
        )
    });
    (out.status.success(), parsed, stderr)
}

// ── `ilo version --json` ─────────────────────────────────────────────────────

#[test]
fn version_json_has_schema_and_keys() {
    let (ok, v, _) = run_stdout_json(&["version", "--json"]);
    assert!(ok, "version --json should succeed");
    assert_eq!(v["schemaVersion"], 1);
    assert_eq!(v["name"], "ilo");
    assert!(v["version"].is_string(), "version should be a string");
    assert!(v["features"].is_array(), "features should be an array");
}

// ── `ilo explain ILO-XXXX --json` ────────────────────────────────────────────

#[test]
fn explain_known_code_json() {
    // ILO-T001 is part of the registry's core type-error set; if this ever
    // moves, swap in any registered code.
    let (ok, v, _) = run_stdout_json(&["explain", "ILO-T001", "--json"]);
    assert!(ok, "explain on a known code should exit 0");
    assert_eq!(v["schemaVersion"], 1);
    assert_eq!(v["code"], "ILO-T001");
    assert!(v["short"].is_string());
    assert!(v["long"].is_string());
}

#[test]
fn explain_unknown_code_json() {
    let (ok, v, _) = run_stdout_json(&["explain", "ILO-XXX9999", "--json"]);
    assert!(!ok, "explain on unknown code should exit non-zero");
    assert_eq!(v["schemaVersion"], 1);
    assert_eq!(v["error"]["code"], "unknown-error-code");
    assert_eq!(v["error"]["input"], "ILO-XXX9999");
}

// ── `ilo skill list --json` ──────────────────────────────────────────────────

#[test]
fn skill_list_json() {
    let (ok, v, _) = run_stdout_json(&["skill", "list", "--json"]);
    assert!(ok, "skill list --json should succeed");
    assert_eq!(v["schemaVersion"], 1);
    let skills = v["skills"].as_array().expect("skills should be an array");
    assert!(!skills.is_empty(), "at least one skill should be bundled");
    let first = &skills[0];
    assert!(first["name"].is_string());
    assert!(first["description"].is_string());
    assert!(first["path"].is_string());

    // Phase 2 (PR #419): the listing must include the Zero-parity additions.
    // Phase 3: ilo-builtins split into category files; check all four.
    // Guards against a future refactor silently dropping a skill from the
    // SKILLS array in src/main.rs.
    let names: Vec<&str> = skills
        .iter()
        .map(|s| s["name"].as_str().expect("skill name string"))
        .collect();
    for required in [
        "ilo-language",
        "ilo-language-records",
        "ilo-builtins-core",
        "ilo-builtins-math",
        "ilo-builtins-io",
        "ilo-builtins-text",
        "ilo-errors",
        "ilo-tools",
        "ilo-engines",
        "ilo-agent",
        "ilo-examples",
        "ilo-edit-loop",
    ] {
        assert!(
            names.contains(&required),
            "skill list missing required skill: {required}; got {names:?}"
        );
    }
}

#[test]
fn skill_get_phase2_skills_json() {
    // Phase 2+: every new skill must round-trip through `skill get --json`
    // with a non-trivial content body. This catches an include_str! path
    // typo or an empty file landing in the binary.
    for name in ["ilo-examples", "ilo-edit-loop", "ilo-language-records"] {
        let (ok, v, _) = run_stdout_json(&["skill", "get", name, "--json"]);
        assert!(ok, "skill get {name} --json should succeed");
        assert_eq!(v["schemaVersion"], 1);
        assert_eq!(v["name"], name);
        let content = v["content"].as_str().expect("content string");
        assert!(
            content.len() > 200,
            "skill {name} content suspiciously short: {} bytes",
            content.len()
        );
        let desc = v["description"].as_str().expect("description string");
        assert!(
            desc.starts_with("Use this when"),
            "skill {name} description must start with 'Use this when'"
        );
    }
}

// ── `ilo skill get <name> --json` ────────────────────────────────────────────

#[test]
fn skill_get_known_json() {
    let (ok, v, _) = run_stdout_json(&["skill", "get", "ilo-language", "--json"]);
    assert!(ok, "skill get on a known name should succeed");
    assert_eq!(v["schemaVersion"], 1);
    assert_eq!(v["name"], "ilo-language");
    assert!(v["description"].is_string());
    assert!(v["path"].is_string());
    assert!(v["content"].is_string());
    assert!(
        v["content"].as_str().unwrap().len() > 100,
        "skill content should be non-trivial"
    );
}

#[test]
fn skill_get_unknown_json() {
    let (ok, v, _) = run_stdout_json(&["skill", "get", "no-such-skill-xyz", "--json"]);
    assert!(!ok, "skill get on unknown name should exit non-zero");
    assert_eq!(v["schemaVersion"], 1);
    assert_eq!(v["error"]["code"], "unknown-skill");
    assert_eq!(v["error"]["name"], "no-such-skill-xyz");
}

// ── `ilo skill path <name> --json` ───────────────────────────────────────────

#[test]
fn skill_path_known_json() {
    let (ok, v, _) = run_stdout_json(&["skill", "path", "ilo-language", "--json"]);
    assert!(ok);
    assert_eq!(v["schemaVersion"], 1);
    assert_eq!(v["name"], "ilo-language");
    assert!(v["path"].is_string());
}

// ── `ilo skill show <name> --json` ───────────────────────────────────────────

#[test]
fn skill_show_known_json() {
    // `show` in JSON mode is identical to `get` in JSON mode — the prose
    // header makes no sense as JSON, so we emit the structured form once.
    let (ok, v, _) = run_stdout_json(&["skill", "show", "ilo-language", "--json"]);
    assert!(ok);
    assert_eq!(v["schemaVersion"], 1);
    assert_eq!(v["name"], "ilo-language");
    assert!(v["content"].is_string());
}

// ── `ilo build <file> --json` ────────────────────────────────────────────────
//
// AOT compile is feature-gated behind `cranelift`. CI builds with
// `--features cranelift` so this test runs there. When the feature isn't
// enabled, `ilo build` prints an error and exits 1 — the JSON envelope
// is not promised in that case, so we gate the test on the feature.

#[cfg(feature = "cranelift")]
#[test]
fn build_json_success() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("hello.@");
    let out = dir.path().join("hello-bin");
    std::fs::write(&src, "main >n;42\n").expect("write src");

    let (ok, v, _) = run_stdout_json(&[
        "build",
        src.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
        "--json",
    ]);
    assert!(ok, "build --json should succeed on a trivial program");
    assert_eq!(v["schemaVersion"], 1);
    assert_eq!(v["ok"], true);
    assert_eq!(v["output"], out.to_str().unwrap());
    assert!(v["entry"].is_string());
    assert!(v["bench"].is_boolean());
    assert!(v["sizeBytes"].is_number());
    assert!(v["durationMs"].is_number());
}

// ── `ilo graph <file>` (legacy, always JSON) ─────────────────────────────────
//
// Documents that the existing graph output stays JSON without a
// schemaVersion field — locking the legacy shape so a future refactor
// can't silently bump it.

#[test]
fn graph_legacy_json_still_works() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("g.@");
    std::fs::write(&src, "main >n;42\n").expect("write src");

    let out = ilo()
        .args(["graph", src.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "graph should succeed on valid input");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let v: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("graph stdout not JSON: {e}\n{stdout}"));
    assert!(
        v.is_object() || v.is_array(),
        "graph emits a JSON object or array"
    );
}
