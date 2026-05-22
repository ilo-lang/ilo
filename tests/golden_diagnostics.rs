// Golden-file snapshot tests for diagnostic JSON shape.
//
// Each test in this module runs `ilo explain <CODE> --json` and compares the
// output against the canonical snapshot stored at
// `conformance/diagnostics/<CODE>.expected.json`.
//
// Running tests:
//   cargo test --features golden
//
// Updating snapshots ("blessing"):
//   cargo test --features golden -- --bless
//   (This overwrites .expected.json files with the current output.
//    See CONTRIBUTING.md for the full bless workflow.)
//
// Note: tests in this file only compile when the `golden` feature is enabled.
// CI runs them on every PR via `cargo test --features golden`.

#![cfg(feature = "golden")]

use std::path::PathBuf;
use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn conformance_dir() -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest)
        .join("conformance")
        .join("diagnostics")
}

/// Run `ilo explain <code> --json` and return the pretty-printed JSON output.
fn explain_json(code: &str) -> String {
    let out = ilo()
        .args(["explain", code, "--json"])
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn ilo explain {code}: {e}"));
    assert!(
        out.status.success(),
        "ilo explain {code} --json failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .unwrap_or_else(|_| panic!("non-UTF8 output from ilo explain {code}"))
}

/// Parse JSON and re-serialise with sorted keys for stable comparison.
fn normalise_json(raw: &str, label: &str) -> serde_json::Value {
    serde_json::from_str(raw)
        .unwrap_or_else(|e| panic!("invalid JSON from {label}: {e}\nRaw output:\n{raw}"))
}

fn golden_path(code: &str) -> PathBuf {
    conformance_dir().join(format!("{code}.expected.json"))
}

/// Core comparison: if --bless is in CARGO_TEST_ARGS or the env var
/// `ILO_GOLDEN_BLESS=1` is set, overwrite the golden file.  Otherwise diff.
fn assert_golden(code: &str) {
    let actual_raw = explain_json(code);
    let actual: serde_json::Value = normalise_json(&actual_raw, &format!("ilo explain {code}"));

    let bless = std::env::args().any(|a| a == "--bless")
        || std::env::var("ILO_GOLDEN_BLESS").as_deref() == Ok("1");

    let path = golden_path(code);

    if bless {
        let pretty = serde_json::to_string_pretty(&actual)
            .unwrap_or_else(|e| panic!("failed to serialise {code}: {e}"));
        std::fs::write(&path, format!("{pretty}\n"))
            .unwrap_or_else(|e| panic!("failed to write golden {}: {e}", path.display()));
        println!("blessed {}", path.display());
        return;
    }

    let expected_raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("missing golden file {}: {e}\nRun `cargo test --features golden -- --bless` to create it", path.display()));

    let expected: serde_json::Value = normalise_json(&expected_raw, &format!("{}", path.display()));

    if actual != expected {
        // Print a readable diff by comparing pretty-printed strings line by line.
        let actual_pretty = serde_json::to_string_pretty(&actual).unwrap();
        let expected_pretty = serde_json::to_string_pretty(&expected).unwrap();

        let diff_lines: Vec<String> = expected_pretty
            .lines()
            .zip(actual_pretty.lines())
            .enumerate()
            .filter(|(_, (e, a))| e != a)
            .map(|(i, (e, a))| format!("  line {}: expected {e:?} got {a:?}", i + 1))
            .collect();

        panic!(
            "golden mismatch for {code}:\n{}\n\nRun `cargo test --features golden -- --bless` to update the golden file.",
            diff_lines.join("\n")
        );
    }
}

// ── Top-20 golden tests ────────────────────────────────────────────────────

#[test]
fn golden_ilo_l001() {
    assert_golden("ILO-L001");
}

#[test]
fn golden_ilo_l002() {
    assert_golden("ILO-L002");
}

#[test]
fn golden_ilo_l003() {
    assert_golden("ILO-L003");
}

#[test]
fn golden_ilo_p001() {
    assert_golden("ILO-P001");
}

#[test]
fn golden_ilo_p002() {
    assert_golden("ILO-P002");
}

#[test]
fn golden_ilo_p003() {
    assert_golden("ILO-P003");
}

#[test]
fn golden_ilo_p004() {
    assert_golden("ILO-P004");
}

#[test]
fn golden_ilo_p005() {
    assert_golden("ILO-P005");
}

#[test]
fn golden_ilo_t001() {
    assert_golden("ILO-T001");
}

#[test]
fn golden_ilo_t002() {
    assert_golden("ILO-T002");
}

#[test]
fn golden_ilo_t003() {
    assert_golden("ILO-T003");
}

#[test]
fn golden_ilo_t004() {
    assert_golden("ILO-T004");
}

#[test]
fn golden_ilo_t005() {
    assert_golden("ILO-T005");
}

#[test]
fn golden_ilo_t006() {
    assert_golden("ILO-T006");
}

#[test]
fn golden_ilo_t007() {
    assert_golden("ILO-T007");
}

#[test]
fn golden_ilo_t008() {
    assert_golden("ILO-T008");
}

#[test]
fn golden_ilo_t009() {
    assert_golden("ILO-T009");
}

#[test]
fn golden_ilo_r001() {
    assert_golden("ILO-R001");
}

#[test]
fn golden_ilo_r003() {
    assert_golden("ILO-R003");
}

#[test]
fn golden_ilo_r005() {
    assert_golden("ILO-R005");
}

// ── Provenance surface sanity check ───────────────────────────────────────

/// Verify that `conformance/provenance-surface.json` is valid JSON and
/// contains at least one entry with the required shape.
#[test]
fn provenance_surface_is_valid() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let path = PathBuf::from(manifest)
        .join("conformance")
        .join("provenance-surface.json");

    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));

    let v: serde_json::Value = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("invalid JSON in provenance-surface.json: {e}"));

    assert_eq!(
        v["schemaVersion"], 1,
        "provenance-surface.json must have schemaVersion: 1"
    );

    let features = v["features"].as_array().expect("features must be an array");
    assert!(!features.is_empty(), "features array must not be empty");

    // Each feature must have the required keys
    for (i, f) in features.iter().enumerate() {
        assert!(
            f["feature"].is_string(),
            "features[{i}].feature must be a string"
        );
        assert!(
            f["compiler_path"].is_string(),
            "features[{i}].compiler_path must be a string"
        );
        assert!(
            f["function"].is_string(),
            "features[{i}].function must be a string"
        );
        assert!(
            f["error_codes"].is_array(),
            "features[{i}].error_codes must be an array"
        );
    }
}

/// Verify that every golden file listed in conformance/diagnostics/ is valid
/// JSON and contains the mandatory keys: schemaVersion, code, phase, short, long.
#[test]
fn all_golden_files_have_required_keys() {
    let dir = conformance_dir();
    let entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
        .collect();

    assert!(
        !entries.is_empty(),
        "no golden files found in conformance/diagnostics/"
    );

    for entry in entries {
        let path = entry.path();
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        let v: serde_json::Value = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("invalid JSON in {}: {e}", path.display()));

        let name = path.display().to_string();
        assert_eq!(v["schemaVersion"], 1, "{name}: schemaVersion must be 1");
        assert!(v["code"].is_string(), "{name}: code must be a string");
        assert!(v["phase"].is_string(), "{name}: phase must be a string");
        assert!(v["short"].is_string(), "{name}: short must be a string");
        assert!(v["long"].is_string(), "{name}: long must be a string");
    }
}
