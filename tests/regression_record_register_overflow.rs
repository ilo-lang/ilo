// Regression tests for record-literal and `with`-update register overflow.
//
// `Expr::Record` originally emitted a single OP_RECNEW that read field
// values from consecutive registers `a+1..a+n_fields`. `Expr::With`
// emitted OP_RECWITH with the same layout for the update values. Both
// required `next_reg + n <= 255`, so any record with more than ~127
// fields, or a moderate one preceded by many locals, hit a hard
// `assert!` panic at compile time on the VM and Cranelift backends.
// The tree-walk interpreter was unaffected (no register file).
//
// The fix mirrors the OP_LISTNEW/OP_LISTAPPEND split from PR #237:
// when the contiguous-register layout would overflow, the compiler
// falls back to OP_RECNEW_EMPTY + per-field OP_RECSETFIELD for record
// literals, and OP_RECCOPY + per-update OP_RECSETFIELD for `with`
// expressions. Both fallbacks only need two registers (result +
// scratch) regardless of field count.
//
// These tests pin the cross-engine behaviour at sizes spanning both
// branches plus the many-leading-locals shape that matched the
// original repro.

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

/// Build a source string with a `big` type of `n` fields and a function
/// that constructs the record assigning `f<i>` = `i`, then returns
/// `f<probe>`.
fn record_source(n: usize, probe: usize) -> String {
    let type_fields: Vec<String> = (0..n).map(|i| format!("f{i}:n")).collect();
    let inits: Vec<String> = (0..n).map(|i| format!("f{i}:{i}")).collect();
    format!(
        "type big{{{}}}\ngo>n;r=big {};r.f{}",
        type_fields.join(";"),
        inits.join(" "),
        probe,
    )
}

/// Source that bumps the register file with ~180 leading locals before
/// emitting a 60-field record literal. Matches the shape that surfaced
/// the bug in agent-written code where prior bindings had eaten most
/// of the 255-register budget.
fn record_leading_locals_source() -> String {
    let locals: Vec<String> = (0..180).map(|i| format!("a{i}=0")).collect();
    let n = 60;
    let type_fields: Vec<String> = (0..n).map(|i| format!("f{i}:n")).collect();
    let inits: Vec<String> = (0..n).map(|i| format!("f{i}:{i}")).collect();
    format!(
        "type big{{{}}}\ngo>n;{};r=big {};r.f50",
        type_fields.join(";"),
        locals.join(";"),
        inits.join(" "),
    )
}

/// Build a source string that exercises `with`: a `big` record of `n`
/// fields starts all-zero, then a single `with` updates every field to
/// its index. Reads back `f<probe>` to confirm the update landed.
fn with_source(n: usize, probe: usize) -> String {
    let type_fields: Vec<String> = (0..n).map(|i| format!("f{i}:n")).collect();
    let zeroes: Vec<String> = (0..n).map(|i| format!("f{i}:0")).collect();
    let updates: Vec<String> = (0..n).map(|i| format!("f{i}:{i}")).collect();
    format!(
        "type big{{{}}}\ngo>n;r=big {};q=r with {};q.f{}",
        type_fields.join(";"),
        zeroes.join(" "),
        updates.join(" "),
        probe,
    )
}

/// `with` companion to record_leading_locals_source.
fn with_leading_locals_source() -> String {
    let locals: Vec<String> = (0..180).map(|i| format!("a{i}=0")).collect();
    let n = 60;
    let type_fields: Vec<String> = (0..n).map(|i| format!("f{i}:n")).collect();
    let zeroes: Vec<String> = (0..n).map(|i| format!("f{i}:0")).collect();
    let updates: Vec<String> = (0..n).map(|i| format!("f{i}:{i}")).collect();
    format!(
        "type big{{{}}}\ngo>n;{};r=big {};q=r with {};q.f50",
        type_fields.join(";"),
        locals.join(";"),
        zeroes.join(" "),
        updates.join(" "),
    )
}

