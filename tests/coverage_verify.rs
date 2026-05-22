// Coverage-driven tests for src/verify.rs.
//
// Goal: exercise verifier arms that the existing test suite leaves cold so
// region coverage of verify.rs lifts above 94%. Each block targets a specific
// builtin/feature dispatcher arm or diagnostic branch in the verifier.
//
// All snippets are run through the CLI inline path so verify gating fires
// (inline + no entry skips the gate). The shape mirrors tests/errors.rs.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(code: &str, entry: &str) -> (bool, String) {
    let out = ilo()
        .arg(code)
        .arg(entry)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn ilo: {e}"));
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    (out.status.success(), stderr)
}

fn assert_err(code: &str, expected_code: &str, entry: &str) {
    let (ok, stderr) = run(code, entry);
    assert!(
        !ok,
        "expected failure for {code:?}\nexpected={expected_code}\nstderr={stderr}"
    );
    assert!(
        stderr.contains(expected_code),
        "expected {expected_code} in stderr for {code:?}\nstderr={stderr}"
    );
}

fn assert_ok(code: &str, entry: &str) {
    let (ok, stderr) = run(code, entry);
    assert!(ok, "expected success for {code:?}\nstderr={stderr}");
}

// ---- builtin type-arg errors (all surface as ILO-T013) ---------------------

#[test]
fn len_wrong_type() {
    assert_err("main>n;len 5", "ILO-T013", "main");
}

#[test]
fn str_text_passthrough() {
    // str is now polymorphic: text input is identity, no error
    assert_ok("main>t;str \"hi\"", "main");
}

#[test]
fn str_wrong_type() {
    // bool is not accepted by str
    assert_err("main x:b>t;str x", "ILO-T013", "main");
}

#[test]
fn num_wrong_type() {
    // num is polymorphic across text and number, so `num 5` is valid post-fix.
    // Bool is the smallest case the verifier still rejects.
    assert_err("main x:b>R n t;num x", "ILO-T013", "main");
}

#[test]
fn abs_wrong_type() {
    assert_err("main>n;abs \"x\"", "ILO-T013", "main");
}

#[test]
fn sqrt_wrong_type() {
    assert_err("main>n;sqrt \"x\"", "ILO-T013", "main");
}

#[test]
fn log_wrong_type() {
    assert_err("main>n;log \"x\"", "ILO-T013", "main");
}

#[test]
fn sin_wrong_type() {
    assert_err("main>n;sin \"x\"", "ILO-T013", "main");
}

#[test]
fn min_list_form_wrong_inner() {
    assert_err("main>n;min [\"a\" \"b\"]", "ILO-T013", "main");
}

#[test]
fn min_list_form_wrong_type() {
    assert_err("main>n;min 5", "ILO-T013", "main");
}

#[test]
fn min_two_arg_wrong_type() {
    assert_err("main>n;min \"a\" 1", "ILO-T013", "main");
}

#[test]
fn pow_wrong_type() {
    assert_err("main>n;pow \"a\" 2", "ILO-T013", "main");
}

#[test]
fn clamp_wrong_type() {
    assert_err("main>n;clamp \"a\" 0 10", "ILO-T013", "main");
}

#[test]
fn range_wrong_type() {
    assert_err("main>L n;range \"a\" 5", "ILO-T013", "main");
}

#[test]
fn rnd_wrong_type() {
    assert_err("main>n;rnd \"a\" 5", "ILO-T013", "main");
}

#[test]
fn rndn_wrong_type() {
    assert_err("main>n;rndn \"a\" 5", "ILO-T013", "main");
}

#[test]
fn spl_wrong_type() {
    assert_err("main>L t;spl 5 \",\"", "ILO-T013", "main");
}

#[test]
fn cat_arg1_wrong() {
    assert_err("main>t;cat 5 \",\"", "ILO-T013", "main");
}

#[test]
fn cat_arg2_wrong() {
    assert_err("main>t;cat [\"a\" \"b\"] 5", "ILO-T013", "main");
}

#[test]
fn has_wrong_type() {
    assert_err("main>b;has 5 1", "ILO-T013", "main");
}

#[test]
fn hd_wrong_type() {
    assert_err("main>n;hd 5", "ILO-T013", "main");
}

#[test]
fn at_index_wrong() {
    assert_err("main>n;at [1 2 3] \"x\"", "ILO-T013", "main");
}

#[test]
fn at_collection_wrong() {
    assert_err("main>n;at 5 0", "ILO-T013", "main");
}

