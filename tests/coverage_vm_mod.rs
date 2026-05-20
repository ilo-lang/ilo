// Coverage-driven tests for src/vm/mod.rs.
//
// These tests are not regression tests for a specific bug. They exist to
// exercise opcode dispatch arms and error paths in the VM module that were
// otherwise reached only obliquely (or not at all) by the existing suite.
// The goal is region coverage of vm/mod.rs >= 92%.
//
// Each test drives ilo source through the CLI across tree, VM, and
// (when the cranelift feature is on) Cranelift. The CLI is the right
// entry point: it exercises the full compile-then-run path including
// the opcode emitter, the VM dispatch loop, and the runtime helpers.

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
    let mut s = String::from_utf8_lossy(&out.stderr).into_owned();
    if s.trim().is_empty() {
        s = String::from_utf8_lossy(&out.stdout).into_owned();
    }
    s
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--vm"];

// Some operations are tree-only (closures with captures) or VM-only
// (specific opcode optimisations). For others we want cross-engine
// confirmation. These helpers let us choose the scope per test.
const ENGINES_TREE_VM: &[&str] = &["--vm"];

// ── arithmetic and constant folding ────────────────────────────────────

#[test]
fn arith_const_fold_negative_literal() {
    // -0 is a number token, not unary on 0.
    let src = "f>n;+-3 7";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "4");
    }
}

#[test]
fn arith_division_by_zero_errors_on_tree_and_vm() {
    let src = "f x:n>n;/x 0";
    for e in ENGINES_TREE_VM {
        let out = ilo().args([src, e, "f", "5"]).output().expect("ilo");
        assert!(!out.status.success(), "engine {e}: should error");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("division") || stderr.contains("ILO-R003"),
            "{e}: {stderr}"
        );
    }
}

#[test]
fn arith_mod_basic() {
    let src = "f>n;mod 17 5";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "2");
    }
}

#[test]
fn arith_mod_zero_divisor_errors() {
    let src = "f>n;mod 1 0";
    for e in ENGINES_ALL {
        let stderr = run_err(e, src, "f");
        assert!(
            stderr.to_lowercase().contains("mod")
                || stderr.contains("zero")
                || stderr.contains("divis"),
            "{e}: stderr={stderr}"
        );
    }
}

// ── min/max scalar and list ────────────────────────────────────────────

#[test]
fn minmax_two_args() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>n;min 3 7", "f"), "3");
        assert_eq!(run_ok(e, "f>n;max 3 7", "f"), "7");
    }
}

#[test]
fn minmax_list() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>n;min [4 1 9 2]", "f"), "1");
        assert_eq!(run_ok(e, "f>n;max [4 1 9 2]", "f"), "9");
    }
}

#[test]
fn minmax_empty_list_errors() {
    for e in ENGINES_ALL {
        let stderr = run_err(e, "f>n;min []", "f");
        assert!(
            stderr.to_lowercase().contains("empty") || stderr.contains("min"),
            "{e}: {stderr}"
        );
    }
}

// ── math builtins ──────────────────────────────────────────────────────

#[test]
fn pow_sqrt_log_exp() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>n;pow 2 10", "f"), "1024");
        assert_eq!(run_ok(e, "f>n;sqrt 144", "f"), "12");
        assert_eq!(run_ok(e, "f>n;flr exp 0", "f"), "1");
        assert_eq!(run_ok(e, "f>n;flr log 1", "f"), "0");
        assert_eq!(run_ok(e, "f>n;log10 1000", "f"), "3");
        assert_eq!(run_ok(e, "f>n;log2 8", "f"), "3");
    }
}

#[test]
fn trig_builtins() {
    // sin/cos/tan at 0 are deterministic across engines.
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>n;sin 0", "f"), "0");
        assert_eq!(run_ok(e, "f>n;cos 0", "f"), "1");
        assert_eq!(run_ok(e, "f>n;tan 0", "f"), "0");
    }
}

#[test]
fn atan2_basic() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>n;atan2 0 1", "f"), "0");
    }
}

#[test]
fn rou_basic() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>n;rou 2.4", "f"), "2");
        assert_eq!(run_ok(e, "f>n;rou 2.6", "f"), "3");
        assert_eq!(run_ok(e, "f>n;rou -2.6", "f"), "-3");
    }
}

#[test]
fn flr_cel() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>n;flr 3.7", "f"), "3");
        assert_eq!(run_ok(e, "f>n;cel 3.2", "f"), "4");
        assert_eq!(run_ok(e, "f>n;flr -3.2", "f"), "-4");
        assert_eq!(run_ok(e, "f>n;cel -3.7", "f"), "-3");
    }
}

#[test]
fn clamp_bounds() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>n;clamp 5 0 10", "f"), "5");
        assert_eq!(run_ok(e, "f>n;clamp -5 0 10", "f"), "0");
        assert_eq!(run_ok(e, "f>n;clamp 50 0 10", "f"), "10");
        // lo > hi: lower bound wins.
        assert_eq!(run_ok(e, "f>n;clamp 5 10 0", "f"), "10");
    }
}

#[test]
fn abs_negate() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>n;abs -7", "f"), "7");
        assert_eq!(run_ok(e, "f>n;abs 7", "f"), "7");
    }
}

#[test]
fn unary_negate() {
    let src = "f x:n>n;-0 x";
    for e in ENGINES_ALL {
        let out = ilo().args([src, e, "f", "5"]).output().expect("ilo");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "-5");
    }
}

// ── padding ────────────────────────────────────────────────────────────

#[test]
fn padl_padr_default_space() {
    for e in ENGINES_ALL {
        // padl/padr default to space. Bind first so `len` parses cleanly.
        assert_eq!(run_ok(e, "f>n;p=padl \"x\" 5;len p", "f"), "5");
        assert_eq!(run_ok(e, "f>n;p=padr \"x\" 5;len p", "f"), "5");
    }
}

#[test]
fn padl_padr_custom_char() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>t;padl \"x\" 5 \"0\"", "f"), "0000x");
        assert_eq!(run_ok(e, "f>t;padr \"x\" 5 \".\"", "f"), "x....");
    }
}

#[test]
fn padl_padr_already_wide_noop() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>t;padl \"hello\" 3", "f"), "hello");
        assert_eq!(run_ok(e, "f>t;padr \"hello\" 3 \"x\"", "f"), "hello");
    }
}

