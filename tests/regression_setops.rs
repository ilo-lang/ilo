// Regression tests for `setunion`/`setinter`/`setdiff` — set operations on
// lists, backed by HashSet<String> with type-prefixed keys (`t:`/`n:`/`b:`)
// to keep Number(5) and Text("5") in separate domains.
//
// Output is sorted (stringwise on the type-prefixed key) for determinism.
// Verified across tree, vm, and cranelift engines.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str) -> String {
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

macro_rules! tri_engine_test {
    ($name:ident, $src:expr, $expect:expr) => {
        mod $name {
            use super::*;
            const SRC: &str = $src;
            const EXPECT: &str = $expect;

            #[test]
            fn tree() {
                assert_eq!(run("--run-tree", SRC, "f"), EXPECT);
            }

            #[test]
            fn vm() {
                assert_eq!(run("--run-vm", SRC, "f"), EXPECT);
            }

            #[test]
            #[cfg(feature = "cranelift")]
            fn cranelift() {
                assert_eq!(run("--run-cranelift", SRC, "f"), EXPECT);
            }
        }
    };
}

// Basic union: combine two number lists, dedupe, sort.
tri_engine_test!(
    union_basic,
    "f>L n;setunion [1,2,3] [2,3,4]",
    "[1, 2, 3, 4]"
);

// Basic intersection.
tri_engine_test!(inter_basic, "f>L n;setinter [1,2,3] [2,3,4]", "[2, 3]");

// Basic difference.
tri_engine_test!(diff_basic, "f>L n;setdiff [1,2,3] [2,3,4]", "[1]");

// Dedup within a single input.
tri_engine_test!(union_dedup, "f>L n;setunion [1,1,2] [2,2,3]", "[1, 2, 3]");

// Empty operand: intersection with anything is empty.
tri_engine_test!(inter_empty, "f>L n;setinter [] [1,2]", "[]");

// Strings: union dedupes and sorts.
tri_engine_test!(
    union_strings,
    "f>L t;setunion [\"a\",\"b\"] [\"b\",\"c\"]",
    "[a, b, c]"
);

// Cross-type guard: Number(1) and Text("1") must NOT collapse.
// Output uses heterogeneous element type — declare as `L a`.
// `n:1` sorts before `t:1` stringwise, so 1 comes before "1".
// (Strings render without quotes; we assert length-2 by listing both.)
tri_engine_test!(union_cross_type, "f>L a;setunion [1] [\"1\"]", "[1, 1]");

// Cross-type guard with distinguishable text element.
tri_engine_test!(
    union_cross_type_distinct,
    "f>L a;setunion [1] [\"x\"]",
    "[1, x]"
);

// Sort is *lexicographic* on the prefixed-string key, not numeric. So
// `setunion [10, 2] []` yields [10, 2] because "n:10" < "n:2" in string
// order. Use `srt` after the set op for numeric order. This test pins the
// documented caveat so any future change to numeric sort is intentional.
tri_engine_test!(
    union_lexicographic_order,
    "f>L n;setunion [10, 2] []",
    "[10, 2]"
);