#[test]
fn lst_index_wrong() {
    assert_err("main>L n;lst [1 2 3] \"x\" 9", "ILO-T013", "main");
}

#[test]
fn lst_value_mismatch() {
    assert_err("main>L n;lst [1 2 3] 0 \"x\"", "ILO-T013", "main");
}

#[test]
fn lst_not_a_list() {
    assert_err("main>L n;lst 5 0 9", "ILO-T013", "main");
}

#[test]
fn zip_arg1_wrong() {
    assert_err("main>L (L _);zip 5 [1 2]", "ILO-T013", "main");
}

#[test]
fn zip_arg2_wrong() {
    assert_err("main>L (L _);zip [1 2] 5", "ILO-T013", "main");
}

#[test]
fn enumerate_wrong() {
    assert_err("main>L (L _);enumerate 5", "ILO-T013", "main");
}

#[test]
fn window_arg1_wrong() {
    assert_err("main>L (L n);window \"x\" [1 2 3]", "ILO-T013", "main");
}

#[test]
fn window_arg2_wrong() {
    assert_err("main>L (L n);window 2 5", "ILO-T013", "main");
}

#[test]
fn setunion_arg1_wrong() {
    assert_err("main>L _;setunion 5 [1 2]", "ILO-T013", "main");
}

#[test]
fn setunion_arg2_wrong() {
    assert_err("main>L _;setunion [1 2] 5", "ILO-T013", "main");
}

#[test]
fn setinter_arg1_wrong() {
    assert_err("main>L _;setinter 5 [1]", "ILO-T013", "main");
}

#[test]
fn setdiff_arg1_wrong() {
    assert_err("main>L _;setdiff 5 [1]", "ILO-T013", "main");
}

#[test]
fn tl_wrong_type() {
    assert_err("main>L n;tl 5", "ILO-T013", "main");
}

#[test]
fn rev_wrong_type() {
    assert_err("main>L n;rev 5", "ILO-T013", "main");
}

#[test]
fn trm_wrong() {
    assert_err("main>t;trm 5", "ILO-T013", "main");
}

#[test]
fn upr_wrong() {
    assert_err("main>t;upr 5", "ILO-T013", "main");
}

#[test]
fn padl_arg1_wrong() {
    assert_err("main>t;padl 5 6", "ILO-T013", "main");
}

#[test]
fn padl_arg2_wrong() {
    assert_err("main>t;padl \"x\" \"y\"", "ILO-T013", "main");
}

#[test]
fn padl_arg3_wrong() {
    assert_err("main>t;padl \"x\" 6 5", "ILO-T013", "main");
}

#[test]
fn ord_wrong() {
    assert_err("main>n;ord 5", "ILO-T013", "main");
}

#[test]
fn chr_wrong() {
    assert_err("main>t;chr \"a\"", "ILO-T013", "main");
}

#[test]
fn chars_wrong() {
    assert_err("main>L t;chars 5", "ILO-T013", "main");
}

#[test]
fn unq_wrong() {
    assert_err("main>L n;unq 5", "ILO-T013", "main");
}

#[test]
fn fmt_template_wrong() {
    assert_err("main>t;fmt 5 \"x\"", "ILO-T013", "main");
}

#[test]
fn fmt2_wrong_type() {
    assert_err("main>t;fmt2 \"a\" 2", "ILO-T013", "main");
}

#[test]
fn fmt_bad_format_spec() {
    assert_err("main>t;fmt \"x={:.2}\" 1", "ILO-T013", "main");
}

#[test]
fn wr_bad_format_literal() {
    assert_err("main>R t t;wr \"/tmp/x\" [1 2] \"xml\"", "ILO-T013", "main");
}

// ---- HOF errors ------------------------------------------------------------

#[test]
fn map_not_a_fn() {
    assert_err("main>L n;map 5 [1 2 3]", "ILO-T013", "main");
}

#[test]
fn flt_not_a_fn() {
    assert_err("main>L n;flt 5 [1 2 3]", "ILO-T013", "main");
}

#[test]
fn ct_not_a_fn() {
    assert_err("main>n;ct 5 [1 2 3]", "ILO-T013", "main");
}

#[test]
fn fld_not_a_fn() {
    assert_err("main>n;fld 5 [1 2 3] 0", "ILO-T013", "main");
}

#[test]
fn grp_not_a_fn() {
    assert_err("main>M t (L n);grp 5 [1 2 3]", "ILO-T013", "main");
}

#[test]
fn uniqby_not_a_fn() {
    assert_err("main>L n;uniqby 5 [1 2 3]", "ILO-T013", "main");
}

