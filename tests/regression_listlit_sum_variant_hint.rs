// Regression tests for ILO-T047 diagnostic: list-literal element that
// is a payload-carrying sum-variant constructor followed by what looks
// like its payload (ILO-467).
//
// Before the fix, `ms=[login "alice" logout "bob" heartbeat 5]` parsed
// the variant names as bare `FnRef`s and left the list with function
// references next to value-text where the agent meant constructor
// application. The verifier didn't flag this — the result was a silent
// heterogeneous list and confusing downstream type errors.
//
// The fix is a verify-time diagnostic at the list-literal element-
// resolution site (sum-variant knowledge is post-parse). Sibling of
// ILO-P101 for variadic builtins; same two canonical rewrites
// (paren-wrap, pre-bind).

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
        "ilo_listlit_t047_{name}_{}_{n}.ilo",
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

// --- Repro: sum-variant constructors inline in a list literal ----------

const VARIANTS_IN_LIST: &str = concat!(
    "type msg = login(t) | logout(t) | heartbeat(n)\n",
    "main>_;ms=[login \"alice\" logout \"bob\" heartbeat 5];prnt len ms",
);

fn check_variant_hint(engine: &str) {
    let (ok, stdout, stderr) = run_capture(engine, VARIANTS_IN_LIST, "main", &[]);
    assert!(
        !ok,
        "sum-variant constructors inline in list literal must trigger ILO-T047. stdout={stdout} stderr={stderr}"
    );
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("ILO-T047"),
        "expected ILO-T047 in output, got: {combined}"
    );
    // Hint names both canonical rewrites.
    assert!(
        combined.contains("paren") || combined.contains("("),
        "diagnostic should mention paren-wrap, got: {combined}"
    );
    assert!(
        combined.contains("bind"),
        "diagnostic should mention bind-first, got: {combined}"
    );
}

// --- Workaround A: paren-wrap each construction ------------------------

const VARIANTS_PARENS: &str = concat!(
    "type msg = login(t) | logout(t) | heartbeat(n)\n",
    "main>_;ms=[(login \"alice\") (logout \"bob\") (heartbeat 5)];prnt len ms",
);

fn check_parens_workaround(engine: &str) {
    let out = run_ok(engine, VARIANTS_PARENS, "main", &[]);
    assert_eq!(
        out, "3",
        "paren-wrapped variants produce a 3-element list {engine}"
    );
}

// --- Workaround B: pre-bind each variant -------------------------------

const VARIANTS_BIND: &str = concat!(
    "type msg = login(t) | logout(t) | heartbeat(n)\n",
    "main>_;a=login \"alice\";b=logout \"bob\";c=heartbeat 5;ms=[a b c];prnt len ms",
);

fn check_bind_workaround(engine: &str) {
    let out = run_ok(engine, VARIANTS_BIND, "main", &[]);
    assert_eq!(
        out, "3",
        "pre-bound variants produce a 3-element list {engine}"
    );
}

// --- Payload-less variants stay valid (no false positive) --------------
//
// `[ready done]` is a perfectly valid two-element list of payload-less
// variants. The diagnostic must NOT fire here.

const PAYLOADLESS_OK: &str = concat!(
    "type state = ready | done | busy\n",
    "main>_;ss=[ready done busy];prnt len ss",
);

fn check_payloadless_unchanged(engine: &str) {
    let out = run_ok(engine, PAYLOADLESS_OK, "main", &[]);
    assert_eq!(
        out, "3",
        "payload-less variants in list still work {engine}"
    );
}

fn check_all(engine: &str) {
    check_variant_hint(engine);
    check_parens_workaround(engine);
    check_bind_workaround(engine);
    check_payloadless_unchanged(engine);
}

#[test]
fn listlit_sum_variant_hint_vm() {
    check_all("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn listlit_sum_variant_hint_cranelift() {
    check_all("--jit");
}
