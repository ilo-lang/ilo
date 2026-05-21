// Cross-engine regression tests for bare-call match scrutinee.
//
// Before this fix, `?fn arg1 arg2 {pat:body;...}` failed to parse:
// the parser treated `fn` as the match subject (a bare `Ref`), then
// the bare-bool prefix-ternary branch greedily consumed `arg1` and
// `arg2` as the two ternary operands. The `{` that followed surfaced
// as `ILO-P001 expected declaration, got '{'`, with a cascade of
// downstream typing errors (`ILO-T038 ternary condition must be a
// bool, got F n n R n t`). Agents had to rebind first:
// `r=fn arg1 arg2;?r{...}` — same friction family as the list-literal
// call trap (pending #5g).
//
// The fix extends `parse_match_stmt`: after `parse_atom` returns a
// bare `Ref(name)` for a known function of arity k>0, if exactly k
// atoms are sitting between the cursor and a `{`, consume them as
// call args and rewrite the subject as `Expr::Call`. Pure shape probe
// — the bare-bool prefix ternary `?h a b` (no trailing `{`) keeps its
// existing semantics: the new rewrite only fires when a `{` follows.
//
// Cross-engine because all three backends already handle
// `Stmt::Match` over a `Call` subject (the bind-first workaround did
// the same thing once parsed), so no engine-level changes were needed,
// but cross-engine pinning catches future drift.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_ok(engine: &str, src: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} {src:?} {args:?} unexpectedly failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--vm"];

// ── Bare-call subject of a user fn returning R n t ────────────────────

const SAFE_DIV: &str = r#"safe-div a:n b:n>R n t;=b 0 ^"zero";~/a b
g a:n b:n>t;?safe-div a b{~v:str v;^er:"e"}"#;

#[test]
fn match_call_subject_ok_arm_cross_engine() {
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, SAFE_DIV, &["g", "10", "2"]), "5");
    }
}

#[test]
fn match_call_subject_err_arm_cross_engine() {
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, SAFE_DIV, &["g", "10", "0"]), "e");
    }
}

// Parenthesised form must keep working — same behaviour as the bare
// form, just preserves the older shape that already parsed cleanly.
const SAFE_DIV_PAREN: &str = r#"safe-div a:n b:n>R n t;=b 0 ^"zero";~/a b
gp a:n b:n>t;?(safe-div a b){~v:str v;^er:"e"}"#;

#[test]
fn match_call_subject_paren_form_still_works_cross_engine() {
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, SAFE_DIV_PAREN, &["gp", "10", "2"]), "5");
        assert_eq!(run_ok(engine, SAFE_DIV_PAREN, &["gp", "10", "0"]), "e");
    }
}

// Inline in a loop body — the original 5b–5d friction family is about
// keeping result-handling logic inline. Pin that this works too.
const SAFE_DIV_LOOP: &str = r#"safe-div a:n b:n>R n t;=b 0 ^"zero";~/a b
f xs:L n>t;acc="";@x xs{s=?safe-div 10 x{~v:str v;^_:"-"};acc=acc+s};acc"#;

#[test]
fn match_call_subject_inside_loop_cross_engine() {
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, SAFE_DIV_LOOP, &["f", "[2,4,0]"]), "52.5-");
    }
}

// Single-arg function as subject — exercises the arity=1 probe path.
const SINGLE_ARG: &str = r#"safe-half n:n>R n t;=n 0 ^"zero";~/n 2
h n:n>t;?safe-half n{~v:str v;^er:"e"}"#;

#[test]
fn match_call_subject_arity_one_cross_engine() {
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, SINGLE_ARG, &["h", "10"]), "5");
        assert_eq!(run_ok(engine, SINGLE_ARG, &["h", "0"]), "e");
    }
}

// ── Negative: bool prefix ternary still parses correctly ──────────────
//
// `?h a b` on a bool ident with no trailing `{` must keep its
// prefix-ternary semantics. The new call-subject probe is gated on a
// `{` follower, so this case must continue to work unchanged.

const BOOL_TERNARY: &str = "f h:b>n;?h 1 0";

#[test]
fn bool_prefix_ternary_unchanged_cross_engine() {
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, BOOL_TERNARY, &["f", "true"]), "1");
        assert_eq!(run_ok(engine, BOOL_TERNARY, &["f", "false"]), "0");
    }
}

// ── Negative: clear error for unsupported zero-arg-call shape ─────────
//
// `?(call-with-no-args) {...}` is the parens form for a zero-arg call.
// The bare form `?call-with-no-args{...}` currently parses the ident as
// the match subject (a function ref) and then errors on type mismatch.
// We keep that behaviour rather than expanding it implicitly — the
// parens form already works and is unambiguous.
#[test]
fn zero_arg_paren_call_subject_works() {
    let src = r#"mk>R n t;~42
g>t;?(mk){~v:str v;^_:"e"}"#;
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["g"]), "42");
    }
}
