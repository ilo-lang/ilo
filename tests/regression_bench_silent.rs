// Regression: `ilo --bench --silent` (and the `-s` short form) suppresses
// the program's own stdout (`prnt` etc.) across all bench iterations but
// still emits the bench output (JSON envelope under `--json`, human-
// readable summary otherwise) on stdout.
//
// Motivation: pending #5bp / Zero #6035 (borrowed 2026-05-20). The persona
// cost-rollup harness scrapes bench numbers from agent transcripts; a
// chatty `prnt` inside a benched function buries the numbers under 10k
// lines of program output. `--silent` lets the harness consume the bench
// JSON cleanly via stdout.
//
// Contract:
//   - Stderr is never silenced (errors still surface).
//   - Bench numbers (`schemaVersion`, `engine`, `perCallNs`, ...) still
//     reach stdout in `--json` mode.
//   - Cross-engine: silencing applies to tree, vm, jit alike.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

/// Tiny program that benchmarks a function which prints on every call.
/// 10k iterations * 4 engines * `prnt` per call = 40k+ noise lines if the
/// silencer doesn't fire. The assertion compares stdout *line count*
/// against the JSON-envelope count to catch any leak.
const NOISY_PROGRAM: &str = "f x:n>n;prnt x;*x 2";

#[test]
fn bench_silent_suppresses_program_stdout_under_json() {
    let out = ilo()
        .arg(NOISY_PROGRAM)
        .arg("--bench")
        .arg("f")
        .arg("3")
        .arg("--json")
        .arg("--silent")
        .output()
        .expect("spawn ilo");
    assert!(
        out.status.success(),
        "ilo --bench --json --silent failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    // Every non-empty stdout line must be a JSON envelope. If `prnt`
    // output leaked, we'd see thousands of lines that don't start with `{`.
    let non_json_lines: Vec<&str> = stdout
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('{'))
        .collect();
    assert!(
        non_json_lines.is_empty(),
        "expected only JSON envelopes on stdout under --silent; saw {} non-JSON lines (first few: {:?})",
        non_json_lines.len(),
        &non_json_lines.iter().take(3).collect::<Vec<_>>()
    );

    // ...and we still got bench numbers — at least one envelope per
    // expected engine. (Tree engine dropped in PR E of ILO-45.)
    assert!(
        stdout.contains("\"engine\":\"vm\""),
        "missing vm engine in --silent bench output: {stdout}"
    );
}

#[test]
fn bench_silent_short_flag_works() {
    // -s is the short form. Same contract as --silent.
    let out = ilo()
        .arg(NOISY_PROGRAM)
        .arg("--bench")
        .arg("f")
        .arg("3")
        .arg("--json")
        .arg("-s")
        .output()
        .expect("spawn ilo");
    assert!(out.status.success(), "ilo --bench --json -s failed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines().filter(|l| !l.is_empty()) {
        assert!(
            line.starts_with('{') && line.contains("\"schemaVersion\""),
            "non-JSON line leaked under -s: {line}"
        );
    }
}

#[test]
fn bench_without_silent_still_shows_program_output() {
    // Sanity: without --silent, the chatty `prnt` output IS visible. This
    // guards against accidentally silencing by default.
    let out = ilo()
        .arg(NOISY_PROGRAM)
        .arg("--bench")
        .arg("f")
        .arg("3")
        .arg("--json")
        .output()
        .expect("spawn ilo");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);

    // We expect lots of `3` lines from `prnt 3` interleaved with JSON.
    // Use total line count as the indicator — any program-stdout leak means
    // far more than the ~4 envelope lines we'd see otherwise.
    let total_lines = stdout.lines().count();
    assert!(
        total_lines > 100,
        "expected program prnt output to leak through without --silent; total stdout lines = {total_lines}. stdout bytes = {}, sample: {:?}",
        stdout.len(),
        stdout.lines().take(8).collect::<Vec<_>>()
    );
}

#[test]
fn bench_json_schema_is_stable_across_engines() {
    // Cross-engine contract test: every JSON envelope has the full set of
    // documented fields, in addition to `engine`. Locks the v1 schema so a
    // future refactor that drops e.g. `perCallNs` for one engine fails
    // loudly. Paired with --silent so chatty programs don't break the
    // line-by-line parse.
    let out = ilo()
        .arg(NOISY_PROGRAM)
        .arg("--bench")
        .arg("f")
        .arg("3")
        .arg("--json")
        .arg("--silent")
        .output()
        .expect("spawn ilo");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);

    let envelopes: Vec<&str> = stdout
        .lines()
        .filter(|l| !l.is_empty() && l.starts_with('{'))
        .collect();
    assert!(
        envelopes.len() >= 3,
        "expected at least 3 engine envelopes (tree + vm fresh + vm reusable), saw {}: {stdout}",
        envelopes.len()
    );

    for line in &envelopes {
        let v: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("bench JSON line not valid JSON ({e}): {line}"));
        assert_eq!(v["schemaVersion"], 1, "wrong schemaVersion: {line}");
        assert!(v["engine"].is_string(), "engine not a string: {line}");
        assert!(v["result"].is_string(), "result not a string: {line}");
        assert!(
            v["iterations"].is_number(),
            "iterations not a number: {line}"
        );
        assert!(v["totalMs"].is_number(), "totalMs not a number: {line}");
        assert!(v["perCallNs"].is_number(), "perCallNs not a number: {line}");
    }
}
