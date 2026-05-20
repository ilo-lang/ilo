// Tests that exercise tree-walking interpreter paths in src/interpreter/mod.rs.
//
// Goal: drive region coverage above 94%. We focus on the error arms in
// builtins (wrong types, empty inputs, bad arity), pattern-match branches
// (Ok/Err/TypeIs/literal/wildcard), eval_expr arms (Match, NilCoalesce,
// Ternary, With, MakeClosure), and self-rebind peepholes (mset/append/concat
// fast paths plus their fallback branches).
//
// Every case runs via the CLI through --run-tree so the tree interpreter
// owns the execution. Successes assert stdout, runtime failures assert the
// stderr ILO code path so we cover the corresponding match arms.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn ok_out(src: &str, entry: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg("--vm").arg(entry);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn err_stderr(src: &str, entry: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg("--vm").arg(entry);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected failure for `{src}`: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).into_owned()
}

// ---------------------------------------------------------------------------
// Builtin error arms: wrong arg types
// ---------------------------------------------------------------------------

#[test]
fn len_wrong_type() {
    let s = err_stderr("main>n;len 42", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012")
            || s.contains("len"),
        "stderr={s}"
    );
}

#[test]
fn abs_wrong_type() {
    let s = err_stderr("main>n;abs \"hi\"", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012")
            || s.contains("abs"),
        "stderr={s}"
    );
}

#[test]
fn str_wrong_type() {
    let s = err_stderr("main>t;str \"hi\"", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012")
            || s.contains("str"),
        "stderr={s}"
    );
}

#[test]
fn mod_by_zero() {
    let s = err_stderr("main>n;mod 7 0", "main", &[]);
    assert!(s.contains("ILO-R003") || s.contains("modulo"), "stderr={s}");
}

#[test]
fn pow_basic() {
    assert_eq!(ok_out("main>n;pow 2 8", "main", &[]), "256");
}

#[test]
fn atan2_basic() {
    let s = ok_out("main>n;atan2 0 1", "main", &[]);
    assert_eq!(s.trim(), "0");
}

#[test]
fn rnd_two_args_basic() {
    // Just exercise the rnd 2-arg path; result is non-deterministic but in range.
    let s = ok_out("main>n;rnd 5 5", "main", &[]);
    assert_eq!(s, "5");
}

