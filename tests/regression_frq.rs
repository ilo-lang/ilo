// Regression tests for the `frq xs` builtin: returns a frequency map
// `M t n` (text key → count of occurrences). Unlike `grp`/`uniqby`, `frq`
// is not a higher-order function — it takes a single list and keys by the
// stringified element values, so it can be wired through every engine.

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

// ── Strings: frq ["a","b","a","c","b","a"] → {t:a:3, t:b:2, t:c:1} ────────
// Map iteration order is non-deterministic, so probe via mget on each key.
// Keys are type-prefixed (`t:` for text) to prevent cross-type collisions.

const STRING_A_SRC: &str = r#"f>n;m=frq ["a","b","a","c","b","a"];r=mget m "t:a";?r{n v:v;_:-1}"#;
const STRING_B_SRC: &str = r#"f>n;m=frq ["a","b","a","c","b","a"];r=mget m "t:b";?r{n v:v;_:-1}"#;
const STRING_C_SRC: &str = r#"f>n;m=frq ["a","b","a","c","b","a"];r=mget m "t:c";?r{n v:v;_:-1}"#;

fn check_string_freq(engine: &str) {
    assert_eq!(run(engine, STRING_A_SRC, "f"), "3", "engine={engine} key=a");
    assert_eq!(run(engine, STRING_B_SRC, "f"), "2", "engine={engine} key=b");
    assert_eq!(run(engine, STRING_C_SRC, "f"), "1", "engine={engine} key=c");
}

#[test]
fn frq_strings_tree() {
    check_string_freq("--run-tree");
}

#[test]
fn frq_strings_vm() {
    check_string_freq("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn frq_strings_cranelift() {
    check_string_freq("--run-cranelift");
}

// ── Numbers: frq [1,2,1,3,2,1] — keys are prefixed `n:<num>` ──────────────

const NUM_1_SRC: &str = r#"f>n;m=frq [1,2,1,3,2,1];r=mget m "n:1";?r{n v:v;_:-1}"#;
const NUM_2_SRC: &str = r#"f>n;m=frq [1,2,1,3,2,1];r=mget m "n:2";?r{n v:v;_:-1}"#;
const NUM_3_SRC: &str = r#"f>n;m=frq [1,2,1,3,2,1];r=mget m "n:3";?r{n v:v;_:-1}"#;

fn check_num_freq(engine: &str) {
    assert_eq!(run(engine, NUM_1_SRC, "f"), "3", "engine={engine} key=1");
    assert_eq!(run(engine, NUM_2_SRC, "f"), "2", "engine={engine} key=2");
    assert_eq!(run(engine, NUM_3_SRC, "f"), "1", "engine={engine} key=3");
}

#[test]
fn frq_numbers_tree() {
    check_num_freq("--run-tree");
}

#[test]
fn frq_numbers_vm() {
    check_num_freq("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn frq_numbers_cranelift() {
    check_num_freq("--run-cranelift");
}

// ── Empty list: frq [] → {} (size 0) ──────────────────────────────────────

// Probe the empty map by asking for a key that can't exist; mget returns nil
// (Optional miss), so the ?? branch yields 0.
const EMPTY_SRC: &str = r#"f>n;xs=tl ["x"];m=frq xs;r=mget m "anything";?r{n v:v;_:0}"#;

fn check_empty(engine: &str) {
    assert_eq!(run(engine, EMPTY_SRC, "f"), "0", "engine={engine}");
}

#[test]
fn frq_empty_tree() {
    check_empty("--run-tree");
}

#[test]
fn frq_empty_vm() {
    check_empty("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn frq_empty_cranelift() {
    check_empty("--run-cranelift");
}

// ── Singleton: frq ["x"] → {t:x:1} ────────────────────────────────────────

const SINGLE_SRC: &str = r#"f>n;m=frq ["x"];r=mget m "t:x";?r{n v:v;_:-1}"#;

fn check_single(engine: &str) {
    assert_eq!(run(engine, SINGLE_SRC, "f"), "1", "engine={engine}");
}

#[test]
fn frq_single_tree() {
    check_single("--run-tree");
}

#[test]
fn frq_single_vm() {
    check_single("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn frq_single_cranelift() {
    check_single("--run-cranelift");
}

// ── Cross-type: frq [1, "1", true] — keys must NOT collide ────────────────
// Without type prefixes, `Number(1)` and `Text("1")` would both stringify to
// `"1"` and the resulting count would be 2 instead of 1 for each. Verify the
// prefixed shape: `{n:1: 1, t:1: 1, b:true: 1}`.

const CROSS_NUM_SRC: &str = r#"f>n;m=frq [1, "1", true];r=mget m "n:1";?r{n v:v;_:-1}"#;
const CROSS_TXT_SRC: &str = r#"f>n;m=frq [1, "1", true];r=mget m "t:1";?r{n v:v;_:-1}"#;
const CROSS_BOOL_SRC: &str = r#"f>n;m=frq [1, "1", true];r=mget m "b:true";?r{n v:v;_:-1}"#;

fn check_cross_type(engine: &str) {
    assert_eq!(run(engine, CROSS_NUM_SRC, "f"), "1", "engine={engine} n:1");
    assert_eq!(run(engine, CROSS_TXT_SRC, "f"), "1", "engine={engine} t:1");
    assert_eq!(
        run(engine, CROSS_BOOL_SRC, "f"),
        "1",
        "engine={engine} b:true"
    );
}

#[test]
fn frq_cross_type_tree() {
    check_cross_type("--run-tree");
}

#[test]
fn frq_cross_type_vm() {
    check_cross_type("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn frq_cross_type_cranelift() {
    check_cross_type("--run-cranelift");
}
