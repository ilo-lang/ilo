// Regression tests for list-literal register overflow.
//
// `Expr::List` originally emitted a single OP_LISTNEW that read items from
// consecutive registers `a+1..a+n`. That required `next_reg + n <= 255`,
// so a long list literal — or a moderate one preceded by many locals —
// hit a hard `assert!` panic at compile time on the VM and Cranelift
// backends. The tree-walk interpreter never had the problem (no register
// file). A 200-element top-level list literal, or a ~77-element literal
// with ~180 leading locals, reproduced it.
//
// The fix keeps the fast OP_LISTNEW path when the items fit contiguously
// and falls back to `OP_LISTNEW a, 0` + per-item `OP_LISTAPPEND` when
// they don't. The append path only needs two registers regardless of
// list size.
//
// These tests pin the cross-engine behaviour at sizes spanning both
// branches (small fast path through to large fallback) plus the
// many-leading-locals shape that matched the original repro.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, func: &str) -> String {
    let out = ilo()
        .args([src, engine, func])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} on `{func}` failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Build a source string with a single function whose body is
/// `len [0 1 2 ... n-1]`, returning the length.
fn len_source(n: usize) -> String {
    let items: Vec<String> = (0..n).map(|i| i.to_string()).collect();
    format!("go>n;len [{}]", items.join(" "))
}

/// Source that bumps the register file with ~180 leading locals before
/// emitting a 77-element list literal. Matches the shape that surfaced
/// the bug in agent-written code where prior bindings had eaten most
/// of the 255-register budget.
fn leading_locals_source() -> String {
    let locals: Vec<String> = (0..180).map(|i| format!("a{i}=0")).collect();
    let items: Vec<String> = (0..77).map(|i| i.to_string()).collect();
    format!("go>n;{};len [{}]", locals.join(";"), items.join(" "))
}

fn check_len(engine: &str, n: usize) {
    let src = len_source(n);
    let out = run(engine, &src, "go");
    assert_eq!(
        out,
        n.to_string(),
        "{engine} n={n}: expected `{n}`, got {out:?}"
    );
}

fn check_leading_locals(engine: &str) {
    let src = leading_locals_source();
    let out = run(engine, &src, "go");
    assert_eq!(
        out, "77",
        "{engine}: expected `77` from 180-locals + 77-elem literal, got {out:?}"
    );
}

// Size sweep covers: small (fits fast path trivially), medium (still
// fast path), the original 77 that failed under load, and two sizes
// that defeat the fast path even at pre_reg=0.
const SIZES: &[usize] = &[1, 64, 77, 256, 1024];

#[test]
fn listlit_size_sweep_tree() {
    for &n in SIZES {
        check_len("--run-tree", n);
    }
}

#[test]
fn listlit_size_sweep_vm() {
    for &n in SIZES {
        check_len("--run-vm", n);
    }
}

#[test]
#[cfg(feature = "cranelift")]
fn listlit_size_sweep_cranelift() {
    for &n in SIZES {
        check_len("--run-cranelift", n);
    }
}

#[test]
fn listlit_leading_locals_tree() {
    check_leading_locals("--run-tree");
}

#[test]
fn listlit_leading_locals_vm() {
    check_leading_locals("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn listlit_leading_locals_cranelift() {
    check_leading_locals("--run-cranelift");
}