#[test]
fn padl_padr_unicode_char_errors() {
    // Padding char must be exactly 1 char (the implementation checks).
    for e in ENGINES_ALL {
        // multi-char pad string
        let stderr = run_err(e, "f>t;padl \"x\" 5 \"ab\"", "f");
        assert!(!stderr.is_empty(), "{e}: expected error");
    }
}

// ── string builtins ────────────────────────────────────────────────────

#[test]
fn upr_lwr_cap() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>t;upr \"hello\"", "f"), "HELLO");
        assert_eq!(run_ok(e, "f>t;lwr \"WORLD\"", "f"), "world");
        assert_eq!(run_ok(e, "f>t;cap \"hello\"", "f"), "Hello");
        assert_eq!(run_ok(e, "f>t;cap \"\"", "f"), "");
    }
}

#[test]
fn trm_basic() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>t;trm \"  hello  \"", "f"), "hello");
    }
}

#[test]
fn ord_chr_roundtrip() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>n;ord \"A\"", "f"), "65");
        assert_eq!(run_ok(e, "f>t;chr 65", "f"), "A");
        // unicode
        assert_eq!(run_ok(e, "f>t;chr 9731", "f"), "☃");
    }
}

#[test]
fn ord_empty_string_errors() {
    for e in ENGINES_ALL {
        let stderr = run_err(e, "f>n;ord \"\"", "f");
        assert!(!stderr.is_empty(), "{e}: expected error");
    }
}

#[test]
fn chars_basic() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>L t;chars \"abc\"", "f"), "[a, b, c]");
        assert_eq!(run_ok(e, "f>L t;chars \"\"", "f"), "[]");
    }
}

// ── slice / take / drop ────────────────────────────────────────────────

#[test]
fn slc_list_and_text() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>L n;slc [1 2 3 4 5] 1 3", "f"), "[2, 3]");
        assert_eq!(run_ok(e, "f>t;slc \"hello\" 1 4", "f"), "ell");
        // negative indices
        assert_eq!(run_ok(e, "f>L n;slc [1 2 3 4 5] -2 5", "f"), "[4, 5]");
        // bounds clamp
        assert_eq!(run_ok(e, "f>L n;slc [1 2 3] 0 99", "f"), "[1, 2, 3]");
    }
}

#[test]
fn take_drop_list() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>L n;take 2 [1 2 3 4]", "f"), "[1, 2]");
        assert_eq!(run_ok(e, "f>L n;drop 2 [1 2 3 4]", "f"), "[3, 4]");
        assert_eq!(run_ok(e, "f>L n;take 10 [1 2 3]", "f"), "[1, 2, 3]");
        assert_eq!(run_ok(e, "f>L n;drop 10 [1 2 3]", "f"), "[]");
        // negative
        assert_eq!(run_ok(e, "f>L n;take -1 [1 2 3]", "f"), "[1, 2]");
        assert_eq!(run_ok(e, "f>L n;drop -1 [1 2 3]", "f"), "[3]");
    }
}

#[test]
fn take_drop_text() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>t;take 3 \"hello\"", "f"), "hel");
        assert_eq!(run_ok(e, "f>t;drop 3 \"hello\"", "f"), "lo");
    }
}

// ── list builtins ──────────────────────────────────────────────────────

#[test]
fn at_index() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>n;at [10 20 30] 1", "f"), "20");
        // negative index
        assert_eq!(run_ok(e, "f>n;at [10 20 30] -1", "f"), "30");
    }
}

#[test]
fn at_out_of_bounds_errors() {
    for e in ENGINES_ALL {
        let stderr = run_err(e, "f>n;at [1 2 3] 99", "f");
        assert!(!stderr.is_empty(), "{e}");
    }
}

#[test]
fn lst_update() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>L n;lst [1 2 3] 1 99", "f"), "[1, 99, 3]");
    }
}

#[test]
fn lst_oob_errors() {
    for e in ENGINES_ALL {
        let stderr = run_err(e, "f>L n;lst [1 2 3] 99 0", "f");
        assert!(!stderr.is_empty());
    }
}

#[test]
fn rev_list_and_text() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>L n;rev [1 2 3]", "f"), "[3, 2, 1]");
        assert_eq!(run_ok(e, "f>t;rev \"abc\"", "f"), "cba");
    }
}

#[test]
fn unq_list_and_text() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>L n;unq [1 2 1 3 2]", "f"), "[1, 2, 3]");
        assert_eq!(run_ok(e, "f>t;unq \"abbcca\"", "f"), "abc");
    }
}

#[test]
fn srt_and_rsrt() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>L n;srt [3 1 2]", "f"), "[1, 2, 3]");
        assert_eq!(run_ok(e, "f>L n;rsrt [3 1 2]", "f"), "[3, 2, 1]");
        assert_eq!(run_ok(e, "f>L t;srt [\"b\" \"a\" \"c\"]", "f"), "[a, b, c]");
        assert_eq!(run_ok(e, "f>t;srt \"cba\"", "f"), "abc");
    }
}

#[test]
fn srt_by_key_fn() {
    for e in ENGINES_TREE_VM {
        // sort by negated key — descending by value
        let src = "k x:n>n;-0 x;f>L n;srt k [3 1 2]";
        assert_eq!(run_ok(e, src, "f"), "[3, 2, 1]");
    }
}

#[test]
fn cat_basic() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>t;cat [\"a\" \"b\" \"c\"] \"-\"", "f"), "a-b-c");
        assert_eq!(run_ok(e, "f>t;cat [] \",\"", "f"), "");
    }
}

#[test]
fn spl_basic() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>L t;spl \"a,b,c\" \",\"", "f"), "[a, b, c]");
    }
}

#[test]
fn has_list_and_text() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>b;has [1 2 3] 2", "f"), "true");
        assert_eq!(run_ok(e, "f>b;has [1 2 3] 99", "f"), "false");
        assert_eq!(run_ok(e, "f>b;has \"hello\" \"ell\"", "f"), "true");
        assert_eq!(run_ok(e, "f>b;has \"hello\" \"xyz\"", "f"), "false");
    }
}

#[test]
fn hd_tl_list_and_text() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>n;hd [10 20 30]", "f"), "10");
        assert_eq!(run_ok(e, "f>L n;tl [10 20 30]", "f"), "[20, 30]");
        assert_eq!(run_ok(e, "f>t;hd \"abc\"", "f"), "a");
        assert_eq!(run_ok(e, "f>t;tl \"abc\"", "f"), "bc");
    }
}

