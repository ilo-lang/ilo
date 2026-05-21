// Regression tests pinning the documented precedence of `??` inside
// prefix-binop chains.
//
// `+a ??d b` parses as `a + (d ?? b)`, NOT `(a ?? d) + b`. This is
// consistent with `+a *b c` = `a + (b*c)` — a prefix op in the
// right-operand slot consumes its own operands greedily. The trap
// is documented in SPEC.md > Operators > Special infix and in the
// `?? precedence trap` section of the site prefix-notation page.
//
// Source: pending P2 #16b (assessment), found by `address-activity`
// persona who wrote `+(cv|0) r-val` thinking `cv|0` would default
// `cv` first. The fix is doc-only — the parse is well-defined and
// unchanged; these tests pin the documented behaviour across every
// engine so future parser refactors can't silently flip the grouping.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_file(engine: &str, src: &str, entry: &str, args: &[&str]) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("ilo_nc_prec_{}_{}.ilo", std::process::id(), seq));
    std::fs::write(&path, src).unwrap();
    let mut cmd = ilo();
    cmd.arg(path.to_str().unwrap()).arg(engine).arg(entry);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for entry={entry} args={args:?}: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// Documented grouping: `+a ??d b` = `a + (d ?? b)`.
// `d` is a plain `n` so `d ?? b` = `d`, so result = a + d.
// With a=1, d=10, b=5 → 11 (NOT 6, which would be `(a??d) + b`).
const TRAP: &str = "trap a:n d:n b:n>n;+a ??d b\n";

// Bind-first canonical form: `(a ?? d) + b`.
// With a=nil, d=10, b=5 → 15.
// With a=1,   d=10, b=5 → 6.
const BIND: &str = "bind a:O n d:n b:n>n;x=??a d;+x b\n";

// Parens form: `(??a d) + b` = `(a ?? d) + b`.
const PARENS: &str = "parens a:O n d:n b:n>n;+(??a d) b\n";

fn check_all(engine: &str) {
    // `+a ??d b` = a + (d ?? b). d is not nil, so result = a + d = 11.
    assert_eq!(
        run_file(engine, TRAP, "trap", &["1", "10", "5"]),
        "11",
        "trap engine={engine}: `+a ??d b` should parse as a + (d ?? b)"
    );

    // bind-first: nil ?? 10 = 10, + 5 = 15
    assert_eq!(
        run_file(engine, BIND, "bind", &["nil", "10", "5"]),
        "15",
        "bind nil engine={engine}"
    );
    // bind-first with value: 1 ?? 10 = 1, + 5 = 6
    assert_eq!(
        run_file(engine, BIND, "bind", &["1", "10", "5"]),
        "6",
        "bind val engine={engine}"
    );

    // parens form behaves identically to bind-first
    assert_eq!(
        run_file(engine, PARENS, "parens", &["nil", "10", "5"]),
        "15",
        "parens nil engine={engine}"
    );
    assert_eq!(
        run_file(engine, PARENS, "parens", &["1", "10", "5"]),
        "6",
        "parens val engine={engine}"
    );
}

#[test]
fn nil_coalesce_precedence_vm() {
    check_all("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn nil_coalesce_precedence_cranelift() {
    check_all("--jit");
}
