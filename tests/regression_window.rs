// Cross-engine regression tests for the `window n xs` builtin.
//
// `window n xs` returns a list of consecutive n-sized sub-lists of xs.
// `window 3 [1,2,3,4,5]` → `[[1,2,3],[2,3,4],[3,4,5]]`.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn engines() -> &'static [&'static str] {
    // Tree-walker and register VM cover the same builtin code paths via
    // shared helpers; cranelift JIT defers to the same Rust helper.
    &["--run-tree", "--run-vm"]
}

fn run_ok(engine: &str, src: &str, fn_name: &str) -> String {
    let out = ilo()
        .args([src, engine, fn_name])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_err(engine: &str, src: &str, fn_name: &str) -> String {
    let out = ilo()
        .args([src, engine, fn_name])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "ilo {engine} unexpectedly succeeded for `{src}`: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

#[test]
fn window_basic_3_of_5() {
    let src = "f>L (L n);window 3 [1,2,3,4,5]";
    for engine in engines() {
        assert_eq!(
            run_ok(engine, src, "f"),
            "[[1, 2, 3], [2, 3, 4], [3, 4, 5]]",
            "engine={engine}"
        );
    }
}

#[test]
fn window_pairs_2_of_3() {
    let src = "f>L (L n);window 2 [10,20,30]";
    for engine in engines() {
        assert_eq!(
            run_ok(engine, src, "f"),
            "[[10, 20], [20, 30]]",
            "engine={engine}"
        );
    }
}

#[test]
fn window_n_greater_than_len_returns_empty() {
    let src = "f>L (L n);window 5 [1,2,3]";
    for engine in engines() {
        assert_eq!(run_ok(engine, src, "f"), "[]", "engine={engine}");
    }
}

#[test]
fn window_size_one() {
    let src = "f>L (L n);window 1 [1,2,3]";
    for engine in engines() {
        assert_eq!(
            run_ok(engine, src, "f"),
            "[[1], [2], [3]]",
            "engine={engine}"
        );
    }
}

#[test]
fn window_zero_errors() {
    let src = "f>L (L n);window 0 [1,2,3]";
    for engine in engines() {
        let err = run_err(engine, src, "f");
        assert!(
            err.to_lowercase().contains("window") || err.to_lowercase().contains("positive"),
            "engine={engine}: stderr should mention window/positive, got {err}"
        );
    }
}

#[test]
fn window_polymorphic_text() {
    // Type variable: works on L t as well as L n.
    let src = "f>L (L t);window 2 [\"a\",\"b\",\"c\"]";
    for engine in engines() {
        assert_eq!(
            run_ok(engine, src, "f"),
            "[[a, b], [b, c]]",
            "engine={engine}"
        );
    }
}