fn check_record(engine: &str, n: usize, probe: usize) {
    let src = record_source(n, probe);
    let out = run(engine, &src, "go");
    assert_eq!(
        out,
        probe.to_string(),
        "{engine} record n={n} probe={probe}: expected `{probe}`, got {out:?}"
    );
}

fn check_with(engine: &str, n: usize, probe: usize) {
    let src = with_source(n, probe);
    let out = run(engine, &src, "go");
    assert_eq!(
        out,
        probe.to_string(),
        "{engine} with n={n} probe={probe}: expected `{probe}`, got {out:?}"
    );
}

fn check_record_leading_locals(engine: &str) {
    let src = record_leading_locals_source();
    let out = run(engine, &src, "go");
    assert_eq!(
        out, "50",
        "{engine}: expected `50` from 180-locals + 60-field record literal, got {out:?}"
    );
}

fn check_with_leading_locals(engine: &str) {
    let src = with_leading_locals_source();
    let out = run(engine, &src, "go");
    assert_eq!(
        out, "50",
        "{engine}: expected `50` from 180-locals + 60-field `with`, got {out:?}"
    );
}

// Size sweep covers: small (fits fast path trivially), medium (still
// fast path), the largest n_fields that fits in a u8 (255 — actually
// 220 to stay within the canonical-order register pressure during
// compile_expr), and the cases that defeat the fast path. 150 is well
// past the 127-ish ceiling where the contiguous-layout assert fires.
const SIZES: &[usize] = &[1, 16, 60, 150, 220];

#[test]
fn record_size_sweep_tree() {
    for &n in SIZES {
        let probe = n - 1;
        check_record("--run-tree", n, probe);
    }
}

#[test]
fn record_size_sweep_vm() {
    for &n in SIZES {
        let probe = n - 1;
        check_record("--run-vm", n, probe);
    }
}

#[test]
#[cfg(feature = "cranelift")]
fn record_size_sweep_cranelift() {
    for &n in SIZES {
        let probe = n - 1;
        check_record("--run-cranelift", n, probe);
    }
}

#[test]
fn with_size_sweep_tree() {
    for &n in SIZES {
        let probe = n - 1;
        check_with("--run-tree", n, probe);
    }
}

#[test]
fn with_size_sweep_vm() {
    for &n in SIZES {
        let probe = n - 1;
        check_with("--run-vm", n, probe);
    }
}

#[test]
#[cfg(feature = "cranelift")]
fn with_size_sweep_cranelift() {
    for &n in SIZES {
        let probe = n - 1;
        check_with("--run-cranelift", n, probe);
    }
}

#[test]
fn record_leading_locals_tree() {
    check_record_leading_locals("--run-tree");
}

