// Coverage tests for `src/vm/jit_cranelift.rs`.
//
// Goal: lift region coverage on jit_cranelift.rs by exercising opcode arms
// that are not reached by the existing JIT integration tests. Each test
// runs an ilo program with `--run-cranelift` so the codegen path under
// test is compiled and executed.
//
// Grouping mirrors the big `match op { … }` in `compile_function_body`:
// stats helpers, matrix ops, set ops, JSON ops, padding/char ops, file IO
// (the helpers can be exercised cheaply because all return Result and the
// runtime accepts a non-existent path), datetime, map ops, miscellaneous
// fallback paths (JMPNN, ENV miss, etc.). The grouping is for navigability;
// the assertions are independent.

#![cfg(feature = "cranelift")]

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

/// Run `ilo --run-cranelift <src> <entry> [args…]` and assert success
/// with `stdout.trim() == expected`. Use an empty `args` slice when the
/// entry takes no CLI args.
fn run_ok(src: &str, entry: &str, args: &[&str], expected: &str) {
    let mut cmd = ilo();
    cmd.args(["--run-cranelift", src, entry]);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "expected success for `{src}` (entry `{entry}`, args {args:?}).\nstdout={stdout}\nstderr={stderr}"
    );
    assert_eq!(
        stdout.trim(),
        expected,
        "stdout mismatch for `{src}` (entry `{entry}`, args {args:?}).\nstderr={stderr}"
    );
}

fn run_err_contains(src: &str, entry: &str, needle: &str) {
    let out = ilo()
        .args(["--run-cranelift", src, entry])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "expected runtime error for `{src}` (entry `{entry}`).\nstdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        stderr.contains(needle),
        "expected stderr to contain {needle:?}, got: {stderr}"
    );
}

// ── Statistical aggregates feeding into arithmetic (F64-shadow refresh) ────

#[test]
fn jit_median_into_arith() {
    run_ok("f xs:L n>n;+(median xs) 1", "f", &["[1,2,3,4,5]"], "4");
}

#[test]
fn jit_sum_into_arith() {
    run_ok("f xs:L n>n;*(sum xs) 2", "f", &["[1,2,3]"], "12");
}

#[test]
fn jit_avg_into_arith() {
    run_ok("f xs:L n>n;+(avg xs) 0", "f", &["[2,4,6]"], "4");
}

#[test]
fn jit_min_lst_into_arith() {
    run_ok("f xs:L n>n;+(min xs) 0", "f", &["[3,1,4,1,5]"], "1");
}

#[test]
fn jit_max_lst_into_arith() {
    run_ok("f xs:L n>n;+(max xs) 0", "f", &["[3,1,4,1,5]"], "5");
}

#[test]
fn jit_stdev_basic() {
    run_ok(
        "f xs:L n>n;rou (stdev xs)",
        "f",
        &["[2,4,4,4,5,5,7,9]"],
        "2",
    );
}

#[test]
fn jit_variance_basic() {
    run_ok(
        "f xs:L n>n;rou (variance xs)",
        "f",
        &["[2,4,4,4,5,5,7,9]"],
        "5",
    );
}

#[test]
fn jit_quantile_basic() {
    run_ok(
        "f xs:L n p:n>n;quantile xs p",
        "f",
        &["[1,2,3,4,5]", "0.5"],
        "3",
    );
}

#[test]
fn jit_cumsum_basic() {
    run_ok("f xs:L n>L n;cumsum xs", "f", &["[1,2,3]"], "[1, 3, 6]");
}

// ── FFT/IFFT ─────────────────────────────────────────────────────────────

#[test]
fn jit_fft_then_len() {
    run_ok("f xs:L n>n;len (fft xs)", "f", &["[1,2,3,4]"], "4");
}

#[test]
fn jit_ifft_roundtrip_len() {
    run_ok("f xs:L n>n;len (ifft (fft xs))", "f", &["[1,2,3,4]"], "4");
}