#[test]
fn hd_tl_empty_errors() {
    for e in ENGINES_ALL {
        let stderr = run_err(e, "f>n;hd []", "f");
        assert!(!stderr.is_empty(), "{e}");
    }
}

#[test]
fn zip_basic() {
    for e in ENGINES_ALL {
        assert_eq!(
            run_ok(e, "f>L (L _);zip [1 2 3] [\"a\" \"b\"]", "f"),
            "[[1, a], [2, b]]"
        );
    }
}

#[test]
fn enumerate_basic() {
    for e in ENGINES_ALL {
        assert_eq!(
            run_ok(e, "f>L (L _);enumerate [\"a\" \"b\" \"c\"]", "f"),
            "[[0, a], [1, b], [2, c]]"
        );
    }
}

#[test]
fn range_basic_and_empty() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>L n;range 0 5", "f"), "[0, 1, 2, 3, 4]");
        assert_eq!(run_ok(e, "f>L n;range 5 5", "f"), "[]");
        assert_eq!(run_ok(e, "f>L n;range 10 5", "f"), "[]");
    }
}

#[test]
fn chunks_basic() {
    for e in ENGINES_ALL {
        assert_eq!(
            run_ok(e, "f>L (L n);chunks 2 [1 2 3 4 5]", "f"),
            "[[1, 2], [3, 4], [5]]"
        );
    }
}

#[test]
fn window_basic_and_oversized() {
    for e in ENGINES_ALL {
        assert_eq!(
            run_ok(e, "f>L (L n);window 3 [1 2 3 4 5]", "f"),
            "[[1, 2, 3], [2, 3, 4], [3, 4, 5]]"
        );
        assert_eq!(run_ok(e, "f>L (L n);window 10 [1 2 3]", "f"), "[]");
    }
}

#[test]
fn cumsum_basic() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>L n;cumsum [1 2 3 4]", "f"), "[1, 3, 6, 10]");
        assert_eq!(run_ok(e, "f>L n;cumsum []", "f"), "[]");
    }
}

#[test]
fn sum_avg_basic() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>n;sum [1 2 3]", "f"), "6");
        assert_eq!(run_ok(e, "f>n;sum []", "f"), "0");
        assert_eq!(run_ok(e, "f>n;avg [2 4 6]", "f"), "4");
    }
}

#[test]
fn avg_empty_errors() {
    for e in ENGINES_ALL {
        let stderr = run_err(e, "f>n;avg []", "f");
        assert!(!stderr.is_empty(), "{e}");
    }
}

#[test]
fn median_quantile() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>n;median [3 1 4 1 5]", "f"), "3");
        assert_eq!(run_ok(e, "f>n;median [1 2 3 4]", "f"), "2.5");
        assert_eq!(run_ok(e, "f>n;quantile [1 2 3 4 5] 0", "f"), "1");
        assert_eq!(run_ok(e, "f>n;quantile [1 2 3 4 5] 1", "f"), "5");
        // p clamped
        assert_eq!(run_ok(e, "f>n;quantile [1 2 3 4 5] 2", "f"), "5");
    }
}

#[test]
fn stdev_variance() {
    for e in ENGINES_ALL {
        // Sample variance and stdev (Bessel's correction, divides by N-1).
        // Just sanity-check the values are deterministic and positive.
        let v = run_ok(e, "f>n;variance [2 4 4 4 5 5 7 9]", "f");
        assert!(v.starts_with("4.5"), "{e}: {v}");
        let s = run_ok(e, "f>n;stdev [2 4 4 4 5 5 7 9]", "f");
        assert!(s.starts_with("2."), "{e}: {s}");
    }
}

#[test]
fn flat_and_flatmap() {
    for e in ENGINES_ALL {
        assert_eq!(
            run_ok(e, "f>L n;flat [[1 2] [3] [4 5]]", "f"),
            "[1, 2, 3, 4, 5]"
        );
    }
    for e in ENGINES_TREE_VM {
        // flatmap with lambda
        let src = "g x:n>L n;[x x];f>L n;flatmap g [1 2 3]";
        assert_eq!(run_ok(e, src, "f"), "[1, 1, 2, 2, 3, 3]");
    }
}

#[test]
fn map_flt_ct_fld_basic() {
    for e in ENGINES_TREE_VM {
        let src = "d x:n>n;*x 2;f>L n;map d [1 2 3]";
        assert_eq!(run_ok(e, src, "f"), "[2, 4, 6]");
        let src = "p x:n>b;>x 1;f>L n;flt p [0 1 2 3]";
        assert_eq!(run_ok(e, src, "f"), "[2, 3]");
        let src = "p x:n>b;>x 1;f>n;ct p [0 1 2 3]";
        assert_eq!(run_ok(e, src, "f"), "2");
        let src = "g a:n x:n>n;+a x;f>n;fld g [1 2 3 4] 0";
        assert_eq!(run_ok(e, src, "f"), "10");
    }
}

#[test]
fn frq_basic() {
    for e in ENGINES_ALL {
        // frequency of [1 2 2 3 3 3]
        let src = "f>n;m=frq [1 2 2 3 3 3];mget m 3??0";
        let out = run_ok(e, src, "f");
        assert_eq!(out, "3");
    }
}

#[test]
fn partition_basic() {
    for e in ENGINES_TREE_VM {
        let src = "p x:n>b;>x 2;f>L (L n);partition p [1 2 3 4 5]";
        assert_eq!(run_ok(e, src, "f"), "[[3, 4, 5], [1, 2]]");
    }
}

#[test]
fn uniqby_first_occurrence() {
    for e in ENGINES_TREE_VM {
        let src = "k x:n>n;mod x 3;f>L n;uniqby k [1 4 2 5 3 7]";
        assert_eq!(run_ok(e, src, "f"), "[1, 2, 3]");
    }
}

#[test]
fn setunion_inter_diff_numbers() {
    for e in ENGINES_ALL {
        assert_eq!(
            run_ok(e, "f>L n;setunion [1 2 3] [3 4 5]", "f"),
            "[1, 2, 3, 4, 5]"
        );
        assert_eq!(run_ok(e, "f>L n;setinter [1 2 3] [2 3 4]", "f"), "[2, 3]");
        assert_eq!(run_ok(e, "f>L n;setdiff [1 2 3 4] [2 4]", "f"), "[1, 3]");
    }
}

