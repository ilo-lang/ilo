// Regression tests for long-form map-op aliases (ILO-82).
//
// Adds discoverability aliases so users can write `map-get`, `map-set`,
// `map-has`, `map-del`, `map-keys`, and `map-values` in addition to the
// canonical short forms `mget`, `mset`, `mhas`, `mdel`, `mkeys`, and `mvals`.
//
// ilo identifiers use hyphens (not underscores), so only hyphen forms are
// valid surface syntax. All aliases resolve to the canonical short form at
// the AST level, so every engine (tree, VM, Cranelift) executes identical
// bytecode. Tests here exercise the VM engine; alias resolution itself is
// engine-independent.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, "--vm", entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo --vm failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// ── map-get ───────────────────────────────────────────────────────────────────

#[test]
fn map_get_alias() {
    let src = "main>n;m=mmap;m=mset m \"k\" 42;map-get m \"k\"";
    assert_eq!(run(src, "main"), "42");
}

// ── map-set ───────────────────────────────────────────────────────────────────

#[test]
fn map_set_alias() {
    let src = "main>n;m=mmap;m=map-set m \"x\" 99;mget m \"x\"";
    assert_eq!(run(src, "main"), "99");
}

// ── map-has ───────────────────────────────────────────────────────────────────

#[test]
fn map_has_alias_present() {
    let src = "main>b;m=mmap;m=mset m \"a\" 1;map-has m \"a\"";
    assert_eq!(run(src, "main"), "true");
}

#[test]
fn map_has_alias_absent() {
    let src = "main>b;m=mmap;map-has m \"z\"";
    assert_eq!(run(src, "main"), "false");
}

// ── map-del ───────────────────────────────────────────────────────────────────

#[test]
fn map_del_alias() {
    let src = "main>n;m=mmap;m=mset m \"a\" 1;m=mset m \"b\" 2;m=map-del m \"a\";len m";
    assert_eq!(run(src, "main"), "1");
}

// ── map-keys ──────────────────────────────────────────────────────────────────

#[test]
fn map_keys_alias() {
    // Single key so output is deterministic without sorting.
    let src = "main>t;m=mmap;m=mset m \"only\" 1;ks=map-keys m;at ks 0";
    assert_eq!(run(src, "main"), "only");
}

// ── map-values ────────────────────────────────────────────────────────────────

#[test]
fn map_values_alias() {
    let src = "main>n;m=mmap;m=mset m \"k\" 77;vs=map-values m;at vs 0";
    assert_eq!(run(src, "main"), "77");
}

// ── Combined: mix long and short forms in one program ─────────────────────────

#[test]
fn map_aliases_mixed_with_canonical() {
    // Uses map-set to build, mhas to check, map-del to remove,
    // then confirms key is gone with mhas.
    let src = "main>b;m=mmap;m=map-set m \"hi\" 1;ok=mhas m \"hi\";m=map-del m \"hi\";mhas m \"hi\"";
    assert_eq!(run(src, "main"), "false");
}

// ── map-get used with map-set (all long-form, no canonical) ──────────────────

#[test]
fn all_long_form_roundtrip() {
    let src = "main>n;m=mmap;m=map-set m \"v\" 5;map-get m \"v\"";
    assert_eq!(run(src, "main"), "5");
}
