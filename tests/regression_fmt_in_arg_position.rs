// Regression tests for the trailing-only `fmt`-in-arg-position parser rule.
//
// Before this change, `prnt fmt "x={}" 42` failed with ILO-T004 + ILO-T006
// because `fmt` was deliberately absent from the parser's arity table and
// fell through to a bare `Ref("fmt")`, leaving the outer call to greedily
// swallow `fmt`'s template + args as extra outer args.
//
// New rule (single sentence): when `fmt` appears as a NESTED arg of a
// known-arity outer, it must be the LAST slot of that outer; in that
// position it eagerly consumes the template + every remaining operand as
// its own args. In any middle slot it raises ILO-P018 with a precise
// hint ("wrap in parens").
//
// Cross-engine: tree, vm, and (when enabled) cranelift JIT — the parser
// change is shared across all three pipelines, so each engine must see
// the same AST and produce the same output.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(name: &str, src: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("ilo_fmtarg_{name}_{}_{n}.ilo", std::process::id()));
    std::fs::write(&path, src).expect("write src");
    path
}

fn run_ok(engine: &str, src: &str, entry: &str) -> String {
    let path = write_src(entry, src);
    let out = ilo()
        .arg(&path)
        .arg(engine)
        .arg(entry)
        .output()
        .expect("failed to run ilo");
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

const ENGINES: &[&str] = &["--run-tree", "--run-vm"];

#[cfg(feature = "cranelift")]
const CRANELIFT_ENGINE: &str = "--run-cranelift";

// ── 1) Trailing fmt at slot 0 of arity-1 outer ────────────────────────────

const PRNT_FMT: &str = "f>n;prnt fmt \"x={}\" 42; 0";

fn check_prnt_fmt(engine: &str) {
    assert_eq!(run_ok(engine, PRNT_FMT, "f"), "x=42\n0", "engine={engine}");
}

#[test]
fn prnt_fmt_tree() {
    check_prnt_fmt("--run-tree");
}

#[test]
fn prnt_fmt_vm() {
    check_prnt_fmt("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn prnt_fmt_cranelift() {
    check_prnt_fmt(CRANELIFT_ENGINE);
}

// ── 2) Trailing fmt at slot 0 of every text-arg arity-1 builtin ───────────
//
// `trm`, `upr`, `lwr`, `cap` all take a single text arg. `fmt` returns
// text, so each wraps the format result.

#[test]
fn trm_upr_lwr_cap_fmt_all_engines() {
    let cases = [
        ("trm", "f>t;trm fmt \"  x={}  \" 1", "x=1"),
        ("upr", "f>t;upr fmt \"x={}\" 1", "X=1"),
        ("lwr", "f>t;lwr fmt \"X={}\" 1", "x=1"),
        ("cap", "f>t;cap fmt \"hello {}\" 1", "Hello 1"),
    ];
    for (label, src, want) in cases {
        for engine in ENGINES {
            assert_eq!(run_ok(engine, src, "f"), want, "{label} engine={engine}");
        }
        #[cfg(feature = "cranelift")]
        {
            assert_eq!(
                run_ok(CRANELIFT_ENGINE, src, "f"),
                want,
                "{label} engine=cranelift"
            );
        }
    }
}

// ── 3) Trailing fmt at slot 1 of arity-2 outer `wr` ───────────────────────
//
// `wr path text` writes `text` to `path`. With the trailing rule,
// `wr "..." fmt "..." 1` parses as `wr("...", fmt("...", 1))`.

fn wr_fmt_temp(engine: &str) -> String {
    let tmp = std::env::temp_dir().join(format!(
        "ilo_wrfmt_{}_{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let path_str = tmp.to_string_lossy().to_string().replace('\\', "\\\\");
    let src = format!("f>n;wr \"{path_str}\" fmt \"x={{}}\" 42; 0");
    let _ = std::fs::remove_file(&tmp);
    let stdout = run_ok(engine, &src, "f");
    let body = std::fs::read_to_string(&tmp).expect("file written");
    let _ = std::fs::remove_file(&tmp);
    assert_eq!(stdout, "0", "engine={engine}");
    body
}

#[test]
fn wr_fmt_tree() {
    assert_eq!(wr_fmt_temp("--run-tree"), "x=42");
}

#[test]
fn wr_fmt_vm() {
    assert_eq!(wr_fmt_temp("--run-vm"), "x=42");
}

#[test]
#[cfg(feature = "cranelift")]
fn wr_fmt_cranelift() {
    assert_eq!(wr_fmt_temp(CRANELIFT_ENGINE), "x=42");
}

// ── 4) Nested: prnt upr fmt — fmt at slot 0 of slot-0-of-prnt's `upr` ─────
//
// `upr` is arity 1, `fmt` is at its trailing slot 0. `prnt` is arity 1,
// `upr fmt ...` is at its trailing slot 0. Both layers cascade.

const PRNT_UPR_FMT: &str = "f>n;prnt upr fmt \"x={}\" 1; 0";

fn check_prnt_upr_fmt(engine: &str) {
    assert_eq!(
        run_ok(engine, PRNT_UPR_FMT, "f"),
        "X=1\n0",
        "engine={engine}"
    );
}

#[test]
fn prnt_upr_fmt_tree() {
    check_prnt_upr_fmt("--run-tree");
}

#[test]
fn prnt_upr_fmt_vm() {
    check_prnt_upr_fmt("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn prnt_upr_fmt_cranelift() {
    check_prnt_upr_fmt(CRANELIFT_ENGINE);
}

// ── 5) Bare top-level fmt call still parses as a multi-arg call ───────────
//
// Sanity: the rule must not regress fmt-as-outer.

const FMT_BARE: &str = "f>t;fmt \"x={}\" 42";

fn check_fmt_bare(engine: &str) {
    assert_eq!(run_ok(engine, FMT_BARE, "f"), "x=42", "engine={engine}");
}

#[test]
fn fmt_bare_tree() {
    check_fmt_bare("--run-tree");
}

#[test]
fn fmt_bare_vm() {
    check_fmt_bare("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn fmt_bare_cranelift() {
    check_fmt_bare(CRANELIFT_ENGINE);
}

// ── 6) fmt inside a list literal still works (comma-delimited) ────────────

const FMT_IN_LIST: &str = "f>n;prnt [fmt \"x={}\" 1, 2]; 0";

fn check_fmt_in_list(engine: &str) {
    assert_eq!(
        run_ok(engine, FMT_IN_LIST, "f"),
        "[x=1, 2]\n0",
        "engine={engine}"
    );
}

#[test]
fn fmt_in_list_tree() {
    check_fmt_in_list("--run-tree");
}

#[test]
fn fmt_in_list_vm() {
    check_fmt_in_list("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn fmt_in_list_cranelift() {
    check_fmt_in_list(CRANELIFT_ENGINE);
}

// ── 7) Parenthesised fmt remains valid in any slot ────────────────────────
//
// The paren idiom is the escape hatch for middle-position use and must not
// regress.

const FMT_PAREN: &str = "f>n;prnt (fmt \"x={}\" 42); 0";

fn check_fmt_paren(engine: &str) {
    assert_eq!(run_ok(engine, FMT_PAREN, "f"), "x=42\n0", "engine={engine}");
}

#[test]
fn fmt_paren_tree() {
    check_fmt_paren("--run-tree");
}

#[test]
fn fmt_paren_vm() {
    check_fmt_paren("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn fmt_paren_cranelift() {
    check_fmt_paren(CRANELIFT_ENGINE);
}

// ── 8) Middle-position fmt emits ILO-P018 with a paren-suggestion hint ────
//
// `slc` is arity 3 (`slc list start end`). `fmt` at slot 0 is a middle
// slot. Same for `rgxsub` (arity 3) at slot 0.

#[test]
fn fmt_middle_slc_slot0_emits_p018_tree() {
    let src = "f>L n;slc fmt \"x={}\" 1 0 2";
    let err = run_err("--run-tree", src, "f");
    assert!(
        err.contains("ILO-P018") && err.contains("must be the last argument to `slc`"),
        "missing ILO-P018 with slc hint: {err}"
    );
    assert!(
        err.contains("wrap in parens"),
        "missing paren suggestion: {err}"
    );
}

#[test]
fn fmt_middle_rgxsub_slot0_emits_p018_tree() {
    let src = "f>t;rgxsub fmt \"p={}\" 1 \"abc\" \"X\"";
    let err = run_err("--run-tree", src, "f");
    assert!(
        err.contains("ILO-P018") && err.contains("must be the last argument to `rgxsub`"),
        "missing ILO-P018 with rgxsub hint: {err}"
    );
}

#[test]
fn fmt_middle_user_fn_slot1_emits_p018_tree() {
    // User-defined 3-arg fn — `fmt` at slot 1 is a middle slot, must be
    // rejected with the same diagnostic shape.
    let src = "g x:t y:t z:t>t;cat x cat y z\nf>t;g \"a\" fmt \"b={}\" 1 \"c\"";
    let err = run_err("--run-tree", src, "f");
    assert!(
        err.contains("ILO-P018"),
        "missing ILO-P018 for user-fn middle slot: {err}"
    );
    assert!(
        err.contains("slot 1 of 3"),
        "missing precise slot indicator: {err}"
    );
}

// The diagnostic is a parser error — same code surfaces identically under
// every engine (parsing happens before engine dispatch). Smoke-check on
// VM and Cranelift to confirm there's no divergence in how the error is
// surfaced (e.g. exit code, code string).

#[test]
fn fmt_middle_p018_vm() {
    let src = "f>L n;slc fmt \"x={}\" 1 0 2";
    let err = run_err("--run-vm", src, "f");
    assert!(err.contains("ILO-P018"), "vm missing ILO-P018: {err}");
}

#[test]
#[cfg(feature = "cranelift")]
fn fmt_middle_p018_cranelift() {
    let src = "f>L n;slc fmt \"x={}\" 1 0 2";
    let err = run_err(CRANELIFT_ENGINE, src, "f");
    assert!(
        err.contains("ILO-P018"),
        "cranelift missing ILO-P018: {err}"
    );
}

// ── 9) Paren-around-fmt rescues a middle-position use ─────────────────────
//
// The diagnostic suggests "wrap in parens". Verify that paren-wrapping
// actually fixes the program (so the suggestion isn't a lie).

const SLC_FMT_PAREN: &str = "f>t;slc (fmt \"abcdef={}\" 9) 0 5";

fn check_slc_fmt_paren(engine: &str) {
    // fmt produces "abcdef=9", slc 0..5 = "abcde".
    assert_eq!(
        run_ok(engine, SLC_FMT_PAREN, "f"),
        "abcde",
        "engine={engine}"
    );
}

#[test]
fn slc_fmt_paren_tree() {
    check_slc_fmt_paren("--run-tree");
}

#[test]
fn slc_fmt_paren_vm() {
    check_slc_fmt_paren("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn slc_fmt_paren_cranelift() {
    check_slc_fmt_paren(CRANELIFT_ENGINE);
}
