// Regression tests for function-as-call-arg composition.
//
// Background: when an agent writes `prnt str nc`, the parser previously
// treated each token as a separate arg, parsing it as `prnt(str, nc)` —
// 2 args to a 1-arg builtin — and erroring. Same for `hd tl xs`,
// `prnt pct s 10`, `flr +*n 0.25`. Every program ended up doing
// `sv=str nc;prnt sv` style 2-line bindings.
//
// The parser now keeps an arity table (builtins + user functions
// registered as their headers are parsed). In arg position, if the next
// Ident names a known function with arity N, we eagerly consume that
// Ident plus N args as a nested call. HOF positions (map/flt/fld/grp
// slot 0, srt slot 0) are excluded so bare-name function references
// still work (`fld max xs 0`, `map dbl xs`, etc.).

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(tag: &str, src: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("ilo_fnarg_{tag}_{}_{n}.ilo", std::process::id()));
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

// ── 1. prnt str nc — builtin inside 1-arg builtin ──────────────────────────
const PRNT_STR_SRC: &str = "f nc:n>n;prnt str nc;nc";

fn check_prnt_str(engine: &str) {
    // `prnt str nc` should print "5" then return nc.
    let out = run_ok(engine, PRNT_STR_SRC, "f", &["5"]);
    // The printed line is "5"; the function also returns 5.
    // Output is "5\n5" — we trimmed, so split on newline.
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.first().copied(), Some("5"), "engine={engine}");
}

#[test]
fn prnt_str_tree() {
    check_prnt_str("--run-tree");
}

#[test]
fn prnt_str_vm() {
    check_prnt_str("--run-vm");
}

// ── 2. hd tl xs — two unary builtins composed ──────────────────────────────
const HD_TL_SRC: &str = "f xs:L n>n;hd tl xs";

fn check_hd_tl(engine: &str) {
    // hd(tl([10,20,30])) = 20
    assert_eq!(
        run_ok(engine, HD_TL_SRC, "f", &["[10,20,30]"]),
        "20",
        "engine={engine}"
    );
}

#[test]
fn hd_tl_tree() {
    check_hd_tl("--run-tree");
}

#[test]
fn hd_tl_vm() {
    check_hd_tl("--run-vm");
}

// ── 3. Mixing 3-arg outer with 2-arg inner ─────────────────────────────────
// Define a 2-arg user `pct a b = a/b*100`, then call `prnt pct s 10`.
// Outer prnt has arity 1 → it should eagerly consume `pct s 10` as a single
// nested call. Result printed: 50.
const PCT_SRC: &str = "pct a:n b:n>n;*/ a b 100\nf s:n>n;prnt pct s 10;s\n";

fn check_pct(engine: &str) {
    let out = run_ok(engine, PCT_SRC, "f", &["5"]);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.first().copied(), Some("50"), "engine={engine}");
}

#[test]
fn pct_tree() {
    check_pct("--run-tree");
}

#[test]
fn pct_vm() {
    check_pct("--run-vm");
}

// ── 4. Function-as-arg mixed with prefix-binop ─────────────────────────────
// `flr +*n 0.25 0` → flr( (n*0.25) + 0 ).  Outer is flr (1-arg builtin).
// Without the fix, the parser would treat each prefix-binop expression as a
// separate arg. With the fix, flr stops after one arg (the prefix-binop
// `+*n 0.25 0`), and the trailing... well there's no trailing here.
// Simpler shape: `flr +*n 0.25`. But `+` requires 2 atoms — use `+*n 0.25 0`.
const FLR_SRC: &str = "f n:n>n;flr +*n 0.25 0";

fn check_flr(engine: &str) {
    // n=10 → 10*0.25 = 2.5 → +0 = 2.5 → flr = 2
    assert_eq!(
        run_ok(engine, FLR_SRC, "f", &["10"]),
        "2",
        "engine={engine}"
    );
}

#[test]
fn flr_tree() {
    check_flr("--run-tree");
}

#[test]
fn flr_vm() {
    check_flr("--run-vm");
}

// ── 5. PR #159 regression: prefix-binop in 3rd-arg position still works ────
// `slc xs i +i 1` — outer slc has arity 3; the `+i 1` is a prefix-binop arg.
const SLC_SRC: &str = "f>L n;ls=[10,20,30];i=0;slc ls i +i 1";

fn check_slc(engine: &str) {
    assert_eq!(run_ok(engine, SLC_SRC, "f", &[]), "[10]", "engine={engine}");
}

#[test]
fn slc_prefix_arg_still_works_tree() {
    check_slc("--run-tree");
}

#[test]
fn slc_prefix_arg_still_works_vm() {
    check_slc("--run-vm");
}

// ── 6. User function as inner call ─────────────────────────────────────────
// `dbl x:n>n;*x 2` then `g>n;prnt dbl 5` → prints "10".
const DBL_SRC: &str = "dbl x:n>n;*x 2\ng>n;prnt dbl 5;0\n";

fn check_dbl(engine: &str) {
    let out = run_ok(engine, DBL_SRC, "g", &[]);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.first().copied(), Some("10"), "engine={engine}");
}

#[test]
fn user_fn_inner_tree() {
    check_dbl("--run-tree");
}

#[test]
fn user_fn_inner_vm() {
    check_dbl("--run-vm");
}

// ── 7. HOF first-arg position stays a bare ref (PR #167 unchanged) ─────────
// `fld max xs 0` — max is a 2-arg builtin, but it's in fld's fn-ref slot
// so it must NOT eagerly consume `xs 0` as its args. xs must remain fld's
// second arg.
const FLD_MAX_SRC: &str = "f xs:L n>n;fld max xs 0";

fn check_fld_max(engine: &str) {
    assert_eq!(
        run_ok(engine, FLD_MAX_SRC, "f", &["[3,1,4,1,5,9,2,6]"]),
        "9",
        "engine={engine}"
    );
}

#[test]
fn fld_max_hof_tree() {
    check_fld_max("--run-tree");
}
// VM/Cranelift don't yet support bare-builtin HOF dispatch
// (see tests/regression_builtins_as_hof.rs), so only tree is exercised here.

// ── 8. Long-form alias keeps HOF semantics ─────────────────────────────────
// `filter pos xs` — `filter` is an alias for `flt`; first arg is a fn-ref.
const FILTER_SRC: &str = "pos x:n>b;>x 0\nmain xs:L n>L n;filter pos xs\n";

fn check_filter(engine: &str) {
    assert_eq!(
        run_ok(engine, FILTER_SRC, "main", &["[-3,0,2,4]"]),
        "[2, 4]",
        "engine={engine}"
    );
}

#[test]
fn filter_alias_tree() {
    check_filter("--run-tree");
}
// VM dispatch for bare user-fn HOF args isn't implemented yet — tree only.