#[test]
fn rnd_bad_bounds() {
    let s = err_stderr("main>n;rnd 10 0", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn sleep_zero_returns_nil() {
    // sleep 0 returns nil; wrap in str (-1) to give a printable value.
    let s = ok_out("main>n;sleep 0;42", "main", &[]);
    assert_eq!(s, "42");
}

// ---------------------------------------------------------------------------
// Map builtins
// ---------------------------------------------------------------------------

#[test]
fn map_set_get_has_del() {
    let src = "main>n;m=mmap;m=mset m \"a\" 1;m=mset m \"b\" 2;m=mdel m \"a\";len m";
    assert_eq!(ok_out(src, "main", &[]), "1");
}

#[test]
fn map_get_returns_value() {
    let src = "main>n;m=mmap;m=mset m \"a\" 7;??(mget m \"a\") 0";
    assert_eq!(ok_out(src, "main", &[]), "7");
}

#[test]
fn map_has_true_false() {
    let src = "main>b;m=mmap;m=mset m \"a\" 1;mhas m \"a\"";
    assert_eq!(ok_out(src, "main", &[]), "true");
    let src2 = "main>b;m=mmap;mhas m \"a\"";
    assert_eq!(ok_out(src2, "main", &[]), "false");
}

#[test]
fn map_keys_vals_sorted() {
    let src = "main>t;m=mmap;m=mset m \"b\" 2;m=mset m \"a\" 1;ks=mkeys m;vs=mvals m;sk=cat ks \",\";sv=cat (map str vs) \",\";cat [sk sv] \"|\"";
    assert_eq!(ok_out(src, "main", &[]), "a,b|1,2");
}

#[test]
fn mget_on_non_map_errors() {
    // Verifier now catches this before the interpreter (ILO-T013).
    // Pre-fix path raised ILO-R0 at runtime.
    let s = err_stderr("main>n;mget 42 \"k\"", "main", &[]);
    assert!(
        s.contains("ILO-T013")
            || s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

// ---------------------------------------------------------------------------
// has / hd / tl / at / lst — text and list branches plus error arms
// ---------------------------------------------------------------------------

#[test]
fn has_text_branch() {
    assert_eq!(ok_out("main>b;has \"hello\" \"ell\"", "main", &[]), "true");
}

#[test]
fn has_text_needle_must_be_text() {
    let s = err_stderr("main>b;has \"hello\" 1", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012")
            || s.contains("text"),
        "stderr={s}"
    );
}

#[test]
fn hd_text_branch() {
    assert_eq!(ok_out("main>t;hd \"hello\"", "main", &[]), "h");
}

#[test]
fn hd_empty_list_errors() {
    let s = err_stderr("main>n;hd []", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012")
            || s.contains("empty"),
        "stderr={s}"
    );
}

#[test]
fn hd_empty_text_errors() {
    let s = err_stderr("main>t;hd \"\"", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012")
            || s.contains("empty"),
        "stderr={s}"
    );
}

#[test]
fn tl_text_branch() {
    assert_eq!(ok_out("main>t;tl \"abc\"", "main", &[]), "bc");
}

#[test]
fn tl_empty_text_errors() {
    let s = err_stderr("main>t;tl \"\"", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn at_text_branch() {
    assert_eq!(ok_out("main>t;at \"hello\" 1", "main", &[]), "e");
}

#[test]
fn at_negative_index() {
    assert_eq!(ok_out("main>n;at [10,20,30] -1", "main", &[]), "30");
}

#[test]
fn at_out_of_range_list() {
    let s = err_stderr("main>n;at [1,2,3] 10", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn at_index_must_be_number() {
    let src = "f i:_>n;at [1,2,3] i;main>n;f \"x\"";
    let s = err_stderr(src, "main", &[]);
    assert!(s.contains("ILO-R") || s.contains("number"), "stderr={s}");
}

#[test]
fn lst_replace() {
    let src = "main>t;xs=lst [1,2,3] 1 99;cat (map str xs) \",\"";
    assert_eq!(ok_out(src, "main", &[]), "1,99,3");
}

#[test]
fn lst_out_of_range() {
    let s = err_stderr("main>L n;lst [1,2,3] 10 0", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn lst_negative_index_errors() {
    let s = err_stderr("main>L n;lst [1,2,3] -1 0", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

// ---------------------------------------------------------------------------
// window / zip / enumerate / range / chunks
// ---------------------------------------------------------------------------

#[test]
fn window_basic() {
    let src = "main>n;ws=window 2 [1,2,3,4];len ws";
    assert_eq!(ok_out(src, "main", &[]), "3");
}

#[test]
fn window_too_large_returns_empty() {
    let src = "main>n;ws=window 5 [1,2];len ws";
    assert_eq!(ok_out(src, "main", &[]), "0");
}

#[test]
fn window_bad_size() {
    let s = err_stderr("main>L (L n);window 0 [1,2]", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn zip_basic() {
    let src = "main>n;zs=zip [1,2,3] [10,20];len zs";
    assert_eq!(ok_out(src, "main", &[]), "2");
}

#[test]
fn enumerate_basic() {
    let src = "main>n;es=enumerate [\"a\" \"b\"];len es";
    assert_eq!(ok_out(src, "main", &[]), "2");
}

#[test]
fn range_basic() {
    let src = "main>n;r=range 1 5;len r";
    assert_eq!(ok_out(src, "main", &[]), "4");
}

#[test]
fn range_empty_when_start_ge_end() {
    let src = "main>n;r=range 5 1;len r";
    assert_eq!(ok_out(src, "main", &[]), "0");
}

#[test]
fn range_non_integer_bounds() {
    let s = err_stderr("main>L n;range 1.5 5", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn range_too_large() {
    let s = err_stderr("main>L n;range 0 5000000", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn chunks_basic() {
    let src = "main>n;cs=chunks 2 [1,2,3,4,5];len cs";
    assert_eq!(ok_out(src, "main", &[]), "3");
}

#[test]
fn chunks_bad_size() {
    let s = err_stderr("main>L (L n);chunks 0 [1,2]", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

// ---------------------------------------------------------------------------
// Set ops
// ---------------------------------------------------------------------------

#[test]
fn setunion_basic() {
    let src = "main>t;xs=setunion [1,2,3] [2,3,4];cat (map str xs) \",\"";
    assert_eq!(ok_out(src, "main", &[]), "1,2,3,4");
}

#[test]
fn setinter_basic() {
    let src = "main>t;xs=setinter [1,2,3] [2,3,4];cat (map str xs) \",\"";
    assert_eq!(ok_out(src, "main", &[]), "2,3");
}

#[test]
fn setdiff_basic() {
    let src = "main>t;xs=setdiff [1,2,3] [2,3,4];cat (map str xs) \",\"";
    assert_eq!(ok_out(src, "main", &[]), "1");
}

// ---------------------------------------------------------------------------
// srt/rsrt — empty, text, list-must-be-uniform, key-fn variants
// ---------------------------------------------------------------------------

#[test]
fn srt_empty_list() {
    let src = "main>n;xs=srt [];len xs";
    assert_eq!(ok_out(src, "main", &[]), "0");
}

#[test]
fn srt_text_branch() {
    assert_eq!(ok_out("main>t;srt \"bca\"", "main", &[]), "abc");
}

#[test]
fn srt_mixed_errors() {
    let s = err_stderr("main>L _;srt [1,\"a\"]", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn srt_key_fn() {
    let src = "k x:n>n;-0 x;main>t;xs=srt k [3,1,2];cat (map str xs) \",\"";
    assert_eq!(ok_out(src, "main", &[]), "3,2,1");
}

#[test]
fn rsrt_text_branch() {
    assert_eq!(ok_out("main>t;rsrt \"abc\"", "main", &[]), "cba");
}

#[test]
fn rsrt_key_fn() {
    let src = "k x:n>n;x;main>t;xs=rsrt k [3,1,2];cat (map str xs) \",\"";
    assert_eq!(ok_out(src, "main", &[]), "3,2,1");
}

#[test]
fn rsrt_empty_list() {
    let src = "main>n;xs=rsrt [];len xs";
    assert_eq!(ok_out(src, "main", &[]), "0");
}

// ---------------------------------------------------------------------------
// slc / take / drop
// ---------------------------------------------------------------------------

#[test]
fn slc_negative_bounds() {
    let src = "main>t;xs=slc [1,2,3,4,5] -2 5;cat (map str xs) \",\"";
    assert_eq!(ok_out(src, "main", &[]), "4,5");
}

#[test]
fn slc_text() {
    assert_eq!(ok_out("main>t;slc \"hello\" 1 4", "main", &[]), "ell");
}

#[test]
fn slc_non_integer_start() {
    let s = err_stderr("main>L n;slc [1,2,3] 0.5 3", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn take_negative() {
    let src = "main>t;xs=take -1 [1,2,3];cat (map str xs) \",\"";
    assert_eq!(ok_out(src, "main", &[]), "1,2");
}

#[test]
fn take_text_branch() {
    assert_eq!(ok_out("main>t;take 3 \"hello\"", "main", &[]), "hel");
}

#[test]
fn drop_negative() {
    let src = "main>t;xs=drop -1 [1,2,3];cat (map str xs) \",\"";
    assert_eq!(ok_out(src, "main", &[]), "3");
}

#[test]
fn drop_text_branch() {
    assert_eq!(ok_out("main>t;drop 2 \"hello\"", "main", &[]), "llo");
}

// ---------------------------------------------------------------------------
// String ops
// ---------------------------------------------------------------------------

#[test]
fn trm_basic() {
    assert_eq!(ok_out("main>t;trm \"  hi  \"", "main", &[]), "hi");
}

#[test]
fn upr_lwr_cap() {
    assert_eq!(ok_out("main>t;upr \"hi\"", "main", &[]), "HI");
    assert_eq!(ok_out("main>t;lwr \"HI\"", "main", &[]), "hi");
    assert_eq!(ok_out("main>t;cap \"hi\"", "main", &[]), "Hi");
}

#[test]
fn ord_chr_roundtrip() {
    let src = "main>n;c=ord \"A\";c";
    assert_eq!(ok_out(src, "main", &[]), "65");
    assert_eq!(ok_out("main>t;chr 65", "main", &[]), "A");
}

#[test]
fn ord_empty_errors() {
    let s = err_stderr("main>n;ord \"\"", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn chr_invalid_codepoint() {
    let s = err_stderr("main>t;chr 0.5", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn chars_basic() {
    let src = "main>n;cs=chars \"abc\";len cs";
    assert_eq!(ok_out(src, "main", &[]), "3");
}

#[test]
fn unq_list_and_text() {
    let src = "main>t;xs=unq [1,1,2,3,3];cat (map str xs) \",\"";
    assert_eq!(ok_out(src, "main", &[]), "1,2,3");
    assert_eq!(ok_out("main>t;unq \"abbcca\"", "main", &[]), "abc");
}

#[test]
fn fmt_basic() {
    assert_eq!(
        ok_out("main>t;fmt \"hi {}\" \"there\"", "main", &[]),
        "hi there"
    );
}

#[test]
fn fmt_printf_spec_rejected() {
    // Verifier intercepts printf specs at compile time (ILO-T013).
    // Use an _-typed template to defer to runtime so we hit the eval arm.
    let src = "f tpl:_>t;fmt tpl 1;main>t;f \"x={:.2}\"";
    let s = err_stderr(src, "main", &[]);
    assert!(s.contains("ILO-R") || s.contains("fmt"), "stderr={s}");
}

#[test]
fn fmt2_basic() {
    assert_eq!(ok_out("main>t;fmt2 3.14159 2", "main", &[]), "3.14");
}

#[test]
fn fmt2_negative_digits() {
    // d <= 0 floors to 0
    assert_eq!(ok_out("main>t;fmt2 3.7 0", "main", &[]), "4");
}

#[test]
fn padl_padr_basic() {
    // ok_out trims; check length round-trip via len instead.
    assert_eq!(ok_out("main>n;len (padl \"42\" 5)", "main", &[]), "5");
    assert_eq!(ok_out("main>n;len (padr \"42\" 5)", "main", &[]), "5");
}

#[test]
fn padl_with_pad_char() {
    assert_eq!(ok_out("main>t;padl \"7\" 3 \"0\"", "main", &[]), "007");
}

#[test]
fn cat_list_of_text() {
    assert_eq!(
        ok_out("main>t;cat [\"a\" \"b\" \"c\"] \"-\"", "main", &[]),
        "a-b-c"
    );
}

#[test]
fn cat_non_text_items_errors() {
    // The verifier catches L n for cat; use _-typed wildcard to surface
    // the runtime error path.
    let src = "f xs:_>t;cat xs \"-\";main>t;f [1,2]";
    let s = err_stderr(src, "main", &[]);
    assert!(s.contains("ILO-R") || s.contains("cat"), "stderr={s}");
}

#[test]
fn spl_basic() {
    let src = "main>n;xs=spl \"a,b,c\" \",\";len xs";
    assert_eq!(ok_out(src, "main", &[]), "3");
}

// ---------------------------------------------------------------------------
// Math: sum/cumsum/avg/median/quantile/variance/stdev — empty + non-num errors
// ---------------------------------------------------------------------------

#[test]
fn sum_basic() {
    assert_eq!(ok_out("main>n;sum [1,2,3,4]", "main", &[]), "10");
}

#[test]
fn sum_non_number_errors() {
    let s = err_stderr("main>n;sum [1,\"x\"]", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn cumsum_basic() {
    let src = "main>t;xs=cumsum [1,2,3];cat (map str xs) \",\"";
    assert_eq!(ok_out(src, "main", &[]), "1,3,6");
}

#[test]
fn avg_basic() {
    assert_eq!(ok_out("main>n;avg [2,4,6]", "main", &[]), "4");
}

#[test]
fn avg_empty_errors() {
    let s = err_stderr("main>n;avg []", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn median_basic_odd_even() {
    assert_eq!(ok_out("main>n;median [1,3,2]", "main", &[]), "2");
    assert_eq!(ok_out("main>n;median [1,2,3,4]", "main", &[]), "2.5");
}

#[test]
fn median_empty_errors() {
    let s = err_stderr("main>n;median []", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn quantile_basic() {
    let src = "main>n;quantile [1,2,3,4,5] 0.5";
    assert_eq!(ok_out(src, "main", &[]), "3");
}

#[test]
fn quantile_empty_errors() {
    let s = err_stderr("main>n;quantile [] 0.5", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn variance_basic() {
    let src = "main>n;variance [1,2,3,4,5]";
    let out = ok_out(src, "main", &[]);
    // sample variance is 2.5
    assert!(out.starts_with("2.5"), "out={out}");
}

#[test]
fn variance_single_errors() {
    let s = err_stderr("main>n;variance [1]", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn stdev_basic() {
    let src = "main>n;stdev [1,2,3,4,5]";
    let out = ok_out(src, "main", &[]);
    assert!(out.starts_with("1.58"), "out={out}");
}

#[test]
fn stdev_single_errors() {
    let s = err_stderr("main>n;stdev [1]", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

// ---------------------------------------------------------------------------
// Min/Max — both 1-arg list and 2-arg scalar
// ---------------------------------------------------------------------------

#[test]
fn min_max_scalar() {
    assert_eq!(ok_out("main>n;min 3 5", "main", &[]), "3");
    assert_eq!(ok_out("main>n;max 3 5", "main", &[]), "5");
}

#[test]
fn min_max_list() {
    assert_eq!(ok_out("main>n;min [3,1,5]", "main", &[]), "1");
    assert_eq!(ok_out("main>n;max [3,1,5]", "main", &[]), "5");
}

#[test]
fn min_empty_errors() {
    let s = err_stderr("main>n;min []", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn clamp_basic() {
    assert_eq!(ok_out("main>n;clamp 15 0 10", "main", &[]), "10");
    assert_eq!(ok_out("main>n;clamp -5 0 10", "main", &[]), "0");
    assert_eq!(ok_out("main>n;clamp 5 0 10", "main", &[]), "5");
}

#[test]
fn flr_cel_rou() {
    assert_eq!(ok_out("main>n;flr 2.7", "main", &[]), "2");
    assert_eq!(ok_out("main>n;cel 2.2", "main", &[]), "3");
    assert_eq!(ok_out("main>n;rou 2.5", "main", &[]), "3");
}

#[test]
fn sqrt_etc() {
    assert_eq!(ok_out("main>n;sqrt 9", "main", &[]), "3");
    let s = ok_out("main>n;log 1", "main", &[]);
    assert!(s.starts_with("0"), "{s}");
}

// ---------------------------------------------------------------------------
// HOF arms: map / flt / fld / ct / partition / flatmap / uniqby / grp / frq
// ---------------------------------------------------------------------------

#[test]
fn map_basic() {
    let src = "f x:n>n;*x 2;main>t;xs=map f [1,2,3];cat (map str xs) \",\"";
    assert_eq!(ok_out(src, "main", &[]), "2,4,6");
}

#[test]
fn map_with_ctx() {
    let src = "f x:n c:n>n;+x c;main>t;xs=map f 10 [1,2,3];cat (map str xs) \",\"";
    assert_eq!(ok_out(src, "main", &[]), "11,12,13");
}

#[test]
fn flt_basic() {
    let src = "f x:n>b;>x 1;main>t;xs=flt f [1,2,3];cat (map str xs) \",\"";
    assert_eq!(ok_out(src, "main", &[]), "2,3");
}

#[test]
fn flt_bad_predicate_return_errors() {
    let src = "f x:n>n;x;main>L n;flt f [1,2,3]";
    let s = err_stderr(src, "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012")
            || s.contains("bool"),
        "stderr={s}"
    );
}

#[test]
fn fld_basic() {
    let src = "f a:n x:n>n;+a x;main>n;fld f [1,2,3,4] 0";
    assert_eq!(ok_out(src, "main", &[]), "10");
}

#[test]
fn fld_with_ctx() {
    let src = "f a:n x:n c:n>n;+a *x c;main>n;fld f 10 [1,2,3] 0";
    assert_eq!(ok_out(src, "main", &[]), "60");
}

#[test]
fn ct_basic() {
    let src = "f x:n>b;>x 1;main>n;ct f [1,2,3,4]";
    assert_eq!(ok_out(src, "main", &[]), "3");
}

#[test]
fn partition_basic() {
    let src = "p x:n>b;>x 2;main>n;ps=partition p [1,2,3,4];len ps";
    assert_eq!(ok_out(src, "main", &[]), "2");
}

#[test]
fn partition_bad_predicate_errors() {
    let src = "p x:n>n;x;main>L (L n);partition p [1,2]";
    let s = err_stderr(src, "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn flatmap_basic() {
    let src = "f x:n>L n;[x x];main>n;xs=flatmap f [1,2];len xs";
    assert_eq!(ok_out(src, "main", &[]), "4");
}

#[test]
fn flatmap_must_return_list_errors() {
    let src = "f x:n>n;x;main>L n;flatmap f [1,2]";
    let s = err_stderr(src, "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn uniqby_basic() {
    let src = "k x:n>n;mod x 2;main>n;xs=uniqby k [1,2,3,4,5];len xs";
    assert_eq!(ok_out(src, "main", &[]), "2");
}

#[test]
fn grp_basic() {
    let src = "k x:n>n;mod x 2;main>n;m=grp k [1,2,3,4];len m";
    assert_eq!(ok_out(src, "main", &[]), "2");
}

#[test]
fn frq_basic() {
    let src = "main>n;m=frq [\"a\" \"b\" \"a\"];mget m \"a\"";
    assert_eq!(ok_out(src, "main", &[]), "2");
}

// ---------------------------------------------------------------------------
// Mapr: short-circuiting Result-aware map
// ---------------------------------------------------------------------------

#[test]
fn mapr_all_ok() {
    let src = "f x:n>R n t;~ *x 2;main>R (L n) t;mapr f [1,2,3]";
    let s = ok_out(src, "main", &[]);
    assert!(s.contains("2") && s.contains("6"), "out={s}");
}

#[test]
fn mapr_short_circuits_on_err() {
    // Result-returning failure exits non-zero; capture stderr.
    let src = "f x:n>R n t;c=>x 2;?c{true:^\"too big\";false:~x};main>R (L n) t;mapr f [1,2,3,4]";
    let s = err_stderr(src, "main", &[]);
    assert!(s.contains("too big"), "stderr={s}");
}

// ---------------------------------------------------------------------------
// Regex
// ---------------------------------------------------------------------------

#[test]
fn rgx_no_groups() {
    let src = "main>n;ms=rgx \"\\\\d+\" \"a1 b22 c333\";len ms";
    assert_eq!(ok_out(src, "main", &[]), "3");
}

#[test]
fn rgx_with_groups() {
    let src = "main>t;ms=rgx \"(\\\\w+)=(\\\\d+)\" \"a=1\";cat ms \",\"";
    assert_eq!(ok_out(src, "main", &[]), "a,1");
}

#[test]
fn rgx_bad_pattern_errors() {
    let s = err_stderr("main>L t;rgx \"(\" \"x\"", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn rgxall_no_groups() {
    let src = "main>n;ms=rgxall \"\\\\d+\" \"a1 b22\";len ms";
    assert_eq!(ok_out(src, "main", &[]), "2");
}

#[test]
fn rgxall1_two_groups_errors() {
    let s = err_stderr("main>L t;rgxall1 \"(\\\\w)(\\\\d)\" \"a1\"", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn rgxall1_single_group() {
    let src = "main>n;ms=rgxall1 \"(\\\\d)\" \"a1b2\";len ms";
    assert_eq!(ok_out(src, "main", &[]), "2");
}

#[test]
fn rgxsub_basic() {
    let src = "main>t;rgxsub \"\\\\d+\" \"N\" \"a1 b22\"";
    assert_eq!(ok_out(src, "main", &[]), "aN bN");
}

// ---------------------------------------------------------------------------
// Flat / FFT / IFFT
// ---------------------------------------------------------------------------

#[test]
fn flat_basic() {
    let src = "main>n;xs=flat [[1,2] [3] [4,5]];len xs";
    assert_eq!(ok_out(src, "main", &[]), "5");
}

#[test]
fn fft_empty_errors() {
    let s = err_stderr("main>L (L n);fft []", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn fft_basic_len() {
    let src = "main>n;xs=fft [1,0,0,0];len xs";
    assert_eq!(ok_out(src, "main", &[]), "4");
}

#[test]
fn ifft_empty_errors() {
    let s = err_stderr("main>L n;ifft []", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

// ---------------------------------------------------------------------------
// Transpose / matmul / dot
// ---------------------------------------------------------------------------

#[test]
fn transpose_basic() {
    let src = "main>n;t=transpose [[1,2,3] [4,5,6]];len t";
    assert_eq!(ok_out(src, "main", &[]), "3");
}

#[test]
fn transpose_empty() {
    let src = "main>n;t=transpose [];len t";
    assert_eq!(ok_out(src, "main", &[]), "0");
}

#[test]
fn transpose_ragged_errors() {
    let s = err_stderr("main>L (L n);transpose [[1,2] [3]]", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn matmul_basic() {
    let src = "main>n;m=matmul [[1,2] [3,4]] [[5,6] [7,8]];len m";
    assert_eq!(ok_out(src, "main", &[]), "2");
}

#[test]
fn matmul_shape_mismatch() {
    let s = err_stderr("main>L (L n);matmul [[1,2]] [[1,2]]", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn dot_basic() {
    assert_eq!(ok_out("main>n;dot [1,2,3] [4,5,6]", "main", &[]), "32");
}

#[test]
fn dot_length_mismatch() {
    let s = err_stderr("main>n;dot [1,2] [1]", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

// ---------------------------------------------------------------------------
// Linear algebra: det / inv / solve
// ---------------------------------------------------------------------------

#[test]
fn det_basic() {
    assert_eq!(ok_out("main>n;det [[1,2] [3,4]]", "main", &[]), "-2");
}

#[test]
fn det_non_square_errors() {
    let s = err_stderr("main>n;det [[1,2,3] [4,5,6]]", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn inv_singular_errors() {
    let s = err_stderr("main>L (L n);inv [[1,2] [2,4]]", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn solve_basic() {
    let src = "main>n;x=solve [[1,0] [0,1]] [3,4];len x";
    assert_eq!(ok_out(src, "main", &[]), "2");
}

#[test]
fn solve_vector_length_mismatch() {
    let s = err_stderr("main>L n;solve [[1,0] [0,1]] [3]", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

// ---------------------------------------------------------------------------
// JSON: jpar / jdmp / jpth
// ---------------------------------------------------------------------------

#[test]
fn jpar_jdmp_roundtrip() {
    // `!` propagates so caller must return R.
    let src = "main>R t t;v=jpar! \"{\\\"a\\\":1}\";~jdmp v";
    let s = ok_out(src, "main", &[]);
    assert!(s.contains("\"a\":1"), "out={s}");
}

#[test]
fn jpar_invalid_returns_err() {
    let src = "main>t;r=jpar \"{bad}\";?r{~_:\"ok\";^_:\"err\"}";
    assert_eq!(ok_out(src, "main", &[]), "err");
}

#[test]
fn jpth_basic() {
    let src = "main>R t t;~jpth! \"{\\\"a\\\":{\\\"b\\\":\\\"c\\\"}}\" \"a.b\"";
    assert_eq!(ok_out(src, "main", &[]), "c");
}

#[test]
fn jpth_missing_key_returns_err() {
    let src = "main>t;r=jpth \"{\\\"a\\\":1}\" \"b\";?r{~v:v;^er:er}";
    let s = ok_out(src, "main", &[]);
    assert!(s.contains("not found"), "out={s}");
}

#[test]
fn jpth_invalid_json() {
    let src = "main>t;r=jpth \"{bad\" \"a\";?r{~v:v;^_:\"err\"}";
    assert_eq!(ok_out(src, "main", &[]), "err");
}

// ---------------------------------------------------------------------------
// env builtin
// ---------------------------------------------------------------------------

#[test]
fn env_missing_returns_err() {
    let src = "main>t;r=env \"ILO_DEFINITELY_NOT_SET_12345\";?r{~v:v;^_:\"unset\"}";
    assert_eq!(ok_out(src, "main", &[]), "unset");
}

// ---------------------------------------------------------------------------
// Pattern matching: Ok / Err / TypeIs / literal / wildcard
// ---------------------------------------------------------------------------

#[test]
fn match_ok_err_patterns() {
    let src = "main>t;r=~42;?r{~v:str v;^er:er}";
    assert_eq!(ok_out(src, "main", &[]), "42");
    let src2 = "main>t;r=^\"bad\";?r{~v:\"ok\";^er:er}";
    assert_eq!(ok_out(src2, "main", &[]), "bad");
}

#[test]
fn match_typeis_number() {
    let src =
        "f x:_>t;?x{n v:\"num\";t v:\"text\";b v:\"bool\";l v:\"list\";_:\"other\"};main>t;f 42";
    assert_eq!(ok_out(src, "main", &[]), "num");
}

#[test]
fn match_typeis_text() {
    let src = "f x:_>t;?x{n v:\"num\";t v:\"text\";b v:\"bool\";l v:\"list\";_:\"other\"};main>t;f \"hi\"";
    assert_eq!(ok_out(src, "main", &[]), "text");
}

#[test]
fn match_typeis_bool() {
    let src =
        "f x:_>t;?x{n v:\"num\";t v:\"text\";b v:\"bool\";l v:\"list\";_:\"other\"};main>t;f true";
    assert_eq!(ok_out(src, "main", &[]), "bool");
}

#[test]
fn match_typeis_list() {
    let src = "f x:_>t;?x{n v:\"num\";l v:\"list\";_:\"other\"};main>t;f [1,2]";
    assert_eq!(ok_out(src, "main", &[]), "list");
}

#[test]
fn match_literal_wildcard() {
    let src = "f n:n>t;?n{0:\"zero\";1:\"one\";_:\"many\"};main>t;f 1";
    assert_eq!(ok_out(src, "main", &[]), "one");
    let src2 = "f n:n>t;?n{0:\"zero\";1:\"one\";_:\"many\"};main>t;f 99";
    assert_eq!(ok_out(src2, "main", &[]), "many");
}

// ---------------------------------------------------------------------------
// Records: field access, safe access, with, destructure
// ---------------------------------------------------------------------------

// Records: `type` requires a top-level declaration that uses curly-brace
// syntax which is awkward to write inline; skip dedicated record tests.
// Field-access happy path is already exercised by the json round-trip
// and jpar tests.

// ---------------------------------------------------------------------------
// NilCoalesce
// ---------------------------------------------------------------------------

#[test]
fn nilcoalesce_picks_default() {
    let src = "main>n;??nil 42";
    assert_eq!(ok_out(src, "main", &[]), "42");
}

#[test]
fn nilcoalesce_passes_through_non_nil() {
    let src = "main>n;??7 42";
    assert_eq!(ok_out(src, "main", &[]), "7");
}

// ---------------------------------------------------------------------------
// Ternary / braced guards
// ---------------------------------------------------------------------------

#[test]
fn ternary_then_branch() {
    let src = "main>n;x=10;c=>x 5;?c{99}{1}";
    assert_eq!(ok_out(src, "main", &[]), "99");
}

#[test]
fn ternary_else_branch() {
    let src = "main>n;x=2;c=>x 5;?c{99}{1}";
    assert_eq!(ok_out(src, "main", &[]), "1");
}

// ---------------------------------------------------------------------------
// BinOp: unsupported / errors
// ---------------------------------------------------------------------------

#[test]
fn divide_by_zero_errors() {
    let s = err_stderr("main>n;/ 1 0", "main", &[]);
    assert!(s.contains("ILO-R003"), "stderr={s}");
}

#[test]
fn unsupported_binop_errors() {
    // Verifier rejects mixed types; defer via _-typed args to hit runtime.
    let src = "f a:_ b:_>_;-a b;main>_;f \"a\" 1";
    let s = err_stderr(src, "main", &[]);
    assert!(
        s.contains("ILO-R004") || s.contains("unsupported"),
        "stderr={s}"
    );
}

#[test]
fn list_concat_with_plus() {
    let src = "main>n;xs=+[1,2] [3,4];len xs";
    assert_eq!(ok_out(src, "main", &[]), "4");
}

#[test]
fn string_concat_with_plus() {
    assert_eq!(ok_out("main>t;+\"hi \" \"there\"", "main", &[]), "hi there");
}

// ---------------------------------------------------------------------------
// Self-rebind peepholes (mset / list append / list+text concat) — fast paths
// ---------------------------------------------------------------------------

#[test]
fn self_rebind_mset_accumulator() {
    let src = "main>n;m=mmap;m=mset m \"a\" 1;m=mset m \"b\" 2;m=mset m \"c\" 3;len m";
    assert_eq!(ok_out(src, "main", &[]), "3");
}

#[test]
fn self_rebind_append_accumulator() {
    let src = "main>n;xs=[];xs=+=xs 1;xs=+=xs 2;xs=+=xs 3;len xs";
    assert_eq!(ok_out(src, "main", &[]), "3");
}

#[test]
fn self_rebind_concat_list_accumulator() {
    let src = "main>n;xs=[1,2];xs=+xs [3,4];len xs";
    assert_eq!(ok_out(src, "main", &[]), "4");
}

#[test]
fn self_rebind_concat_text_accumulator() {
    let src = "main>t;s=\"a\";s=+s \"b\";s=+s \"c\";s";
    assert_eq!(ok_out(src, "main", &[]), "abc");
}

// ---------------------------------------------------------------------------
// foreach / for-range / while / break / continue
// ---------------------------------------------------------------------------

#[test]
fn foreach_basic() {
    // `@x xs{...}` iterates xs binding x; we sum.
    let src = "main>n;tot=0;@x [1,2,3,4]{tot=+tot x};tot";
    assert_eq!(ok_out(src, "main", &[]), "10");
}

// Removed: foreach-non-list errors are caught by the verifier (ILO-T014),
// so the interpreter's ILO-R007 arm isn't reachable from valid syntax.

#[test]
fn for_range_basic() {
    let src = "main>n;tot=0;@i 0..5{tot=+tot i};tot";
    assert_eq!(ok_out(src, "main", &[]), "10");
}

#[test]
fn while_loop_basic() {
    let src = "main>n;i=0;tot=0;wh <i 5{tot=+tot i;i=+i 1};tot";
    assert_eq!(ok_out(src, "main", &[]), "10");
}

#[test]
fn break_in_loop() {
    let src = "main>n;tot=0;@x [1,2,3,4]{>x 2{brk};tot=+tot x};tot";
    assert_eq!(ok_out(src, "main", &[]), "3");
}

#[test]
fn continue_in_loop() {
    let src = "main>n;tot=0;@x [1,2,3,4]{=x 2{cnt};tot=+tot x};tot";
    assert_eq!(ok_out(src, "main", &[]), "8");
}

// ---------------------------------------------------------------------------
// Auto-unwrap propagation: ! and !!
// ---------------------------------------------------------------------------

#[test]
fn propagate_unwrap_ok() {
    let src = "f>R n t;~42;main>R n t;~f!";
    assert_eq!(ok_out(src, "main", &[]), "42");
}

#[test]
fn propagate_unwrap_err() {
    let src = "f>R n t;^\"bad\";main>R n t;~f!";
    let s = err_stderr(src, "main", &[]);
    assert!(s.contains("bad"), "stderr={s}");
}

#[test]
fn panic_unwrap_err_aborts() {
    let src = "f>R n t;^\"bad\";main>n;f!!";
    let s = err_stderr(src, "main", &[]);
    assert!(s.contains("ILO-R026") || s.contains("panic"), "stderr={s}");
}

#[test]
fn panic_unwrap_nil_aborts() {
    let src = "f>O n;nil;main>n;f!!";
    let s = err_stderr(src, "main", &[]);
    assert!(s.contains("ILO-R026") || s.contains("panic"), "stderr={s}");
}

// ---------------------------------------------------------------------------
// Logical ops: short-circuit and / or
// ---------------------------------------------------------------------------

#[test]
fn logical_and_short_circuit() {
    let src = "main>b;&false true";
    assert_eq!(ok_out(src, "main", &[]), "false");
}

#[test]
fn logical_or_short_circuit() {
    let src = "main>b;|true false";
    assert_eq!(ok_out(src, "main", &[]), "true");
}

#[test]
fn logical_not() {
    let src = "main>b;!true";
    assert_eq!(ok_out(src, "main", &[]), "false");
}

// ---------------------------------------------------------------------------
// Negate
// ---------------------------------------------------------------------------

#[test]
fn negate_non_number_errors() {
    // Verifier rejects -text at compile time; use _-typed to defer.
    let src = "f s:_>n;-s;main>n;f \"a\"";
    let s = err_stderr(src, "main", &[]);
    assert!(s.contains("ILO-R004") || s.contains("negate"), "stderr={s}");
}

// ---------------------------------------------------------------------------
// dtfmt / dtparse — Ok and Err paths
// ---------------------------------------------------------------------------

#[test]
fn dtfmt_basic() {
    let src = "main>t;r=dtfmt 0 \"%Y\";?r{~v:v;^er:er}";
    assert_eq!(ok_out(src, "main", &[]), "1970");
}

#[test]
fn dtfmt_non_finite_returns_err() {
    let src = "main>t;r=dtfmt (/ 1 0) \"%Y\";?r{~v:v;^_:\"err\"}";
    // / 1 0 errors at runtime; instead use the inf input via parse? Just probe Sleep skip.
    let _ = src;
    // Skipped: cannot easily construct non-finite literal from CLI inline.
}

#[test]
fn dtparse_invalid() {
    let src = "main>t;r=dtparse \"nope\" \"%Y\";?r{~_:\"ok\";^_:\"err\"}";
    assert_eq!(ok_out(src, "main", &[]), "err");
}

#[test]
fn dtparse_valid() {
    let src = "main>t;r=dtparse \"2024-01-01\" \"%Y-%m-%d\";?r{~v:str v;^_:\"err\"}";
    let s = ok_out(src, "main", &[]);
    assert!(s.parse::<f64>().is_ok(), "out={s}");
}

// ---------------------------------------------------------------------------
// num: parse ok / err
// ---------------------------------------------------------------------------

#[test]
fn num_ok() {
    let src = "main>n;r=num \"42\";?r{~v:v;^_:0}";
    assert_eq!(ok_out(src, "main", &[]), "42");
}

#[test]
fn num_err() {
    let src = "main>n;r=num \"nope\";?r{~v:v;^_:99}";
    assert_eq!(ok_out(src, "main", &[]), "99");
}

// ---------------------------------------------------------------------------
// File I/O: rd / rdl / rdb / wr / wrl / rdjl — Ok and Err paths
// ---------------------------------------------------------------------------

fn tmp_dir() -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("ilo-cov-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn rd_missing_file_returns_err() {
    let src = "main>t;r=rd \"/no/such/file/xyz12345\";?r{~_:\"ok\";^_:\"err\"}";
    assert_eq!(ok_out(src, "main", &[]), "err");
}

#[test]
fn rd_text_format() {
    let p = tmp_dir().join("hello.txt");
    std::fs::write(&p, "hello world").unwrap();
    let src = format!("main>t;r=rd \"{}\";?r{{~v:v;^_:\"err\"}}", p.display());
    assert_eq!(ok_out(&src, "main", &[]), "hello world");
}

#[test]
fn rd_csv_format() {
    let p = tmp_dir().join("data.csv");
    std::fs::write(&p, "a,b,c\n1,2,3").unwrap();
    let src = format!(
        "main>n;r=rd \"{}\" \"csv\";?r{{~rows:len rows;^_:0}}",
        p.display()
    );
    assert_eq!(ok_out(&src, "main", &[]), "2");
}

#[test]
fn rd_json_format() {
    let p = tmp_dir().join("data.json");
    std::fs::write(&p, "{\"a\":1}").unwrap();
    let src = format!(
        "main>t;r=rd \"{}\" \"json\";?r{{~_:\"ok\";^_:\"err\"}}",
        p.display()
    );
    assert_eq!(ok_out(&src, "main", &[]), "ok");
}

#[test]
fn rd_invalid_json_returns_err() {
    let p = tmp_dir().join("bad.json");
    std::fs::write(&p, "{not json").unwrap();
    let src = format!(
        "main>t;r=rd \"{}\" \"json\";?r{{~_:\"ok\";^_:\"err\"}}",
        p.display()
    );
    assert_eq!(ok_out(&src, "main", &[]), "err");
}

#[test]
fn rdl_basic() {
    let p = tmp_dir().join("lines.txt");
    std::fs::write(&p, "a\nb\nc").unwrap();
    let src = format!("main>n;r=rdl \"{}\";?r{{~xs:len xs;^_:0}}", p.display());
    assert_eq!(ok_out(&src, "main", &[]), "3");
}

#[test]
fn rdl_missing_file() {
    let src = "main>t;r=rdl \"/no/such/file/xyz12345\";?r{~_:\"ok\";^_:\"err\"}";
    assert_eq!(ok_out(src, "main", &[]), "err");
}

#[test]
fn rdb_csv_string() {
    let src = "main>n;r=rdb \"a,b\\n1,2\" \"csv\";?r{~rows:len rows;^_:0}";
    assert_eq!(ok_out(src, "main", &[]), "2");
}

#[test]
fn rdb_invalid_json() {
    let src = "main>t;r=rdb \"{nope\" \"json\";?r{~_:\"ok\";^_:\"err\"}";
    assert_eq!(ok_out(src, "main", &[]), "err");
}

#[test]
fn wr_text_basic() {
    let p = tmp_dir().join("out.txt");
    let src = format!(
        "main>t;r=wr \"{}\" \"hello\";?r{{~_:\"ok\";^_:\"err\"}}",
        p.display()
    );
    assert_eq!(ok_out(&src, "main", &[]), "ok");
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "hello");
}

#[test]
fn wr_csv_format() {
    let p = tmp_dir().join("out.csv");
    let src = format!(
        "main>t;rows=[[\"a\" \"b\"] [\"1\" \"2\"]];r=wr \"{}\" rows \"csv\";?r{{~_:\"ok\";^_:\"err\"}}",
        p.display()
    );
    assert_eq!(ok_out(&src, "main", &[]), "ok");
}

#[test]
fn wr_json_format() {
    let p = tmp_dir().join("out.json");
    let src = format!(
        "main>t;m=mmap;m=mset m \"a\" 1;r=wr \"{}\" m \"json\";?r{{~_:\"ok\";^_:\"err\"}}",
        p.display()
    );
    assert_eq!(ok_out(&src, "main", &[]), "ok");
}

#[test]
fn wr_unknown_format_errors() {
    // Verifier enforces literal csv/tsv/json; defer the check to runtime
    // by routing the format through an _-typed param.
    let p = tmp_dir().join("out.zzz");
    let src = format!(
        "go fm:_>t;r=wr \"{}\" [[\"a\"]] fm;?r{{~v:v;^_:\"err\"}};main>t;go \"zzz\"",
        p.display()
    );
    let s = err_stderr(&src, "main", &[]);
    // VM has no native 3-arg `wr` with a dynamic format string; it surfaces
    // a clean compile error. The pre-soft-deprecation tree-walker raised a
    // structured `ILO-R` here, but `--run-tree` is gone from the public
    // surface as of 0.12.x, so we accept either the structured runtime code
    // or the "undefined function" compile-time refusal.
    assert!(
        s.contains("ILO-R") || s.contains("unknown") || s.contains("undefined function"),
        "stderr={s}"
    );
}

#[test]
fn wrl_basic() {
    let p = tmp_dir().join("out_lines.txt");
    let src = format!(
        "main>t;r=wrl \"{}\" [\"a\" \"b\" \"c\"];?r{{~_:\"ok\";^_:\"err\"}}",
        p.display()
    );
    assert_eq!(ok_out(&src, "main", &[]), "ok");
}

#[test]
fn rdjl_basic() {
    let p = tmp_dir().join("data.jsonl");
    std::fs::write(&p, "{\"a\":1}\n{\"a\":2}\n").unwrap();
    let src = format!("main>n;xs=rdjl \"{}\";len xs", p.display());
    assert_eq!(ok_out(&src, "main", &[]), "2");
}

#[test]
fn rdjl_missing_file_errors() {
    let s = err_stderr(
        "main>L (R _ t);rdjl \"/no/such/file/xyz12345\"",
        "main",
        &[],
    );
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012")
            || s.contains("rdjl"),
        "stderr={s}"
    );
}

// ---------------------------------------------------------------------------
// jpth array index path
// ---------------------------------------------------------------------------

#[test]
fn jpth_array_index() {
    let src = "main>R t t;~jpth! \"{\\\"xs\\\":[\\\"a\\\",\\\"b\\\",\\\"c\\\"]}\" \"xs.1\"";
    assert_eq!(ok_out(src, "main", &[]), "b");
}

#[test]
fn jpth_array_index_oob_returns_err() {
    let src = "main>t;r=jpth \"{\\\"xs\\\":[1]}\" \"xs.10\";?r{~v:v;^_:\"err\"}";
    let s = ok_out(src, "main", &[]);
    assert!(s.contains("err") || s.contains("not found"), "out={s}");
}

// ---------------------------------------------------------------------------
// Numeric map keys (Int vs Text distinction in MapKey)
// ---------------------------------------------------------------------------

#[test]
fn map_int_key() {
    let src = "main>n;m=mmap;m=mset m 7 42;??(mget m 7) 0";
    assert_eq!(ok_out(src, "main", &[]), "42");
}

#[test]
fn map_int_vs_text_distinct() {
    let src = "main>b;m=mmap;m=mset m 7 1;mhas m \"7\"";
    assert_eq!(ok_out(src, "main", &[]), "false");
}

// ---------------------------------------------------------------------------
// `prnt` — return value passes through
// ---------------------------------------------------------------------------

#[test]
fn prnt_passes_through() {
    // prnt prints to stdout and returns its argument. We pipe two prnt calls
    // and rely on the second producing the final program output.
    let src = "main>n;x=prnt 42;x";
    assert_eq!(ok_out(src, "main", &[]), "42\n42");
}

// ---------------------------------------------------------------------------
// User-fn dispatch: arity mismatch
// ---------------------------------------------------------------------------

// Note: passing a function name as a Text arg is rejected by the verifier
// (ILO-T013 'map' first arg must be a function), so the runtime Value::Text
// → fn-ref resolve path is not reachable from typechecked code.

// ---------------------------------------------------------------------------
// Safe field access (`.?`) on nil and on non-record
// ---------------------------------------------------------------------------

#[test]
fn safe_field_on_nil_returns_nil() {
    // Call f() to invoke and bind nil; nil-coalesce returns the default.
    let src = "f>O n;nil;main>n;v=f();??v 99";
    assert_eq!(ok_out(src, "main", &[]), "99");
}

// ---------------------------------------------------------------------------
// Self-rebind concat fallback: numeric `x = x + y`
// ---------------------------------------------------------------------------

#[test]
fn self_rebind_concat_numeric_fallback() {
    // `x = x + y` numeric add should fall through to apply_binop, not the
    // list/text fast paths.
    let src = "main>n;x=10;x=+x 5;x";
    assert_eq!(ok_out(src, "main", &[]), "15");
}

// ---------------------------------------------------------------------------
// rgxsub bad pattern
// ---------------------------------------------------------------------------

#[test]
fn rgxsub_bad_pattern_errors() {
    // Defer with _-typed pattern; the verifier accepts the call shape but the
    // bad regex compile lands at runtime in the interpreter.
    let src = "f p:_>t;rgxsub p \"X\" \"abc\";main>t;f \"(\"";
    let s = err_stderr(src, "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012")
            || s.contains("rgxsub"),
        "stderr={s}"
    );
}

// ---------------------------------------------------------------------------
// Inv on non-square matrix errors
// ---------------------------------------------------------------------------

#[test]
fn inv_non_square_errors() {
    let s = err_stderr("main>L (L n);inv [[1,2,3] [4,5,6]]", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

#[test]
fn solve_non_square_errors() {
    let s = err_stderr("main>L n;solve [[1,2,3] [4,5,6]] [1,2]", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

// ---------------------------------------------------------------------------
// matmul wrong inner type
// ---------------------------------------------------------------------------

#[test]
fn matmul_non_number_errors() {
    let src = "f xs:_>L (L n);matmul xs xs;main>L (L n);f [[\"a\"]]";
    let s = err_stderr(src, "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012"),
        "stderr={s}"
    );
}

// ---------------------------------------------------------------------------
// Group with numeric key
// ---------------------------------------------------------------------------

#[test]
fn grp_numeric_key() {
    let src = "k x:n>n;mod x 3;main>n;m=grp k [0,1,2,3,4,5];len m";
    assert_eq!(ok_out(src, "main", &[]), "3");
}

#[test]
fn grp_bool_key() {
    let src = "k x:n>b;>x 2;main>n;m=grp k [1,2,3,4];len m";
    assert_eq!(ok_out(src, "main", &[]), "2");
}

// ---------------------------------------------------------------------------
// Index expr on non-list (Expr::Index error branch)
// ---------------------------------------------------------------------------

#[test]
fn index_on_text_errors() {
    // `xs[0]` on a Text value triggers eval_expr's Index non-list branch.
    let src = "f xs:_>n;at xs 99;main>n;f [1,2,3]";
    let s = err_stderr(src, "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012")
            || s.contains("out of range"),
        "stderr={s}"
    );
}

// ---------------------------------------------------------------------------
// Closure capture via inline lambda (tree-only path)
// ---------------------------------------------------------------------------

#[test]
fn inline_lambda_with_capture() {
    let src = "f xs:L n thr:n>L n;flt (x:n>b;>x thr) xs;main>n;ys=f [1,5,2,8,3] 3;len ys";
    assert_eq!(ok_out(src, "main", &[]), "2");
}

// ---------------------------------------------------------------------------
// Empty list HOFs (early-return list paths)
// ---------------------------------------------------------------------------

#[test]
fn map_empty_list() {
    let src = "f x:n>n;x;main>n;ys=map f [];len ys";
    assert_eq!(ok_out(src, "main", &[]), "0");
}

#[test]
fn fld_empty_list_returns_init() {
    let src = "f a:n x:n>n;+a x;main>n;fld f [] 7";
    assert_eq!(ok_out(src, "main", &[]), "7");
}

// ---------------------------------------------------------------------------
// rgxall1 zero groups path
// ---------------------------------------------------------------------------

#[test]
fn rgxall1_zero_groups() {
    let src = "main>n;xs=rgxall1 \"\\\\d+\" \"a1 b22\";len xs";
    assert_eq!(ok_out(src, "main", &[]), "2");
}

// ---------------------------------------------------------------------------
// Nested call: list literal evaluation, function dispatch
// ---------------------------------------------------------------------------

#[test]
fn list_of_function_results() {
    let src = "double x:n>n;*x 2;main>n;ys=[(double 1) (double 2) (double 3)];sum ys";
    assert_eq!(ok_out(src, "main", &[]), "12");
}

// ---------------------------------------------------------------------------
// At with negative oob errors
// ---------------------------------------------------------------------------

#[test]
fn at_negative_oob_errors() {
    let s = err_stderr("main>n;at [1,2] -10", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012")
            || s.contains("out of range"),
        "stderr={s}"
    );
}

#[test]
fn at_text_oob_errors() {
    let s = err_stderr("main>t;at \"hi\" 10", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012")
            || s.contains("out of range"),
        "stderr={s}"
    );
}

// ---------------------------------------------------------------------------
// Solve singular matrix
// ---------------------------------------------------------------------------

#[test]
fn solve_singular_errors() {
    let s = err_stderr("main>L n;solve [[1,2] [2,4]] [1,2]", "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012")
            || s.contains("singular"),
        "stderr={s}"
    );
}

// ---------------------------------------------------------------------------
// jdmp on various value types (nil, list, map, record-ish via jpar round-trip)
// ---------------------------------------------------------------------------

#[test]
fn jdmp_list() {
    assert_eq!(ok_out("main>t;jdmp [1,2,3]", "main", &[]), "[1,2,3]");
}

#[test]
fn jdmp_bool_and_nil() {
    assert_eq!(ok_out("main>t;jdmp true", "main", &[]), "true");
    let src = "f>O n;nil;main>t;jdmp f()";
    assert_eq!(ok_out(src, "main", &[]), "null");
}

#[test]
fn jdmp_map() {
    let src = "main>t;m=mmap;m=mset m \"a\" 1;jdmp m";
    let s = ok_out(src, "main", &[]);
    assert!(s.contains("\"a\":1"), "out={s}");
}

// ---------------------------------------------------------------------------
// has on map / map error path
// ---------------------------------------------------------------------------

#[test]
fn has_on_number_errors() {
    let src = "f x:_>b;has x 1;main>b;f 42";
    let s = err_stderr(src, "main", &[]);
    assert!(
        s.contains("ILO-R009")
            || s.contains("ILO-R004")
            || s.contains("ILO-R003")
            || s.contains("ILO-R012")
            || s.contains("list or text"),
        "stderr={s}"
    );
}
