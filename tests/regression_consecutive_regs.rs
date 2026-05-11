// Regression tests for OP_SLC / OP_MSET consecutive-register miscompile.
//
// Before the fix, both opcodes implicitly read a 4th register from R[C+1]
// in dispatch but the emitter only guaranteed that layout via a debug_assert
// that vanished in release. When the 3rd arg was a variable (common inside
// loops), the 4th register read pulled garbage. Now both ops carry the 4th
// register in a trailing data word.
//
// These tests pin the cross-engine behaviour of the repros so the bug can't
// silently come back.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

const SLC_REPRO: &str =
    r#"go>L t;ls=["a","b","c","d","e"];@i 0..3{b=*i 1;b1=+b 1;s=slc ls b b1;p=prnt s};ls"#;
const MSET_REPRO: &str = r#"go>M t t;m=mset mmap "init" "x";@i 0..3{b=*i 1;b1=+b 1;ix=slc "abc" b b1;m=mset m ix "v"};m"#;
// OP_MDEL had the same map-clone RC bug as OP_MSET (HashMap::clone bit-copies
// NanVals without bumping heap RCs, and HashMap::remove drops the removed entry
// without drop_rc on its inner value). This repro mutates the map in a loop so
// any RC imbalance shows up as a panic / parity drift across engines.
const MDEL_REPRO: &str = r#"go>M t t;m=mset mmap "keep" "kv";@i 0..3{b=*i 1;b1=+b 1;k=slc "abc" b b1;m=mset m k "v";m=mdel m k};m"#;

fn run(engine: &str, src: &str) -> String {
    let out = ilo()
        .args([src, engine, "go"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn check_slc(engine: &str) {
    let out = run(engine, SLC_REPRO);
    let lines: Vec<&str> = out.lines().collect();
    assert!(
        lines.len() >= 3,
        "{engine}: expected at least 3 lines, got: {out:?}"
    );
    assert_eq!(lines[0], "[a]", "{engine} line 0: {out:?}");
    assert_eq!(lines[1], "[b]", "{engine} line 1: {out:?}");
    assert_eq!(lines[2], "[c]", "{engine} line 2: {out:?}");
}

fn check_mset(engine: &str) {
    let out = run(engine, MSET_REPRO);
    for needle in ["a: v", "b: v", "c: v", "init: x"] {
        assert!(
            out.contains(needle),
            "{engine} missing `{needle}` in output: {out:?}"
        );
    }
}

#[test]
fn slc_in_loop_tree() {
    check_slc("--run-tree");
}

#[test]
fn slc_in_loop_vm() {
    check_slc("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn slc_in_loop_cranelift() {
    check_slc("--run-cranelift");
}

#[test]
fn mset_in_loop_tree() {
    check_mset("--run-tree");
}

#[test]
fn mset_in_loop_vm() {
    check_mset("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn mset_in_loop_cranelift() {
    check_mset("--run-cranelift");
}

fn check_mdel(engine: &str) {
    let out = run(engine, MDEL_REPRO);
    // The loop adds then immediately deletes each of "a","b","c"; only the
    // pre-existing "keep" entry should survive. Cross-engine output must match.
    let trimmed = out.trim();
    assert_eq!(
        trimmed, "{keep: kv}",
        "{engine}: unexpected output: {out:?}"
    );
}

#[test]
fn mdel_in_loop_tree() {
    check_mdel("--run-tree");
}

#[test]
fn mdel_in_loop_vm() {
    check_mdel("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn mdel_in_loop_cranelift() {
    check_mdel("--run-cranelift");
}