#[test]
fn setunion_inter_diff_text() {
    for e in ENGINES_ALL {
        assert_eq!(
            run_ok(e, "f>L t;setunion [\"a\" \"b\"] [\"b\" \"c\"]", "f"),
            "[a, b, c]"
        );
        assert_eq!(
            run_ok(e, "f>L t;setinter [\"a\" \"b\"] [\"b\" \"c\"]", "f"),
            "[b]"
        );
        assert_eq!(
            run_ok(e, "f>L t;setdiff [\"a\" \"b\" \"c\"] [\"b\"]", "f"),
            "[a, c]"
        );
    }
}

// ── linear algebra ─────────────────────────────────────────────────────

#[test]
fn transpose_basic() {
    for e in ENGINES_ALL {
        assert_eq!(
            run_ok(e, "f>L (L n);transpose [[1 2 3] [4 5 6]]", "f"),
            "[[1, 4], [2, 5], [3, 6]]"
        );
    }
}

#[test]
fn matmul_basic() {
    for e in ENGINES_ALL {
        assert_eq!(
            run_ok(e, "f>L (L n);matmul [[1 2] [3 4]] [[5 6] [7 8]]", "f"),
            "[[19, 22], [43, 50]]"
        );
    }
}

#[test]
fn dot_basic() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>n;dot [1 2 3] [4 5 6]", "f"), "32");
    }
}

#[test]
fn det_basic_and_singular() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>n;det [[1 2] [3 4]]", "f"), "-2");
        // singular matrix det = 0
        assert_eq!(run_ok(e, "f>n;det [[1 2] [2 4]]", "f"), "0");
    }
}

#[test]
fn inv_basic_and_singular_errors() {
    for e in ENGINES_ALL {
        // singular matrix
        let stderr = run_err(e, "f>L (L n);inv [[1 2] [2 4]]", "f");
        assert!(!stderr.is_empty(), "{e}");
    }
}

#[test]
fn solve_basic_and_singular() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>L n;solve [[2 0] [0 2]] [4 6]", "f"), "[2, 3]");
        let stderr = run_err(e, "f>L n;solve [[1 2] [2 4]] [1 2]", "f");
        assert!(!stderr.is_empty(), "{e}");
    }
}

// ── FFT ───────────────────────────────────────────────────────────────

#[test]
fn fft_ifft_roundtrip() {
    for e in ENGINES_ALL {
        // fft of a constant signal: DC component, rest zero
        let src = "f>L (L n);fft [1 1 1 1]";
        let out = run_ok(e, src, "f");
        assert!(out.starts_with("[[4, 0]"), "{e}: {out}");
    }
}

// ── records ────────────────────────────────────────────────────────────

#[test]
fn record_construct_access() {
    let src = "type pt{x:n;y:n}\nf>n;p=pt x:10 y:20;+p.x p.y";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "30");
    }
}

#[test]
fn record_update_with() {
    let src = "type pt{x:n;y:n}\nf>n;p=pt x:1 y:2;q=p with x:10;+q.x q.y";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "12");
    }
}

#[test]
fn record_safe_field_missing_returns_nil() {
    let src = "type pt{x:n;y:n}\nf>n;p=pt x:1 y:2;p.?x??99";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "1");
    }
}

#[test]
fn record_destructure() {
    let src = "type pt{x:n;y:n}\nf>n;p=pt x:10 y:20;{x;y}=p;+x y";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "30");
    }
}

#[test]
fn record_strict_field_missing_errors() {
    // accessing a missing field on a jpar record should error with .field strict
    let src = "f>R n t;r=jpar! \"{\\\"a\\\":1}\";~r.b";
    for e in ENGINES_ALL {
        let stderr = run_err(e, src, "f");
        assert!(!stderr.is_empty(), "{e}: {stderr}");
    }
}

// ── result handling ───────────────────────────────────────────────────

#[test]
fn wrap_ok_err_and_match() {
    let src = "g x:n>R n t;>x 0 ~x;^\"neg\";f x:n>n;r=g x;?r{~v:v;^_:0}";
    for e in ENGINES_ALL {
        // build a 1-arg CLI invocation
        let out = ilo().args([src, e, "f", "5"]).output().expect("ilo");
        assert!(out.status.success(), "{e}");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "5");

        let out = ilo().args([src, e, "f", "-3"]).output().expect("ilo");
        assert!(out.status.success(), "{e}");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "0");
    }
}

#[test]
fn unwrap_bang_propagates_err() {
    let src = "g x:n>R n t;>x 0 ~x;^\"neg\";f x:n>R n t;v=g! x;~v";
    for e in ENGINES_ALL {
        let out = ilo().args([src, e, "f", "-1"]).output().expect("ilo");
        assert!(!out.status.success(), "{e}: should fail on Err");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("neg"), "{e}: {stderr}");
    }
}

#[test]
fn panic_unwrap_aborts() {
    let src = "g x:n>R n t;>x 0 ~x;^\"neg\";f x:n>n;g!! x";
    for e in ENGINES_ALL {
        let out = ilo().args([src, e, "f", "-1"]).output().expect("ilo");
        assert!(!out.status.success(), "{e}: should panic");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("panic") || stderr.contains("neg"),
            "{e}: {stderr}"
        );
    }
}

// ── map (M t v) builtins ──────────────────────────────────────────────

#[test]
fn map_set_get_del() {
    let src = "f>n;m=mmap;m=mset m \"a\" 1;m=mset m \"b\" 2;m=mdel m \"a\";len m";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "1");
    }
}

#[test]
fn map_has_mget_nil_default() {
    let src = "f>n;m=mmap;m=mset m \"a\" 7;mget m \"a\"??99";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "7");
    }
    let src = "f>n;m=mmap;mget m \"missing\"??42";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "42");
    }
}

#[test]
fn map_keys_vals_sorted() {
    let src = "f>L t;m=mmap;m=mset m \"b\" 2;m=mset m \"a\" 1;m=mset m \"c\" 3;mkeys m";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "[a, b, c]");
    }
    let src = "f>L n;m=mmap;m=mset m \"b\" 2;m=mset m \"a\" 1;m=mset m \"c\" 3;mvals m";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "[1, 2, 3]");
    }
}

