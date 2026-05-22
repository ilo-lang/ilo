// Regression tests for native integer type annotations (ILO-394).
//
// U32, U64, and I64 type annotations are accepted in parameter and return
// positions. The tree-walker engine stores all values as f64; the types are
// used for documentation and static checking only. Precision is exact for
// values ≤ 2^32 (U32), and exact up to 2^53 for U64/I64.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

/// Run `ilo <source> --vm <entry_fn> <args...>`
fn run_ok(src: &str, entry_fn: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg("--vm").arg(entry_fn);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo --vm failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// --- U32 parameter and return type ---
const U32_SRC: &str = "add-u32 a:U32 b:U32>U32;+ a b";

#[test]
fn u32_param_return_type_accepted() {
    assert_eq!(run_ok(U32_SRC, "add-u32", &["3", "4"]), "7");
}

// --- U64 parameter and return type ---
const U64_SRC: &str = "add-u64 a:U64 b:U64>U64;+ a b";

#[test]
fn u64_param_return_type_accepted() {
    assert_eq!(run_ok(U64_SRC, "add-u64", &["100000", "200000"]), "300000");
}

// --- I64 parameter and return type ---
const I64_SRC: &str = "negate a:I64>I64;- 0 a";

#[test]
fn i64_param_return_type_accepted() {
    assert_eq!(run_ok(I64_SRC, "negate", &["42"]), "-42");
}

// --- U32 in list type position ---
const LIST_U32_SRC: &str = "len-u32 xs:L U32>U32;len xs";

#[test]
fn u32_in_list_type_position() {
    assert_eq!(run_ok(LIST_U32_SRC, "len-u32", &["[1, 2, 3]"]), "3");
}

// --- U32 in optional type position ---
const OPT_U32_SRC: &str = "unwrap-or x:O U32>U32;?? x 0";

#[test]
fn u32_in_optional_nil() {
    assert_eq!(run_ok(OPT_U32_SRC, "unwrap-or", &["nil"]), "0");
}

#[test]
fn u32_in_optional_value() {
    assert_eq!(run_ok(OPT_U32_SRC, "unwrap-or", &["5"]), "5");
}

// --- Mixed: U32 compatible with n (both stored as f64) ---
const MIX_SRC: &str = "double x:U32>n;* x 2";

#[test]
fn u32_compatible_with_number_return() {
    assert_eq!(run_ok(MIX_SRC, "double", &["21"]), "42");
}

// --- Bitwise ops work on U32-annotated values ---
const BAND_U32_SRC: &str = "mask a:U32 b:U32>U32;band a b";

#[test]
fn band_on_u32_annotated_values() {
    assert_eq!(run_ok(BAND_U32_SRC, "mask", &["12", "10"]), "8");
}

// --- I64 with negative results ---
const DIFF_I64_SRC: &str = "diff a:I64 b:I64>I64;- a b";

#[test]
fn i64_negative_result() {
    assert_eq!(run_ok(DIFF_I64_SRC, "diff", &["3", "10"]), "-7");
}

// --- U64 in F (function) type position ---
const FN_U64_SRC: &str = "apply-fn f:F U64 U64 x:U64>U64;f x";

#[test]
fn u64_in_fn_type_position() {
    // pass `double` (a U64->U64 builtin-like function)
    // We use a simple helper instead
    let src = "dbl x:U64>U64;* x 2\napply-fn f:F U64 U64 x:U64>U64;f x";
    assert_eq!(run_ok(src, "apply-fn", &["dbl", "5"]), "10");
}

// --- U32 type alias compatibility ---
// `alias` declares a type alias; `type` declares a record type.
const ALIAS_SRC: &str = "alias uint U32\nadd-uint a:uint b:uint>uint;+ a b";

#[test]
fn u32_type_alias() {
    assert_eq!(run_ok(ALIAS_SRC, "add-uint", &["6", "7"]), "13");
}
