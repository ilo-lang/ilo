// Regression: paren-grouped prefix-comparison inside an inline-lambda
// body must parse as a grouped expression, not a synthesised closure
// literal (0.12.1).
//
// outlier-detection rerun12 (assessment doc L13455) tried the natural
// numpy-shaped mask form for an IQR outlier filter:
//
//     outs = flt (x:n>b;| (<x lo) (>x hi)) xs
//
// On 0.12.0 the body's `(>x hi)` triggered `looks_like_inline_lambda`
// (a leading `>` at paren-depth 1 was sufficient), so the parser
// lifted it into a synthetic `__lit_N` zero-param lambda. The outer
// `flt` then received a non-bool fn-ref as the predicate, surfacing
// as `flt: predicate must return bool, got Closure { fn_name: ..., captures: [...] }`.
//
// The persona's workaround was to rewrite as fully-unparenthesised
// prefix `| <x lo >x hi`, which works but is alien to anyone used to
// `(a) | (b)` Boolean composition.
//
// PR #445 (which fixed the related `?h (> p 0.5) 1 0` silent-truthy
// bug) tightened `looks_like_inline_lambda` to additionally require a
// `;` body separator at paren-depth 1. That fix incidentally repairs
// this shape as well: `(>x hi)` has no `;` so it now parses as a
// grouped prefix-comparison call. The outlier-detection bug shares
// the same root cause as the ml-tabular ternary bug but lives in a
// different surface (a HOF predicate body, not a ternary cond), so
// this test pins it as its own regression. Future tweaks to the
// disambiguator must keep both shapes green.
//
// Cross-engine pinning because parsing is engine-agnostic but the
// downstream `flt` dispatch runs on each engine separately, and we
// want to catch any regression that flows through dispatch too.

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

// VM + JIT cover the full public engine matrix in 0.12.x (the tree-
// walker is no longer reachable as a top-level engine; it still runs
// HOF callbacks the VM bails to, so VM exercises it transitively).
#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--run-vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--run-vm"];

// ── The outlier-detection shape: `flt` with paren-grouped prefix-cmp ──
//
// Canonical "values outside [lo, hi]" filter. Before the fix, the
// `(>x hi)` half mis-lifted into a closure and `flt` failed with a
// non-bool predicate. After the fix, both halves parse as grouped
// `Call { function: "<" / ">" }` and `|` composes the two bools.
//
// The xs list is a literal in the source rather than a CLI-supplied
// `L n` param so the test isolates the parser / HOF-dispatch surface
// being fixed here from the separate text-to-typed-list argv coercion
// path (which uses a different code path).

#[test]
fn flt_paren_grouped_or_mask_cross_engine() {
    let src = "out lo:n hi:n>L n;flt (x:n>b;| (<x lo) (>x hi)) [1.0 9.9 -8.5 2.0]";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["out", "0.0", "5.0"]),
            "[9.9, -8.5]",
            "{engine}: only the two outliers survive the | (<x lo) (>x hi) mask"
        );
    }
}

#[test]
fn flt_paren_grouped_or_mask_all_inside_cross_engine() {
    let src = "out lo:n hi:n>L n;flt (x:n>b;| (<x lo) (>x hi)) [1.0 2.0 3.0 4.0]";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["out", "0.0", "5.0"]),
            "[]",
            "{engine}: nothing outside the band"
        );
    }
}

#[test]
fn flt_paren_grouped_or_mask_all_outside_cross_engine() {
    let src = "out lo:n hi:n>L n;flt (x:n>b;| (<x lo) (>x hi)) [-1.5 6.5 -2.5 7.5]";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["out", "0.0", "5.0"]),
            "[-1.5, 6.5, -2.5, 7.5]",
            "{engine}: every value outside the band"
        );
    }
}

// `map` with a paren-grouped prefix-comparison body — sibling HOF
// to `flt`, same parser-level surface. Pins that the fix is HOF-
// agnostic, not just `flt`-specific.
#[test]
fn map_paren_grouped_comparison_cross_engine() {
    // Returns a list of bools; one true for each value outside the band.
    let src = "mask lo:n hi:n>L b;map (x:n>b;| (<x lo) (>x hi)) [1.0 9.9 -8.5 2.0]";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["mask", "0.0", "5.0"]),
            "[false, true, true, false]",
            "{engine}: per-element outlier mask"
        );
    }
}

// Standalone paren-grouped prefix-cmp at top level (no enclosing
// lambda). Belt-and-braces: pin that the disambiguator doesn't
// regress on the simplest possible shape.
#[test]
fn standalone_paren_gt_cross_engine() {
    let src = "f x:n hi:n>b;(>x hi)";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["f", "9.9", "5.0"]),
            "true",
            "{engine}"
        );
        assert_eq!(
            run_ok(engine, src, &["f", "1.0", "5.0"]),
            "false",
            "{engine}"
        );
    }
}

#[test]
fn standalone_paren_lt_cross_engine() {
    let src = "f x:n lo:n>b;(<x lo)";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["f", "-8.5", "0.0"]),
            "true",
            "{engine}"
        );
        assert_eq!(
            run_ok(engine, src, &["f", "1.0", "0.0"]),
            "false",
            "{engine}"
        );
    }
}

// `(a) | (b)` Boolean composition reads naturally to anyone coming
// from numpy / SQL / etc. Pin the two-paren form composes both halves
// correctly. This is the exact shape the persona reached for first.
#[test]
fn paren_or_paren_bool_composition_cross_engine() {
    let src = "f x:n lo:n hi:n>b;| (<x lo) (>x hi)";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["f", "9.9", "0.0", "5.0"]),
            "true",
            "{engine}: above hi"
        );
        assert_eq!(
            run_ok(engine, src, &["f", "-8.5", "0.0", "5.0"]),
            "true",
            "{engine}: below lo"
        );
        assert_eq!(
            run_ok(engine, src, &["f", "2.0", "0.0", "5.0"]),
            "false",
            "{engine}: inside band"
        );
    }
}

// The persona's workaround form (fully-unparenthesised prefix) still
// works. Pin so we don't accidentally break the escape hatch agents
// already discovered before the parser fix landed.
#[test]
fn unparenthesised_prefix_or_mask_still_works_cross_engine() {
    let src = "out lo:n hi:n>L n;flt (x:n>b;| <x lo >x hi) [1.0 9.9 -8.5 2.0]";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["out", "0.0", "5.0"]),
            "[9.9, -8.5]",
            "{engine}: unparenthesised escape-hatch form still composes correctly"
        );
    }
}

// Real zero-param inline lambda (the shape `looks_like_inline_lambda`
// is meant to trigger on) still parses correctly. Pin so the
// disambiguator doesn't over-correct away from the lambda case.
#[test]
fn zero_param_inline_lambda_unaffected_cross_engine() {
    let src = "main>n;f=(>n;42);f";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["main"]), "42", "{engine}");
    }
}
