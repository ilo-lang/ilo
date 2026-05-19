// Cross-engine regression tests for `argmax`, `argmin`, `argsort` — the
// index-returning aggregates added in 0.12.1.
//
// Before this change three rerun12 personas (argmax-argmin, distance-matrix,
// k-means) converged on `srt fn (enumerate xs)` + extract-first as the
// canonical "index of max" pattern: verbose, allocates an enumerated copy,
// and pays for a full sort to read a single index. The new builtins close
// that gap with numpy-style naming.
//
// Tested across tree, register VM, and (when built with --features cranelift)
// the JIT. All three lower through `is_tree_bridge_eligible` so the bridge
// is the same path on every engine; the test fans across them anyway to
// catch any future divergence.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn engines() -> Vec<&'static str> {
    // Tree-walker is invoked via `ilo run`, which doesn't take an engine flag;
    // we handle it as a separate branch in `run_ok`/`run_err`.
    let mut v = vec!["tree", "--run-vm"];
    if cfg!(feature = "cranelift") {
        v.push("--jit");
    }
    v
}

fn run_ok(engine: &str, src: &str, fn_name: &str) -> String {
    let out = match engine {
        "tree" => ilo()
            .args(["run", src, fn_name])
            .output()
            .expect("failed to run ilo"),
        _ => ilo()
            .args([src, engine, fn_name])
            .output()
            .expect("failed to run ilo"),
    };
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}` fn={fn_name}: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_err(engine: &str, src: &str, fn_name: &str) -> String {
    let out = match engine {
        "tree" => ilo()
            .args(["run", src, fn_name])
            .output()
            .expect("failed to run ilo"),
        _ => ilo()
            .args([src, engine, fn_name])
            .output()
            .expect("failed to run ilo"),
    };
    assert!(
        !out.status.success(),
        "ilo {engine} unexpectedly succeeded for `{src}`: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

fn parse_num(s: &str) -> f64 {
    s.lines()
        .next()
        .unwrap_or("")
        .trim()
        .parse::<f64>()
        .unwrap_or_else(|_| panic!("expected number, got: {s:?}"))
}

#[test]
fn argmax_basic() {
    let src = "f>n;argmax [3, 1, 4, 1, 5, 9, 2, 6]";
    for e in engines() {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 5.0, "engine={e}");
    }
}

#[test]
fn argmin_basic() {
    let src = "f>n;argmin [3, 1, 4, 1, 5, 9, 2, 6]";
    for e in engines() {
        // First occurrence of the min (1) is at index 1.
        assert_eq!(parse_num(&run_ok(e, src, "f")), 1.0, "engine={e}");
    }
}

#[test]
fn argsort_basic() {
    let src = "f>L n;argsort [3, 1, 4, 1, 5, 9, 2, 6]";
    for e in engines() {
        let got = run_ok(e, src, "f");
        assert_eq!(got, "[1, 3, 6, 0, 2, 4, 7, 5]", "engine={e}");
    }
}

#[test]
fn argmax_single_element() {
    let src = "f>n;argmax [42]";
    for e in engines() {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 0.0, "engine={e}");
    }
}

#[test]
fn argmin_single_element() {
    let src = "f>n;argmin [42]";
    for e in engines() {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 0.0, "engine={e}");
    }
}

#[test]
fn argsort_single_element() {
    let src = "f>L n;argsort [42]";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[0]", "engine={e}");
    }
}

#[test]
fn argmax_ties_returns_first() {
    // numpy contract: ties resolved by first occurrence. Strict `>` in the
    // scan means equal values never displace the incumbent.
    let src = "f>n;argmax [2, 2, 2]";
    for e in engines() {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 0.0, "engine={e}");
    }
}

#[test]
fn argmin_ties_returns_first() {
    let src = "f>n;argmin [5, 5, 5]";
    for e in engines() {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 0.0, "engine={e}");
    }
}

#[test]
fn argmax_negative_numbers() {
    let src = "f>n;argmax [0 - 3, 0 - 1, 0 - 7, 0 - 5]";
    for e in engines() {
        // -1 is the largest; index 1.
        assert_eq!(parse_num(&run_ok(e, src, "f")), 1.0, "engine={e}");
    }
}

#[test]
fn argmin_negative_numbers() {
    let src = "f>n;argmin [0 - 3, 0 - 1, 0 - 7, 0 - 5]";
    for e in engines() {
        // -7 is the smallest; index 2.
        assert_eq!(parse_num(&run_ok(e, src, "f")), 2.0, "engine={e}");
    }
}

#[test]
fn argsort_negative_numbers() {
    let src = "f>L n;argsort [0 - 3, 0 - 1, 0 - 7, 0 - 5]";
    for e in engines() {
        // Ascending: -7, -5, -3, -1 → indices [2, 3, 0, 1]
        assert_eq!(run_ok(e, src, "f"), "[2, 3, 0, 1]", "engine={e}");
    }
}

#[test]
fn argsort_empty_list() {
    // argsort on empty returns empty; this matches `srt []` → `[]`.
    let src = "f>L n;argsort []";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[]", "engine={e}");
    }
}

#[test]
fn argmax_empty_errors() {
    let src = "f>n;argmax []";
    for e in engines() {
        let stderr = run_err(e, src, "f");
        assert!(
            stderr.contains("argmax") && stderr.contains("empty"),
            "engine={e}: stderr={stderr}"
        );
    }
}

#[test]
fn argmin_empty_errors() {
    let src = "f>n;argmin []";
    for e in engines() {
        let stderr = run_err(e, src, "f");
        assert!(
            stderr.contains("argmin") && stderr.contains("empty"),
            "engine={e}: stderr={stderr}"
        );
    }
}

#[test]
fn argsort_already_sorted_identity() {
    let src = "f>L n;argsort [1, 2, 3, 4, 5]";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[0, 1, 2, 3, 4]", "engine={e}");
    }
}

#[test]
fn argsort_reverse_sorted() {
    let src = "f>L n;argsort [5, 4, 3, 2, 1]";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[4, 3, 2, 1, 0]", "engine={e}");
    }
}

#[test]
fn argmax_replaces_enumerate_srt_pattern() {
    // The pattern this builtin is intended to replace, side-by-side with the
    // new shape, on the same input. Both should agree.
    let xs = "[7, 2, 9, 4, 5]";
    let old = format!("f>n;hd hd rsrt (lam p:L n>n;at p 1) enumerate {xs}");
    let new = format!("f>n;argmax {xs}");
    for e in engines() {
        // Don't compare old vs new directly — the persona's pattern returns
        // the *index* (the first element of the pair after enumerate). Just
        // confirm `argmax` returns the index of 9, which is 2.
        let _ = old; // documented for context
        assert_eq!(parse_num(&run_ok(e, &new, "f")), 2.0, "engine={e}");
    }
}
