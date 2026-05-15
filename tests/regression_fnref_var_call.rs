// Regression: calling a variable that holds a FnRef should dispatch
// dynamically on every engine. Before PR 3d the VM compiler treated
// `Expr::Call.function` as a static function-name lookup and surfaced
// `UndefinedFunction` for `f=dbl; f 10` shapes where `f` was a local.
// The tree interpreter has always resolved these via `callee_from_scope`,
// so the cross-engine contract was silently broken on VM and Cranelift.
//
// Three call-site shapes are covered, each across tree, vm and cranelift:
//
// 1. User-fn assigned to a local — `f = dbl; f 10`
// 2. Builtin assigned to a local — `f = abs; f -3`
// 3. FnRef threaded through a function argument — `apl f x = f x;
//    apl dbl 7`. The HOF receives a FnRef param and the call site
//    `f x` must dispatch via OP_CALL_DYN.
//
// Each test calls every engine to make sure they agree on the output.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "engine={engine}: run failed, stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[cfg(feature = "cranelift")]
const ENGINES: &[&str] = &["--run-tree", "--run-vm", "--run-cranelift"];
#[cfg(not(feature = "cranelift"))]
const ENGINES: &[&str] = &["--run-tree", "--run-vm"];

// User-fn assigned to a local, then invoked: `f = dbl; f 10` → 20.
#[test]
fn fnref_var_call_user_fn() {
    let src = "dbl x:n>n;*x 2 main>n;f=dbl;f 10";
    for engine in ENGINES {
        let out = run(engine, src, "main");
        assert_eq!(out, "20", "engine={engine}");
    }
}

// Builtin assigned to a local, then invoked. `abs` is a one-arg builtin
// available as a value (its FnRef tag round-trips through registers).
#[test]
fn fnref_var_call_builtin() {
    let src = "main>n;f=abs;f -3";
    for engine in ENGINES {
        let out = run(engine, src, "main");
        assert_eq!(out, "3", "engine={engine}");
    }
}

// FnRef threaded through a function parameter typed `F n n`. The HOF
// `apl` receives the callback as `f` and invokes it inside its body.
// This is the canonical user-defined HOF shape PR 3d unblocks.
#[test]
fn fnref_var_call_through_hof_param() {
    let src = "dbl x:n>n;*x 2 apl f:F n n x:n>n;f x main>n;apl dbl 7";
    for engine in ENGINES {
        let out = run(engine, src, "main");
        assert_eq!(out, "14", "engine={engine}");
    }
}

// FnRef param chained through two user HOFs. Tests that the FnRef
// value survives a register hop between functions and still dispatches
// dynamically at the inner call site. Pre-fix this errored with
// `UndefinedFunction: cb` on VM/Cranelift.
#[test]
fn fnref_var_call_chained_hofs() {
    let src = "inc x:n>n;+x 1 apl f:F n n x:n>n;f x twice f:F n n x:n>n;apl f (apl f x) main>n;twice inc 5";
    for engine in ENGINES {
        let out = run(engine, src, "main");
        assert_eq!(out, "7", "engine={engine}");
    }
}
