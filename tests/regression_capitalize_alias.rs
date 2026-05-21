// Regression tests pinning the `capitalize` → `cap` alias contract (ILO-81).
//
// `capitalize` is the standard method name for title-casing the first letter
// in Python and Ruby. Personas from those languages reach for the long form.
// The alias rewrites `capitalize` to canonical `cap` at parse time; bytecode
// and fmt output stay on `cap`.
//
// Contracts to lock in:
// 1. `capitalize` resolves to `cap` at the alias-table level.
// 2. `cap` remains canonical in the builtin registry.
// 3. `capitalize "hello"` runs correctly cross-engine and produces "Hello".
// 4. `capitalize` as a binding name is rejected with ILO-P011.
// 5. A hint mentioning both `capitalize` and `cap` is emitted on first use.

use ilo::ast::resolve_alias;
use ilo::builtins::Builtin;
use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} {src:?} failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--vm"];

#[test]
fn cap_remains_canonical() {
    let b = Builtin::from_name("cap").expect("`cap` must be a canonical builtin");
    assert_eq!(b.name(), "cap");
    assert!(
        Builtin::from_name("capitalize").is_none(),
        "`capitalize` must not be a canonical name; it is an alias for `cap`"
    );
    assert_eq!(
        resolve_alias("capitalize"),
        Some("cap"),
        "`capitalize` must resolve to canonical `cap`"
    );
}

#[test]
fn capitalize_dispatches_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, "f>t;capitalize \"hello\"", "f");
        assert_eq!(
            out, "Hello",
            "{engine}: `capitalize \"hello\"` expected Hello, got {out}"
        );
    }
}

#[test]
fn cap_canonical_still_works() {
    for engine in ENGINES_ALL {
        let out = run(engine, "f>t;cap \"world\"", "f");
        assert_eq!(out, "World");
    }
}

#[test]
fn capitalize_rejected_as_binding_name() {
    let out = ilo()
        .args(["main>t;capitalize=\"hi\";capitalize"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(
        stderr.contains("ILO-P011"),
        "expected ILO-P011, got: {stderr}"
    );
    assert!(
        stderr.contains("capitalize") && stderr.contains("cap"),
        "error must name alias and canonical, got: {stderr}"
    );
}

#[test]
fn capitalize_rejected_as_user_function_name() {
    let out = ilo()
        .args(["capitalize s:t>t;cap s"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(
        stderr.contains("ILO-P011"),
        "expected ILO-P011, got: {stderr}"
    );
}

#[test]
fn capitalize_emits_canonical_hint() {
    let out = ilo()
        .args(["f>t;capitalize \"world\"", "--vm", "f"])
        .output()
        .expect("failed to run ilo");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("capitalize") && combined.contains("cap"),
        "expected hint mentioning `capitalize` and `cap`, got: {combined}"
    );
}