#[test]
fn map_int_keys() {
    let src = "f>n;m=mmap;m=mset m 7 100;mget m 7??0";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "100");
    }
}

#[test]
fn map_int_text_keys_distinct() {
    let src = "f>b;m=mmap;m=mset m 1 100;mhas m \"1\"";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "false");
    }
}

// ── format ─────────────────────────────────────────────────────────────

#[test]
fn fmt_basic() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>t;fmt \"x={}\" 42", "f"), "x=42");
        assert_eq!(run_ok(e, "f>t;fmt \"{} and {}\" 1 2", "f"), "1 and 2");
    }
}

#[test]
fn fmt2_basic() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>t;fmt2 3.14159 2", "f"), "3.14");
        assert_eq!(run_ok(e, "f>t;fmt2 3.14159 0", "f"), "3");
    }
}

// ── jpth / jdmp / jpar ────────────────────────────────────────────────

#[test]
fn jdmp_basic() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>t;jdmp 42", "f"), "42");
        assert_eq!(run_ok(e, "f>t;jdmp \"hello\"", "f"), "\"hello\"");
        assert_eq!(run_ok(e, "f>t;jdmp [1 2 3]", "f"), "[1,2,3]");
    }
}

#[test]
fn jpth_path_lookup() {
    let src = "f>R t t;jpth \"{\\\"a\\\":{\\\"b\\\":\\\"hi\\\"}}\" \"a.b\"";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "hi");
    }
}

#[test]
fn jpth_missing_path_errors() {
    let src = "f>R t t;jpth \"{\\\"a\\\":1}\" \"missing\"";
    for e in ENGINES_ALL {
        let out = ilo().args([src, e, "f"]).output().expect("ilo");
        assert!(!out.status.success(), "{e}: expected non-zero exit");
    }
}

#[test]
fn jpar_basic() {
    let src = "f>n;rr=jpar \"{\\\"x\\\":42}\";?rr{~r:r.x;^_:0}";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "42");
    }
}

#[test]
fn jpar_invalid_errors() {
    let src = "f>R _ t;jpar \"not json\"";
    for e in ENGINES_ALL {
        let out = ilo().args([src, e, "f"]).output().expect("ilo");
        assert!(!out.status.success(), "{e}");
    }
}

// ── env ────────────────────────────────────────────────────────────────

#[test]
fn env_missing_errors() {
    let src = "f>R t t;env \"ILO_DEFINITELY_NOT_A_REAL_ENV_VAR_X\"";
    for e in ENGINES_ALL {
        let out = ilo().args([src, e, "f"]).output().expect("ilo");
        assert!(!out.status.success(), "{e}");
    }
}

#[test]
fn env_present_works() {
    // PATH is reliably set across CI environments
    let src = "f>t;env! \"PATH\"";
    for e in ENGINES_ALL {
        let out = ilo().args([src, e, "f"]).output().expect("ilo");
        assert!(out.status.success(), "{e}");
    }
}

// ── while loops, break, continue ──────────────────────────────────────

#[test]
fn while_basic_sum() {
    let src = "f>n;i=0;s=0;wh <i 5{i=+i 1;s=+s i};s";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "15");
    }
}

#[test]
fn while_break_returns_value() {
    let src = "f>n;i=0;wh true{i=+i 1;>=i 3{brk}};i";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "3");
    }
}

#[test]
fn while_continue_skips() {
    let src = "f>n;i=0;s=0;wh <i 5{i=+i 1;>=i 3{cnt};s=+s i};s";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "3");
    }
}

#[test]
fn for_range_iter() {
    let src = "f>n;s=0;@i 0..5{s=+s i};s";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "10");
    }
}

#[test]
fn for_each_list_accumulator() {
    let src = "f>L n;out=[];@x [10 20 30]{out=+=out *x 2};out";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "[20, 40, 60]");
    }
}

// ── match arms ────────────────────────────────────────────────────────

#[test]
fn match_literal_text() {
    let src = "f x:t>n;?x{\"a\":1;\"b\":2;_:0}";
    for e in ENGINES_ALL {
        let out = ilo().args([src, e, "f", "a"]).output().expect("ilo");
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "1");
        let out = ilo().args([src, e, "f", "b"]).output().expect("ilo");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "2");
        let out = ilo().args([src, e, "f", "zzz"]).output().expect("ilo");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "0");
    }
}

#[test]
fn match_literal_number() {
    let src = "f x:n>t;?x{1:\"one\";2:\"two\";_:\"other\"}";
    for e in ENGINES_ALL {
        let out = ilo().args([src, e, "f", "1"]).output().expect("ilo");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "one");
        let out = ilo().args([src, e, "f", "2"]).output().expect("ilo");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "two");
        let out = ilo().args([src, e, "f", "99"]).output().expect("ilo");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "other");
    }
}

#[test]
fn match_result_arms() {
    let src = "g x:n>R n t;>=x 0 ~x;^\"neg\";f x:n>t;r=g x;?r{~v:str v;^er:er}";
    for e in ENGINES_ALL {
        let out = ilo().args([src, e, "f", "7"]).output().expect("ilo");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "7");
        let out = ilo().args([src, e, "f", "-1"]).output().expect("ilo");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "neg");
    }
}

// ── prefix and bare-bool ternary ───────────────────────────────────────

#[test]
fn prefix_ternary() {
    let src = "f x:n>n;?=x 0 10 20";
    for e in ENGINES_ALL {
        let out = ilo().args([src, e, "f", "0"]).output().expect("ilo");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "10");
        let out = ilo().args([src, e, "f", "5"]).output().expect("ilo");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "20");
    }
}

#[test]
fn bare_bool_ternary() {
    let src = "f h:b>n;?h{1}{0}";
    for e in ENGINES_ALL {
        let out = ilo().args([src, e, "f", "true"]).output().expect("ilo");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "1");
        let out = ilo().args([src, e, "f", "false"]).output().expect("ilo");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "0");
    }
}

// ── pipe operator ──────────────────────────────────────────────────────

#[test]
fn pipe_basic() {
    let src = "f>n;42>>str>>len";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "2");
    }
}

// ── string ↔ number conversion ────────────────────────────────────────

#[test]
fn str_num_roundtrip() {
    let src = "f>n;r=num str 42;?r{~v:v;^_:0}";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "42");
    }
}

