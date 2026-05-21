// Regression tests pinning the second batch of muscle-memory aliases
// (ILO-264, ILO-265, ILO-266).
//
// ILO-264: HTTP verb aliases `delete` → `del` and `patch` → `pat`.
//   Agents from Python/JS/Rust reach for the full HTTP verb names; these
//   aliases resolve at parse time to the canonical 3-char short forms so
//   they work without modification.
//
// ILO-265: Map accessor aliases `keys` → `mkeys` and `values` → `mvals`.
//   Universal map-accessor names (Python dict.keys()/.values(), JS
//   Object.keys(), Rust HashMap::keys) that newcomers expect. Canonical
//   m-prefixed names stay in bytecode and fmt output.
//
// ILO-266: `join` → `cat` string-join alias. Python str.join / JS
//   Array.join muscle memory. `append` is NOT added because there is no
//   canonical `app` builtin — list appending is the `+=` operator
//   (OP_LISTAPPEND), not a named function.
//
// Contracts locked in for each alias:
//   1. The alias resolves to the correct canonical in the alias table.
//   2. The canonical name remains in the builtin registry; the alias does not.
//   3. The alias is rejected as a binding/function name (ILO-P011).
//   4. Cross-engine execution (tree/vm/jit) produces the same result as the
//      canonical.

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
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// ── ILO-264: delete → del ────────────────────────────────────────────────────

#[test]
fn del_remains_the_canonical_name_for_delete() {
    let b = Builtin::from_name("del").expect("`del` must be a canonical builtin");
    assert_eq!(b.name(), "del", "canonical name is `del`");
    assert!(
        Builtin::from_name("delete").is_none(),
        "`delete` must not be a canonical name; it is an alias for `del`"
    );
    assert_eq!(
        resolve_alias("delete"),
        Some("del"),
        "`delete` must resolve to canonical `del`"
    );
}