// ── Matrix / linear algebra ──────────────────────────────────────────────

#[test]
fn jit_transpose_basic() {
    run_ok(
        "f>L (L n);transpose [[1,2],[3,4]]",
        "f",
        &[],
        "[[1, 3], [2, 4]]",
    );
}

#[test]
fn jit_matmul_basic() {
    run_ok(
        "f>L (L n);matmul [[1,2],[3,4]] [[5,6],[7,8]]",
        "f",
        &[],
        "[[19, 22], [43, 50]]",
    );
}

#[test]
fn jit_dot_basic() {
    run_ok("f>n;dot [1,2,3] [4,5,6]", "f", &[], "32");
}

#[test]
fn jit_det_basic() {
    run_ok("f>n;det [[4,3],[6,3]]", "f", &[], "-6");
}

#[test]
fn jit_inv_then_len() {
    run_ok("f>n;len (inv [[1,0],[0,1]])", "f", &[], "2");
}

#[test]
fn jit_solve_basic() {
    run_ok("f>L n;solve [[1,0],[0,1]] [3,4]", "f", &[], "[3, 4]");
}

#[test]
fn jit_solve_singular_runtime_error() {
    run_err_contains("f>L n;solve [[1,2],[2,4]] [1,2]", "f", "labels");
}

// ── Set ops ──────────────────────────────────────────────────────────────

#[test]
fn jit_setunion_basic() {
    run_ok("f>L n;setunion [1,2,3] [2,3,4]", "f", &[], "[1, 2, 3, 4]");
}

#[test]
fn jit_setinter_basic() {
    run_ok("f>L n;setinter [1,2,3] [2,3,4]", "f", &[], "[2, 3]");
}

#[test]
fn jit_setdiff_basic() {
    run_ok("f>L n;setdiff [1,2,3] [2,3,4]", "f", &[], "[1]");
}

// ── List shaping ─────────────────────────────────────────────────────────

#[test]
fn jit_zip_basic() {
    run_ok(
        "f>L (L n);zip [1,2,3] [10,20,30]",
        "f",
        &[],
        "[[1, 10], [2, 20], [3, 30]]",
    );
}

#[test]
fn jit_enumerate_basic() {
    run_ok(
        "f>L (L n);enumerate [10,20,30]",
        "f",
        &[],
        "[[0, 10], [1, 20], [2, 30]]",
    );
}

#[test]
fn jit_range_basic() {
    run_ok("f>L n;range 0 4", "f", &[], "[0, 1, 2, 3]");
}

#[test]
fn jit_chunks_basic() {
    run_ok(
        "f>L (L n);chunks 2 [1,2,3,4,5]",
        "f",
        &[],
        "[[1, 2], [3, 4], [5]]",
    );
}

#[test]
fn jit_flat_basic() {
    run_ok("f>L n;flat [[1,2],[3,4]]", "f", &[], "[1, 2, 3, 4]");
}

#[test]
fn jit_take_basic() {
    run_ok("f>L n;take 2 [1,2,3,4]", "f", &[], "[1, 2]");
}

#[test]
fn jit_drop_basic() {
    run_ok("f>L n;drop 2 [1,2,3,4]", "f", &[], "[3, 4]");
}

#[test]
fn jit_unq_basic() {
    run_ok("f>L n;unq [1,2,2,3,3,3]", "f", &[], "[1, 2, 3]");
}

#[test]
fn jit_frq_then_mhas() {
    run_ok(
        "f>b;m=frq [\"a\",\"b\",\"a\"];mhas m \"a\"",
        "f",
        &[],
        "true",
    );
}

// ── Padding / char ops ───────────────────────────────────────────────────

#[test]
fn jit_padl_basic() {
    run_ok("f>n;len (padl \"x\" 4)", "f", &[], "4");
}

