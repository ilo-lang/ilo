// Regression tests for the `partition fn xs` builtin: split a list into
// `[passing, failing]` based on a predicate. Cleaner than two `flt` calls.
//
// VM and Cranelift JIT don't implement HOF/FnRef dispatch yet (same posture
// as map/flt/fld/grp/uniqby), so partition is exercised only on the tree
// walker for the success cases.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(name: &str, src: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "ilo_partition_{name}_{}_{n}.ilo",
        std::process::id()
    ));
    std::fs::write(&path, src).expect("write src");
    path
}

fn run_ok(engine: &str, src: &str, entry: &str, args: &[&str]) -> String {
    let path = write_src(entry, src);
    let mut cmd = ilo();
    cmd.arg(&path).arg(engine).arg(entry);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// ── Basic: positive vs non-positive numbers ────────────────────────────────

const POS_SRC: &str = "pos x:n>b;>x 0\nf xs:L n>L (L n);partition pos xs";

#[test]
fn partition_pos_neg_tree() {
    assert_eq!(
        run_ok("--run-tree", POS_SRC, "f", &["[-1,2,-3,4]"]),
        "[[2, 4], [-1, -3]]"
    );
}

#[test]
fn partition_empty_input_tree() {
    assert_eq!(run_ok("--run-tree", POS_SRC, "f", &["[]"]), "[[], []]");
}

#[test]
fn partition_all_pass_tree() {
    assert_eq!(
        run_ok("--run-tree", POS_SRC, "f", &["[1,2,3]"]),
        "[[1, 2, 3], []]"
    );
}

// ── All-fail case using a negativity predicate ─────────────────────────────

const NEG_SRC: &str = "neg x:n>b;<x 0\nf xs:L n>L (L n);partition neg xs";

#[test]
fn partition_all_fail_tree() {
    assert_eq!(
        run_ok("--run-tree", NEG_SRC, "f", &["[1,2,3]"]),
        "[[], [1, 2, 3]]"
    );
}

// ── Even / odd partition (preserves input order in both groups) ────────────

const EVEN_SRC: &str = "even x:n>b;=(mod x 2) 0\nf xs:L n>L (L n);partition even xs";

#[test]
fn partition_even_odd_tree() {
    assert_eq!(
        run_ok("--run-tree", EVEN_SRC, "f", &["[1,2,3,4,5,6]"]),
        "[[2, 4, 6], [1, 3, 5]]"
    );
}