#[test]
fn flatmap_not_a_fn() {
    assert_err("main>L n;flatmap 5 [1 2 3]", "ILO-T013", "main");
}

#[test]
fn partition_not_a_fn() {
    assert_err("main>L (L n);partition 5 [1 2 3]", "ILO-T013", "main");
}

#[test]
fn srt_not_a_fn() {
    assert_err("main>L n;srt 5 [1 2 3]", "ILO-T013", "main");
}

#[test]
fn rsrt_not_a_fn() {
    assert_err("main>L n;rsrt 5 [1 2 3]", "ILO-T013", "main");
}

#[test]
fn srt_wrong_type() {
    assert_err("main>L n;srt 5", "ILO-T013", "main");
}

#[test]
fn rsrt_wrong_type() {
    assert_err("main>L n;rsrt 5", "ILO-T013", "main");
}

#[test]
fn mapr_not_a_fn() {
    assert_err("main>R (L n) t;mapr 5 [1 2 3]", "ILO-T013", "main");
}

#[test]
fn mapr_fn_not_result() {
    // mapr requires fn to return a Result; sq returns n
    let src = "sq x:n>n;*x x;main>R (L n) _;mapr sq [1 2 3]";
    assert_err(src, "ILO-T013", "main");
}

// arg-count mismatch (wrong arity) for HOF closure-bind fn
#[test]
fn map_closure_bind_wrong_arg_count() {
    // fn takes 3 params, map closure-bind expects 2
    let src = "f a:n b:n c:n>n;+a (+b c);main>L n;map f 1 [1 2 3]";
    assert_err(src, "ILO-T013", "main");
}

#[test]
fn flt_closure_bind_wrong_arg_count() {
    let src = "f a:n b:n c:n>b;>a b;main>L n;flt f 1 [1 2 3]";
    assert_err(src, "ILO-T013", "main");
}

#[test]
fn ct_closure_bind_wrong_arg_count() {
    let src = "f a:n b:n c:n>b;>a b;main>n;ct f 1 [1 2 3]";
    assert_err(src, "ILO-T013", "main");
}

#[test]
fn fld_closure_bind_wrong_arg_count() {
    let src = "f a:n>n;a;main>n;fld f 1 [1 2 3] 0";
    assert_err(src, "ILO-T013", "main");
}

#[test]
fn srt_closure_bind_wrong_arg_count() {
    let src = "f a:n>n;a;main>L n;srt f 1 [1 2 3]";
    assert_err(src, "ILO-T013", "main");
}

#[test]
fn rsrt_closure_bind_wrong_arg_count() {
    let src = "f a:n>n;a;main>L n;rsrt f 1 [1 2 3]";
    assert_err(src, "ILO-T013", "main");
}

// ---- map / mset / mhas / mkeys / mdel --------------------------------------

#[test]
fn mhas_not_a_map() {
    assert_err("main>b;mhas 5 \"k\"", "ILO-T013", "main");
}

#[test]
fn mkeys_not_a_map() {
    assert_err("main>L t;mkeys 5", "ILO-T013", "main");
}

// ---- numeric / linear-algebra ----------------------------------------------

#[test]
fn fft_wrong() {
    assert_err("main>L (L n);fft 5", "ILO-T013", "main");
}

#[test]
fn ifft_wrong() {
    assert_err("main>L n;ifft 5", "ILO-T013", "main");
}

#[test]
fn median_wrong() {
    assert_err("main>n;median 5", "ILO-T013", "main");
}

#[test]
fn stdev_wrong() {
    assert_err("main>n;stdev 5", "ILO-T013", "main");
}

#[test]
fn variance_wrong() {
    assert_err("main>n;variance 5", "ILO-T013", "main");
}

#[test]
fn quantile_first_wrong() {
    assert_err("main>n;quantile 5 0.5", "ILO-T013", "main");
}

#[test]
fn quantile_second_wrong() {
    assert_err("main>n;quantile [1 2 3] \"x\"", "ILO-T013", "main");
}

#[test]
fn transpose_wrong() {
    assert_err("main>L (L n);transpose 5", "ILO-T013", "main");
}

#[test]
fn matmul_arg1_wrong() {
    assert_err("main>L (L n);matmul 5 [[1 2] [3 4]]", "ILO-T013", "main");
}

#[test]
fn matmul_arg2_wrong() {
    assert_err("main>L (L n);matmul [[1 2] [3 4]] 5", "ILO-T013", "main");
}

#[test]
fn matvec_arg1_wrong() {
    assert_err("main>L n;matvec 5 [1 2]", "ILO-T013", "main");
}

