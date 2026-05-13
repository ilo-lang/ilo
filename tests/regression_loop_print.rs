// Top-level auto-print suppression for loop-tail function results.
//
// Background: `prnt v` prints v and returns v (passthrough). A function body
// whose last statement is a loop returns the loop's last body value. So a
// "print every item" loop like `@x xs{prnt x}` used to double-print: each item
// inside the loop, and then the loop's final body value (the last printed
// item) again from the top-level auto-print. Personas worked around this with
// trailing `+0 0` sentinels.
//
// Fix (print-layer only, `src/main.rs`): when the program's entry function
// body ends with `@`/`wh` AND has no early-return path, the plain-text top-
// level auto-print is suppressed. The check is *syntactic* — it looks at the
// AST tail, not at the runtime value — so a function that explicitly returns
// `nil` from a non-loop tail (e.g. `nothing>O n;nil`) still prints "nil".
// Function-internal loop-as-expression semantics are unchanged: a caller that
// consumes such a function's return value still sees the loop's last body
// value. JSON mode is untouched.
//
// This test suite pins:
//   1. Top-level print-loop: only the loop body's prints reach stdout (no
//      trailing duplicate).
//   2. Nested print-loop: caller observes the loop's last-body-value as the
//      callee's return — semantics inside functions are unchanged.
//   3. Empty-list loop at top level: nothing prints (loop tail catches the
//      Nil and suppresses; previously this leaked a `nil` line).
//   4. Loop with `brk` at top level: still suppressed.
//   5. While loop at top level: suppressed.
//   6. Trailing-expression sentinel still prints normally (the `+0 0` /
//      explicit-expression idiom must keep working — non-regression).
//   7. Early-return present + loop tail: top-level value still prints (we
//      can't tell at print time whether the value came from the loop or the
//      `ret`, so we err on the side of printing).
//   8. Explicit nil return from a non-loop tail still prints "nil" (the rule
//      is syntactic loop-tail, never blanket Nil).
//   9. JSON mode is unaffected: `--json` always emits a structured line.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(src: &str, tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "ilo_loop_print_{}_{}_{}.ilo",
        std::process::id(),
        seq,
        tag,
    ));
    std::fs::write(&path, src).unwrap();
    path
}

