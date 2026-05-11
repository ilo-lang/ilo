// Regression tests for using pure builtins as higher-order function args.
//
// Background: every numerics persona writing `fld max ps 0` or `fld min ps 99999`
// hit `undefined variable 'max'` from the verifier and had to write a trivial
// wrapper like `mx2 a b>n;>=a b{ret a};+b 0`. The verifier now promotes pure
// builtin names to `Ty::Fn` when used as values, and the tree-walking
// interpreter resolves them to `Value::FnRef(name)` so `call_function`
// dispatches via the existing builtin path.
//
// VM and Cranelift JIT do not yet implement HOF dispatch at all, so those
// engines are exercised only for the verifier-error case where a HOF call
// is malformed.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(name: &str, src: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("ilo_hof_{name}_{}_{n}.ilo", std::process::id()));
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

// ── fld max — bare builtin as fold function ────────────────────────────────

const FLD_MAX_SRC: &str = "f xs:L n>n;fld max xs 0";

#[test]
fn fld_max_tree() {
    assert_eq!(
        run_ok("--run-tree", FLD_MAX_SRC, "f", &["[3,1,4,1,5,9,2,6]"]),
        "9"
    );
}

// ── fld min — bare builtin with high seed ──────────────────────────────────

const FLD_MIN_SRC: &str = "f xs:L n>n;fld min xs 99999";

#[test]
fn fld_min_tree() {
    assert_eq!(
        run_ok("--run-tree", FLD_MIN_SRC, "f", &["[3,1,4,1,5,9,2,6]"]),
        "1"
    );
}

// ── map abs — 1-arg numeric builtin ────────────────────────────────────────

const MAP_ABS_SRC: &str = "f xs:L n>L n;map abs xs";

#[test]
fn map_abs_tree() {
    assert_eq!(
        run_ok("--run-tree", MAP_ABS_SRC, "f", &["[-1,2,-3]"]),
        "[1, 2, 3]"
    );
}

// ── verifier: passing a builtin whose signature doesn't fit gives a clear
//    diagnostic, not a runtime panic. `prnt` has no `Ty::Fn` mapping, so a
//    bare `prnt` in arg position is still an "undefined variable" — which
//    is the expected behaviour for IO/side-effecting builtins.

const FLD_PRNT_SRC: &str = "f xs:L n>n;fld prnt xs 0";

#[test]
fn fld_io_builtin_rejected_tree() {
    let err = run_err("--run-tree", FLD_PRNT_SRC, "f");
    assert!(
        err.contains("undefined variable 'prnt'") || err.contains("'prnt'"),
        "expected verifier error mentioning 'prnt', got: {err}"
    );
}

#[test]
fn fld_io_builtin_rejected_vm() {
    let err = run_err("--run-vm", FLD_PRNT_SRC, "f");
    assert!(
        err.contains("undefined variable 'prnt'") || err.contains("'prnt'"),
        "expected verifier error mentioning 'prnt', got: {err}"
    );
}

// ── wrapper-using shape still works (no regression for existing programs) ──

const WRAPPER_SRC: &str = "mx2 a:n b:n>n;>=a b{ret a};+b 0\nf xs:L n>n;fld mx2 xs 0";

#[test]
fn wrapper_still_works_tree() {
    assert_eq!(
        run_ok("--run-tree", WRAPPER_SRC, "f", &["[3,1,4,1,5,9,2,6]"]),
        "9"
    );
}