#[test]
fn matvec_arg2_wrong() {
    assert_err("main>L n;matvec [[1 2] [3 4]] 5", "ILO-T013", "main");
}

#[test]
fn dot_arg1_wrong() {
    assert_err("main>n;dot 5 [1 2]", "ILO-T013", "main");
}

#[test]
fn dot_arg2_wrong() {
    assert_err("main>n;dot [1 2] 5", "ILO-T013", "main");
}

#[test]
fn solve_arg1_wrong() {
    assert_err("main>L n;solve 5 [1 2]", "ILO-T013", "main");
}

#[test]
fn solve_arg2_wrong() {
    assert_err("main>L n;solve [[1.0 2.0] [3.0 4.0]] 5", "ILO-T013", "main");
}

#[test]
fn inv_wrong() {
    assert_err("main>L (L n);inv 5", "ILO-T013", "main");
}

#[test]
fn det_wrong() {
    assert_err("main>n;det 5", "ILO-T013", "main");
}

#[test]
fn sleep_wrong() {
    assert_err("main>_;sleep \"x\"", "ILO-T013", "main");
}

// ---- slc / take / drop / chunks --------------------------------------------

#[test]
fn slc_wrong_type() {
    assert_err("main>L n;slc 5 0 2", "ILO-T013", "main");
}

#[test]
fn slc_index_wrong() {
    assert_err("main>L n;slc [1 2 3] \"a\" 2", "ILO-T013", "main");
}

#[test]
fn take_count_wrong() {
    assert_err("main>L n;take \"a\" [1 2 3]", "ILO-T013", "main");
}

#[test]
fn take_collection_wrong() {
    assert_err("main>L n;take 2 5", "ILO-T013", "main");
}

#[test]
fn drop_count_wrong() {
    assert_err("main>L n;drop \"a\" [1 2 3]", "ILO-T013", "main");
}

#[test]
fn chunks_arg1_wrong() {
    assert_err("main>L (L n);chunks \"x\" [1 2 3]", "ILO-T013", "main");
}

#[test]
fn chunks_arg2_wrong() {
    assert_err("main>L (L n);chunks 2 5", "ILO-T013", "main");
}

#[test]
fn frq_wrong() {
    assert_err("main>M n n;frq 5", "ILO-T013", "main");
}

#[test]
fn cumsum_wrong() {
    assert_err("main>L n;cumsum 5", "ILO-T013", "main");
}

#[test]
fn cumsum_wrong_inner() {
    assert_err("main>L n;cumsum [\"a\" \"b\"]", "ILO-T013", "main");
}

// ---- IO / HTTP / file errors -----------------------------------------------

#[test]
fn get_wrong() {
    assert_err("main>R t t;get 5", "ILO-T013", "main");
}

#[test]
fn get_headers_wrong() {
    assert_err("main>R t t;get \"http://x\" 5", "ILO-T013", "main");
}

#[test]
fn pst_wrong() {
    assert_err("main>R t t;pst 5 \"body\"", "ILO-T013", "main");
}

#[test]
fn pst_body_wrong() {
    assert_err("main>R t t;pst \"http://x\" 5", "ILO-T013", "main");
}

#[test]
fn pst_headers_wrong() {
    assert_err("main>R t t;pst \"http://x\" \"b\" 5", "ILO-T013", "main");
}

#[test]
fn getmany_wrong() {
    assert_err("main>L (R t t);get-many 5", "ILO-T013", "main");
}

// HTTP verb cluster (#5z) — verifier arms for put/pat/del/hed/opt.
// Mirror the pst/get verifier-error tests above.

#[test]
fn put_wrong_url() {
    assert_err("main>R t t;put 5 \"body\"", "ILO-T013", "main");
}

#[test]
fn put_wrong_body() {
    assert_err("main>R t t;put \"http://x\" 5", "ILO-T013", "main");
}

#[test]
fn put_wrong_headers() {
    assert_err("main>R t t;put \"http://x\" \"b\" 5", "ILO-T013", "main");
}

#[test]
fn pat_wrong_url() {
    assert_err("main>R t t;pat 5 \"body\"", "ILO-T013", "main");
}

#[test]
fn pat_wrong_body() {
    assert_err("main>R t t;pat \"http://x\" 5", "ILO-T013", "main");
}

#[test]
fn pat_wrong_headers() {
    assert_err("main>R t t;pat \"http://x\" \"b\" 5", "ILO-T013", "main");
}