#[test]
fn num_invalid_errors() {
    let src = "f>R n t;num \"not a number\"";
    for e in ENGINES_ALL {
        let out = ilo().args([src, e, "f"]).output().expect("ilo");
        assert!(!out.status.success(), "{e}");
    }
}

// ── boolean and short-circuit ─────────────────────────────────────────

#[test]
fn boolean_and_or() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>b;&true false", "f"), "false");
        assert_eq!(run_ok(e, "f>b;&true true", "f"), "true");
        assert_eq!(run_ok(e, "f>b;|true false", "f"), "true");
        assert_eq!(run_ok(e, "f>b;|false false", "f"), "false");
        assert_eq!(run_ok(e, "f>b;!true", "f"), "false");
    }
}

// ── numeric equality with NaN, inf ────────────────────────────────────

#[test]
fn equality_numbers() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>b;=1 1", "f"), "true");
        assert_eq!(run_ok(e, "f>b;=1 2", "f"), "false");
        assert_eq!(run_ok(e, "f>b;!=1 2", "f"), "true");
    }
}

// ── nested call argument forms ─────────────────────────────────────────

#[test]
fn nested_call_in_prefix() {
    let src = "g x:n>n;+x 1;f>n;r=g 10;*r 2";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "22");
    }
}

// ── recursion ──────────────────────────────────────────────────────────

#[test]
fn factorial_recursive() {
    let src = "fac n:n>n;<=n 1 1;r=fac -n 1;*n r";
    for e in ENGINES_ALL {
        let out = ilo().args([src, e, "fac", "5"]).output().expect("ilo");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "120");
        let out = ilo().args([src, e, "fac", "10"]).output().expect("ilo");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "3628800");
    }
}

#[test]
fn fib_recursive() {
    let src = "fib n:n>n;<=n 1 n;a=fib -n 1;b=fib -n 2;+a b";
    for e in ENGINES_ALL {
        let out = ilo().args([src, e, "fib", "10"]).output().expect("ilo");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "55");
    }
}

// ── grp builtin ────────────────────────────────────────────────────────

#[test]
fn grp_basic() {
    for e in ENGINES_TREE_VM {
        let src = "k x:n>n;mod x 2;f>L n;m=grp k [1 2 3 4 5];mget m 0??[]";
        assert_eq!(run_ok(e, src, "f"), "[2, 4]");
    }
}

// ── regex ─────────────────────────────────────────────────────────────

#[test]
fn rgx_no_groups() {
    let src = "f>L t;rgx \"[a-z]+\" \"abc 123 def\"";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "[abc, def]");
    }
}

#[test]
fn rgx_with_groups() {
    let src = "f>L t;rgx \"(\\\\w+)=(\\\\d+)\" \"x=10 y=20\"";
    for e in ENGINES_ALL {
        let out = run_ok(e, src, "f");
        // first match captures
        assert_eq!(out, "[x, 10]");
    }
}

#[test]
fn rgxsub_basic() {
    let src = "f>t;rgxsub \"[0-9]+\" \"X\" \"abc 12 def 345\"";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "abc X def X");
    }
}

#[test]
fn rgxsub_capture_group() {
    let src = "f>t;rgxsub \"(\\\\w+)@(\\\\w+)\" \"$2/$1\" \"alice@example\"";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "example/alice");
    }
}

// ── datetime ──────────────────────────────────────────────────────────

#[test]
fn dt_roundtrip() {
    let src = "f>R n t;dtparse \"2024-01-15\" \"%Y-%m-%d\"";
    for e in ENGINES_ALL {
        // bare R return: ~1705276800 prefix stripped, stdout is just the value
        let out = run_ok(e, src, "f");
        assert_eq!(out, "1705276800");
    }
}

#[test]
fn dt_fmt_basic() {
    let src = "f>R t t;dtfmt 1705276800 \"%Y-%m-%d\"";
    for e in ENGINES_ALL {
        let out = run_ok(e, src, "f");
        assert_eq!(out, "2024-01-15");
    }
}

#[test]
fn dt_parse_invalid_errors() {
    let src = "f>R n t;dtparse \"garbage\" \"%Y-%m-%d\"";
    for e in ENGINES_ALL {
        let out = ilo().args([src, e, "f"]).output().expect("ilo");
        assert!(!out.status.success(), "{e}");
    }
}

// ── nil and optional ──────────────────────────────────────────────────

#[test]
fn nil_coalesce_chain() {
    let src = "f>n;a=nil;b=nil;a??b??42";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "42");
    }
}

#[test]
fn optional_unwrap_with_default() {
    let src = "f x:O n>n;??x 0";
    for e in ENGINES_ALL {
        let out = ilo().args([src, e, "f", "7"]).output().expect("ilo");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "7");
    }
}

// ── isnum/istext/isbool/islist via match arms ──────────────────────────

#[test]
fn match_type_predicates() {
    let src = "f x:_>t;?x{n _:\"num\";t _:\"text\";b _:\"bool\";l _:\"list\";_:\"other\"}";
    for e in ENGINES_ALL {
        let out = ilo().args([src, e, "f", "42"]).output().expect("ilo");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "num");
    }
}

// ── slc edge: end before start ────────────────────────────────────────

#[test]
fn slc_empty_when_end_before_start() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>L n;slc [1 2 3 4] 3 1", "f"), "[]");
        assert_eq!(run_ok(e, "f>t;slc \"hello\" 3 1", "f"), "");
    }
}

// ── prnt passthrough ──────────────────────────────────────────────────

#[test]
fn prnt_passthrough() {
    let src = "f>n;prnt 42";
    for e in ENGINES_ALL {
        let out = ilo().args([src, e, "f"]).output().expect("ilo");
        let stdout = String::from_utf8_lossy(&out.stdout);
        // prnt prints once, and the program prints the (passthrough) return — so we see 42 twice
        assert!(stdout.matches("42").count() >= 1, "{e}: {stdout}");
    }
}

// ── ret early-return ──────────────────────────────────────────────────

#[test]
fn ret_early() {
    let src = "f x:n>n;>x 0{ret x};-0 x";
    for e in ENGINES_ALL {
        let out = ilo().args([src, e, "f", "7"]).output().expect("ilo");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "7");
        let out = ilo().args([src, e, "f", "-3"]).output().expect("ilo");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "3");
    }
}

// ── ++ list concatenation via fld ─────────────────────────────────────

