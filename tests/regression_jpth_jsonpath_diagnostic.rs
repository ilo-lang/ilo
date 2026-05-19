// Regression: `jpth` is dot-path only, not JSONPath. When the path argument
// looks like JSONPath (leading `$`, `*` wildcard, `[...]` bracket selector),
// every engine returns a diagnostic error pointing the agent at the correct
// dot-path form instead of a misleading `key not found: $`.
//
// Originating friction: security-researcher rerun9 — coming in cold tried
// `$.vulnerabilities[*].cve.id` (the textbook JSONPath shape) and got
// `^key not found: $`. The name `jpth` + docs that said "JSON path lookup"
// implied JSONPath; the existing error implied the JSON was wrong, not the
// path syntax.
//
// Fix: `crate::builtins::jpth_jsonpath_diagnostic` runs before parsing on
// every engine (tree-walk interpreter, VM `OP_JPTH`, Cranelift JIT helper
// `jit_jpth`). On match it returns `^<diagnostic>` with the exact dot-path
// shape and a pointer at `@i` iteration for wildcards.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

#[cfg(feature = "cranelift")]
const ENGINES: &[&str] = &["--vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES: &[&str] = &["--vm"];

const JSON: &str = r#"{"vulnerabilities":[{"cve":{"id":"CVE-1"}}]}"#;

fn run_jpth(engine: &str, path: &str) -> (bool, String, String) {
    // jpth returns R t t. We surface it on stdout/stderr via the standard
    // top-level Ok/Err split so the test can assert on the error message
    // without parsing JSON output.
    let src = format!(r#"main j:t>R t t;jpth j "{}""#, path.replace('"', "\\\""));
    let out = ilo()
        .arg(engine)
        .arg(&src)
        .arg(JSON)
        .output()
        .expect("failed to spawn ilo");
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    (out.status.success(), stdout, stderr)
}

#[test]
fn jpth_jsonpath_root_selector_diagnoses_dot_path() {
    for engine in ENGINES {
        let (ok, stdout, stderr) = run_jpth(engine, "$.vulnerabilities[0].cve.id");
        assert!(
            !ok,
            "jsonpath-shape path should fail on {engine}, got stdout={stdout}"
        );
        assert!(
            stderr.contains("jpth is dot-path only"),
            "{engine}: stderr lacks dot-path diagnostic, got: {stderr}"
        );
    }
}

#[test]
fn jpth_jsonpath_wildcard_diagnoses_at_iteration() {
    for engine in ENGINES {
        let (ok, _stdout, stderr) = run_jpth(engine, "vulnerabilities.*.cve.id");
        assert!(!ok, "wildcard path should fail on {engine}");
        assert!(
            stderr.contains("jpth is dot-path only") && stderr.contains("`*`"),
            "{engine}: stderr missing wildcard hint, got: {stderr}"
        );
    }
}

#[test]
fn jpth_jsonpath_bracket_indexing_diagnoses_dot_index() {
    for engine in ENGINES {
        let (ok, _stdout, stderr) = run_jpth(engine, "vulnerabilities[0].cve.id");
        assert!(!ok, "bracket path should fail on {engine}");
        assert!(
            stderr.contains("jpth is dot-path only") && stderr.contains("dot before array"),
            "{engine}: stderr missing bracket-form hint, got: {stderr}"
        );
    }
}

#[test]
fn jpth_dot_path_still_works_unchanged() {
    // Sanity: correct dot-path still extracts the nested value cleanly on
    // every engine. Guards against the diagnostic being too eager.
    for engine in ENGINES {
        let (ok, stdout, stderr) = run_jpth(engine, "vulnerabilities.0.cve.id");
        assert!(
            ok,
            "valid dot-path should succeed on {engine}, stderr={stderr}"
        );
        assert_eq!(stdout, "CVE-1", "{engine}: unexpected stdout {stdout}");
    }
}

#[test]
fn jpth_literal_dollar_key_still_resolves() {
    // A path that is literally the key "$" (no `.` / `[` / `*` after it) is
    // still treated as a normal dot-path segment, so JSON with a `$` field
    // keeps working. Guards against false positives on the diagnostic.
    let json = r#"{"$":"raw-dollar-key"}"#;
    for engine in ENGINES {
        let src = r#"main j:t>R t t;jpth j "$""#;
        let out = ilo()
            .arg(engine)
            .arg(src)
            .arg(json)
            .output()
            .expect("failed to spawn ilo");
        assert!(
            out.status.success(),
            "{engine}: bare `$` key should resolve as dot-path, stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        assert_eq!(stdout, "raw-dollar-key", "{engine}: stdout={stdout}");
    }
}

#[test]
fn jpth_unit_helper_matches_jsonpath_shapes() {
    // Direct unit test on the shared helper so each shape is covered without
    // engine round-tripping. Keeps the contract pinned even if the engines
    // refactor where they call from.
    use ilo::builtins::jpth_jsonpath_diagnostic;

    assert!(jpth_jsonpath_diagnostic("$.a.b").is_some());
    assert!(jpth_jsonpath_diagnostic("$[0].a").is_some());
    assert!(jpth_jsonpath_diagnostic("$*").is_some());
    assert!(jpth_jsonpath_diagnostic("a.*.b").is_some());
    assert!(jpth_jsonpath_diagnostic("a[0].b").is_some());

    // Bare `$` is a valid dot-path key, not JSONPath.
    assert!(jpth_jsonpath_diagnostic("$").is_none());
    assert!(jpth_jsonpath_diagnostic("a.b.0.c").is_none());
    assert!(jpth_jsonpath_diagnostic("name").is_none());
}