#[test]
fn del_wrong_url() {
    assert_err("main>R t t;del 5", "ILO-T013", "main");
}

#[test]
fn del_wrong_headers() {
    assert_err("main>R t t;del \"http://x\" 5", "ILO-T013", "main");
}

#[test]
fn hed_wrong_url() {
    assert_err("main>R t t;hed 5", "ILO-T013", "main");
}

#[test]
fn hed_wrong_headers() {
    assert_err("main>R t t;hed \"http://x\" 5", "ILO-T013", "main");
}

#[test]
fn opt_wrong_url() {
    assert_err("main>R t t;opt 5", "ILO-T013", "main");
}

#[test]
fn opt_wrong_headers() {
    assert_err("main>R t t;opt \"http://x\" 5", "ILO-T013", "main");
}

#[test]
fn rd_wrong() {
    assert_err("main>R _ t;rd 5", "ILO-T013", "main");
}

#[test]
fn rd_fmt_wrong() {
    assert_err("main>R _ t;rd \"/tmp/x\" 5", "ILO-T013", "main");
}

#[test]
fn rdl_wrong() {
    assert_err("main>R (L t) t;rdl 5", "ILO-T013", "main");
}

#[test]
fn wr_path_wrong() {
    assert_err("main>R t t;wr 5 \"data\"", "ILO-T013", "main");
}

#[test]
fn wr_content_wrong() {
    assert_err("main>R t t;wr \"/tmp/x\" 5", "ILO-T013", "main");
}

#[test]
fn wr_fmt_wrong() {
    assert_err("main>R t t;wr \"/tmp/x\" [1 2] 5", "ILO-T013", "main");
}

#[test]
fn jpth_wrong() {
    assert_err("main>R t t;jpth 5 \"$.x\"", "ILO-T013", "main");
}

#[test]
fn rgxsub_wrong() {
    assert_err("main>t;rgxsub 5 \"x\" \"y\"", "ILO-T013", "main");
}

#[test]
fn jpar_wrong() {
    assert_err("main>R _ t;jpar 5", "ILO-T013", "main");
}

#[test]
fn rdjl_wrong() {
    assert_err("main>L (R _ t);rdjl 5", "ILO-T013", "main");
}

#[test]
fn dtfmt_first_wrong() {
    assert_err("main>R t t;dtfmt \"x\" \"%Y\"", "ILO-T013", "main");
}

#[test]
fn dtfmt_second_wrong() {
    assert_err("main>R t t;dtfmt 0 5", "ILO-T013", "main");
}

#[test]
fn dtparse_first_wrong() {
    assert_err("main>R n t;dtparse 5 \"%Y\"", "ILO-T013", "main");
}

#[test]
fn dtparse_second_wrong() {
    assert_err("main>R n t;dtparse \"2026\" 5", "ILO-T013", "main");
}

// ---- arity errors ----------------------------------------------------------

#[test]
fn arity_rnd() {
    assert_err("main>n;rnd 1", "ILO-T006", "main");
}

#[test]
fn arity_srt() {
    assert_err("main>L n;srt 1 2 3 4", "ILO-T006", "main");
}

#[test]
fn arity_map() {
    assert_err("main>L n;map 1", "ILO-T006", "main");
}

#[test]
fn arity_fld() {
    assert_err("main>n;fld 1 2", "ILO-T006", "main");
}

#[test]
fn arity_rd() {
    assert_err("main>R _ t;rd \"p\" \"json\" \"extra\"", "ILO-T006", "main");
}

#[test]
fn arity_get() {
    assert_err("main>R t t;get \"u\" 5 9", "ILO-T006", "main");
}

#[test]
fn arity_pst() {
    assert_err("main>R t t;pst \"u\"", "ILO-T006", "main");
}

#[test]
fn arity_wr() {
    assert_err("main>R t t;wr \"p\"", "ILO-T006", "main");
}

#[test]
fn arity_padl() {
    assert_err("main>t;padl \"x\"", "ILO-T006", "main");
}

#[test]
fn arity_padr() {
    assert_err("main>t;padr \"x\"", "ILO-T006", "main");
}

#[test]
fn arity_min_max() {
    assert_err("main>n;min 1 2 3", "ILO-T006", "main");
}

// arity_fmt: fmt requires at least 1 arg, but a zero-arg `fmt` parses as
// `Ref("fmt")` and never reaches the arity check. Skipped (no easy way to
// trigger ILO-T006 for fmt via inline path).

#[test]
fn arity_userfn() {
    let src = "f a:n b:n>n;+a b;main>n;f 1 2 3";
    assert_err(src, "ILO-T006", "main");
}

