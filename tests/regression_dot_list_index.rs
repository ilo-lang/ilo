// Regression tests for literal-int dot-index on lists: `xs.0` desugars to
// `at xs 0` at parse time (parser emits Expr::Index, which every engine
// already supports). These tests lock in the cross-engine behaviour and
// the lexer's handling of chained dot-index on nested lists, where
// `xs.0.0` previously lost the trailing `.0` to the float regex.

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
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

const ENGINES: &[&str] = &[
    "--run-tree",
    "--run-vm",
    #[cfg(feature = "cranelift")]
    "--run-cranelift",
];

// xs.0 — first element.
#[test]
fn dot_index_zero() {
    let src = "f>n;xs=[10,20,30];xs.0";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "f"), "10", "engine={engine}");
    }
}

// xs.2 — third element (last in this list).
#[test]
fn dot_index_two() {
    let src = "f>n;xs=[10,20,30];xs.2";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "f"), "30", "engine={engine}");
    }
}

// xs.5 — out-of-range on tree/vm produces a runtime error. Cranelift
// JIT mirrors `hd`/`at`'s JIT behaviour and returns nil instead.
#[test]
fn dot_index_out_of_range_tree_vm() {
    let src = "f>n;xs=[10,20,30];xs.5";
    for engine in ["--run-tree", "--run-vm"] {
        let out = ilo()
            .args([src, engine, "f"])
            .output()
            .expect("failed to run ilo");
        assert!(
            !out.status.success(),
            "engine={engine}: expected runtime error for xs.5, got stdout={}",
            String::from_utf8_lossy(&out.stdout)
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("out of bounds")
                || stderr.contains("out of range")
                || stderr.contains("ILO-R006")
                || stderr.contains("ILO-R009"),
            "engine={engine}: expected out-of-range error, got stderr={stderr}"
        );
    }
}

#[test]
#[cfg(feature = "cranelift")]
fn dot_index_out_of_range_cranelift() {
    // After the JIT permissive-nil sweep (batch 1), Cranelift surfaces
    // a runtime error for OOB literal-index OP_INDEX, matching tree/VM.
    let src = "f>n;xs=[10,20,30];xs.5";
    let out = ilo()
        .args([src, "--run-cranelift", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "cranelift: expected runtime error for xs.5, got stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("out of bounds") || stderr.contains("ILO-R004"),
        "cranelift: expected out-of-bounds diagnostic, got stderr={stderr}"
    );
}

// Records still dot-access by name (not by integer).
#[test]
fn record_field_access_unaffected() {
    let src = "type point{x:n;y:n}\nf>n;p=point x:10 y:20;p.y";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "f"), "20", "engine={engine}");
    }
}

// Verifier rejects integer dot-access on a record (no field "0").
#[test]
fn record_numeric_dot_fails_verify() {
    let src = "type point{x:n;y:n}\nf>n;p=point x:10 y:20;p.0";
    let out = ilo()
        .args([src, "--run-tree", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected verify/type error for p.0, got stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
}

// Mixed shape: nested list access `xss.0.1`. Previously the lexer's
// number regex would swallow `.1` as a fractional part of `0.1`, so
// this test pins the lexer split-on-Dot fix.
#[test]
fn dot_index_nested_list() {
    let src = "f>n;xss=[[1,2],[3,4]];xss.0.1";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "f"), "2", "engine={engine}");
    }
}

// Triply nested: xss.1.0.1 lexes correctly when the leading group is
// `Dot Number(1.0)` and the trailing `.1` follows. Each pair is split
// independently by the lexer pass.
#[test]
fn dot_index_nested_list_deep() {
    let src = "f>n;xss=[[[1,2]],[[3,4],[5,6]]];xss.1.1.0";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "f"), "5", "engine={engine}");
    }
}

// Sanity: float literals outside of post-Dot position are unchanged.
#[test]
fn float_literal_outside_dot_unchanged() {
    let src = "f>n;1.5";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "f"), "1.5", "engine={engine}");
    }
}