#[test]
fn record_leading_locals_vm() {
    check_record_leading_locals("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn record_leading_locals_cranelift() {
    check_record_leading_locals("--run-cranelift");
}

#[test]
fn with_leading_locals_tree() {
    check_with_leading_locals("--run-tree");
}

#[test]
fn with_leading_locals_vm() {
    check_with_leading_locals("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn with_leading_locals_cranelift() {
    check_with_leading_locals("--run-cranelift");
}

// Adjacent coverage: confirm a large `with` preserves untouched fields
// from the source record (the fallback path runs OP_RECCOPY which must
// clone_rc every field, not just memcpy raw bits).
#[test]
fn with_preserves_untouched_fields_vm() {
    let n = 150;
    let type_fields: Vec<String> = (0..n).map(|i| format!("f{i}:n")).collect();
    let inits: Vec<String> = (0..n).map(|i| format!("f{i}:{i}")).collect();
    // Update only f0 — every other field must survive intact.
    let src = format!(
        "type big{{{}}}\ngo>n;r=big {};q=r with f0:999;q.f140",
        type_fields.join(";"),
        inits.join(" "),
    );
    let out = run("--run-vm", &src, "go");
    assert_eq!(out, "140");
}

#[test]
#[cfg(feature = "cranelift")]
fn with_preserves_untouched_fields_cranelift() {
    let n = 150;
    let type_fields: Vec<String> = (0..n).map(|i| format!("f{i}:n")).collect();
    let inits: Vec<String> = (0..n).map(|i| format!("f{i}:{i}")).collect();
    let src = format!(
        "type big{{{}}}\ngo>n;r=big {};q=r with f0:999;q.f140",
        type_fields.join(";"),
        inits.join(" "),
    );
    let out = run("--run-cranelift", &src, "go");
    assert_eq!(out, "140");
}

// Adjacent coverage: large `with` must NOT mutate the original record.
// The OP_RECCOPY fallback allocates fresh storage, then OP_RECSETFIELD
// mutates the new record only.
#[test]
fn with_does_not_mutate_original_vm() {
    let n = 150;
    let type_fields: Vec<String> = (0..n).map(|i| format!("f{i}:n")).collect();
    let zeroes: Vec<String> = (0..n).map(|i| format!("f{i}:0")).collect();
    let updates: Vec<String> = (0..n).map(|i| format!("f{i}:{i}")).collect();
    let src = format!(
        "type big{{{}}}\ngo>n;r=big {};q=r with {};r.f140",
        type_fields.join(";"),
        zeroes.join(" "),
        updates.join(" "),
    );
    let out = run("--run-vm", &src, "go");
    assert_eq!(out, "0", "original record should be untouched after with");
}

#[test]
#[cfg(feature = "cranelift")]
fn with_does_not_mutate_original_cranelift() {
    let n = 150;
    let type_fields: Vec<String> = (0..n).map(|i| format!("f{i}:n")).collect();
    let zeroes: Vec<String> = (0..n).map(|i| format!("f{i}:0")).collect();
    let updates: Vec<String> = (0..n).map(|i| format!("f{i}:{i}")).collect();
    let src = format!(
        "type big{{{}}}\ngo>n;r=big {};q=r with {};r.f140",
        type_fields.join(";"),
        zeroes.join(" "),
        updates.join(" "),
    );
    let out = run("--run-cranelift", &src, "go");
    assert_eq!(out, "0", "original record should be untouched after with");
}

// Adjacent coverage: large record literal containing heap field values
// (strings). Confirms clone_rc bookkeeping in OP_RECSETFIELD is correct
// across the fallback boundary.
#[test]
fn record_with_string_fields_vm() {
    let n = 150;
    let type_fields: Vec<String> = (0..n)
        .map(|i| {
            if i % 2 == 0 {
                format!("f{i}:t")
            } else {
                format!("f{i}:n")
            }
        })
        .collect();
    let inits: Vec<String> = (0..n)
        .map(|i| {
            if i % 2 == 0 {
                format!("f{i}:\"s{i}\"")
            } else {
                format!("f{i}:{i}")
            }
        })
        .collect();
    let src = format!(
        "type big{{{}}}\ngo>t;r=big {};r.f140",
        type_fields.join(";"),
        inits.join(" "),
    );
    let out = run("--run-vm", &src, "go");
    assert_eq!(out, "s140");
}

#[test]
#[cfg(feature = "cranelift")]
fn record_with_string_fields_cranelift() {
    let n = 150;
    let type_fields: Vec<String> = (0..n)
        .map(|i| {
            if i % 2 == 0 {
                format!("f{i}:t")
            } else {
                format!("f{i}:n")
            }
        })
        .collect();
    let inits: Vec<String> = (0..n)
        .map(|i| {
            if i % 2 == 0 {
                format!("f{i}:\"s{i}\"")
            } else {
                format!("f{i}:{i}")
            }
        })
        .collect();
    let src = format!(
        "type big{{{}}}\ngo>t;r=big {};r.f140",
        type_fields.join(";"),
        inits.join(" "),
    );
    let out = run("--run-cranelift", &src, "go");
    assert_eq!(out, "s140");
}