#[test]
fn type_mismatch_userfn_text_to_num() {
    let src = "f a:n>n;a;main>n;f \"x\"";
    assert_err(src, "ILO-T007", "main");
}

#[test]
fn type_mismatch_userfn_num_to_text() {
    let src = "f a:t>t;a;main>t;f 5";
    assert_err(src, "ILO-T007", "main");
}

// ---- match / records / fields / ternary ------------------------------------

#[test]
fn undefined_type_record() {
    assert_err("main>n;x=zomg n:1;1", "ILO-T003", "main");
}

#[test]
fn missing_field() {
    assert_err("type pt{x:n;y:n}main>pt;pt x:1", "ILO-T015", "main");
}

#[test]
fn unknown_field() {
    assert_err("type pt{x:n;y:n}main>pt;pt x:1 y:2 z:3", "ILO-T016", "main");
}

#[test]
fn field_type_mismatch() {
    assert_err("type pt{x:n;y:n}main>pt;pt x:1 y:\"a\"", "ILO-T017", "main");
}

#[test]
fn field_on_nonrecord() {
    let src = "main>n;x=5;x.foo";
    assert_err(src, "ILO-T018", "main");
}

#[test]
fn no_field_on_type() {
    let src = "type pt{x:n;y:n}main>n;p=pt x:1 y:2;p.zorbo";
    assert_err(src, "ILO-T019", "main");
}

#[test]
fn with_on_nonrecord() {
    assert_err("main>n;x=5;x with a:1", "ILO-T020", "main");
}

#[test]
fn with_unknown_field() {
    let src = "type pt{x:n;y:n}main>pt;p=pt x:1 y:2;p with z:9";
    assert_err(src, "ILO-T021", "main");
}

#[test]
fn with_field_type_mismatch() {
    let src = "type pt{x:n;y:n}main>pt;p=pt x:1 y:2;p with x:\"a\"";
    assert_err(src, "ILO-T022", "main");
}

#[test]
fn with_anon_record_new_field_rejected() {
    // ILO-368: `with` on an anonymous record must not add new fields
    let src = "main>_;r={x:1 y:2};r with z:3";
    assert_err(src, "ILO-T045", "main");
}

#[test]
fn with_anon_record_existing_field_ok() {
    // ILO-368: `with` on an anonymous record updating an existing field is fine
    let src = "main>_;r={x:1 y:2};r with x:9";
    assert_ok(src, "main");
}

#[test]
fn index_on_nonlist() {
    assert_err("main>n;x=5;x.0", "ILO-T023", "main");
}

// match exhaustiveness
#[test]
fn match_non_exhaustive_result_missing_err() {
    let src = "main>n;r=num \"5\";?r{~v:v}";
    assert_err(src, "ILO-T024", "main");
}

#[test]
fn match_non_exhaustive_result_missing_ok() {
    let src = "main>n;r=num \"5\";?r{^_:0}";
    assert_err(src, "ILO-T024", "main");
}

#[test]
fn match_non_exhaustive_bool_missing_true() {
    let src = "main>n;b=>1 2;?b{false:0}";
    assert_err(src, "ILO-T024", "main");
}

#[test]
fn match_non_exhaustive_bool_missing_false() {
    let src = "main>n;b=>1 2;?b{true:1}";
    assert_err(src, "ILO-T024", "main");
}

#[test]
fn match_non_exhaustive_other_no_wildcard() {
    let src = "main>n;x=5;?x{1:1;2:2}";
    assert_err(src, "ILO-T024", "main");
}

// ---- ILO-T035: Result arm on Option subject ---------------------------------

// `~v:` on an Option subject (mget returns O n) silently falls through to the
// wildcard at runtime. The verifier now flags this.
#[test]
fn match_ok_arm_on_option_subject() {
    let src = "main>n;m=mmap;m=mset m \"k\" 5;?(mget m \"k\"){~v:v;_:0}";
    assert_err(src, "ILO-T035", "main");
}

#[test]
fn match_err_arm_on_option_subject() {
    let src = "main>n;m=mmap;m=mset m \"k\" 5;?(mget m \"k\"){^er:0;_:9}";
    assert_err(src, "ILO-T035", "main");
}

#[test]
fn match_both_ok_and_err_arm_on_option_subject() {
    let src = "main>n;m=mmap;m=mset m \"k\" 5;?(mget m \"k\"){~v:v;^er:0;_:9}";
    assert_err(src, "ILO-T035", "main");
}

