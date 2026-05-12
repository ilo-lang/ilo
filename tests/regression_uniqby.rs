// Regression tests for the `uniqby fn xs` builtin: keyed dedup that keeps the
// first occurrence per key. Mirrors `unq` (primitive dedup) but lets agents
// dedup records or transformed values without pre-mapping.
//
// VM and Cranelift JIT don't implement HOF/FnRef dispatch yet (same posture
// as map/flt/fld/grp), so they are exercised only for verifier-error cases
// where it makes sense.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(name: &str, src: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("ilo_uniqby_{name}_{}_{n}.ilo", std::process::id()));
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
        "expected failure but ilo {engine} succeeded for `{src}`"
    );
    let mut s = String::from_utf8_lossy(&out.stderr).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stdout));
    s
}

// ── Basic: dedup by computed key (first character) ─────────────────────────

const BASIC_SRC: &str = "fc s:t>t;hd s f xs:L t>L t;uniqby fc xs";

#[test]
fn uniqby_basic_dedup_by_key_tree() {
    assert_eq!(
        run_ok(
            "--run-tree",
            BASIC_SRC,
            "f",
            &["[\"apple\",\"ant\",\"banana\",\"blueberry\",\"cherry\"]"],
        ),
        "[apple, banana, cherry]"
    );
}

// ── Empty input list returns empty list ────────────────────────────────────

#[test]
fn uniqby_empty_list_tree() {
    assert_eq!(run_ok("--run-tree", BASIC_SRC, "f", &["[]"]), "[]");
}

// ── All-same-key: keep only the first element ──────────────────────────────

#[test]
fn uniqby_all_same_key_keeps_first_tree() {
    assert_eq!(
        run_ok(
            "--run-tree",
            BASIC_SRC,
            "f",
            &["[\"alpha\",\"avocado\",\"apricot\"]"],
        ),
        "[alpha]"
    );
}

// ── Order preservation: first-seen wins, original order preserved ──────────

const PARITY_SRC: &str = "par n:n>t;?=(mod n 2) 0 \"even\" \"odd\"\nf xs:L n>L n;uniqby par xs";

#[test]
fn uniqby_preserves_order_tree() {
    // Input [1,3,2,4,5,6]:
    //   1 → odd (kept)
    //   3 → odd (dropped)
    //   2 → even (kept)
    //   rest → dropped
    // Expected: [1, 2] in original positional order.
    assert_eq!(
        run_ok("--run-tree", PARITY_SRC, "f", &["[1,3,2,4,5,6]"]),
        "[1, 2]"
    );
}

// ── Verifier: wrong first arg (not a fn) is rejected on every engine ──────

const BAD_FN_SRC: &str = "f xs:L n>L n;uniqby 42 xs";

#[test]
fn uniqby_wrong_fn_arg_tree() {
    let err = run_err("--run-tree", BAD_FN_SRC, "f");
    assert!(
        err.contains("uniqby") || err.contains("fn") || err.contains("function"),
        "got: {err}"
    );
}

#[test]
fn uniqby_wrong_fn_arg_vm() {
    let err = run_err("--run-vm", BAD_FN_SRC, "f");
    assert!(
        err.contains("uniqby") || err.contains("fn") || err.contains("function"),
        "got: {err}"
    );
}