#[test]
fn delete_rejected_as_binding_name() {
    let out = ilo()
        .args(["main>t;delete=\"val\";delete"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "expected `delete=\"val\"` to fail at parse time"
    );
    assert!(
        stderr.contains("ILO-P011"),
        "expected ILO-P011 reserved-name error, got: {stderr}"
    );
    assert!(
        stderr.contains("delete") && stderr.contains("del"),
        "error must name the alias and the canonical builtin, got: {stderr}"
    );
}

#[test]
fn delete_rejected_as_user_function_name() {
    let out = ilo()
        .args(["delete url:t>R t t;del url"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "expected `delete url:t>...` to fail at parse time"
    );
    assert!(
        stderr.contains("ILO-P011"),
        "expected ILO-P011 reserved-name error, got: {stderr}"
    );
}

// ── ILO-264: patch → pat ─────────────────────────────────────────────────────

#[test]
fn pat_remains_the_canonical_name_for_patch() {
    let b = Builtin::from_name("pat").expect("`pat` must be a canonical builtin");
    assert_eq!(b.name(), "pat", "canonical name is `pat`");
    assert!(
        Builtin::from_name("patch").is_none(),
        "`patch` must not be a canonical name; it is an alias for `pat`"
    );
    assert_eq!(
        resolve_alias("patch"),
        Some("pat"),
        "`patch` must resolve to canonical `pat`"
    );
}

#[test]
fn patch_rejected_as_binding_name() {
    let out = ilo()
        .args(["main>t;patch=\"val\";patch"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "expected `patch=\"val\"` to fail at parse time"
    );
    assert!(
        stderr.contains("ILO-P011"),
        "expected ILO-P011 reserved-name error, got: {stderr}"
    );
    assert!(
        stderr.contains("patch") && stderr.contains("pat"),
        "error must name the alias and the canonical builtin, got: {stderr}"
    );
}

#[test]
fn patch_rejected_as_user_function_name() {
    let out = ilo()
        .args(["patch url:t body:t>R t t;pat url body"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "expected `patch url:t body:t>...` to fail at parse time"
    );
    assert!(
        stderr.contains("ILO-P011"),
        "expected ILO-P011 reserved-name error, got: {stderr}"
    );
}

// ── ILO-265: keys → mkeys ────────────────────────────────────────────────────

#[test]
fn mkeys_remains_the_canonical_name_for_keys() {
    let b = Builtin::from_name("mkeys").expect("`mkeys` must be a canonical builtin");
    assert_eq!(b.name(), "mkeys", "canonical name is `mkeys`");
    assert!(
        Builtin::from_name("keys").is_none(),
        "`keys` must not be a canonical name; it is an alias for `mkeys`"
    );
    assert_eq!(
        resolve_alias("keys"),
        Some("mkeys"),
        "`keys` must resolve to canonical `mkeys`"
    );
}

const KEYS_ALIAS_SRC: &str = "f>L t;m=mset (mset mmap \"a\" 1) \"b\" 2;keys m";

#[test]
fn keys_alias_vm_tree() {
    let result = run("--vm", KEYS_ALIAS_SRC, "f");
    assert!(
        result.contains("a") && result.contains("b"),
        "keys alias must return map keys via vm (tree-walk), got: {result}"
    );
}

#[test]
fn keys_alias_vm() {
    let result = run("--vm", KEYS_ALIAS_SRC, "f");
    assert!(
        result.contains("a") && result.contains("b"),
        "keys alias must return map keys via vm, got: {result}"
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn keys_alias_jit() {
    let result = run("--jit", KEYS_ALIAS_SRC, "f");
    assert!(
        result.contains("a") && result.contains("b"),
        "keys alias must return map keys via jit, got: {result}"
    );
}

#[test]
fn keys_canonical_mkeys_still_works() {
    let result = run("--vm", "f>L t;m=mset mmap \"x\" 99;mkeys m", "f");
    assert!(result.contains("x"), "canonical `mkeys` must still work, got: {result}");
}

#[test]
fn keys_rejected_as_binding_name() {
    let out = ilo()
        .args(["main>t;keys=[];keys"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "expected `keys=[]` to fail at parse time"
    );
    assert!(
        stderr.contains("ILO-P011"),
        "expected ILO-P011 reserved-name error, got: {stderr}"
    );
    assert!(
        stderr.contains("keys") && stderr.contains("mkeys"),
        "error must name the alias and the canonical builtin, got: {stderr}"
    );
}

// ── ILO-265: values → mvals ──────────────────────────────────────────────────

#[test]
fn mvals_remains_the_canonical_name_for_values() {
    let b = Builtin::from_name("mvals").expect("`mvals` must be a canonical builtin");
    assert_eq!(b.name(), "mvals", "canonical name is `mvals`");
    assert!(
        Builtin::from_name("values").is_none(),
        "`values` must not be a canonical name; it is an alias for `mvals`"
    );
    assert_eq!(
        resolve_alias("values"),
        Some("mvals"),
        "`values` must resolve to canonical `mvals`"
    );
}

const VALUES_ALIAS_SRC: &str = "f>L n;m=mset (mset mmap \"a\" 10) \"b\" 20;values m";

#[test]
fn values_alias_vm_tree() {
    let result = run("--vm", VALUES_ALIAS_SRC, "f");
    assert!(
        result.contains("10") && result.contains("20"),
        "values alias must return map values via vm (tree-walk), got: {result}"
    );
}

#[test]
fn values_alias_vm() {
    let result = run("--vm", VALUES_ALIAS_SRC, "f");
    assert!(
        result.contains("10") && result.contains("20"),
        "values alias must return map values via vm, got: {result}"
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn values_alias_jit() {
    let result = run("--jit", VALUES_ALIAS_SRC, "f");
    assert!(
        result.contains("10") && result.contains("20"),
        "values alias must return map values via jit, got: {result}"
    );
}

#[test]
fn values_canonical_mvals_still_works() {
    let result = run("--vm", "f>L n;m=mset mmap \"x\" 42;mvals m", "f");
    assert!(result.contains("42"), "canonical `mvals` must still work, got: {result}");
}

#[test]
fn values_rejected_as_binding_name() {
    let out = ilo()
        .args(["main>t;values=[];values"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "expected `values=[]` to fail at parse time"
    );
    assert!(
        stderr.contains("ILO-P011"),
        "expected ILO-P011 reserved-name error, got: {stderr}"
    );
    assert!(
        stderr.contains("values") && stderr.contains("mvals"),
        "error must name the alias and the canonical builtin, got: {stderr}"
    );
}

// ── ILO-266: join → cat (string-join) ────────────────────────────────────────
//
// Note: `append` is NOT aliased. There is no canonical `app` builtin;
// list appending is the `+=` operator (OP_LISTAPPEND), not a named function.
// That gap is tracked as a separate ticket if needed.

#[test]
fn cat_remains_the_canonical_name_for_join() {
    let b = Builtin::from_name("cat").expect("`cat` must be a canonical builtin");
    assert_eq!(b.name(), "cat", "canonical name is `cat`");
    assert!(
        Builtin::from_name("join").is_none(),
        "`join` must not be a canonical name; it is an alias for `cat`"
    );
    assert_eq!(
        resolve_alias("join"),
        Some("cat"),
        "`join` must resolve to canonical `cat`"
    );
}

const JOIN_ALIAS_SRC: &str = "f>t;join [\"a\",\"b\",\"c\"] \",\"";

#[test]
fn join_alias_vm() {
    assert_eq!(
        run("--vm", JOIN_ALIAS_SRC, "f"),
        "a,b,c",
        "`join` alias must produce same output as `cat` via vm"
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn join_alias_jit() {
    assert_eq!(
        run("--jit", JOIN_ALIAS_SRC, "f"),
        "a,b,c",
        "`join` alias must produce same output as `cat` via jit"
    );
}

#[test]
fn join_canonical_cat_still_works() {
    assert_eq!(
        run("--vm", "f>t;cat [\"x\",\"y\"] \"-\"", "f"),
        "x-y",
        "canonical `cat` must still work"
    );
}

#[test]
fn join_rejected_as_binding_name() {
    let out = ilo()
        .args(["main>t;join=\"val\";join"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "expected `join=\"val\"` to fail at parse time"
    );
    assert!(
        stderr.contains("ILO-P011"),
        "expected ILO-P011 reserved-name error, got: {stderr}"
    );
    assert!(
        stderr.contains("join") && stderr.contains("cat"),
        "error must name the alias and the canonical builtin, got: {stderr}"
    );
}
