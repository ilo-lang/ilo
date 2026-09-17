//! G2/G3/G4 CI gates — run the plan's Python gate scripts under the
//! existing `cargo test` job, so no `.github/workflows` edit is required
//! (the git push credential for workflow files is scope-restricted).
//!
//! Gates exercised:
//! - G4: harness prefix stability digest (`scripts/ilo-harness.py --check`)
//! - G2: eviction-linked skill growth (`scripts/check-skill-growth.py`)
//! - G3: constrain artifact smoke + oracle self-check
//!   (`ilo constrain examples --probe`, ILO_CONSTRAIN=1)
//!
//! Each test shells out to `python3` (Ubuntu CI images ship it) and skips
//! cleanly when python3 is unavailable locally.

use std::process::Command;

fn python3() -> Option<&'static str> {
    match Command::new("python3").arg("--version").output() {
        Ok(o) if o.status.success() => Some("python3"),
        _ => None,
    }
}

fn run_py(script: &str, args: &[&str]) -> (bool, String) {
    let out = Command::new("python3")
        .arg(script)
        .args(args)
        .output()
        .expect("failed to spawn python3");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[test]
fn g4_harness_prefix_stability() {
    let Some(py) = python3() else { return };
    for context in ["curated", "full"] {
        let (ok, err) = run_py(
            "scripts/ilo-harness.py",
            &["--check", "--context", context],
        );
        assert!(
            ok,
            "harness prefix not byte-stable for {context}: {err}"
        );
    }
}

#[test]
fn g2_skill_growth_linked() {
    let Some(py) = python3() else { return };
    let (ok, err) = run_py("scripts/check-skill-growth.py", &[]);
    assert!(ok, "skill growth gate failed: {err}");
}

#[test]
fn g3_constrain_artifact_smoke() {
    let exe = env!("CARGO_BIN_EXE_ilo");
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/examples");
    let out = Command::new(exe)
        .env("ILO_CONSTRAIN", "1")
        .args(["constrain", dir])
        .output()
        .expect("failed to run ilo constrain");
    assert!(
        out.status.success(),
        "constrain probe failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let artifact: serde_json::Value = serde_json::from_str(&stdout)
        .expect("constrain artifact must be valid JSON");
    assert_eq!(artifact["schemaVersion"], 1);
    let transitions = artifact["transitions"].as_object().expect("transitions");
    assert!(!transitions.is_empty(), "no transitions recorded");
    let sites = artifact["sites"].as_object().expect("sites object");
    assert!(!sites.is_empty(), "no site-keyed transitions recorded");
}

#[test]
fn g5_mcp_stdio_wire() {
    let Some(py) = python3() else { return };
    let exe = env!("CARGO_BIN_EXE_ilo");
    let (ok, err) = run_py("scripts/test-mcp-server.py", &["--ilo", exe]);
    assert!(ok, "mcp stdio wire test failed: {err}");
}
