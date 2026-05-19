// Regression tests pinning the contract that EVERY builtin alias (`head`,
// `length`, `filter`, ...) is rejected as a binding LHS or user-function
// name with `ILO-P011`, mirroring the existing short-form alias guards for
// `rng` and `rand`.
//
// Originating: rerun-prompt-generator and changelog-validator (rerun12) both
// bound `head=...`, and the alias resolver silently rewrote later call
// positions to `hd ...`, emitting empty output with no diagnostic. The
// pre-existing P011 binding-LHS guard used `Builtin::is_builtin(name)` —
// which only matches canonical names — and the alias guards were special-
// cased to `rng` / `rand` only. Every long-form alias (`head`/`length`/
// `tail`/`filter`/`concat`/...) leaked through.
//
// Contracts locked in here:
// 1. `head=...` at top level → ILO-P011 (parse_decl site).
// 2. `head=...` inside a function body → ILO-P011 (parse_stmt site).
// 3. `head>n;42` as a user fn-decl → ILO-P011 (parse_fn_decl site).
// 4. Every entry in `BUILTIN_ALIASES` is rejected in all three positions
//    (drift guard — if a new alias is added without the guard catching it,
//    this test goes red).

use ilo::ast::all_builtin_aliases;
use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn parse_fails_with_p011(src: &str) -> String {
    let out = ilo().args([src]).output().expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(!out.status.success(), "expected parse failure for {src:?}");
    assert!(
        stderr.contains("ILO-P011"),
        "expected ILO-P011 for {src:?}, got: {stderr}"
    );
    stderr
}

#[test]
fn head_rejected_as_top_level_binding() {
    let stderr = parse_fails_with_p011("head=5");
    assert!(
        stderr.contains("`head`") && stderr.contains("`hd`"),
        "diagnostic must name alias and canonical, got: {stderr}"
    );
}

#[test]
fn head_rejected_as_local_binding_inside_fn_body() {
    // Same shape as the rerun-prompt-generator / changelog-validator papercut.
    let stderr = parse_fails_with_p011("main>n;head=5;1");
    assert!(
        stderr.contains("`head`") && stderr.contains("`hd`"),
        "diagnostic must name alias and canonical, got: {stderr}"
    );
}

#[test]
fn head_rejected_as_user_function_name() {
    let stderr = parse_fails_with_p011("head x:n>n;+x 1");
    assert!(
        stderr.contains("`head`") && stderr.contains("`hd`"),
        "diagnostic must name alias and canonical, got: {stderr}"
    );
}

#[test]
fn length_rejected_in_all_three_positions() {
    parse_fails_with_p011("length=5");
    parse_fails_with_p011("main>n;length=5;1");
    parse_fails_with_p011("length x:n>n;+x 1");
}

#[test]
fn filter_rejected_in_all_three_positions() {
    parse_fails_with_p011("filter=5");
    parse_fails_with_p011("main>n;filter=5;1");
    parse_fails_with_p011("filter x:n>n;+x 1");
}

#[test]
fn every_alias_rejected_as_binding_and_fn_decl() {
    // Drift guard: walk the entire `BUILTIN_ALIASES` table and confirm each
    // entry is rejected at all three guard sites. New aliases land with the
    // protection automatically; missing protection fails this test loud.
    for (alias, canonical) in all_builtin_aliases() {
        // Top-level binding
        let top = format!("{alias}=5");
        let stderr = parse_fails_with_p011(&top);
        assert!(
            stderr.contains(alias) && stderr.contains(canonical),
            "top-level binding diagnostic for `{alias}` must name canonical `{canonical}`, got: {stderr}"
        );

        // Local binding inside fn body
        let local = format!("main>n;{alias}=5;1");
        let stderr = parse_fails_with_p011(&local);
        assert!(
            stderr.contains(alias) && stderr.contains(canonical),
            "local binding diagnostic for `{alias}` must name canonical `{canonical}`, got: {stderr}"
        );

        // User fn-decl
        let fn_decl = format!("{alias} x:n>n;+x 1");
        let stderr = parse_fails_with_p011(&fn_decl);
        assert!(
            stderr.contains(alias) && stderr.contains(canonical),
            "fn-decl diagnostic for `{alias}` must name canonical `{canonical}`, got: {stderr}"
        );
    }
}