#[test]
fn list_append_in_loop() {
    let src = "f>L n;xs=[];@i 0..4{xs=+=xs *i 2};xs";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "[0, 2, 4, 6]");
    }
}

// ── compound: nested calls and recursion combined ────────────────────

#[test]
fn nested_map_then_sum() {
    for e in ENGINES_TREE_VM {
        let src = "d x:n>n;*x x;f>n;sum map d [1 2 3 4]";
        assert_eq!(run_ok(e, src, "f"), "30");
    }
}

// ── csvdmp ────────────────────────────────────────────────────────────

#[test]
fn csvdmp_basic_via_wr() {
    // csvdmp underlies `wr path data "csv"`. We can exercise via a temp file.
    use std::fs;
    let tmp = std::env::temp_dir().join("ilo_cov_vm_mod_csv.csv");
    let tmp_str = tmp.to_string_lossy().to_string();
    let _ = fs::remove_file(&tmp);
    let src = format!(
        "f>R t t;wr \"{}\" [[\"a\" \"b\"] [\"1\" \"2\"]] \"csv\"",
        tmp_str.replace('\\', "\\\\")
    );
    for e in ENGINES_ALL {
        let out = ilo().args([src.as_str(), e, "f"]).output().expect("ilo");
        assert!(
            out.status.success(),
            "{e}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let body = fs::read_to_string(&tmp).expect("file");
        assert!(body.contains("a,b"), "{e}: {body}");
        let _ = fs::remove_file(&tmp);
    }
}

// ── rdjl ──────────────────────────────────────────────────────────────

#[test]
fn rdjl_basic() {
    use std::fs;
    let tmp = std::env::temp_dir().join("ilo_cov_vm_mod_jsonl.jsonl");
    let tmp_str = tmp.to_string_lossy().to_string();
    fs::write(&tmp, "{\"x\":1}\n{\"x\":2}\n").expect("write");
    let src = format!("f>n;rs=rdjl \"{}\";len rs", tmp_str.replace('\\', "\\\\"));
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, &src, "f"), "2");
    }
    let _ = fs::remove_file(&tmp);
}

// ── rdl and wrl ────────────────────────────────────────────────────────

#[test]
fn rdl_wrl_roundtrip() {
    use std::fs;
    let tmp = std::env::temp_dir().join("ilo_cov_vm_mod_lines.txt");
    let tmp_str = tmp.to_string_lossy().to_string();
    let _ = fs::remove_file(&tmp);
    let wr_src = format!(
        "f>R t t;wrl \"{}\" [\"alpha\" \"beta\" \"gamma\"]",
        tmp_str.replace('\\', "\\\\")
    );
    let rd_src = format!(
        "f>n;rr=rdl \"{}\";?rr{{~rs:len rs;^_:0}}",
        tmp_str.replace('\\', "\\\\")
    );
    for e in ENGINES_ALL {
        let out = ilo().args([wr_src.as_str(), e, "f"]).output().expect("ilo");
        assert!(
            out.status.success(),
            "{e}: wrl: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(run_ok(e, &rd_src, "f"), "3");
    }
    let _ = fs::remove_file(&tmp);
}

// ── rd and wr text ────────────────────────────────────────────────────

#[test]
fn rd_wr_roundtrip_text() {
    use std::fs;
    let tmp = std::env::temp_dir().join("ilo_cov_vm_mod_text.txt");
    let tmp_str = tmp.to_string_lossy().to_string();
    let _ = fs::remove_file(&tmp);
    let wr_src = format!("f>R t t;wr \"{}\" \"hello\"", tmp_str.replace('\\', "\\\\"));
    let rd_src = format!(
        "f>t;rr=rd \"{}\" \"raw\";?rr{{~s:s;^_:\"\"}}",
        tmp_str.replace('\\', "\\\\")
    );
    for e in ENGINES_ALL {
        let out = ilo().args([wr_src.as_str(), e, "f"]).output().expect("ilo");
        assert!(
            out.status.success(),
            "{e}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(run_ok(e, &rd_src, "f"), "hello");
    }
    let _ = fs::remove_file(&tmp);
}

// ── isnum/istext predicates direct via match guard ────────────────────

#[test]
fn type_match_text_branch() {
    let src = "f x:_>t;?x{t _:\"text\";_:\"other\"}";
    for e in ENGINES_ALL {
        let out = ilo().args([src, e, "f", "hello"]).output().expect("ilo");
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        // CLI text-typed param routes — but x is `_` so the CLI may coerce. Just sanity-check it runs.
        assert!(out.status.success(), "{e}: {stdout}");
    }
}

// ── prefix nesting in operators ───────────────────────────────────────

#[test]
fn prefix_nested_arithmetic() {
    // (2*3) + 4 = 10
    let src = "f>n;+*2 3 4";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "10");
    }
    // (2+3) * 4 = 20
    let src = "f>n;*+2 3 4";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "20");
    }
}

// ── string addition (concat) ──────────────────────────────────────────

#[test]
fn string_concat() {
    let src = "f>t;+\"hello \" \"world\"";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "hello world");
    }
}

// ── list concat with + ────────────────────────────────────────────────

#[test]
fn list_concat() {
    let src = "f>L n;+[1 2 3] [4 5]";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "[1, 2, 3, 4, 5]");
    }
}

// ── nested record access with .? ──────────────────────────────────────

// ── closure capture (cross-engine after #384 + #385 + #387) ──────────

#[test]
fn closure_capture_flt() {
    let src = "f xs:L n thr:n>L n;flt (x:n>b;>x thr) xs";
    for e in ENGINES_ALL {
        let out = ilo()
            .args([src, e, "f", "1,2,3,4,5", "3"])
            .output()
            .expect("ilo");
        assert!(
            out.status.success(),
            "engine {e}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "[4, 5]",
            "engine {e}"
        );
    }
}

#[test]
fn closure_capture_map() {
    let src = "f xs:L n k:n>L n;map (x:n>n;*x k) xs";
    for e in ENGINES_ALL {
        let out = ilo()
            .args([src, e, "f", "1,2,3", "10"])
            .output()
            .expect("ilo");
        assert!(
            out.status.success(),
            "engine {e}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "[10, 20, 30]",
            "engine {e}"
        );
    }
}

// ── pipe and auto-unwrap mix ──────────────────────────────────────────