// Negative: `~v:` / `^er:` on a Result subject still verify cleanly.
#[test]
fn match_ok_arm_on_result_subject_ok() {
    let src = "main>n;r=num \"5\";?r{~v:v;^er:0}";
    assert_ok(src, "main");
}

// Negative: Option subject with literal + wildcard arms is fine (no ~/^ arm).
#[test]
fn match_literal_and_wildcard_on_option_subject_ok() {
    let src = "main>t;m=mmap;m=mset m \"k\" 5;?(mget m \"k\"){5:\"hit\";_:\"miss\"}";
    assert_ok(src, "main");
}

// Negative: Option subject with only wildcard is fine.
#[test]
fn match_wildcard_only_on_option_subject_ok() {
    let src = "main>n;m=mmap;m=mset m \"k\" 5;?(mget m \"k\"){_:0}";
    assert_ok(src, "main");
}

// ---- bang / unwrap / Result --------------------------------------------------

#[test]
fn bang_on_non_result_func() {
    let src = "g x:n>n;*x 2 main>n;r=g! 1;r";
    assert_err(src, "ILO-T025", "main");
}

#[test]
fn bang_propagate_wrong_enclosing() {
    let src = "g>R n t;~5 main>n;r=g!;r";
    assert_err(src, "ILO-T026", "main");
}

#[test]
fn double_bang_panic_on_non_result() {
    let src = "g x:n>n;*x 2 main>n;r=g!! 1;r";
    assert_err(src, "ILO-T025", "main");
}

// ILO-T034: bang on bound value (not a call)
#[test]
fn bang_on_value_ident() {
    let src = "main>R n t;x=5;x!";
    assert_err(src, "ILO-T034", "main");
}

#[test]
fn doublebang_on_value_ident() {
    let src = "main>n;x=5;x!!";
    assert_err(src, "ILO-T034", "main");
}

// ILO-T005: ident bound as non-fn used as function
#[test]
fn call_on_non_function_ident() {
    let src = "main>n;x=5;x 1 2";
    assert_err(src, "ILO-T005", "main");
}

// ILO-T004 / undefined var
#[test]
fn undefined_var() {
    assert_err("main>n;blorp", "ILO-T004", "main");
}

#[test]
fn undefined_function_call() {
    assert_err("main>n;noproc 1 2", "ILO-T005", "main");
}

// ---- type aliases, duplicates ----------------------------------------------

#[test]
fn alias_shadows_builtin() {
    let src = "alias n t main>n;1";
    assert_err(src, "ILO-T031", "main");
}

#[test]
fn duplicate_alias() {
    let src = "alias aa n alias aa t main>n;1";
    assert_err(src, "ILO-T001", "main");
}

#[test]
fn duplicate_typedef() {
    let src = "type pt{x:n}type pt{y:n}main>n;1";
    assert_err(src, "ILO-T001", "main");
}

#[test]
fn duplicate_function() {
    let src = "f>n;1;f>n;2;main>n;f";
    assert_err(src, "ILO-T002", "main");
}

#[test]
fn undefined_named_type_in_sig() {
    let src = "f a:zorbo>n;1;main>n;1";
    assert_err(src, "ILO-T003", "main");
}

// ---- binops, comparisons, append, negate -----------------------------------

#[test]
fn binop_add_mismatch() {
    let src = "main>n;+1 \"x\"";
    assert_err(src, "ILO-T009", "main");
}

#[test]
fn binop_sub_text() {
    let src = "main>n;-1 \"x\"";
    assert_err(src, "ILO-T009", "main");
}

#[test]
fn binop_div_text() {
    let src = "main>n;/1 \"x\"";
    assert_err(src, "ILO-T009", "main");
}

#[test]
fn compare_type_mismatch() {
    let src = "main>b;>1 \"x\"";
    assert_err(src, "ILO-T010", "main");
}

#[test]
fn append_to_nonlist() {
    let src = "main>n;x=5;+=x 1";
    assert_err(src, "ILO-T011", "main");
}

#[test]
fn append_wrong_elem_type() {
    let src = "main>L n;xs=[1 2 3];+=xs \"a\"";
    assert_err(src, "ILO-T011", "main");
}

#[test]
fn negate_text() {
    let src = "main>n;-\"x\"";
    assert_err(src, "ILO-T012", "main");
}

// ---- return type, ternary, range, foreach ----------------------------------

#[test]
fn return_type_mismatch_n_to_t() {
    let src = "main>t;1";
    assert_err(src, "ILO-T008", "main");
}