#[test]
fn jit_padr_basic() {
    run_ok("f>n;len (padr \"x\" 4)", "f", &[], "4");
}

#[test]
fn jit_padlc_basic() {
    run_ok("f>t;padl \"x\" 4 \"0\"", "f", &[], "000x");
}

#[test]
fn jit_padrc_basic() {
    run_ok("f>t;padr \"x\" 4 \"0\"", "f", &[], "x000");
}

#[test]
fn jit_chr_basic() {
    run_ok("f>t;chr 65", "f", &[], "A");
}

#[test]
fn jit_ord_basic() {
    run_ok("f>n;ord \"A\"", "f", &[], "65");
}

#[test]
fn jit_chars_basic() {
    run_ok("f>L t;chars \"abc\"", "f", &[], "[a, b, c]");
}

#[test]
fn jit_trm_basic() {
    run_ok("f>t;trm \"  hi  \"", "f", &[], "hi");
}

#[test]
fn jit_upr_basic() {
    run_ok("f>t;upr \"abc\"", "f", &[], "ABC");
}

#[test]
fn jit_lwr_basic() {
    run_ok("f>t;lwr \"ABC\"", "f", &[], "abc");
}

#[test]
fn jit_cap_basic() {
    run_ok("f>t;cap \"hello\"", "f", &[], "Hello");
}

// ── Type-pattern matches (emit OP_ISNUM / OP_ISTEXT / OP_ISBOOL / OP_ISLIST) ─

#[test]
fn jit_match_type_number() {
    run_ok(
        "f x:_>t;?x{n v:\"num\";t s:\"txt\";_:\"other\"}",
        "f",
        &["42"],
        "num",
    );
}

#[test]
fn jit_match_type_text() {
    run_ok(
        "f x:_>t;?x{n v:\"num\";t s:\"txt\";_:\"other\"}",
        "f",
        &["hi"],
        "txt",
    );
}

#[test]
fn jit_match_type_bool() {
    run_ok("f x:_>t;?x{b y:\"b\";_:\"other\"}", "f", &["true"], "b");
}

// ── Map ops ──────────────────────────────────────────────────────────────

#[test]
fn jit_mapnew_then_set_get() {
    run_ok("f>n;m=mset mmap \"k\" 42;mget m \"k\"", "f", &[], "42");
}

#[test]
fn jit_mhas_true() {
    run_ok("f>b;m=mset mmap \"k\" 1;mhas m \"k\"", "f", &[], "true");
}

#[test]
fn jit_mkeys_basic() {
    run_ok(
        "f>L t;m=mset (mset mmap \"a\" 1) \"b\" 2;mkeys m",
        "f",
        &[],
        "[a, b]",
    );
}

#[test]
fn jit_mvals_basic() {
    run_ok(
        "f>L n;m=mset (mset mmap \"a\" 1) \"b\" 2;mvals m",
        "f",
        &[],
        "[1, 2]",
    );
}

#[test]
fn jit_mdel_basic() {
    run_ok(
        "f>L t;m=mset (mset mmap \"a\" 1) \"b\" 2;mkeys (mdel m \"a\")",
        "f",
        &[],
        "[b]",
    );
}

#[test]
fn jit_mset_inplace_shape() {
    run_ok("f>n;m=mmap;m=mset m \"k\" 7;mget m \"k\"", "f", &[], "7");
}

// ── JSON ops ─────────────────────────────────────────────────────────────

#[test]
fn jit_jdmp_number() {
    run_ok("f x:n>t;jdmp x", "f", &["42"], "42");
}

#[test]
fn jit_jpar_ok() {
    run_ok("f>R n t;jpar \"42\"", "f", &[], "42");
}

#[test]
fn jit_jpth_basic() {
    run_ok("f>R t t;jpth \"{\\\"a\\\":7}\" \"a\"", "f", &[], "7");
}