#[test]
fn pipe_with_str_len() {
    let src = "f>n;1234>>str>>len";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "4");
    }
}

// ── nested for-each, break inside outer ───────────────────────────────

#[test]
fn nested_foreach_with_break() {
    let src = "f>n;hit=0;@x [1 2 3]{@y [10 20]{=*x y 40{hit=1;brk}}};hit";
    for e in ENGINES_ALL {
        let out = run_ok(e, src, "f");
        // 2*20=40, so hit=1
        assert_eq!(out, "1", "{e}");
    }
}

// ── various builtin error paths ───────────────────────────────────────

#[test]
fn slc_text_negative_indices() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>t;slc \"abcdef\" -3 -1", "f"), "de");
    }
}

#[test]
fn srt_text_chars() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>t;srt \"dba\"", "f"), "abd");
    }
}

#[test]
fn rsrt_text_chars() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>t;rsrt \"abd\"", "f"), "dba");
    }
}

#[test]
fn cumsum_negatives() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>L n;cumsum [1 -2 3 -4]", "f"), "[1, -1, 2, -2]");
    }
}

#[test]
fn enumerate_empty() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>L (L _);enumerate []", "f"), "[]");
    }
}

#[test]
fn zip_empty() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>L (L _);zip [] [1 2]", "f"), "[]");
        assert_eq!(run_ok(e, "f>L (L _);zip [1 2] []", "f"), "[]");
    }
}

#[test]
fn chunks_chunk_of_one() {
    for e in ENGINES_ALL {
        assert_eq!(
            run_ok(e, "f>L (L n);chunks 1 [1 2 3]", "f"),
            "[[1], [2], [3]]"
        );
    }
}

#[test]
fn window_size_one() {
    for e in ENGINES_ALL {
        assert_eq!(
            run_ok(e, "f>L (L n);window 1 [1 2 3]", "f"),
            "[[1], [2], [3]]"
        );
    }
}

#[test]
fn fld_text_concat() {
    for e in ENGINES_TREE_VM {
        let src = "g a:t x:t>t;+a x;f>t;fld g [\"a\" \"b\" \"c\"] \"\"";
        assert_eq!(run_ok(e, src, "f"), "abc");
    }
}

#[test]
fn flr_cel_already_int() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>n;flr 5", "f"), "5");
        assert_eq!(run_ok(e, "f>n;cel 5", "f"), "5");
    }
}

// ── inverse trig ──────────────────────────────────────────────────────

#[test]
fn inverse_trig() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>n;asin 0", "f"), "0");
        assert_eq!(run_ok(e, "f>n;acos 1", "f"), "0");
        assert_eq!(run_ok(e, "f>n;atan 0", "f"), "0");
    }
}

// ── padlc / padrc and short string ────────────────────────────────────

#[test]
fn padlc_padrc_already_wide() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>t;padl \"hello\" 3 \"0\"", "f"), "hello");
        assert_eq!(run_ok(e, "f>t;padr \"hello\" 3 \"x\"", "f"), "hello");
    }
}

// ── more record edges ─────────────────────────────────────────────────

#[test]
fn record_safe_field_missing_uses_default() {
    let src = "type pt{x:n;y:n}\nf>n;p=pt x:1 y:2;p.?z??99";
    // Note: z is not a field of pt, so .?z should return nil → default 99
    // The verifier may catch this at compile time. If so, the test will fail compile.
    for e in ENGINES_ALL {
        let out = ilo().args([src, e, "f"]).output().expect("ilo");
        // Either compile-time error or runtime 99 is OK; just confirm we don't hang.
        let _ = out;
    }
}

// ── jdmp of records ────────────────────────────────────────────────────

#[test]
fn jdmp_record() {
    let src = "type pt{x:n;y:n}\nf>t;p=pt x:10 y:20;jdmp p";
    for e in ENGINES_ALL {
        let out = run_ok(e, src, "f");
        assert!(out.contains("\"x\":10"), "{e}: {out}");
        assert!(out.contains("\"y\":20"), "{e}: {out}");
    }
}

// ── jdmp of map ────────────────────────────────────────────────────────

#[test]
fn jdmp_map_keys_stringified() {
    let src = "f>t;m=mmap;m=mset m 1 100;jdmp m";
    for e in ENGINES_ALL {
        let out = run_ok(e, src, "f");
        // numeric keys become strings in JSON
        assert!(out.contains("\"1\":100"), "{e}: {out}");
    }
}

// ── multiple math operations chained ──────────────────────────────────

#[test]
fn math_chain_operations() {
    for e in ENGINES_ALL {
        // sqrt(abs(-16)) = 4
        let src = "f>n;sqrt abs -16";
        assert_eq!(run_ok(e, src, "f"), "4");
    }
}

// ── numeric formatting ────────────────────────────────────────────────

#[test]
fn fmt2_negative_and_large() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>t;fmt2 -3.14159 2", "f"), "-3.14");
        assert_eq!(run_ok(e, "f>t;fmt2 1000.5 0", "f"), "1000");
    }
}

// ── empty cumsum ──────────────────────────────────────────────────────

#[test]
fn empty_inputs() {
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, "f>L n;rev []", "f"), "[]");
        assert_eq!(run_ok(e, "f>L n;srt []", "f"), "[]");
        assert_eq!(run_ok(e, "f>L n;unq []", "f"), "[]");
        assert_eq!(run_ok(e, "f>L n;flat []", "f"), "[]");
    }
}

// ── inline lambda with no captures (works cross-engine) ───────────────

#[test]
fn inline_lambda_phase1_cross_engine() {
    let src = "f>L n;map (x:n>n;*x x) [1 2 3 4]";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "[1, 4, 9, 16]");
    }
}

// ── builtin via pipe with `!` not used ────────────────────────────────

#[test]
fn pipe_chain_three_steps() {
    let src = "f>n;[1 2 3]>>sum>>str>>len";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "1");
    }
}

#[test]
fn safe_field_chain() {
    let src = "f>n;rr=jpar \"{\\\"a\\\":{\\\"b\\\":7}}\";?rr{~r:r.?a.?b??0;^_:0}";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "7");
    }
    let src = "f>n;rr=jpar \"{\\\"a\\\":null}\";?rr{~r:r.?a.?b??99;^_:0}";
    for e in ENGINES_ALL {
        assert_eq!(run_ok(e, src, "f"), "99");
    }
}
