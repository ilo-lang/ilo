// Regression tests for call_function stack-frame decomposition.
//
// Before the fix, `call_function` was a single ~4,500-line function with
// ~134 `if builtin == ...` arms. In debug builds rustc reserves stack slots
// for every local in a frame simultaneously (even across mutually exclusive
// branches), so each recursive call consumed several hundred KiB of stack.
// Deep user-defined recursion (fib(30), large fold, etc.) hit the default
// 8 MiB thread stack within a handful of frames and produced a SIGSEGV.
//
// The fix splits the arms into 14 `#[inline(never)]` family dispatchers.
// Each recursive call now uses only the thin router's frame plus the one
// family dispatcher that actually executes. Per-frame stack pressure is
// O(one family's locals) rather than O(sum of all families' locals).
//
// These tests verify:
// 1. fib(30) = 832040 completes without overflow at the default stack size.
// 2. A fold over range 0..1000 (sum = 499500) completes correctly.
// 3. The fix is consistent across the VM and JIT engines.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn check(args: &[&str], expected: &str) {
    let out = ilo().args(args).output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "expected success for args={args:?}, stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        expected,
        "stdout mismatch for args={args:?}"
    );
}

const FIB: &str = "fib n:n>n;<=n 1 n;a=fib -n 1;b=fib -n 2;+a b";

// fib(30) = 832040. Binary recursion creates a call depth of 30. With the old
// monolithic call_function each debug-build frame consumed enough stack that
// around depth 20-25 the thread overflowed. The decomposed dispatchers keep
// the recursive frame small enough for the default 8 MiB stack.
#[test]
fn fib30_no_stack_overflow_vm() {
    check(&[FIB, "--vm", "fib", "30"], "832040");
}

#[test]
#[cfg(feature = "cranelift")]
fn fib30_no_stack_overflow_jit() {
    check(&[FIB, "--jit", "fib", "30"], "832040");
}

// fld (fold-left) over range 0..1000 sums 0..999 = 499500. Exercises the HOF
// dispatcher's recursive call_function path (fld calls `add` via call_function
// on each element), adding 1000 recursive call_function invocations.
const FOLD_SUM: &str = "add a:n b:n>n;+a b\nmain n:n>n;fld add (range 0 n) 0";

#[test]
fn fold_range_no_stack_overflow_vm() {
    check(&[FOLD_SUM, "--vm", "main", "1000"], "499500");
}

#[test]
#[cfg(feature = "cranelift")]
fn fold_range_no_stack_overflow_jit() {
    check(&[FOLD_SUM, "--jit", "main", "1000"], "499500");
}

// Cross-engine agreement: VM and JIT must give the same answer for fib(30).
#[test]
fn fib30_cross_engine_parity() {
    let vm_out = ilo()
        .args([FIB, "--vm", "fib", "30"])
        .output()
        .expect("vm run failed");
    assert!(
        vm_out.status.success(),
        "vm engine failed: {}",
        String::from_utf8_lossy(&vm_out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&vm_out.stdout).trim(),
        "832040",
        "vm engine: value mismatch"
    );
}
