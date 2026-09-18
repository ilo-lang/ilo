// Regression tests for postfix `!` / `!!` written after a call's argument list.
//
// SPEC.md:1583 documents the form
//
//     fp=sha256 (rd "config.json")!
//
// i.e. the unwrap operator glued to the *end of an argument list*, not to the
// callee name. `func! args` (bang on the name) was the only implemented shape;
// the post-args shape fell through to the generic operand-start rule, which
// treats a leading `!` as prefix logical-not. Two failure modes followed, both
// pinned here:
//
//   A. Silent misparse / statement swallowing. `maybe_postfix_unwrap` only ran
//      directly after a callee *ident*, and `can_start_operand()` reported `!`
//      as a valid operand start, so a trailing `!` was consumed as the *first
//      token of the next argument*:
//
//          f x:n>R n t;~x
//          d=f 5!
//          prnt d
//
//      parsed as `let d = f(5, Not(prnt), d)` — the bang vanished, the next
//      statement was eaten as a call argument, and the program failed with a
//      misleading `ILO-T004 undefined variable`. Nested (`prnt f 5!`) surfaced
//      `ILO-P010 expected expression, got EOF`.
//
//   B. Documented shapes rejected outright. A bang glued to a closing `)` or
//      after a paren-form call's argument list had no production at all:
//      `d=(f 5)!` -> `ILO-P001 expected declaration, got '!'`, and
//      `d=f(5)!`  -> the bang was dropped via the same fallthrough as A.
//
// The fix makes the operator *postfix*: it binds to the call whose argument
// list it immediately follows, by teaching the operand-start rule that a bang
// glued to an operand-ending token cannot itself start an operand.
//
// Adjacency is the whole disambiguation, exactly as it already is for
// `func!` vs `func !x`: a spaced bang is still prefix logical-not, and the
// controls below (`!false`, `!true` as a second argument) must keep evaluating
// as `Not`, not as an unwrap.

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
        "ilo_postfix_unwrap_{}_{}.@",
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

// `f` returns `Ok x`, so `f x!` is well-typed and yields `x`.
const F: &str = "f x:n>R n t;~x\n";

// Every case runs inside `main>R n t`, because `!` (propagate) is illegal in a
// `_`-returning function — ILO-T026, "'!' used in function which returns _".
// The runner echoes main's result, so each expected stdout ends with the
// returned `0`.
const TAIL: &str = "\n0";

/// Build a case body: `f`, then a `main>R n t` holding the statements.
///
/// Statements are `;`-separated rather than newline-separated: an un-indented
/// newline closes the declaration (ILO-439), which would move the second
/// statement to the top level. A3 uses an indented continuation line to keep
/// its newline boundary *inside* `main`.
fn wrapped(stmts: &str) -> String {
    format!("{F}main>R n t;{stmts};~0")
}

fn expect_src(engine: Option<&str>, src: &str, want: &str, label: &str) {
    assert_eq!(
        run(engine, src, "main"),
        format!("{want}{TAIL}"),
        "{label} (engine={engine:?})\n  src: {src}"
    );
}

/// A `main>R n t` whose body is `stmts`, for cases needing no extra decls.
fn expect(engine: Option<&str>, stmts: &str, want: &str, label: &str) {
    let src = wrapped(stmts);
    assert_eq!(
        run(engine, &src, "main"),
        format!("{want}{TAIL}"),
        "{label} (engine={engine:?})\n  src: {src}"
    );
}

fn check_all(engine: Option<&str>) {
    // --- A: bang after a spaced (postfix) argument list ----------------------
    // The statement-swallowing shape: without the fix this consumed the rest of
    // the body as extra arguments of `f` and reported ILO-P009 / ILO-P010.
    expect(
        engine,
        "d=f 5!;prnt d",
        "5",
        "A1 let-bound call, bang after args",
    );
    // Same, at expression-statement head.
    expect(
        engine,
        "prnt f 5!",
        "5",
        "A2 statement-head call, bang after args",
    );
    // The next line is a real statement and must survive; pre-fix the trailing
    // `!` made the call loop greedy across the newline.
    expect(
        engine,
        "f 5!\n  prnt 99",
        "99",
        "A3 statement after a bang-unwrapped call survives",
    );
    // Multiple args, bang on the last.
    expect_src(
        engine,
        &format!("{F}g x:n y:n>R n t;~(+x y)\nmain>R n t;d=g 3 4!;prnt d;~0"),
        "7",
        "A4 multi-arg call, bang after args",
    );
    // Panic-unwrap variant.
    expect(engine, "d=f 5!!;prnt d", "5", "A5 `!!` after args");
    // The unwrapped value is a normal expression: usable in arithmetic, as a
    // prefix-binop operand, and as a nested call argument.
    expect(
        engine,
        "d=f 5!;prnt *d 2",
        "10",
        "A6 unwrap result feeds arithmetic",
    );
    expect(
        engine,
        "prnt +f 5! 1",
        "6",
        "A7 unwrap result feeds a prefix binop",
    );
    expect_src(
        engine,
        &format!("{F}g n:n>n;+n 1\nmain>R n t;prnt g f 5!;~0"),
        "6",
        "A8 unwrap result as a nested call arg",
    );

    // --- B: bang after a parenthesised argument list -------------------------
    // `d=(f 5)!` — pre-fix `ILO-P001 expected declaration, got '!'`.
    expect(
        engine,
        "d=(f 5)!;prnt d",
        "5",
        "B1 bang after a paren-grouped call",
    );
    // `d=f(5)!` — pre-fix the bang was dropped.
    expect(
        engine,
        "d=f(5)!;prnt d",
        "5",
        "B2 bang after a paren-form call",
    );
    // SPEC.md:1583 verbatim shape, asserting the unwrap produced text: the
    // SHA-256 of /dev/null (empty input) is 64 hex chars. The bang binds to the
    // innermost call ending at the preceding token (`rd "/dev/null"`), which is
    // what makes the documented line type-correct — `sha256` takes text.
    expect(
        engine,
        "fp=sha256 (rd \"/dev/null\")!;prnt len fp",
        "64",
        "B3 SPEC `fp=sha256 (rd \"...\")!` shape",
    );
    // Same shape with no I/O: `num` returns a Result, unwrapped by the bang.
    expect(
        engine,
        "fp=num \"42\"!;prnt +fp 1",
        "43",
        "B4 bang after a builtin's argument list",
    );

    // --- Controls: a *spaced* bang is still prefix logical-not ---------------
    // `h 5 !false` is `h(5, Not(false))`. This must not be re-read as an unwrap
    // of `h 5`, or the A-series fix would have broken the documented
    // `func !x` argument form.
    expect_src(
        engine,
        "h a:n b:b>n;?b 1 a\nmain>R n t;prnt h 5 !false;~0",
        "1",
        "C1 spaced `!false` argument stays logical-not",
    );
    expect_src(
        engine,
        "h a:n b:b>n;?b 1 a\nmain>R n t;prnt h 5 !true;~0",
        "5",
        "C2 spaced `!true` argument stays logical-not",
    );
}

#[test]
fn postfix_unwrap_after_call_default() {
    check_all(None);
}

#[test]
fn postfix_unwrap_after_call_vm() {
    check_all(Some("--vm"));
}

#[test]
#[cfg(feature = "cranelift")]
fn postfix_unwrap_after_call_cranelift() {
    check_all(Some("--jit"));
}
