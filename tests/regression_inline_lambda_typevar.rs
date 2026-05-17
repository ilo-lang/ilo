// Regression: inline lambdas with type-variable params at every position.
//
// Background: nlp-engineer rerun6 (v0.11.5) reported that
// `rsrt (r:L a>n;at r 0) rows` errored with cascading
// `ILO-T002 duplicate function definition 'a'` and `ILO-P001 expected
// declaration, got RParen`. The persona inferred the lambda-lifter was
// trying to lift the type variable `a` as a top-level fn name.
//
// Investigation on origin/main (f6b3271, v0.11.5) could not reproduce —
// every variant of the reported shape parses, verifies, and runs
// correctly across tree, VM, and Cranelift. `src/verify.rs` already
// lowers any unaliased single lowercase letter (other than `n`/`t`/`b`)
// to `Ty::Unknown` (treating it as a type variable), and the lambda
// lifter in `src/parser/mod.rs` always names lifted decls `__lit_N`
// (never `a`). The bug was likely closed in passing by one of the
// parser/lambda fixes between v0.11.4 and v0.11.5
// (#321 fix/hof-error-parity, #324 fix/bare-bang-nil, etc.).
//
// This file pins every variant of the reported shape so the bug
// cannot regress silently. Cross-engine wherever the shape supports it.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(name: &str, src: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "ilo_lambda_typevar_{name}_{}_{n}.ilo",
        std::process::id()
    ));
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

fn run_all(src: &str, entry: &str, args: &[&str], expected: &str) {
    for engine in ["--run-tree", "--run-vm", "--run-cranelift"] {
        let actual = run_ok(engine, src, entry, args);
        assert_eq!(
            actual, expected,
            "engine {engine} produced {actual:?}, expected {expected:?} for src `{src}`"
        );
    }
}

// ── Type-var as param and return type ──────────────────────────────────
//
// `(x:a>a;x)` — the identity lambda. `a` is a single-lowercase-letter
// not in {n,t,b} so it must be treated as a type variable and lower
// to `Ty::Unknown`, NOT as a fn name.

#[test]
fn lambda_typevar_identity_a_to_a_map() {
    let src = "main xs:L n>L n;map (x:a>a;x) xs";
    run_all(src, "main", &["[1,2,3]"], "[1, 2, 3]");
}

// ── Type-var as param type, concrete return ────────────────────────────
//
// `(x:a>n;...)` — element is polymorphic, result is number. The shape
// the nlp-engineer report flagged as failing.

#[test]
fn lambda_typevar_a_param_concrete_return_srt() {
    // Sort by a noop key (literal 0) — value-level result doesn't matter,
    // we're pinning that the parse + lift accepts `a` as param type.
    let src = "main xs:L n>L n;srt (x:a>n;0) xs";
    run_all(src, "main", &["[3,1,2]"], "[3, 1, 2]");
}

// ── Type-var inside a compound type `L a` ──────────────────────────────
//
// The exact shape from `ilo_assessment_feedback.md` line 5960:
//   `rsrt (r:L a>n;at r 0) rows`
// `L a` is a list whose element type is a type variable. The lifter
// must see `a` as a Type::Named that lowers to Ty::Unknown, not as
// a separate top-level fn.

#[test]
fn lambda_typevar_list_of_a_rsrt_at_zero() {
    let src = "main rows:L (L n)>L (L n);rsrt (r:L a>n;at r 0) rows";
    run_all(
        src,
        "main",
        &["[[1,2],[3,4],[2,5]]"],
        "[[3, 4], [2, 5], [1, 2]]",
    );
}

#[test]
fn lambda_typevar_list_of_a_srt_at_zero() {
    // Same shape with `srt` (ascending) instead of `rsrt`.
    let src = "main rows:L (L n)>L (L n);srt (r:L a>n;at r 0) rows";
    run_all(
        src,
        "main",
        &["[[3,1],[1,2],[2,3]]"],
        "[[1, 2], [2, 3], [3, 1]]",
    );
}

// ── Two lambdas with the SAME type-var name in one function ────────────
//
// If the lifter mis-treated `a` as a top-level fn, two same-shape
// lambdas would have collided with `ILO-T002 duplicate function
// definition 'a'`. The synthetic names are `__lit_N` (monotonic
// counter), so two lambdas must coexist cleanly.

