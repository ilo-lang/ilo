// Regression tests for the `flatmap` higher-order builtin: map then flatten
// one level. Mirrors regression_builtins_as_hof.rs in shape.
//
// VM and Cranelift JIT do not yet implement HOF/FnRef dispatch, so cross-
// engine coverage is limited to the tree-walking interpreter for end-to-end
// behaviour. The VM/JIT path falls through to OP_CALL → interpreter when
// invoked via the CLI, but we still pin the tree behaviour here.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(name: &str, src: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("ilo_flatmap_{name}_{}_{n}.ilo", std::process::id()));
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

fn run_err(engine: &str, src: &str, entry: &str) -> String {
    let path = write_src(entry, src);
    let out = ilo()
        .arg(&path)
        .arg(engine)
        .arg(entry)
        .output()
        .expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    assert!(
        !out.status.success(),
        "expected failure for `{src}` on {engine}, stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).into_owned()
}

// ── basic: function returning a fixed-size list per element ───────────────

const PAIR_SRC: &str = "pr x:n>L n;[x, x] f xs:L n>L n;flatmap pr xs";

#[test]
fn flatmap_pair_tree() {
    assert_eq!(
        run_ok("--run-tree", PAIR_SRC, "f", &["[1,2,3]"]),
        "[1, 1, 2, 2, 3, 3]"
    );
}

// ── empty input list ──────────────────────────────────────────────────────

#[test]
fn flatmap_empty_input_tree() {
    assert_eq!(run_ok("--run-tree", PAIR_SRC, "f", &["[]"]), "[]");
}

// ── fn returns empty list for every element (zero-flatten) ────────────────

const NONE_SRC: &str = "none x:n>L n;[] f xs:L n>L n;flatmap none xs";

#[test]
fn flatmap_fn_returns_empty_tree() {
    assert_eq!(run_ok("--run-tree", NONE_SRC, "f", &["[1,2,3]"]), "[]");
}

// ── type variable: list of text, fn returns a list of text ────────────────

const SPLIT_SRC: &str = "sp s:t>L t;spl s \":\" f xs:L t>L t;flatmap sp xs";

#[test]
fn flatmap_split_tree() {
    // ["a:b", "c"] -> [["a","b"], ["c"]] -> ["a", "b", "c"]
    assert_eq!(
        run_ok("--run-tree", SPLIT_SRC, "f", &["[\"a:b\",\"c\"]"]),
        "[a, b, c]"
    );
}

// ── verifier rejects a non-function in the fn position under --run-vm.
// Pinned so a future refactor that drops the flatmap verify arm gets caught
// on the VM dispatch path too, not just tree.

const BAD_FN_SRC: &str = "f xs:L n>L n;flatmap 42 xs";

#[test]
fn flatmap_wrong_fn_arg_vm() {
    let err = run_err("--run-vm", BAD_FN_SRC, "f");
    assert!(
        err.contains("flatmap") || err.contains("fn") || err.contains("function"),
        "got: {err}"
    );
}
