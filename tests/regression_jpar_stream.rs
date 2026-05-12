// Cross-engine regression tests for `rdjl` — JSONL streaming.
//
// rdjl path:t → L (R _ t)
//
// Reads a file line by line, parses each non-empty line as JSON, and
// wraps the result so a single malformed line never poisons the whole
// stream. These tests cover:
//   1. happy path: every line parses
//   2. mixed valid / invalid: malformed lines yield Err entries
//   3. empty file: empty list
//   4. blank lines: skipped, not surfaced as Err
// Every case is exercised through the tree-walker, the register VM, and
// (when compiled in) the Cranelift JIT, matching the cross-engine
// convention used elsewhere in this crate.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

// Unique temp paths per call: pid + monotonic counter keeps cross-engine
// runs from racing on the same fixture when `cargo test` schedules them
// in parallel.
fn temp_path(tag: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("ilo_rdjl_{tag}_{pid}_{n}.jsonl"))
}

fn write_fixture(path: &PathBuf, contents: &str) {
    std::fs::write(path, contents).expect("write fixture");
}

fn run(engine: &str, src: &str, entry: &str, extra: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine).arg(entry);
    for a in extra {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}` (entry={entry}): stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn engines() -> Vec<&'static str> {
    let mut v = vec!["--run-tree", "--run-vm"];
    if cfg!(feature = "cranelift") {
        v.push("--run-cranelift");
    }
    v
}

// ── len of result list: works on every engine (no higher-order calls) ─
//
// All four numeric assertions share this one entry point — we just
// vary the file contents — which keeps the test surface small while
// still exercising rdjl on each engine.
const COUNT_SRC: &str = "count p:t>n;es=rdjl p;len es";

#[test]
fn rdjl_three_well_formed_lines() {
    let path = temp_path("happy");
    write_fixture(&path, "{\"amount\":10}\n{\"amount\":20}\n{\"amount\":12}\n");
    for engine in engines() {
        let got = run(engine, COUNT_SRC, "count", &[path.to_str().unwrap()]);
        assert_eq!(got, "3", "engine={engine}");
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn rdjl_empty_file_yields_empty_list() {
    let path = temp_path("empty");
    write_fixture(&path, "");
    for engine in engines() {
        let got = run(engine, COUNT_SRC, "count", &[path.to_str().unwrap()]);
        assert_eq!(got, "0", "engine={engine}");
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn rdjl_skips_blank_lines() {
    let path = temp_path("blanks");
    write_fixture(&path, "{\"x\":1}\n\n{\"x\":2}\n\n\n{\"x\":3}\n");
    for engine in engines() {
        let got = run(engine, COUNT_SRC, "count", &[path.to_str().unwrap()]);
        assert_eq!(got, "3", "engine={engine}");
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn rdjl_mixed_lines_each_wrapped() {
    let path = temp_path("mixed");
    // Three valid lines and two malformed ones interleaved. rdjl is
    // expected to yield five entries total (3 Ok + 2 Err) rather than
    // halting at the first parse error.
    write_fixture(
        &path,
        "{\"a\":1}\nnot json\n{\"a\":2}\n{also bad\n{\"a\":3}\n",
    );
    for engine in engines() {
        let got = run(engine, COUNT_SRC, "count", &[path.to_str().unwrap()]);
        assert_eq!(got, "5", "engine={engine}");
    }
    let _ = std::fs::remove_file(&path);
}

// ── tree-only: verify Ok and Err entries are distinguishable ─────────
//
// `?r{~v:..;^e:..}` Result-matching is supported on the tree-walker.
// The VM/JIT lack higher-order builtins for the same expressivity, so
// the structural Ok/Err assertion is tree-only — the count tests above
// already confirm the entry shape on the other engines.
const FIRST_OK_SRC: &str = "head-amt p:t>n;es=rdjl p;e=hd es;?e{~v:v.amount;^er:999}";

#[test]
fn rdjl_first_line_unwraps_to_record_field() {
    let path = temp_path("first");
    write_fixture(&path, "{\"amount\":7}\n{\"amount\":8}\n");
    let got = run(
        "--run-tree",
        FIRST_OK_SRC,
        "head-amt",
        &[path.to_str().unwrap()],
    );
    assert_eq!(got, "7");
    let _ = std::fs::remove_file(&path);
}

const HEAD_ERR_SRC: &str = "head-tag p:t>n;es=rdjl p;e=hd es;?e{~v:1;^er:0}";

#[test]
fn rdjl_malformed_first_line_is_err() {
    let path = temp_path("err");
    write_fixture(&path, "not json\n{\"ok\":true}\n");
    let got = run(
        "--run-tree",
        HEAD_ERR_SRC,
        "head-tag",
        &[path.to_str().unwrap()],
    );
    // First line is unparseable, so head returns the Err arm (0).
    assert_eq!(got, "0");
    let _ = std::fs::remove_file(&path);
}
