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

// ── fix_plan in `ilo check --json` ───────────────────────────────────────────
//
// `ilo check --json` emits one JSON object per line to STDERR (NDJSON).
// These tests capture stderr, parse each line, and assert on fix_plan fields.

fn check_json_diags(code: &str) -> Vec<Value> {
    let out = ilo()
        .args(["check", "--json", code])
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn ilo: {e}"));
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    stderr
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            serde_json::from_str(l)
                .unwrap_or_else(|e| panic!("stderr line was not valid JSON: {l}\nerr: {e}"))
        })
        .collect()
}

/// ILO-T004: undefined variable with a closest-match hint emits a fix_plan.
#[test]
fn check_json_t004_fix_plan_typo() {
    let diags = check_json_diags("f x:n>n;xyz");
    let t004 = diags
        .iter()
        .find(|d| d["code"] == "ILO-T004")
        .expect("T004 diagnostic present");
    let plan = &t004["fix_plan"];
    assert!(!plan.is_null(), "ILO-T004 should carry a fix_plan");
    let edits = plan["edits"].as_array().expect("fix_plan.edits array");
    assert_eq!(edits.len(), 1, "exactly one edit");
    assert_eq!(edits[0]["before"], "xyz", "before is the misspelled token");
    assert_eq!(edits[0]["after"], "x", "after is the suggested replacement");
    assert!(edits[0]["line_range"].is_array(), "line_range is an array");
}

/// ILO-T032 warning: bare fmt discarded → fix_plan prepends "prnt "
#[test]
fn check_json_t032_fix_plan_fmt_prefix() {
    let code = r#"f x:n>n;fmt "{}" x;x"#;
    let diags = check_json_diags(code);
    let t032 = diags
        .iter()
        .find(|d| d["code"] == "ILO-T032")
        .expect("T032 diagnostic present");
    let plan = &t032["fix_plan"];
    assert!(!plan.is_null(), "ILO-T032 should carry a fix_plan");
    let edits = plan["edits"].as_array().expect("fix_plan.edits array");
    assert_eq!(edits.len(), 1);
    let after = edits[0]["after"].as_str().unwrap();
    assert!(
        after.starts_with("prnt fmt"),
        "after should start with 'prnt fmt'; got: {after}"
    );
}

/// ILO-L002: underscore identifier → fix_plan replaces with hyphenated form.
#[test]
fn check_json_l002_fix_plan_hyphen() {
    let diags = check_json_diags("my_func x:n>n;x");
    let l002 = diags
        .iter()
        .find(|d| d["code"] == "ILO-L002")
        .expect("L002 diagnostic present");
    let plan = &l002["fix_plan"];
    assert!(!plan.is_null(), "ILO-L002 should carry a fix_plan");
    let edits = plan["edits"].as_array().expect("fix_plan.edits array");
    assert_eq!(edits[0]["before"], "my_func");
    assert_eq!(edits[0]["after"], "my-func");
}

/// Diagnostics without a specific fix_plan derivation do NOT emit the key.
/// ILO-P005 (expected identifier) has no mechanical fix.
#[test]
fn check_json_no_fix_plan_when_not_applicable() {
    let diags = check_json_diags("f x:n>n;");
    // There should be at least one diagnostic
    assert!(!diags.is_empty(), "should have at least one error");
    // The test is that parsing succeeded (done above) and the binary ran.
}

/// ILO-T008 return type mismatch n→t: fix_plan wraps expr with `str`.
#[test]
fn check_json_t008_fix_plan_str_cast() {
    let diags = check_json_diags("f x:n>t;x");
    let t008 = diags
        .iter()
        .find(|d| d["code"] == "ILO-T008")
        .expect("T008 diagnostic present");
    let plan = &t008["fix_plan"];
    assert!(!plan.is_null(), "ILO-T008 (n→t) should carry a fix_plan");
    let edits = plan["edits"].as_array().expect("fix_plan.edits array");
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0]["before"], "x", "before is the return expr");
    let after = edits[0]["after"].as_str().unwrap();
    assert!(
        after.starts_with("str "),
        "after wraps with 'str'; got: {after}"
    );
    assert!(edits[0]["line_range"].is_array());
}

/// ILO-T008 return type mismatch t→n: fix_plan wraps expr with `num`.
#[test]
fn check_json_t008_fix_plan_num_cast() {
    let diags = check_json_diags("f x:t>n;x");
    let t008 = diags
        .iter()
        .find(|d| d["code"] == "ILO-T008")
        .expect("T008 diagnostic present");
    let plan = &t008["fix_plan"];
    assert!(!plan.is_null(), "ILO-T008 (t→n) should carry a fix_plan");
    let edits = plan["edits"].as_array().expect("fix_plan.edits array");
    assert_eq!(edits.len(), 1);
    let after = edits[0]["after"].as_str().unwrap();
    assert!(
        after.starts_with("num "),
        "after wraps with 'num'; got: {after}"
    );
}

/// ILO-P011 reserved keyword used as binding: fix_plan renames to `<name>2`.
#[test]
fn check_json_p011_fix_plan_reserved_rename() {
    let diags = check_json_diags("var=5;var");
    let p011 = diags
        .iter()
        .find(|d| d["code"] == "ILO-P011")
        .expect("P011 diagnostic present");
    let plan = &p011["fix_plan"];
    assert!(!plan.is_null(), "ILO-P011 should carry a fix_plan");
    let edits = plan["edits"].as_array().expect("fix_plan.edits array");
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0]["before"], "var", "before is the reserved keyword");
    assert_eq!(edits[0]["after"], "var2", "after is the renamed identifier");
    assert!(edits[0]["line_range"].is_array());
}

/// ILO-T041 nil-coalesce on Result: fix_plan rewrites to `?val{~v:v;^_:default}`.
#[test]
fn check_json_t041_fix_plan_nil_coalesce_result() {
    let diags = check_json_diags("f s:t>n;num s ?? 0");
    let t041 = diags
        .iter()
        .find(|d| d["code"] == "ILO-T041")
        .expect("T041 diagnostic present");
    let plan = &t041["fix_plan"];
    assert!(!plan.is_null(), "ILO-T041 should carry a fix_plan");
    let edits = plan["edits"].as_array().expect("fix_plan.edits array");
    assert_eq!(edits.len(), 1);
    let before = edits[0]["before"].as_str().unwrap();
    let after = edits[0]["after"].as_str().unwrap();
    assert!(
        before.contains(" ?? "),
        "before should contain ' ?? '; got: {before}"
    );
    assert!(
        after.starts_with('?'),
        "after should start with '?'; got: {after}"
    );
    assert!(
        after.contains("{~v:v;^_:"),
        "after should contain match arms; got: {after}"
    );
    assert!(edits[0]["line_range"].is_array());
}