fn run_file(engine: &str, src: &str, fn_name: &str, args: &[&str]) -> String {
    let path = write_src(
        src,
        &format!("{}_{}", fn_name, engine.trim_start_matches("--")),
    );
    let mut cmd = ilo();
    cmd.arg(path.to_str().unwrap()).arg(engine).arg(fn_name);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for fn={fn_name} src=`{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Don't trim — leading/trailing newlines matter for "did it auto-print?"
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn run_file_json(engine: &str, src: &str, fn_name: &str) -> String {
    let path = write_src(
        src,
        &format!("{}_{}_json", fn_name, engine.trim_start_matches("--")),
    );
    let out = ilo()
        .arg(path.to_str().unwrap())
        .arg(engine)
        .arg("--json")
        .arg(fn_name)
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} --json failed for fn={fn_name}: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

// ── Sources ───────────────────────────────────────────────────────────────

// 1. Print-loop at top level. Used to print "1\n2\n3\n3\n".
const PRINT_LOOP: &str = "f>n;xs=[1 2 3];@x xs{prnt x}";

// 2. Print-loop nested in a caller. Outer function consumes the loop's
//    return value. `inner` must still return 3 so `+v 0` evaluates to 3.
const NESTED_PRINT_LOOP: &str = "inner>n;xs=[1 2 3];@x xs{prnt x}\nouter>n;v=inner();+v 0";

// 3. Empty-list loop at top level. Loop never runs, last_value stays Nil,
//    function returns Nil. Used to print "nil\n".
const EMPTY_LOOP: &str = "f>n;xs=[];@x xs{prnt x}";

// 4. brk-loop at top level. Loop prints the running sum, breaks when i>=3.
//    Function ends with the loop so the loop tail bubbles up. Suppressed.
//    Body ends with `prnt s` so the loop's value type is `n` (typechecker
//    requires the tail to resolve to the declared return type).
const BRK_LOOP: &str = "f>n;s=0;@i 0..10{>=i 3{brk};s=+s i;prnt s}";

// 5. While loop at top level. Body ends with `prnt i` for the same type
//    reason as BRK_LOOP — the loop's value must type-check as `n`.
const WHILE_LOOP: &str = "f>n;i=0;wh <i 3{i=+i 1;prnt i}";

// 6. Trailing-expression sentinel (e.g. function returns 42). Loop is NOT
//    the syntactic tail — there's a final `+42 0`. Must still print "42".
const TRAILING_SENTINEL: &str = "f>n;xs=[1 2 3];@x xs{prnt x};+42 0";

// 7. Early-return + loop tail. Guard early-returns 99 when n>0, otherwise
//    the loop tail runs. Suppression must NOT apply (body has a `ret`-like
//    path), so the value is printed for both branches.
const EARLY_RETURN_THEN_LOOP: &str = "f n:n>n;>n 0{ret 99};xs=[1 2 3];@x xs{prnt x}";

// 8. Explicit nil return. Pin that a function whose body-tail is a bare
//    `nil` (NOT a loop) still auto-prints "nil". Suppression must be
//    syntactic loop-tail only — never blanket-Nil — so legitimate
//    Optional-returning functions like `nothing>O n;nil` keep working.
const EXPLICIT_NIL: &str = "f>O n;nil";

// ── Engine-coverage harness ───────────────────────────────────────────────

fn check_all(engine: &str) {
    // 1. Top-level print-loop: only "1\n2\n3\n" — no trailing duplicate.
    let out = run_file(engine, PRINT_LOOP, "f", &[]);
    assert_eq!(
        out, "1\n2\n3\n",
        "print-loop double-output regression engine={engine}: got `{out}`"
    );

    // 2. Nested: outer consumes inner. Loop body prints 1,2,3; outer returns
    //    inner-return-value + 0 = 3, which DOES print because outer's tail is
    //    `+v 0`, not a loop.
    let out = run_file(engine, NESTED_PRINT_LOOP, "outer", &[]);
    assert_eq!(
        out, "1\n2\n3\n3\n",
        "nested print-loop must still propagate inner's return engine={engine}: got `{out}`"
    );

    // 3. Empty loop: no body prints, Nil suppressed → empty stdout.
    let out = run_file(engine, EMPTY_LOOP, "f", &[]);
    assert_eq!(
        out, "",
        "empty-loop nil should be suppressed engine={engine}: got `{out}`"
    );

    // 4. brk-loop: body prints running sum (0,1,3) then brk before i=3 in the
    //    next iteration. Loop tail value (3 from the last prnt) is suppressed.
    let out = run_file(engine, BRK_LOOP, "f", &[]);
    assert_eq!(
        out, "0\n1\n3\n",
        "brk-loop tail suppressed engine={engine}: got `{out}`"
    );

    // 5. while-loop: body increments and prints, prints 1,2,3. No trailing dupe.
    let out = run_file(engine, WHILE_LOOP, "f", &[]);
    assert_eq!(
        out, "1\n2\n3\n",
        "while-loop double-output engine={engine}: got `{out}`"
    );

    // 6. Trailing sentinel: must still auto-print 42.
    let out = run_file(engine, TRAILING_SENTINEL, "f", &[]);
    assert_eq!(
        out, "1\n2\n3\n42\n",
        "trailing-expression sentinel auto-print engine={engine}: got `{out}`"
    );

    // 7a. Early-return fires (n=1 > 0 → ret 99). Loop never runs. 99 prints
    //     because the body has an early-return path so suppression is off.
    let out = run_file(engine, EARLY_RETURN_THEN_LOOP, "f", &["1"]);
    assert_eq!(
        out, "99\n",
        "early-return value must still print engine={engine}: got `{out}`"
    );

    // 7b. Early-return doesn't fire (n=0). Loop runs and prints 1,2,3, then
    //     because the body has an early-return path the loop tail (3) also
    //     prints — preserves the conservative rule.
    let out = run_file(engine, EARLY_RETURN_THEN_LOOP, "f", &["0"]);
    assert_eq!(
        out, "1\n2\n3\n3\n",
        "early-return + loop-tail conservative rule engine={engine}: got `{out}`"
    );

    // 7c. Explicit nil return: must still print "nil". The fix is loop-tail-
    //     scoped, not blanket Nil suppression, so legitimate `>O n;nil`
    //     functions are unaffected.
    let out = run_file(engine, EXPLICIT_NIL, "f", &[]);
    assert_eq!(
        out, "nil\n",
        "explicit nil return must auto-print engine={engine}: got `{out}`"
    );

    // 8. JSON mode unaffected: print-loop tail emits "{\"ok\":3}".
    let out = run_file_json(engine, PRINT_LOOP, "f");
    assert!(
        out.contains("\"ok\":3") || out.contains("\"ok\": 3"),
        "JSON mode must still emit structured result engine={engine}: got `{out}`"
    );

    // 8b. JSON mode for empty-loop: must still emit `{"ok":null}` (Nil).
    let out = run_file_json(engine, EMPTY_LOOP, "f");
    assert!(
        out.contains("\"ok\":null") || out.contains("\"ok\": null"),
        "JSON mode must still emit nil engine={engine}: got `{out}`"
    );
}

#[test]
fn loop_print_tree() {
    check_all("--run-tree");
}

#[test]
fn loop_print_vm() {
    check_all("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn loop_print_cranelift() {
    check_all("--run-cranelift");
}