#[test]
fn return_type_mismatch_t_to_n() {
    let src = "main>n;\"a\"";
    assert_err(src, "ILO-T008", "main");
}

#[test]
fn foreach_nonlist() {
    let src = "main>n;x=5;@e x{+e 1};0";
    assert_err(src, "ILO-T014", "main");
}

// ---- ILO-T032: bare fmt at non-tail (warning) ------------------------------

#[test]
fn bare_fmt_non_tail_warns() {
    // Should succeed but emit ILO-T032 warning
    let src = "main>n;fmt \"x={}\" 1;0";
    let (ok, stderr) = run(src, "main");
    let _ = ok; // warning, may still pass
    assert!(
        stderr.contains("ILO-T032"),
        "expected ILO-T032 warning, stderr={stderr}"
    );
}

#[test]
fn bare_mset_non_tail_warns() {
    let src = "main>n;m=mmap;mset m \"k\" 1;0";
    let (_ok, stderr) = run(src, "main");
    assert!(
        stderr.contains("ILO-T033"),
        "expected ILO-T033 warning, stderr={stderr}"
    );
}

#[test]
fn bare_append_non_tail_warns() {
    let src = "main>n;xs=[1 2 3];+=xs 4;0";
    let (_ok, stderr) = run(src, "main");
    assert!(
        stderr.contains("ILO-T033"),
        "expected ILO-T033 warning, stderr={stderr}"
    );
}

#[test]
fn bare_mdel_non_tail_warns() {
    let src = "main>n;m=mmap;mdel m \"k\";0";
    let (_ok, stderr) = run(src, "main");
    assert!(
        stderr.contains("ILO-T033"),
        "expected ILO-T033 warning, stderr={stderr}"
    );
}

// ---- ILO-T029: unreachable after ret/brk -----------------------------------

#[test]
fn unreachable_after_return() {
    let src = "f a:n>n;ret a;+a 1;main>n;f 1";
    let (_ok, stderr) = run(src, "main");
    assert!(
        stderr.contains("ILO-T029"),
        "expected ILO-T029 warning, stderr={stderr}"
    );
}

// ---- closure / builtin-as-fn-ty branches -----------------------------------

#[test]
fn builtin_as_hof_fld_min() {
    assert_ok("main>n;fld min [3 1 2] 99", "main");
}

#[test]
fn builtin_as_hof_fld_max() {
    assert_ok("main>n;fld max [3 1 2] 0", "main");
}

#[test]
fn builtin_as_hof_map_abs() {
    assert_ok("main>L n;map abs [-1 -2 3]", "main");
}

#[test]
fn builtin_as_hof_map_str() {
    assert_ok("main>L t;map str [1 2 3]", "main");
}

// ---- good programs that exercise positive arms -----------------------------

#[test]
fn map_with_user_fn() {
    let src = "sq x:n>n;*x x;main>L n;map sq [1 2 3]";
    assert_ok(src, "main");
}

#[test]
fn flt_with_user_fn() {
    let src = "pos x:n>b;>x 0;main>L n;flt pos [-1 2 -3 4]";
    assert_ok(src, "main");
}

#[test]
fn fld_with_user_fn() {
    let src = "ad a:n b:n>n;+a b;main>n;fld ad [1 2 3] 0";
    assert_ok(src, "main");
}

#[test]
fn match_result_exhaustive() {
    let src = "main>n;r=num \"5\";?r{~v:v;^_:0}";
    assert_ok(src, "main");
}

#[test]
fn match_wildcard_arm() {
    let src = "main>n;x=5;?x{1:1;_:99}";
    assert_ok(src, "main");
}

#[test]
fn record_field_access() {
    let src = "type pt{x:n;y:n}main>n;p=pt x:1 y:2;p.x";
    assert_ok(src, "main");
}

#[test]
fn record_with_update() {
    let src = "type pt{x:n;y:n}main>pt;p=pt x:1 y:2;p with x:9";
    assert_ok(src, "main");
}

#[test]
fn destructure_record() {
    let src = "type pt{x:n;y:n}main>n;p=pt x:1 y:2;{x;y}=p;+x y";
    assert_ok(src, "main");
}

#[test]
fn type_alias_resolves() {
    let src = "alias mynum n f a:mynum>mynum;a main>n;f 7";
    assert_ok(src, "main");
}

#[test]
fn nested_list_type_validates() {
    let src = "main>L (L n);[[1 2] [3 4]]";
    assert_ok(src, "main");
}

#[test]
fn result_propagate_ok() {
    let src = "main>R n t;v=num! \"5\";~v";
    assert_ok(src, "main");
}
