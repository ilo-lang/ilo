// Regression: `ilo --bench <fn> --json` emits one JSON envelope per engine
// with a top-level `engine` field naming which engine produced the timing.
//
// Background: adoption brief 4 (engines as contracts) asked for an `engine`
// field in `--json` bench output so agents reading bench results can tell
// which engine generated the numbers. Before this fix, `--bench` ignored
// `--json` entirely and dumped human-readable text only.
//
// Contract:
//   - Each engine measurement emits one line of JSON to stdout.
//   - Top-level `"engine"` value is one of `"tree"`, `"vm"`, `"jit"`, `"llvm"`.
//     (`"aot"` is reserved but `--bench` doesn't measure AOT today — use
//     `ilo build` then time the produced binary.)
//   - Top-level `"schemaVersion": 1`.
//   - The two VM runs (fresh-compile vs reusable VmState) both emit
//     `"engine":"vm"` and distinguish via a `"variant"` field.
//   - Text mode (default / explicit `--text`) is unchanged.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

/// Run `ilo --bench --json` on a small program and return stdout.
fn bench_json(source: &str, func: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(source).arg("--bench").arg(func);
    for a in args {
        cmd.arg(a);
    }
    cmd.arg("--json");
    let out = cmd.output().expect("spawn ilo");
    assert!(
        out.status.success(),
        "ilo --bench --json exited non-zero: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn bench_json_emits_engine_field_per_engine() {
    // Tiny program: add two numbers. Every engine handles this.
    let stdout = bench_json("add a:n b:n>n;+a b", "add", &["1", "2"]);

    // Each JSON line must include schemaVersion and the right engine name.
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    assert!(
        !lines.is_empty(),
        "expected JSON envelopes on stdout, got nothing. full stdout: {stdout}"
    );

    for line in &lines {
        assert!(
            line.contains("\"schemaVersion\":1"),
            "missing schemaVersion in bench JSON line: {line}"
        );
        assert!(
            line.starts_with('{') && line.ends_with('}'),
            "bench JSON line not a single-line object: {line}"
        );
        assert!(
            line.contains("\"engine\":\""),
            "bench JSON line missing top-level engine field: {line}"
        );
    }
}

#[test]
fn bench_json_covers_tree_vm_jit() {
    let stdout = bench_json("add a:n b:n>n;+a b", "add", &["1", "2"]);

    // Pull the engine name from each line. Test relies only on these being
    // present — not on order — so the bench can re-order engines without
    // breaking the contract.
    let engines: Vec<String> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            // Find the `"engine":"X"` substring and pull out X.
            let key = "\"engine\":\"";
            let start = line.find(key).expect("engine field present") + key.len();
            let rest = &line[start..];
            let end = rest.find('"').expect("engine field terminator");
            rest[..end].to_string()
        })
        .collect();

    // tree, vm, jit are mandatory under the default feature set. llvm only
    // shows up under `--features llvm` and isn't asserted here.
    assert!(
        engines.iter().any(|e| e == "tree"),
        "expected an `engine: tree` record in bench JSON output. saw: {engines:?}"
    );
    assert!(
        engines.iter().any(|e| e == "vm"),
        "expected an `engine: vm` record in bench JSON output. saw: {engines:?}"
    );
    // jit only available under `--features cranelift`. When the build does
    // include cranelift (the default for our CI matrix), assert it.
    if cfg!(feature = "cranelift") {
        assert!(
            engines.iter().any(|e| e == "jit"),
            "expected an `engine: jit` record (cranelift feature on). saw: {engines:?}"
        );
    }

    // VM should appear twice (fresh + reusable variants) — both labeled vm,
    // distinguished by the `variant` field.
    let vm_count = engines.iter().filter(|e| e.as_str() == "vm").count();
    assert_eq!(
        vm_count, 2,
        "expected exactly two `engine: vm` records (fresh + reusable). saw: {engines:?}"
    );
    let stdout_lower = stdout.to_lowercase();
    assert!(
        stdout_lower.contains("\"variant\":\"fresh\""),
        "expected `variant: fresh` on one of the vm records. stdout: {stdout}"
    );
    assert!(
        stdout_lower.contains("\"variant\":\"reusable\""),
        "expected `variant: reusable` on one of the vm records. stdout: {stdout}"
    );
}

#[test]
fn bench_json_records_carry_timing_fields() {
    let stdout = bench_json("add a:n b:n>n;+a b", "add", &["1", "2"]);

    for line in stdout.lines().filter(|l| !l.is_empty()) {
        assert!(
            line.contains("\"iterations\":"),
            "bench JSON line missing iterations: {line}"
        );
        assert!(
            line.contains("\"totalMs\":"),
            "bench JSON line missing totalMs: {line}"
        );
        assert!(
            line.contains("\"perCallNs\":"),
            "bench JSON line missing perCallNs: {line}"
        );
        assert!(
            line.contains("\"result\":\"3\""),
            "bench JSON line missing/wrong result: {line}"
        );
    }
}

#[test]
fn bench_text_mode_unchanged() {
    // Explicit --text bypasses auto-detect. Output must contain the legacy
    // human-readable headings and NOT contain JSON envelopes.
    let out = ilo()
        .arg("add a:n b:n>n;+a b")
        .arg("--bench")
        .arg("add")
        .arg("1")
        .arg("2")
        .arg("--text")
        .output()
        .expect("spawn ilo");
    assert!(out.status.success(), "ilo --bench --text failed");
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(
        stdout.contains("Rust interpreter"),
        "text mode missing Rust interpreter heading: {stdout}"
    );
    assert!(
        stdout.contains("Register VM"),
        "text mode missing Register VM heading: {stdout}"
    );
    assert!(
        !stdout.contains("\"schemaVersion\""),
        "text mode unexpectedly emitted JSON envelope: {stdout}"
    );
}
