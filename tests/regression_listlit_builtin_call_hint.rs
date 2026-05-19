// Regression tests for ILO-P101 diagnostic: list-literal element that
// starts with a variadic or arity-unknown builtin (e.g. `fmt`, `fmt2`)
// followed by operands. Before this fix, `[k str c fmt2 rv 2]` fell
// through as a bare Ref `fmt2` and the verifier emitted a misleading
// ILO-T004 "undefined variable 'fmt2'" - pointing the agent at typos
// rather than the real fix (wrap in parens or bind first).
//
// Source: data-wrangler rerun10 in /Users/dan/code/ilo_assessment_feedback.md.
//
// The fix is a parse-time diagnostic: when in list-literal mode and a
// bare ref name is `Builtin::is_builtin(name)` true with operands
// following, emit ILO-P101 with a hint pointing at parens or bind-first.
//
// Cross-engine: ILO-P101 fires in the parser, so output is identical
// across tree / vm / jit. We still run all three to confirm none of the
// engines diverge on the parser path (e.g. a backend-specific re-parse
// that misses the new check).

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(name: &str, src: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "ilo_listlit_p101_{name}_{}_{n}.ilo",
        std::process::id()
    ));
    std::fs::write(&path, src).expect("write src");
    path
}

fn run_capture(engine: &str, src: &str, entry: &str, args: &[&str]) -> (bool, String, String) {
    let path = write_src(entry, src);
    let mut cmd = ilo();
    cmd.arg(&path).arg(engine).arg(entry);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn run_ok(engine: &str, src: &str, entry: &str, args: &[&str]) -> String {
    let (ok, stdout, stderr) = run_capture(engine, src, entry, args);
    assert!(ok, "ilo {engine} failed: stderr={stderr}");
    stdout.trim().to_string()
}

// --- Repro: variadic builtin (fmt2) inside list literal -----------------
//
// The exact shape from data-wrangler rerun10: a CSV row built inline,
// mixing locals (`k`, `c`) with a formatted-number column.

const FMT2_IN_LIST: &str = "f rv:n>L t;k=\"foo\";c=\"bar\";[k c fmt2 rv 2]";

fn check_fmt2_hint(engine: &str) {
    let (ok, stdout, stderr) = run_capture(engine, FMT2_IN_LIST, "f", &["3.14"]);
    assert!(!ok, "fmt2 inside list literal must reject at parse time");
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("ILO-P101"),
        "expected ILO-P101 in output, got: {combined}"
    );
    assert!(
        combined.contains("fmt2"),
        "diagnostic should name the offending builtin, got: {combined}"
    );
    // The hint should mention parens or bind-first - both shapes are
    // documented in the registry entry.
    assert!(
        combined.contains("paren") || combined.contains("(") || combined.contains("bind"),
        "diagnostic should suggest parens or bind-first, got: {combined}"
    );
}

// --- Repro: bare `fmt` inside list literal ------------------------------
//
// Same trap with `fmt` (the template form). `fmt` is also variadic and
// not in the parser's arity table.

const FMT_IN_LIST: &str = "f v:n>L t;k=\"foo\";[k fmt \"x={}\" v]";

fn check_fmt_hint(engine: &str) {
    let (ok, _stdout, stderr) = run_capture(engine, FMT_IN_LIST, "f", &["1"]);
    assert!(!ok, "fmt inside list literal must reject at parse time");
    let combined = stderr;
    assert!(
        combined.contains("ILO-P101"),
        "expected ILO-P101 for fmt inside list, got: {combined}"
    );
}

// --- Workaround A: parens make the call one element --------------------

const FMT2_PARENS: &str = "f rv:n>L t;k=\"foo\";[k (fmt2 rv 2)]";

fn check_parens_workaround(engine: &str) {
    let out = run_ok(engine, FMT2_PARENS, "f", &["3.14"]);
    assert_eq!(
        out, "[foo, 3.14]",
        "parens wrap fmt2 as one list element {engine}"
    );
}

// --- Workaround B: bind-first --------------------------------------------

const FMT2_BIND: &str = "f rv:n>L t;k=\"foo\";s=fmt2 rv 2;[k s]";

fn check_bind_workaround(engine: &str) {
    let out = run_ok(engine, FMT2_BIND, "f", &["3.14"]);
    assert_eq!(
        out, "[foo, 3.14]",
        "bind-first works for fmt2 in list {engine}"
    );
}

// --- Existing arity-known behaviour still works (no regression) ---------
//
// `str` is a 1-arg builtin in fn_arity, so `[str a str b]` keeps eating
// exactly one operand per call - the listlit_fnref_greedy invariant.

const STR_TRIO: &str = "f>L t;a=1;b=2;c=3;[str a str b str c]";

fn check_known_arity_unchanged(engine: &str) {
    let out = run_ok(engine, STR_TRIO, "f", &[]);
    assert_eq!(
        out, "[1, 2, 3]",
        "known-arity builtins still auto-expand {engine}"
    );
}

// --- Existing bare-ref behaviour still works (no regression) ------------
//
// Locals stay as elements; the diagnostic must NOT fire on
// non-builtin idents.

const BARE_LOCALS: &str = "f>L n;a=1;b=2;c=3;[a b c]";

fn check_bare_locals_unchanged(engine: &str) {
    let out = run_ok(engine, BARE_LOCALS, "f", &[]);
    assert_eq!(out, "[1, 2, 3]", "bare locals unaffected {engine}");
}

fn check_all(engine: &str) {
    check_fmt2_hint(engine);
    check_fmt_hint(engine);
    check_parens_workaround(engine);
    check_bind_workaround(engine);
    check_known_arity_unchanged(engine);
    check_bare_locals_unchanged(engine);
}

#[test]
fn listlit_builtin_call_hint_vm() {
    check_all("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn listlit_builtin_call_hint_cranelift() {
    check_all("--jit");
}
