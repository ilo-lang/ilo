// Regression: silent-truthy bug in `?h cond then else` (0.12.1).
//
// ml-tabular rerun11 had `?h (> p 0.5) 1 0` silently always take the
// then-branch because the parenthesised prefix-comparison `(> p 0.5)`
// was being mis-parsed as a zero-param inline lambda (`looks_like_inline_lambda`
// triggered on the leading `>` without confirming a `;` body separator
// existed at depth 1). The mis-lifted `__lit_0` synth fn captured only
// `0.5` and returned a number, which the runtime treated as truthy in
// cond position. streaming-tail and devops-sre rerun11 hit the same
// family.
//
// The fix lives in two layers:
//
//   1. Parser (`looks_like_inline_lambda` in src/parser/mod.rs): also
//      require a `;` at paren-depth 1 before the closing `)` for the
//      `(> ...)` zero-param-lambda shape. `(> p 0.5)` now parses as a
//      grouped prefix-comparison call instead of a synthesised lambda.
//
//   2. Verifier (`Expr::Ternary` in src/verify.rs): defence-in-depth —
//      a ternary condition must type-check to `b`. ILO-T038 fires for
//      any non-bool cond (partial-applied fn-refs, Result<b> without
//      unwrap, etc.). The hint steers toward the bound-first form
//      `c=<expr>;?h c a b` or the brace pattern-match `?cond{...}`.
//
// Verifier is engine-agnostic so the parser fix lands once and all
// backends see correct runtime behaviour. Cross-engine pinning here
// catches future drift.

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

fn check_err(src: &str) -> String {
    // The verifier runs ahead of every engine, so failing here means the
    // cond type-check fired before any backend got involved. Use the
    // default (--run-vm) entry point with a fn name so we exercise the
    // same code path agents hit.
    let out = ilo()
        .arg(src)
        .arg("--run-vm")
        .arg("f")
        .output()
        .expect("ilo");
    assert!(
        !out.status.success(),
        "expected verifier failure for src={src:?}, got stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let merged = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    merged
}

// `--run-tree` was removed from the public CLI in 0.12.1 (the tree-walker
// remains in-tree as the VM's dispatch target for the shapes the VM
// hasn't lifted yet, but the flag is gone). VM + JIT is the full public
// matrix — matches every other cross-engine regression test in this
// directory (see e.g. `regression_bool_ternary_prefix.rs`).
#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--run-vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--run-vm"];

// ── Parser fix: parenthesised prefix-comparison in cond position ─────
//
// `?h (> p 0.5) 1 0` — the cond is `(> p 0.5)`, a paren-grouped prefix
// comparison. Before the fix this was mis-parsed as a zero-param lambda
// and silently always took the then-branch. After the fix it parses as
// a `BinOp { GreaterThan, p, 0.5 }` and dispatches correctly.

#[test]
fn paren_prefix_comparison_cond_true_branch_cross_engine() {
    let src = "classify p:n>n;?h (> p 0.5) 1 0";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["classify", "0.7"]),
            "1",
            "{engine}: 0.7 > 0.5 → then-branch (1)"
        );
    }
}

#[test]
fn paren_prefix_comparison_cond_false_branch_cross_engine() {
    let src = "classify p:n>n;?h (> p 0.5) 1 0";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["classify", "0.3"]),
            "0",
            "{engine}: 0.3 > 0.5 → else-branch (0). This is the regression: \
             before the fix the parser mis-lifted `(> p 0.5)` into a synthetic \
             `__lit_0` lambda and the cond was always truthy, so 0.3 wrongly \
             returned 1."
        );
    }
}

#[test]
fn paren_prefix_comparison_cond_boundary_cross_engine() {
    // Boundary: 0.5 is not strictly greater than 0.5 → else-branch.
    let src = "classify p:n>n;?h (> p 0.5) 1 0";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["classify", "0.5"]),
            "0",
            "{engine}: 0.5 > 0.5 is false → else-branch"
        );
    }
}

// Other relational operators in paren-cond position.
#[test]
fn paren_prefix_eq_cond_cross_engine() {
    let src = "f x:n>n;?h (= x 0) 1 0";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "0"]), "1", "{engine}: x==0");
        assert_eq!(run_ok(engine, src, &["f", "1"]), "0", "{engine}: x!=0");
    }
}

#[test]
fn paren_prefix_lt_cond_cross_engine() {
    let src = "f x:n>n;?h (< x 10) 1 0";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "5"]), "1", "{engine}: 5<10");
        assert_eq!(run_ok(engine, src, &["f", "20"]), "0", "{engine}: 20<10");
    }
}

// ── Workaround still works (bound-first cond) ────────────────────────
//
// Personas hitting the silent-truthy bug worked around it by binding
// the cond to a local first. Pin that this form keeps working — both
// the cn=>x 0 builtin-call form and the cn=(>x 0) paren-grouped form.

#[test]
fn bound_first_cond_still_works_cross_engine() {
    let src = "classify p:n>n;c=> p 0.5;?h c 1 0";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["classify", "0.7"]), "1");
        assert_eq!(run_ok(engine, src, &["classify", "0.3"]), "0");
    }
}

#[test]
fn bound_first_paren_grouped_cond_cross_engine() {
    let src = "classify p:n>n;c=(> p 0.5);?h c 1 0";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["classify", "0.7"]), "1");
        assert_eq!(run_ok(engine, src, &["classify", "0.3"]), "0");
    }
}

// ── Real zero-param inline lambdas still parse correctly ─────────────
//
// The parser fix tightens `looks_like_inline_lambda` to require a `;`
// at depth 1 — a real zero-param lambda `(>n;42)` has one. Pin that
// this form keeps working so the fix doesn't over-correct.

#[test]
fn zero_param_inline_lambda_unaffected() {
    let src = "main>n;f=(>n;42);f";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["main"]), "42", "{engine}");
    }
}

// ── Verifier ILO-T038: non-bool cond rejected ────────────────────────
//
// Defence-in-depth: even if a future shape sneaks a non-bool into cond
// position, the verifier catches it. The number-literal cond is the
// minimal case.

#[test]
fn verify_rejects_number_literal_cond() {
    let stderr = check_err("f>n;?h 7 1 0");
    assert!(
        stderr.contains("ILO-T038"),
        "expected ILO-T038 for number cond, got: {stderr}"
    );
}

#[test]
fn verify_rejects_text_literal_cond() {
    let stderr = check_err("f>n;?h \"yes\" 1 0");
    assert!(
        stderr.contains("ILO-T038"),
        "expected ILO-T038 for text cond, got: {stderr}"
    );
}

#[test]
fn verify_rejects_number_param_cond() {
    // A number parameter in cond position — exactly the partial-applied /
    // wrong-binding shape the silent-truthy bug used to admit at runtime.
    let stderr = check_err("f x:n>n;?h x 1 0");
    assert!(
        stderr.contains("ILO-T038"),
        "expected ILO-T038 for n-typed param cond, got: {stderr}"
    );
}

#[test]
fn verify_accepts_bool_param_cond() {
    // The bool-subject form (PR #330) must still verify.
    let src = "f h:b>n;?h 7 9";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "true"]), "7");
        assert_eq!(run_ok(engine, src, &["f", "false"]), "9");
    }
}

#[test]
fn verify_accepts_paren_comparison_cond() {
    // After the parser fix the cond types as `b`; the verifier accepts it.
    let src = "classify p:n>n;?h (> p 0.5) 1 0";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["classify", "0.9"]), "1");
        assert_eq!(run_ok(engine, src, &["classify", "0.1"]), "0");
    }
}