#[test]
fn lambda_typevar_two_lambdas_same_typevar_same_fn() {
    let src = "main xs:L n>L n;a=map (x:a>a;x) xs;map (y:a>a;y) a";
    run_all(src, "main", &["[1,2,3]"], "[1, 2, 3]");
}

#[test]
fn lambda_typevar_two_list_a_lambdas_same_fn() {
    // The nlp-engineer mi.ilo shape: two `rsrt (r:L a>n; ...)` calls
    // in the same function body, different bodies. Cannot collide.
    // Sort by col 0 desc → [[3,4],[2,5],[1,2]], then by col 1 desc →
    // [[2,5],[3,4],[1,2]]. The important property is that two same-shape
    // type-var lambdas in one function don't collide on the `a` name.
    let src = "fa rows:L (L n)>L (L n);\
               a=rsrt (r:L a>n;at r 0) rows;\
               rsrt (r:L a>n;at r 1) a\n\
               main>L (L n);fa [[1,2],[3,4],[2,5]]";
    run_all(src, "main", &[], "[[2, 5], [3, 4], [1, 2]]");
}

// ── Lifted into a NON-`main` top-level function ────────────────────────
//
// The persona's original site was inside `main`, but the bug claim was
// general. Pin the same shape lifted from a named helper too — confirms
// the lifter doesn't depend on the enclosing fn name.

#[test]
fn lambda_typevar_in_non_main_top_level_fn() {
    let src = "by-first rows:L (L n)>L (L n);\
               rsrt (r:L a>n;at r 0) rows\n\
               main>L (L n);by-first [[1,2],[3,4],[2,5]]";
    run_all(src, "main", &[], "[[3, 4], [2, 5], [1, 2]]");
}

// ── Other single-letter type-var names ─────────────────────────────────
//
// The spec says "any single lowercase letter except n, t, b" is a
// type variable. Pin a representative sample so a future change that
// hard-codes `a` doesn't silently break `z`/`x`/`k`.

#[test]
fn lambda_typevar_letter_z() {
    let src = "main xs:L n>L n;map (x:z>z;x) xs";
    run_all(src, "main", &["[1,2,3]"], "[1, 2, 3]");
}

#[test]
fn lambda_typevar_letter_k_in_list() {
    let src = "main xs:L n>L n;flt (x:k>b;>x 0) xs";
    run_all(src, "main", &["[-1,2,-3,4]"], "[2, 4]");
}

// ── Reserved-letter type-var names are NOT type vars ───────────────────
//
// `n`, `t`, `b` are concrete primitive types, not type variables.
// These tests pin that the lambda still parses (with `n` as the actual
// number type), confirming the type-var fast-path doesn't over-trigger.

#[test]
fn lambda_concrete_n_still_works() {
    let src = "main xs:L n>L n;map (x:n>n;+x 1) xs";
    run_all(src, "main", &["[1,2,3]"], "[2, 3, 4]");
}

// ── fld with type-var seed/accumulator (3-param lambda shape) ──────────

#[test]
fn lambda_typevar_fld_polymorphic_accumulator() {
    // fld signature: (acc, elem) -> acc. Use a type-var for both so
    // the lifter sees `a` in TWO param positions of the same lambda.
    let src = "main xs:L n>n;fld (acc:a x:a>n;+(*acc 1) x) xs 0";
    run_all(src, "main", &["[1,2,3,4]"], "10");
}

// ── Type-var inside nested compound: `L (L a)` ─────────────────────────
//
// Deeper nesting: list of lists of a type-var. The lifter must walk
// into both layers and recognise `a` as a type var, not as a fn name.

#[test]
fn lambda_typevar_nested_list_of_list_of_a() {
    let src = "main rows:L (L (L n))>L (L (L n));\
               srt (r:L (L a)>n;len r) rows";
    run_all(
        src,
        "main",
        &["[[[1],[2]],[[3]],[[4],[5],[6]]]"],
        "[[[3]], [[1], [2]], [[4], [5], [6]]]",
    );
}
