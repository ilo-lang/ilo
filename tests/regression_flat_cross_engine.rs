// Cross-engine regression tests for `flat`.
//
// Before this fix, the VM compiler had no dispatch arm for `Builtin::Flat`
// and Cranelift had no helper wired through, so any call fell through to
// the named-function lookup and failed with
// "Compile error: undefined function: flat" on both `--run-vm` and
// `--run-cranelift`. The tree-walking interpreter handled it directly and
// worked correctly.
//
// These tests pin the behaviour across all three engines: happy paths,
// the empty-list contract, the non-list-elements pass-through contract,
// and the wrong-type error case.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_ok(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} {src:?} unexpectedly failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_err(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "ilo {engine} {src:?} unexpectedly succeeded: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm", "--run-cranelift"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm"];

// ── happy paths ─────────────────────────────────────────────────────────

#[test]
fn flat_basic_nested_cross_engine() {
    let src = "f>L n;flat [[1, 2], [3, 4]]";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, "f"),
            "[1, 2, 3, 4]",
            "{engine}: flat [[1, 2], [3, 4]]"
        );
    }
}

#[test]
fn flat_three_inner_lists_cross_engine() {
    let src = "f>L n;flat [[1], [2, 3], [4, 5, 6]]";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, "f"),
            "[1, 2, 3, 4, 5, 6]",
            "{engine}: flat three inner lists"
        );
    }
}

#[test]
fn flat_inner_empties_dropped_cross_engine() {
    // [[1], [], [2]] → [1, 2]: empty inner lists splice to nothing.
    let src = "f>L n;flat [[1], [], [2]]";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, "f"),
            "[1, 2]",
            "{engine}: empty inner list dropped"
        );
    }
}

#[test]
fn flat_singleton_outer_cross_engine() {
    let src = "f>L n;flat [[1, 2, 3]]";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, "f"),
            "[1, 2, 3]",
            "{engine}: singleton outer"
        );
    }
}

// ── empty outer list ────────────────────────────────────────────────────

#[test]
fn flat_empty_outer_cross_engine() {
    let src = "f>L n;flat []";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "f"), "[]", "{engine}: flat []");
    }
}

// ── non-list elements pass through ──────────────────────────────────────

#[test]
fn flat_scalars_only_pass_through_cross_engine() {
    // Per tree semantics: `flat` is "flatten one level", a list of scalars
    // is returned with scalars in place.
    let src = "f>L n;flat [1, 2, 3]";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, "f"),
            "[1, 2, 3]",
            "{engine}: scalar pass-through"
        );
    }
}

#[test]
fn flat_mixed_pass_non_list_through_cross_engine() {
    // Nested lists are spliced, scalars are kept in place.
    let src = "f>L n;flat [[1, 2], 3, [4, 5]]";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, "f"),
            "[1, 2, 3, 4, 5]",
            "{engine}: mixed list/scalar"
        );
    }
}

// ── strings inside the outer list ───────────────────────────────────────

#[test]
fn flat_nested_text_lists_cross_engine() {
    let src = "f>L t;flat [[\"a\", \"b\"], [\"c\"]]";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, "f"),
            "[a, b, c]",
            "{engine}: nested text lists"
        );
    }
}

// ── error: arg is not a list ───────────────────────────────────────────

#[test]
fn flat_wrong_arg_type_cross_engine() {
    // Verifier should catch this at compile-time on all engines with a
    // type error mentioning `flat`. We just check that the program
    // doesn't succeed and that the diagnostic mentions `flat`.
    let src = "f>L n;flat 42";
    for engine in ENGINES_ALL {
        let err = run_err(engine, src, "f");
        assert!(
            err.contains("flat") || err.contains("list"),
            "{engine}: expected flat/list mention, got: {err}"
        );
    }
}

// ── arithmetic on result list ──────────────────────────────────────────

#[test]
fn flat_result_consumed_by_len_cross_engine() {
    // Pipe the flat result into `len` to make sure the returned NanVal is
    // a real list across all backends (not stashed in the F64 shadow).
    let src = "f>n;len (flat [[1, 2], [3, 4, 5]])";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, "f"),
            "5",
            "{engine}: len of flatten result"
        );
    }
}

#[test]
fn flat_result_consumed_by_sum_cross_engine() {
    // Composes with another cross-engine reducer added in PR #295.
    let src = "f>n;sum (flat [[1, 2], [3, 4]])";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, "f"),
            "10",
            "{engine}: sum of flatten result"
        );
    }
}
