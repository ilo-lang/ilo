// Cross-engine regression tests for `bisect xs target > n` — O(log N)
// insertion-point search in a sorted numeric list (Python `bisect_left`).
//
// Added in 0.12.x to close the `flt fn xs` + len cascade that three rerun12
// personas converged on for sorted-list dispatch (k-sorted-search,
// schedule-merge, range-bucket): O(N) where the input was already sorted
// and an O(log N) primitive would do.
//
// Tested across the tree-walker, register VM, and (when built with
// --features cranelift) the JIT. All three lower through
// `is_tree_bridge_eligible` so the bridge is the same path on every engine;
// the test fans across them anyway to catch any future divergence.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn engines() -> Vec<&'static str> {
    // Tree-walker is invoked via `ilo run`; VM/JIT take an engine flag.
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

fn parse_num(s: &str) -> f64 {
    s.lines()
        .next()
        .unwrap_or("")
        .trim()
        .parse::<f64>()
        .unwrap_or_else(|_| panic!("expected number, got: {s:?}"))
}

#[test]
fn bisect_in_range() {
    // 4 belongs between 3 and 5 → insertion point 2.
    let src = "f>n;bisect [1, 3, 5, 7] 4";
    for e in engines() {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 2.0, "engine={e}");
    }
}

#[test]
fn bisect_before_first() {
    // 0 is smaller than every element → insertion point 0.
    let src = "f>n;bisect [1, 3, 5, 7] 0";
    for e in engines() {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 0.0, "engine={e}");
    }
}

#[test]
fn bisect_after_last() {
    // 9 exceeds every element → insertion point at len(xs) = 4.
    let src = "f>n;bisect [1, 3, 5, 7] 9";
    for e in engines() {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 4.0, "engine={e}");
    }
}

#[test]
fn bisect_duplicates_returns_leftmost() {
    // bisect_left contract: equal elements land to the right of the
    // insertion point. For target=2 in [1,2,2,2,3] the leftmost match
    // is index 1, so that's the answer.
    let src = "f>n;bisect [1, 2, 2, 2, 3] 2";
    for e in engines() {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 1.0, "engine={e}");
    }
}

#[test]
fn bisect_empty_list_returns_zero() {
    // Empty list: never errors, always returns 0. Mirrors `srt []` → `[]`
    // and `argsort []` → `[]` — empty input is well-defined.
    let src = "f>n;bisect [] 5";
    for e in engines() {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 0.0, "engine={e}");
    }
}

#[test]
fn bisect_single_element_exact_match() {
    // bisect_left places the target to the LEFT of an equal element,
    // so target=5 in [5] lands at index 0, not 1.
    let src = "f>n;bisect [5] 5";
    for e in engines() {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 0.0, "engine={e}");
    }
}

#[test]
fn bisect_single_element_below() {
    let src = "f>n;bisect [5] 3";
    for e in engines() {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 0.0, "engine={e}");
    }
}

#[test]
fn bisect_single_element_above() {
    let src = "f>n;bisect [5] 7";
    for e in engines() {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 1.0, "engine={e}");
    }
}

#[test]
fn bisect_exact_match_picks_leftmost() {
    // target=5 already in the list. bisect_left returns the index of the
    // first equal element, not the index that would preserve order on the
    // right-hand side.
    let src = "f>n;bisect [1, 3, 5, 7] 5";
    for e in engines() {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 2.0, "engine={e}");
    }
}

#[test]
fn bisect_nan_target_propagates() {
    // NaN has no total order against any number, so every comparison
    // branch is false. We pin the policy used by `argmax`/`argmin` and
    // return NaN rather than an arbitrary lo/hi result.
    // sqrt of a negative number produces NaN under IEEE 754 without
    // raising at runtime; `/ 0 0` errors with ILO-R003 in tree/VM so it's
    // not a usable NaN source.
    let src = "f>n;bisect [1, 2, 3] (sqrt (0 - 1))";
    for e in engines() {
        let got = parse_num(&run_ok(e, src, "f"));
        assert!(got.is_nan(), "engine={e}, got={got}");
    }
}

#[test]
fn bisect_negative_numbers() {
    // Strict `<` against sorted negatives — same loop, no special-casing.
    let src = "f>n;bisect [0 - 5, 0 - 3, 0 - 1, 1, 3] (0 - 2)";
    for e in engines() {
        // -2 sits between -3 (idx 1) and -1 (idx 2) → insertion point 2.
        assert_eq!(parse_num(&run_ok(e, src, "f")), 2.0, "engine={e}");
    }
}

#[test]
fn bisect_replaces_filter_count_pattern() {
    // The pattern this builtin replaces: count how many elements are
    // strictly less than target. Both shapes should agree on a sorted
    // input — that's the whole contract.
    let xs = "[1, 3, 5, 7, 9, 11]";
    let target = 6.0;
    let new = format!("f>n;bisect {xs} {target}");
    for e in engines() {
        // 1, 3, 5 are below 6 → 3 elements → insertion point 3.
        assert_eq!(parse_num(&run_ok(e, &new, "f")), 3.0, "engine={e}");
    }
}