#[test]
fn jit_jdmp_list() {
    run_ok("f>t;jdmp [1,2,3]", "f", &[], "[1,2,3]");
}

// ── Datetime ─────────────────────────────────────────────────────────────

#[test]
fn jit_dtfmt_basic() {
    run_ok("f>R t t;dtfmt 0 \"%Y\"", "f", &[], "1970");
}

#[test]
fn jit_dtparse_basic() {
    run_ok("f>R n t;dtparse \"1970-01-01\" \"%Y-%m-%d\"", "f", &[], "0");
}

// ── File IO error paths ─────────────────────────────────────────────────

#[test]
fn jit_rd_missing_path() {
    run_err_contains(
        "f>t;rd! \"/nonexistent-ilo-cov-jit-rd-path.txt\"",
        "f",
        "labels",
    );
}

#[test]
fn jit_rdl_missing_path() {
    run_err_contains(
        "f>L t;rdl! \"/nonexistent-ilo-cov-jit-rdl-path.txt\"",
        "f",
        "labels",
    );
}

// ── ENV builtin ─────────────────────────────────────────────────────────

#[test]
fn jit_env_missing_runtime_error() {
    run_err_contains(
        "f>R t t;env \"ILO_DEFINITELY_NOT_SET_VAR_XYZ\"",
        "f",
        "not set",
    );
}

// ── Math (rare helper arms: log10/log2/asin/acos/atan/atan2/sin/cos/tan) ─

#[test]
fn jit_log10_basic() {
    run_ok("f>n;log10 100", "f", &[], "2");
}

#[test]
fn jit_log2_basic() {
    run_ok("f>n;log2 8", "f", &[], "3");
}

#[test]
fn jit_sin_zero() {
    run_ok("f>n;sin 0", "f", &[], "0");
}

#[test]
fn jit_cos_zero() {
    run_ok("f>n;cos 0", "f", &[], "1");
}

#[test]
fn jit_tan_zero() {
    run_ok("f>n;tan 0", "f", &[], "0");
}

#[test]
fn jit_asin_zero() {
    run_ok("f>n;asin 0", "f", &[], "0");
}

#[test]
fn jit_acos_one() {
    run_ok("f>n;acos 1", "f", &[], "0");
}

#[test]
fn jit_atan_zero() {
    run_ok("f>n;atan 0", "f", &[], "0");
}

#[test]
fn jit_atan2_basic() {
    run_ok("f>n;atan2 0 1", "f", &[], "0");
}

#[test]
fn jit_pow_basic() {
    run_ok("f>n;pow 2 8", "f", &[], "256");
}

#[test]
fn jit_sqrt_basic() {
    run_ok("f>n;sqrt 16", "f", &[], "4");
}

#[test]
fn jit_log_basic() {
    run_ok("f>n;log 1", "f", &[], "0");
}

#[test]
fn jit_exp_zero() {
    run_ok("f>n;exp 0", "f", &[], "1");
}

// ── Rounding / clamp / mod / abs ─────────────────────────────────────────

#[test]
fn jit_flr_neg() {
    run_ok("f>n;flr -1.5", "f", &[], "-2");
}

#[test]
fn jit_cel_neg() {
    run_ok("f>n;cel -1.5", "f", &[], "-1");
}

#[test]
fn jit_rou_half() {
    run_ok("f>n;rou 0.5", "f", &[], "1");
}

#[test]
fn jit_mod_basic() {
    run_ok("f>n;mod 10 3", "f", &[], "1");
}

#[test]
fn jit_clamp_basic() {
    run_ok("f>n;clamp 50 0 10", "f", &[], "10");
}

#[test]
fn jit_abs_neg() {
    run_ok("f>n;abs -7", "f", &[], "7");
}

// ── Sort variants ────────────────────────────────────────────────────────

#[test]
fn jit_srt_basic() {
    run_ok("f>L n;srt [3,1,2]", "f", &[], "[1, 2, 3]");
}

