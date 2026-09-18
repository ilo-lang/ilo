// Regression tests for builtin-named locals in operand and capture position.
//
// `69565d44` ("allow builtin names as bindings (shadow in value position)")
// admitted builtin names as binding names, and guarded exactly one site: the
// *leading* operand of a prefix binop. Two gaps survived. Replaying the 22
// recorded `ilo` attempts of the 2026-08-13 closed-loop run, 9 of them bind a
// builtin name and use it in one of these two positions; for 3
// (grade-calculator a1/a4/a5) the resulting phantom error was the *first*
// one reported, masking the program's genuine next error:
//
//   A. Later greedy-expansion sites still consulted only the builtin tables.
//      `g avg:n>n;y=+avg 1;y` parsed `1` as the argument of a phantom
//      `avg(1)` call, leaving `+` an operand short:
//      `ILO-P009 expected expression, got ';'`. The same shape at top level
//      surfaced `ILO-P010 expected expression, got EOF`.
//
//   B. Lambda free-variable analysis (`collect_free_in_expr`) treated any
//      known top-level/builtin name as a fn-ref, so a shadowed local was
//      never captured. The lifted `__lit_N` body then referenced a name that
//      did not exist in its frame: `ILO-R004 unsupported operation / arity`
//      from the runtime, with the lambda blamed instead of the binding.
//
// Both are the *half of the shadowing rule* an over-eager change is most
// likely to break, so they are pinned here per engine. Call position is
// deliberately unchanged: `len xs` still dispatches to the builtin (the
// verifier checks `is_builtin` before locals), and the unshadowed builtin
// must keep its greedy `wh >len q 0` expansion — the A6/A7 controls.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

/// Run `src` on `engine` (None = default engine) and return trimmed stdout.
fn run(engine: Option<&str>, src: &str, entry: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "ilo_shadow_operand_{}_{}.@",
        std::process::id(),
        seq
    ));
    std::fs::write(&path, src).unwrap();
    let mut cmd = ilo();
    cmd.arg(&path);
    if let Some(e) = engine {
        cmd.arg(e);
    }
    cmd.arg(entry);
    let out = cmd.output().expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "engine={engine:?} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn check_all(engine: Option<&str>) {
    // --- A: shadowed name in operand position -------------------------------
    // Top-level (script mode) binding, leading operand of a prefix binop.
    assert_eq!(
        run(engine, "len=5\nprnt +len 1", "main"),
        "6",
        "A1 script-mode local shadow, engine={engine:?}"
    );
    // Function parameter, leading operand.
    assert_eq!(
        run(engine, "g avg:n>n;y=+avg 1;y\nprnt g 5", "main"),
        "6",
        "A2 param shadow, engine={engine:?}"
    );
    // `let`-bound local inside a fn body, leading operand.
    assert_eq!(
        run(engine, "g x:n>n;len=x;y=+len 1;y\nprnt g 5", "main"),
        "6",
        "A3 let-bound shadow, engine={engine:?}"
    );
    // Other prefix operators take the same path.
    assert_eq!(
        run(engine, "len=5\nprnt *len 2", "main"),
        "10",
        "A4 mul operand shadow, engine={engine:?}"
    );
    assert_eq!(
        run(engine, "g sum:n>n;y=+sum 2;y\nprnt g 5", "main"),
        "7",
        "A5 param shadow (sum), engine={engine:?}"
    );

    // --- A controls: unshadowed builtins keep greedy expansion ---------------
    // `+len [1 2] 1` is `+(Call(len,[[1 2]]), 1)` = 3, not `+(Ref(len), ...)`.
    assert_eq!(
        run(engine, "prnt +len [1 2] 1", "main"),
        "3",
        "A6 unshadowed builtin still expands, engine={engine:?}"
    );
    // A shadowing param must not leak past its function.
    assert_eq!(
        run(engine, "g len:n>n;+len 1\nprnt len [1 2 3]", "main"),
        "3",
        "A7 no scope leak after shadowing fn, engine={engine:?}"
    );

    // --- C: a shadow inside a `{...}` block is scoped to that block ----------
    // The parser's shadow frames must close with the block. Otherwise a
    // builtin-named block-local keeps suppressing greedy operand expansion for
    // the rest of the enclosing body: `b{len=n};+len [1 2 3] 1` would parse the
    // `len [1 2 3]` operand as a bare Ref and orphan the `1`.
    assert_eq!(
        run(
            engine,
            "f n:n>n;b=true;b{len=n};prnt +len [1 2 3] 1\nprnt f 5",
            "main"
        ),
        "4\n4",
        "C1 block-local shadow does not leak, engine={engine:?}"
    );
    // The shadow is still honoured *inside* the block.
    assert_eq!(
        run(
            engine,
            "f n:n>n;b=true;b{len=n;prnt +len 1}\nprnt f 5",
            "main"
        ),
        "6\n6",
        "C2 block-local shadow applies inside the block, engine={engine:?}"
    );

    // --- B: shadowed name captured by a nested lambda ------------------------
    // Top-level local, captured by a brace lambda passed to a HOF.
    assert_eq!(
        run(engine, "len=5\nv=[1 2 3]\nprnt map {q> +q len} v", "main"),
        "[6, 7, 8]",
        "B1 top-level local captured, engine={engine:?}"
    );
    // Enclosing fn param, captured by a brace lambda.
    assert_eq!(
        run(
            engine,
            "g len:n>L n;xs=map {q> +q len} [1 2 3];xs\nprnt g 5",
            "main"
        ),
        "[6, 7, 8]",
        "B2 param captured, engine={engine:?}"
    );

    // --- B controls: real builtins/fns inside lambdas unaffected -------------
    assert_eq!(
        run(engine, "v=[\"aa\" \"b\"]\nprnt map {w> len w} v", "main"),
        "[2, 1]",
        "B3 builtin call inside lambda, engine={engine:?}"
    );
    assert_eq!(
        run(
            engine,
            "slen w:t>n;len w\nv=[\"aa\" \"b\"]\nprnt map {w> slen w} v",
            "main"
        ),
        "[2, 1]",
        "B4 user fn inside lambda, engine={engine:?}"
    );
    // Capture must not leak a captured name into sibling scopes.
    assert_eq!(
        run(
            engine,
            "g len:n>L n;map {q> +q len} [1]\nprnt len [1 2 3]",
            "main"
        ),
        "3",
        "B5 no capture leak, engine={engine:?}"
    );
}

#[test]
fn shadowed_builtin_operand_default() {
    check_all(None);
}

#[test]
fn shadowed_builtin_operand_vm() {
    check_all(Some("--vm"));
}

#[test]
#[cfg(feature = "cranelift")]
fn shadowed_builtin_operand_cranelift() {
    check_all(Some("--jit"));
}
