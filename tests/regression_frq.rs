// Regression tests for the `frq xs` builtin: returns a frequency map keyed
// by the element type. `frq [text]` produces `M t n`; `frq [number]` produces
// `M n n`. Unlike `grp`/`uniqby`, `frq` is not a higher-order function — it
// takes a single list and keys by the typed element values, so it can be
// wired through every engine.

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

// ── Strings: frq ["a","b","a","c","b","a"] → {"a":3, "b":2, "c":1} ────────
// Map iteration order is non-deterministic, so probe via mget on each key.
// Keys are bare stringified element values (no type prefix), matching the
// `grp` convention. Heterogeneous-type collisions are covered separately
// below as a documented behaviour.

const STRING_A_SRC: &str = r#"f>n;m=frq ["a","b","a","c","b","a"];r=mget m "a";?r{n v:v;_:-1}"#;
const STRING_B_SRC: &str = r#"f>n;m=frq ["a","b","a","c","b","a"];r=mget m "b";?r{n v:v;_:-1}"#;
const STRING_C_SRC: &str = r#"f>n;m=frq ["a","b","a","c","b","a"];r=mget m "c";?r{n v:v;_:-1}"#;

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

// ── Numbers: frq [1,2,1,3,2,1] — keys preserve the number type ───────────

const NUM_1_SRC: &str = r#"f>n;m=frq [1,2,1,3,2,1];r=mget m 1;?r{n v:v;_:-1}"#;
const NUM_2_SRC: &str = r#"f>n;m=frq [1,2,1,3,2,1];r=mget m 2;?r{n v:v;_:-1}"#;
const NUM_3_SRC: &str = r#"f>n;m=frq [1,2,1,3,2,1];r=mget m 3;?r{n v:v;_:-1}"#;

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

// ── Singleton: frq ["x"] → {"x":1} ────────────────────────────────────────

const SINGLE_SRC: &str = r#"f>n;m=frq ["x"];r=mget m "x";?r{n v:v;_:-1}"#;

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

// ── Bare keys round-trip through mkeys: the regression that motivated the
//    fix. Persona devops-sre flagged that `mkeys (frq xs)` returned
//    `["t:a","t:b",...]` which broke downstream `fmt`/`cat` of the key. Lock
//    the bare-key surface: sort the keys and read the first one back, which
//    proves the prefix is gone end-to-end (not just on the mget probe path).

const MKEYS_SRC: &str = r#"f>t;m=frq ["b","a","a"];ks=srt (mkeys m);hd ks"#;

fn check_mkeys(engine: &str) {
    assert_eq!(run(engine, MKEYS_SRC, "f"), "a", "engine={engine}");
}

#[test]
fn frq_mkeys_bare_tree() {
    check_mkeys("--run-tree");
}

#[test]
fn frq_mkeys_bare_vm() {
    check_mkeys("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn frq_mkeys_bare_cranelift() {
    check_mkeys("--run-cranelift");
}

// ── Cross-type keys: frq [1, "1", true] — with typed `MapKey`, `Int(1)`,
// `Text("1")` and `Text("true")` are distinct keys (no collision). Bools are
// stringified to Text at the MapKey boundary. This documents the typed-key
// behaviour introduced with MapKey: each typed value has its own slot.

const CROSS_NUM_SRC: &str = r#"f>n;m=frq [1, "1", true];r=mget m 1;?r{n v:v;_:-1}"#;
const CROSS_TXT_SRC: &str = r#"f>n;m=frq [1, "1", true];r=mget m "1";?r{n v:v;_:-1}"#;
const CROSS_BOOL_SRC: &str = r#"f>n;m=frq [1, "1", true];r=mget m "true";?r{n v:v;_:-1}"#;

fn check_cross_type(engine: &str) {
    // Number(1) keeps its own typed key — count is 1.
    assert_eq!(
        run(engine, CROSS_NUM_SRC, "f"),
        "1",
        "engine={engine} numeric key 1"
    );
    // Text("1") is a distinct key from Int(1) — count is 1.
    assert_eq!(
        run(engine, CROSS_TXT_SRC, "f"),
        "1",
        "engine={engine} text key '1'"
    );
    // Bool stringifies to Text("true") at the MapKey boundary — count is 1.
    assert_eq!(
        run(engine, CROSS_BOOL_SRC, "f"),
        "1",
        "engine={engine} key 'true'"
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