#[test]
fn jit_rev_basic() {
    run_ok("f>L n;rev [1,2,3]", "f", &[], "[3, 2, 1]");
}

#[test]
fn jit_tl_basic() {
    run_ok("f>L n;tl [1,2,3]", "f", &[], "[2, 3]");
}

// ── Slice / lst happy paths ──────────────────────────────────────────────

#[test]
fn jit_slc_basic() {
    run_ok("f>L n;slc [1,2,3,4,5] 1 4", "f", &[], "[2, 3, 4]");
}

#[test]
fn jit_lst_basic() {
    run_ok("f>L n;lst [1,2,3] 0 99", "f", &[], "[99, 2, 3]");
}

// ── INDEX OOB through fn param shape ─────────────────────────────────────

#[test]
fn jit_index_oob_runtime_error_labels() {
    run_err_contains("f>n;xs=[1,2];xs.99", "f", "labels");
}

// ── String split / concat-many ───────────────────────────────────────────

#[test]
fn jit_spl_basic() {
    run_ok("f>L t;spl \"a,b,c\" \",\"", "f", &[], "[a, b, c]");
}

#[test]
fn jit_cat_basic() {
    run_ok("f>t;cat [\"a\",\"b\",\"c\"] \",\"", "f", &[], "a,b,c");
}

// ── HAS ─────────────────────────────────────────────────────────────────

#[test]
fn jit_has_list_true() {
    run_ok("f>b;has [1,2,3] 2", "f", &[], "true");
}

#[test]
fn jit_has_text_true() {
    run_ok("f>b;has \"hello\" \"ell\"", "f", &[], "true");
}

// ── num / str builtins ───────────────────────────────────────────────────

#[test]
fn jit_num_from_text() {
    run_ok("f>R n t;num \"42\"", "f", &[], "42");
}

#[test]
fn jit_str_from_num() {
    run_ok("f>t;str 42", "f", &[], "42");
}

// ── len on string and list ───────────────────────────────────────────────

#[test]
fn jit_len_text() {
    run_ok("f>n;len \"hello\"", "f", &[], "5");
}

#[test]
fn jit_len_list() {
    run_ok("f>n;len [1,2,3,4]", "f", &[], "4");
}

// ── prt builtin ──────────────────────────────────────────────────────────

#[test]
fn jit_prnt_runs() {
    let out = ilo()
        .args(["--run-cranelift", "f>n;prnt 7", "f"])
        .output()
        .expect("failed to run ilo");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "prnt should succeed, stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("7"),
        "expected stdout to contain 7: {stdout}"
    );
}

// ── Foreach over numeric list with arithmetic body ───────────────────────

#[test]
fn jit_foreach_sum_via_listget_foreachnext() {
    run_ok(
        "f xs:L n>n;s=0;@x xs{s=+s x};s",
        "f",
        &["[1,2,3,4,5]"],
        "15",
    );
}

// ── JMPNN (??-coalesce on optional) ──────────────────────────────────────

#[test]
fn jit_nilcoalesce_default_used() {
    run_ok("f>n;??nil 99", "f", &[], "99");
}

#[test]
fn jit_nilcoalesce_value_used() {
    run_ok("f>n;??7 99", "f", &[], "7");
}

// ── Boolean dispatch ────────────────────────────────────────────────────

#[test]
fn jit_not_true() {
    run_ok("f>b;!true", "f", &[], "false");
}

#[test]
fn jit_not_false() {
    run_ok("f>b;!false", "f", &[], "true");
}

#[test]
fn jit_ternary_truthy() {
    // `cond{then}{else}` brace form — exercises OP_JMPF.
    run_ok("f x:n>n;>x 0{1}{2}", "f", &["5"], "1");
}

#[test]
fn jit_ternary_falsy() {
    run_ok("f x:n>n;>x 0{1}{2}", "f", &["-5"], "2");
}
