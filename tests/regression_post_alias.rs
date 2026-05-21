// Regression tests pinning the `post` → `pst` alias contract (ILO-78).
//
// `post` was the canonical HTTP-POST verb name before 0.12.0 when it was
// renamed to the 3-char `pst` to match the short-form convention. Users who
// learned the language pre-0.12.0 have `post` as muscle memory. The alias
// resolves `post` → `pst` at parse time so those users get a canonical-name
// hint on first run and keep working without modification.
//
// Contracts to lock in:
// 1. `post` resolves to `pst` at the alias-table level.
// 2. `pst` remains the canonical name in the builtin registry.
// 3. `post` as a binding name is rejected at parse time with ILO-P011.
// 4. A hint mentioning both `post` and `pst` is emitted on first use.

use ilo::ast::resolve_alias;
use ilo::builtins::Builtin;
use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

#[test]
fn pst_remains_the_canonical_name() {
    let b = Builtin::from_name("pst").expect("`pst` must be a canonical builtin");
    assert_eq!(b.name(), "pst", "canonical name is `pst`");
    assert!(
        Builtin::from_name("post").is_none(),
        "`post` must not be a canonical name; it is an alias for `pst`"
    );
    assert_eq!(
        resolve_alias("post"),
        Some("pst"),
        "`post` must resolve to canonical `pst`"
    );
}

#[test]
fn post_rejected_as_binding_name() {
    let out = ilo()
        .args(["main>t;post=\"body\";post"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "expected `post=\"body\"` to fail at parse time"
    );
    assert!(
        stderr.contains("ILO-P011"),
        "expected ILO-P011 reserved-name error, got: {stderr}"
    );
    assert!(
        stderr.contains("post") && stderr.contains("pst"),
        "error must name the alias and the canonical builtin, got: {stderr}"
    );
}

#[test]
fn post_rejected_as_user_function_name() {
    let out = ilo()
        .args(["post url:t body:t>R t t;pst url body"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "expected `post url:t body:t>...` to fail at parse time"
    );
    assert!(
        stderr.contains("ILO-P011"),
        "expected ILO-P011 reserved-name error, got: {stderr}"
    );
}

#[test]
fn post_alias_hint_data_is_correct() {
    // The hint system calls `resolve_alias(word)` on each lexed identifier and
    // emits "hint: `word` → `canonical` (canonical form)" on successful runs.
    // We verify the alias-table data that drives the hint rather than firing a
    // real HTTP request in tests (which would be network-dependent).
    let alias = resolve_alias("post").expect("`post` must be in BUILTIN_ALIASES");
    assert_eq!(alias, "pst", "hint would say `post` → `pst`");
}
