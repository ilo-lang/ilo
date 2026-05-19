// Regression coverage for the `fmt` silent mis-fill footgun (0.12.1).
//
// pdf-analyst rerun11 found that
//
//     fmt "x={} y={}" [a, b]
//
// silently produced `x=[a, b] y={}` — the list literal bound to the first
// `{}` slot and the second slot was left as literal `{}`, with no
// diagnostic. The persona expected `fmt`'s varargs to splat the list, but
// `fmt` formats lists as a single value.
//
// The fix counts `{}` slots in literal templates and requires
// slot-count == value-arg-count at verify, with a targeted hint when a
// list literal is the sole value arg for a multi-slot template.
//
// These tests exercise the verifier (engine-agnostic) by shelling out to
// the CLI and asserting the diagnostic fires before any engine runs. We
// still iterate over the live engines (VM + JIT) so the verifier wiring
// is confirmed identical across the bytecode and Cranelift paths.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(src: &str, engine: &str) -> std::process::Output {
    ilo()
        .args([src, engine, "f"])
        .output()
        .expect("failed to run ilo")
}

fn combined(out: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    format!("{stderr}{stdout}")
}

const ENGINES: &[&str] = &["--run-vm", "--jit"];

// ── the persona repro: list literal where positional was meant ─────────

#[test]
fn list_literal_into_multi_slot_template_errors_cross_engine() {
    let src = r#"f>t;fmt "x={} y={}" [10, 20]"#;
    for engine in ENGINES {
        let out = run(src, engine);
        assert!(
            !out.status.success(),
            "engine {engine}: expected verify failure, got success: {}",
            combined(&out)
        );
        let body = combined(&out);
        assert!(
            body.contains("ILO-T013"),
            "engine {engine}: expected ILO-T013, got: {body}"
        );
        assert!(
            body.contains("list literal"),
            "engine {engine}: expected list-literal hint, got: {body}"
        );
        assert!(
            body.contains("splat"),
            "engine {engine}: expected splat clarification, got: {body}"
        );
    }
}

// ── canonical fix: positional args still work everywhere ───────────────

#[test]
fn multi_slot_positional_args_ok_cross_engine() {
    let src = r#"f>t;fmt "x={} y={}" 10 20"#;
    for engine in ENGINES {
        let out = run(src, engine);
        assert!(
            out.status.success(),
            "engine {engine}: expected success, got: {}",
            combined(&out)
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "x=10 y=20",
            "engine {engine}"
        );
    }
}

// ── deliberate single-slot list-as-value is still permitted ────────────

#[test]
fn single_slot_list_as_value_ok_cross_engine() {
    // When the template has exactly one `{}` and the user passes a list,
    // they almost certainly want the list formatted as one value. No
    // diagnostic; the list shows up in its display form.
    let src = r#"f>t;fmt "list={}" [10, 20]"#;
    for engine in ENGINES {
        let out = run(src, engine);
        assert!(
            out.status.success(),
            "engine {engine}: expected success, got: {}",
            combined(&out)
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "list=[10, 20]",
            "engine {engine}"
        );
    }
}

// ── too-few-args (no list, just plain mismatch) ────────────────────────

#[test]
fn too_few_args_errors_with_actionable_hint() {
    let src = r#"f>t;fmt "{} and {}" 1"#;
    let out = run(src, "--run-vm");
    assert!(!out.status.success(), "expected verify failure");
    let body = combined(&out);
    assert!(body.contains("ILO-T013"), "got: {body}");
    assert!(body.contains("2 `{}` slot"), "got: {body}");
    assert!(body.contains("1 value arg"), "got: {body}");
    // Hint should suggest filling the slots, not the list-splat hint.
    assert!(
        !body.contains("list literal"),
        "should not mention list literal when no list literal passed: {body}"
    );
}

// ── too-many-args ──────────────────────────────────────────────────────

#[test]
fn too_many_args_errors() {
    let src = r#"f>t;fmt "x={}" 1 2 3"#;
    let out = run(src, "--run-vm");
    assert!(!out.status.success(), "expected verify failure");
    let body = combined(&out);
    assert!(body.contains("ILO-T013"), "got: {body}");
    assert!(body.contains("1 `{}` slot"), "got: {body}");
    assert!(body.contains("3 value arg"), "got: {body}");
}

// ── zero-slot template with extra args ─────────────────────────────────

#[test]
fn zero_slot_with_extra_args_errors() {
    let src = r#"f>t;fmt "hello" 1"#;
    let out = run(src, "--run-vm");
    assert!(!out.status.success(), "expected verify failure");
    let body = combined(&out);
    assert!(body.contains("ILO-T013"), "got: {body}");
}

// ── zero-slot template with no args is fine ────────────────────────────

#[test]
fn zero_slot_no_args_ok() {
    let src = r#"f>t;fmt "hello""#;
    let out = run(src, "--run-vm");
    assert!(
        out.status.success(),
        "expected success, got: {}",
        combined(&out)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hello");
}

// ── non-literal template (variable) — runtime path, no static check ────

#[test]
fn non_literal_template_skips_static_check() {
    // When the template is not a string literal we can't statically count
    // slots, so the static check is skipped. Runtime falls back to the
    // historical lenient behaviour. This preserves dynamism for users who
    // build templates programmatically.
    let src = r#"f>t;t="x={} y={}";fmt t 1 2"#;
    let out = run(src, "--run-vm");
    assert!(
        out.status.success(),
        "expected success, got: {}",
        combined(&out)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "x=1 y=2");
}
