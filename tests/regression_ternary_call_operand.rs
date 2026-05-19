// Cross-engine regression tests for known-arity calls in the operand
// slots of the prefix-ternary family (`?=cond a b`, `?>cond a b`, …)
// and the `?h cond a b` general keyword form.
//
// Before this fix, those slots were parsed via `parse_operand` (atom
// only), so a bare known-arity call had to be wrapped in parens or
// bound to a local first:
//
//   ?h =a b sev sc "NONE"      → parser ate `sev` as bare Ref, then
//                                 choked on `sc "NONE"`
//   ?=a b sev sc "NONE"        → same failure shape
//
// Workarounds (paren-grouping or bind-first) still work, but the
// no-paren shape is the manifesto-aligned default — same rule as the
// `wh >len q 0` prefix-binop expansion from #332. Operand slots now
// dispatch via `parse_prefix_binop_operand`, which expands a known-arity
// ident plus enough trailing operands into a nested `Call` expression.

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

// ── `?h cond a b` keyword form ───────────────────────────────────────

#[test]
fn h_keyword_then_slot_is_unparenthesised_call_cross_engine() {
    // `?h =a b sev sc "NONE"` parses `sev sc` as `Call(sev, [sc])` in the
    // then-slot, leaving `"NONE"` as the else-slot atom. Mirrors the bug
    // report from security-researcher rerun9.
    let src = "sev sc:n>t;>sc 5 \"HI\";\"LO\"\nf a:n b:n sc:n>t;?h =a b sev sc \"NONE\"";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "1", "1", "10"]), "HI");
        assert_eq!(run_ok(engine, src, &["f", "1", "1", "0"]), "LO");
        assert_eq!(run_ok(engine, src, &["f", "1", "2", "10"]), "NONE");
    }
}

#[test]
fn h_keyword_else_slot_is_unparenthesised_call_cross_engine() {
    // Symmetric to the then-slot test: a known-arity call in the else
    // slot must consume exactly its declared arity.
    let src = "sev sc:n>t;>sc 5 \"HI\";\"LO\"\nf a:n b:n sc:n>t;?h =a b \"NONE\" sev sc";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "1", "1", "10"]), "NONE");
        assert_eq!(run_ok(engine, src, &["f", "1", "2", "10"]), "HI");
        assert_eq!(run_ok(engine, src, &["f", "1", "2", "0"]), "LO");
    }
}

#[test]
fn h_keyword_both_branches_unparenthesised_calls_cross_engine() {
    // Both then and else slots are calls. Each must consume only its
    // declared arity so the other branch's call sits cleanly afterwards.
    // `hi`/`lo` are strict `>` guards: `hi sc=10` is "H", `hi sc=5` is "h";
    // `lo sc=10` is "L", `lo sc=0` is "l".
    let src = "hi sc:n>t;>sc 5 \"H\";\"h\"\nlo sc:n>t;>sc 0 \"L\";\"l\"\nf a:n sc:n>t;?h =a 1 hi sc lo sc";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "1", "10"]), "H");
        assert_eq!(run_ok(engine, src, &["f", "1", "5"]), "h");
        assert_eq!(run_ok(engine, src, &["f", "0", "10"]), "L");
        assert_eq!(run_ok(engine, src, &["f", "0", "0"]), "l");
    }
}

// ── `?=cond a b` family ──────────────────────────────────────────────

#[test]
fn eq_prefix_then_slot_is_unparenthesised_call_cross_engine() {
    // `?=a b sev sc "NONE"` — known-arity call in the then-slot of the
    // comparison-led prefix ternary.
    let src = "sev sc:n>t;>sc 5 \"HI\";\"LO\"\nf a:n b:n sc:n>t;?=a b sev sc \"NONE\"";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "1", "1", "10"]), "HI");
        assert_eq!(run_ok(engine, src, &["f", "1", "1", "0"]), "LO");
        assert_eq!(run_ok(engine, src, &["f", "1", "2", "10"]), "NONE");
    }
}

#[test]
fn gt_prefix_else_slot_is_unparenthesised_call_cross_engine() {
    // `?>x 0 a b` with a known-arity call (`dbl x`) in the else-slot.
    // For x > 0 returns `x`; otherwise returns `dbl x`.
    let src = "dbl x:n>n;+*x 2 0\nf x:n>n;?>x 0 x dbl x";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "5"]), "5");
        assert_eq!(run_ok(engine, src, &["f", "-3"]), "-6");
        assert_eq!(run_ok(engine, src, &["f", "0"]), "0");
    }
}

// ── Parenthesised form remains a valid alternative ───────────────────

#[test]
fn parenthesised_call_still_works_cross_engine() {
    // Paren-grouped calls were always accepted via `parse_atom`'s
    // LParen branch. The fix must not regress that path.
    let src = "sev sc:n>t;>sc 5 \"HI\";\"LO\"\nf a:n b:n sc:n>t;?h =a b (sev sc) \"NONE\"";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "1", "1", "10"]), "HI");
        assert_eq!(run_ok(engine, src, &["f", "1", "2", "10"]), "NONE");
    }
}

// ── PR #330's 2-operand `?h a b` bool-subject form must survive ──────

#[test]
fn two_operand_bool_subject_with_call_then_branch_cross_engine() {
    // `?h dbl x 0` with subject `h:b`. With the new keyword-detector
    // taking precedence at three operand atoms, this case must still
    // resolve as the 2-operand form because subject `h` is a `b` param
    // (the 3-arg keyword form requires subject ident `h` AND the
    // existing parser already disambiguates by operand count).
    //
    // Concretely: with the call expansion, `dbl x` becomes a single
    // operand (Call), `0` is the second, no third remains — so it's
    // the 2-operand `?h:cond then else` form where cond = h, then =
    // (dbl x), else = 0.
    let src = "dbl x:n>n;+*x 2 0\nf h:b x:n>n;?h dbl x 0";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "true", "5"]), "10");
        assert_eq!(run_ok(engine, src, &["f", "false", "5"]), "0");
    }
}

// ── Plain refs in operand slots still resolve as bare Refs ───────────

#[test]
fn local_ref_in_operand_slot_resolves_as_ref_cross_engine() {
    // `parse_prefix_binop_operand` only expands when the ident is a
    // known-arity fn AND the next token can start another operand. A
    // local that is *not* a user fn must stay a bare Ref. This pins
    // that behaviour for ternary operand slots too.
    let src = "g x:n>n;+*x 2 0\nf a:n>n;v=g a;?=a 1 v 0";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "1"]), "2");
        assert_eq!(run_ok(engine, src, &["f", "2"]), "0");
    }
}
