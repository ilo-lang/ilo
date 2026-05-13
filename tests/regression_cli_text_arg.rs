// Regression: CLI args declared as `t` (text) used to be silently coerced
// to `Number` when the raw string parsed as a finite float. The fix wires
// declared param types into the CLI parser, so `t` params keep their raw
// string verbatim as `Text`.
//
// Originating assessment entry (ilo_assessment_feedback.md, 2026-05-13):
//   "🔴 CLI string-to-number coercion silently corrupts t-typed params.
//    When a function declares arg:t and the CLI receives "2", the value
//    passed in is a number at runtime, not text. num "2" then returns nil
//    (which then breaks the ?r{~i:..;^e:..} match because there's no nil
//    arm)."
//
// This test exercises every engine (default JIT, --run-tree, --run-vm,
// --run-cranelift). Pre-fix all four returned `nil`; post-fix all four
// return the parsed number.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_temp(content: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("prog.ilo");
    std::fs::write(&path, content).expect("write temp ilo");
    (dir, path)
}

fn run_engine(path: &str, func: &str, arg: &str, engine_flag: Option<&str>) -> String {
    let mut cmd = ilo();
    cmd.arg(path).arg(func).arg(arg);
    if let Some(flag) = engine_flag {
        cmd.arg(flag);
    }
    let out = cmd
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn ilo: {e}"));
    assert!(
        out.status.success(),
        "engine {:?}: exit={:?}, stderr={}",
        engine_flag,
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// ── num round-trip through a `t` param ────────────────────────────────────────

/// `f arg:t>n; r=num arg; ?r{~i:i;^e:0-1}` with CLI input `"2"` must
/// return `2` (not `-1`, not `nil`) on every engine.
#[test]
fn text_param_with_digit_input_num_unwraps_across_engines() {
    let src = "f arg:t>n;r=num arg;?r{~i:i;^e:0 - 1}\n";
    let (_dir, path) = write_temp(src);
    let p = path.to_str().unwrap();

    for engine in [
        None,
        Some("--run-tree"),
        Some("--run-vm"),
        Some("--run-cranelift"),
    ] {
        let out = run_engine(p, "f", "2", engine);
        assert_eq!(
            out, "2",
            "engine {engine:?}: expected `2`, got `{out}` (pre-fix bug: arg arrived as Number, num returned nil, match collapsed)"
        );
    }
}

/// A non-numeric input must still take the error arm. Sanity check that
/// the fix doesn't make `num` accept everything.
#[test]
fn text_param_with_non_numeric_input_hits_err_arm_across_engines() {
    let src = "f arg:t>n;r=num arg;?r{~i:i;^e:0 - 1}\n";
    let (_dir, path) = write_temp(src);
    let p = path.to_str().unwrap();

    for engine in [
        None,
        Some("--run-tree"),
        Some("--run-vm"),
        Some("--run-cranelift"),
    ] {
        let out = run_engine(p, "f", "abc", engine);
        assert_eq!(out, "-1", "engine {engine:?}: expected `-1`, got `{out}`");
    }
}

// ── identity preserves bool/nil/list-shaped text inputs ───────────────────────

/// `id-text arg:t>t; arg` with input `"true"` must return the literal
/// string `true`, not silently coerce to `Value::Bool(true)`.
#[test]
fn text_param_preserves_bool_shaped_input_across_engines() {
    let src = "id arg:t>t;arg\n";
    let (_dir, path) = write_temp(src);
    let p = path.to_str().unwrap();

    for engine in [
        None,
        Some("--run-tree"),
        Some("--run-vm"),
        Some("--run-cranelift"),
    ] {
        let out = run_engine(p, "id", "true", engine);
        assert_eq!(out, "true", "engine {engine:?}: got `{out}`");
    }
}

/// `id arg:t>t; arg` with input `"nil"` must return the literal text
/// `nil`, not `Value::Nil` (which would print as the special nil token
/// and break any caller that branches on the declared text type).
#[test]
fn text_param_preserves_nil_shaped_input_across_engines() {
    let src = "id arg:t>t;arg\n";
    let (_dir, path) = write_temp(src);
    let p = path.to_str().unwrap();

    for engine in [
        None,
        Some("--run-tree"),
        Some("--run-vm"),
        Some("--run-cranelift"),
    ] {
        let out = run_engine(p, "id", "nil", engine);
        assert_eq!(out, "nil", "engine {engine:?}: got `{out}`");
    }
}

// ── number params still parse as numbers (no regression on `n`) ───────────────

/// Cross-check: a `n`-typed param with the same `"2"` input must still
/// arrive as a Number, so existing call sites don't regress.
#[test]
fn number_param_still_parses_as_number_across_engines() {
    let src = "double x:n>n;*x 2\n";
    let (_dir, path) = write_temp(src);
    let p = path.to_str().unwrap();

    for engine in [
        None,
        Some("--run-tree"),
        Some("--run-vm"),
        Some("--run-cranelift"),
    ] {
        let out = run_engine(p, "double", "21", engine);
        assert_eq!(out, "42", "engine {engine:?}: got `{out}`");
    }
}
