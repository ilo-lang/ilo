// Regression tests for FnRef plumbing (PR 1 of the VM/Cranelift HOF
// dispatch effort).
//
// Background: until this PR, `Value::FnRef(name)` could not survive a
// NanVal round-trip — `NanVal::from_value` lossily encoded it as
// `heap_string("<fn:name>")`, so any function name used as a value
// (passed to a HOF, returned, stored) was a dead string on the VM and
// Cranelift engines. HOFs (`map`, `flt`, `fld`, ...) were tree-only as
// a consequence.
//
// PR 1 introduces:
//   - A non-singleton QNAN tag (`TAG_FNREF`) for function references.
//   - `OP_LOADFN` to materialise a FnRef into a register from a bytecode
//     immediate (kind + id).
//   - `OP_CALL_DYN` to call a FnRef-holding register dynamically (PR 2
//     onward will start emitting it; PR 1 just wires the dispatcher).
//   - Compiler emission for `Expr::Ref` of a function or builtin name.
//
// HOFs are still emitted as tree-only (those land in PR 2/3). The tests
// here pin the round-trip behaviour and the compiler's choice of
// OP_LOADFN over the previous "undefined variable" error path.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(name: &str, src: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("ilo_fnref_{name}_{}_{n}.ilo", std::process::id()));
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

// ── User function as a value: round-trips across every engine ──────────
//
// `mk` returns the user function `sq` as a value. Every engine must
// render it as `<fn:sq>` (Value::FnRef display), not `"<fn:sq>"` as a
// dead string (the pre-PR-1 bug).

const USER_FNREF_SRC: &str = "sq x:n>n;*x x\nmk>F n n;sq";

#[test]
fn user_fnref_round_trip_tree() {
    assert_eq!(run_ok("--run-tree", USER_FNREF_SRC, "mk", &[]), "<fn:sq>");
}

#[test]
fn user_fnref_round_trip_vm() {
    assert_eq!(run_ok("--run-vm", USER_FNREF_SRC, "mk", &[]), "<fn:sq>");
}

#[test]
fn user_fnref_round_trip_cranelift() {
    assert_eq!(
        run_ok("--run-cranelift", USER_FNREF_SRC, "mk", &[]),
        "<fn:sq>"
    );
}

// ── Builtin name as a value: same story ────────────────────────────────
//
// `mk` returns the builtin `abs` as a value. Pure builtins are
// verifier-promoted to `Ty::Fn` (#165), so the Ref position type-checks
// on every engine.

const BUILTIN_FNREF_SRC: &str = "mk>F n n;abs";

#[test]
fn builtin_fnref_round_trip_tree() {
    assert_eq!(
        run_ok("--run-tree", BUILTIN_FNREF_SRC, "mk", &[]),
        "<fn:abs>"
    );
}

#[test]
fn builtin_fnref_round_trip_vm() {
    assert_eq!(run_ok("--run-vm", BUILTIN_FNREF_SRC, "mk", &[]), "<fn:abs>");
}

#[test]
fn builtin_fnref_round_trip_cranelift() {
    assert_eq!(
        run_ok("--run-cranelift", BUILTIN_FNREF_SRC, "mk", &[]),
        "<fn:abs>"
    );
}

// ── FnRef stored in a local, then returned ──────────────────────────────
//
// Exercises the path where OP_LOADFN's result lives in a register
// across other instructions before reaching OP_RET. The pre-PR-1
// emitter raised "undefined variable" for the `f=sq` binding.

const FNREF_BIND_SRC: &str = "sq x:n>n;*x x\nmk>F n n;f=sq;f";

#[test]
fn fnref_bind_then_return_tree() {
    assert_eq!(run_ok("--run-tree", FNREF_BIND_SRC, "mk", &[]), "<fn:sq>");
}

#[test]
fn fnref_bind_then_return_vm() {
    assert_eq!(run_ok("--run-vm", FNREF_BIND_SRC, "mk", &[]), "<fn:sq>");
}

#[test]
fn fnref_bind_then_return_cranelift() {
    assert_eq!(
        run_ok("--run-cranelift", FNREF_BIND_SRC, "mk", &[]),
        "<fn:sq>"
    );
}
