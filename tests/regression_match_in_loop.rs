// Regression tests for tree-walker miscompile where a `?match{}` expression
// at the tail of an `@` loop body silently early-returned from the enclosing
// function instead of yielding into the loop body.
//
// Before the fix, `Stmt::Match` in the tree interpreter converted a
// `BodyResult::Value(v)` into `BodyResult::Return(v)` whenever the match was
// the last statement of any body — including a loop body. That escaped the
// loop AND the function with the matched arm's value. VM and Cranelift were
// correct; only the tree-walker mis-handled this. These tests pin the
// cross-engine behaviour so the bug cannot silently come back.

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
        "ilo {engine} failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm", "--run-cranelift"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm"];

// Match-as-tail of an `@` loop must not escape the function. The function's
// tail value (`5`) is what should be returned.
const LOOP_TAIL: &str = r#"f>n;cs=["1"];@c cs{rn=num c;?rn{^_:99;~v:42}};5"#;

#[test]
fn match_in_loop_tail_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, LOOP_TAIL, "f");
        assert_eq!(out.trim(), "5", "{engine}: {out:?}");
    }
}

// Sanity: match as the tail of a function (no surrounding loop) still returns
// the matched arm's value. The fix must not break this case.
const FN_TAIL: &str = r#"f>n;rn=num "1";?rn{^_:99;~v:42}"#;

#[test]
fn match_as_fn_tail_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, FN_TAIL, "f");
        assert_eq!(out.trim(), "42", "{engine}: {out:?}");
    }
}

// Match arm body with side effects inside an `@` loop must run for every
// iteration, mutate the accumulator, and reach the function's tail.
const LOOP_SIDE_EFFECTS: &str =
    r#"f>n;n=0;cs=["1","x","2","y","3"];@c cs{rn=num c;?rn{^_:n;~v:n=+n 1}};n"#;

#[test]
fn match_side_effects_in_loop_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, LOOP_SIDE_EFFECTS, "f");
        assert_eq!(out.trim(), "3", "{engine}: {out:?}");
    }
}

// `brk` inside a match inside an `@` loop must still propagate to the loop
// (not return from the function). Tail of the function returns the
// accumulator after the loop exits early. The non-numeric arm fires a braced
// guard with `brk`; the numeric arm increments.
const BRK_IN_MATCH: &str =
    r#"f>n;n=0;cs=["1","2","x","3"];@c cs{rn=num c;?rn{^_:1{brk}{n=n};~v:n=+n 1}};n"#;

#[test]
fn brk_in_match_in_loop_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, BRK_IN_MATCH, "f");
        assert_eq!(out.trim(), "2", "{engine}: {out:?}");
    }
}

// Same shape as LOOP_TAIL but with a `wh` (while) loop instead of `@`. The
// match at the tail of the while body must yield into the loop, not the
// caller; the function's own tail (`5`) is what should be returned.
const WHILE_LOOP_TAIL: &str = r#"f>n;i=0;wh <i 3{i=+i 1;rn=num "1";?rn{^_:99;~v:42}};5"#;

#[test]
fn match_in_while_loop_tail_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, WHILE_LOOP_TAIL, "f");
        assert_eq!(out.trim(), "5", "{engine}: {out:?}");
    }
}

// `cnt` inside a match inside an `@` loop must propagate to the loop.
// All non-numeric entries hit the wildcard arm and fire a braced-guard `cnt`;
// numeric entries increment the accumulator.
const CNT_IN_MATCH: &str =
    r#"f>n;n=0;cs=["1","x","2","y","3"];@c cs{rn=num c;?rn{^_:1{cnt}{n=n};~v:n=+n 1}};n"#;

#[test]
fn cnt_in_match_in_loop_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, CNT_IN_MATCH, "f");
        assert_eq!(out.trim(), "3", "{engine}: {out:?}");
    }
}
