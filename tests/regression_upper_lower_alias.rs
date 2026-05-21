// Regression tests pinning the `upper` → `upr` and `lower` → `lwr` alias
// contracts (ILO-79).
//
// `upper`/`lower` are the standard method names for case conversion in
// Python, JavaScript, Go, and Rust. Personas from those languages reach for
// the verbose forms first. The aliases rewrite to canonical 3-char `upr`/`lwr`
// at parse time; bytecode and fmt output stay on the canonical names.
//
// Contracts to lock in:
// 1. `upper` resolves to `upr` and `lower` resolves to `lwr` at alias-table level.
// 2. `upr` and `lwr` remain canonical in the builtin registry.
// 3. `upper`/`lower` run correctly cross-engine and produce the right output.
// 4. `upper`/`lower` as binding names are rejected with ILO-P011.
// 5. A hint mentioning both forms is emitted on first use.

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
fn upr_lwr_remain_canonical() {
    let b = Builtin::from_name("upr").expect("`upr` must be a canonical builtin");
    assert_eq!(b.name(), "upr");
    let b = Builtin::from_name("lwr").expect("`lwr` must be a canonical builtin");
    assert_eq!(b.name(), "lwr");

    assert!(
        Builtin::from_name("upper").is_none(),
        "`upper` must not be a canonical name"
    );
    assert!(
        Builtin::from_name("lower").is_none(),
        "`lower` must not be a canonical name"
    );

    assert_eq!(resolve_alias("upper"), Some("upr"));
    assert_eq!(resolve_alias("lower"), Some("lwr"));
}

#[test]
fn upper_dispatches_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, "f>t;upper \"hello\"", "f");
        assert_eq!(
            out, "HELLO",
            "{engine}: `upper \"hello\"` expected HELLO, got {out}"
        );
    }
}

#[test]
fn lower_dispatches_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, "f>t;lower \"HELLO\"", "f");
        assert_eq!(
            out, "hello",
            "{engine}: `lower \"HELLO\"` expected hello, got {out}"
        );
    }
}

#[test]
fn upr_canonical_still_works() {
    for engine in ENGINES_ALL {
        let out = run(engine, "f>t;upr \"world\"", "f");
        assert_eq!(out, "WORLD");
    }
}

#[test]
fn lwr_canonical_still_works() {
    for engine in ENGINES_ALL {
        let out = run(engine, "f>t;lwr \"WORLD\"", "f");
        assert_eq!(out, "world");
    }
}

#[test]
fn upper_rejected_as_binding_name() {
    let out = ilo()
        .args(["main>t;upper=\"hi\";upper"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(
        stderr.contains("ILO-P011"),
        "expected ILO-P011, got: {stderr}"
    );
    assert!(
        stderr.contains("upper") && stderr.contains("upr"),
        "error must name alias and canonical, got: {stderr}"
    );
}

#[test]
fn lower_rejected_as_binding_name() {
    let out = ilo()
        .args(["main>t;lower=\"hi\";lower"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(
        stderr.contains("ILO-P011"),
        "expected ILO-P011, got: {stderr}"
    );
    assert!(
        stderr.contains("lower") && stderr.contains("lwr"),
        "error must name alias and canonical, got: {stderr}"
    );
}

#[test]
fn upper_emits_canonical_hint() {
    let out = ilo()
        .args(["f>t;upper \"abc\"", "--vm", "f"])
        .output()
        .expect("failed to run ilo");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("upper") && combined.contains("upr"),
        "expected hint mentioning `upper` and `upr`, got: {combined}"
    );
}

#[test]
fn lower_emits_canonical_hint() {
    let out = ilo()
        .args(["f>t;lower \"ABC\"", "--vm", "f"])
        .output()
        .expect("failed to run ilo");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("lower") && combined.contains("lwr"),
        "expected hint mentioning `lower` and `lwr`, got: {combined}"
    );
}
