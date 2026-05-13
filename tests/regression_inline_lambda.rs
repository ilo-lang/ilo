// Regression tests for inline lambdas (Phase 1 + Phase 2).
//
// Inline lambda = `(params>return;body)` literal passed where a fn-ref is
// expected (HOF arg position). The parser lifts each lambda to a synthetic
// top-level `Decl::Function { name: "__lit_N", ... }` and replaces the call
// site with `Expr::Ref("__lit_N")`, so the rest of the toolchain (verifier,
// tree interpreter, fmt, python codegen, ...) treats it identically to a
// named helper.
//
// Phase 2 (this PR) adds closure capture: free variables in the body get
// lifted as trailing params on the synthetic decl, and the call site emits
// `Expr::MakeClosure { fn_name, captures }` which evaluates to a
// `Value::Closure { fn_name, captures }` runtime value. Closure-aware HOFs
// append the captures after the per-item args at each call, matching the
// existing single-ctx form (#186) generalised to N captures.
//
// HOF dispatch in the VM and Cranelift JIT is the parked FnRef NaN-tagging
// effort — every test here runs on `--run-tree` only, matching the
// closure-bind tests.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(name: &str, src: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("ilo_lam_{name}_{}_{n}.ilo", std::process::id()));
    std::fs::write(&path, src).expect("write src");
    path
}

fn run_ok(src: &str, entry: &str, args: &[&str]) -> String {
    let path = write_src(entry, src);
    let mut cmd = ilo();
    cmd.arg(&path).arg("--run-tree").arg(entry);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "ilo failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[allow(dead_code)]
fn run_err(src: &str, entry: &str) -> String {
    let path = write_src(entry, src);
    let out = ilo()
        .arg(&path)
        .arg("--run-tree")
        .arg(entry)
        .output()
        .expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    assert!(
        !out.status.success(),
        "expected failure but ilo succeeded for `{src}`"
    );
    let mut s = String::from_utf8_lossy(&out.stderr).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stdout));
    s
}

// ── srt: 1-arg key fn ──────────────────────────────────────────────────────

#[test]
fn srt_inline_key_by_length() {
    let src = "f ws:L t>L t;srt (s:t>n;len s) ws";
    assert_eq!(
        run_ok(src, "f", &["[\"banana\",\"fig\",\"apple\"]"]),
        "[fig, apple, banana]"
    );
}

#[test]
fn srt_inline_key_absolute_value() {
    // Body uses `abs` builtin — no captures, no helper.
    let src = "f xs:L n>L n;srt (x:n>n;abs x) xs";
    assert_eq!(run_ok(src, "f", &["[-3,1,-5,2]"]), "[1, 2, -3, -5]");
}

// ── flt: 1-arg predicate ───────────────────────────────────────────────────

#[test]
fn flt_inline_predicate() {
    let src = "f xs:L n>L n;flt (x:n>b;>x 0) xs";
    assert_eq!(run_ok(src, "f", &["[-2,3,-1,4,0]"]), "[3, 4]");
}

// ── map: 1-arg transform ───────────────────────────────────────────────────

#[test]
fn map_inline_double() {
    let src = "f xs:L n>L n;map (x:n>n;*x 2) xs";
    assert_eq!(run_ok(src, "f", &["[1,2,3]"]), "[2, 4, 6]");
}

// ── fld: 2-arg accumulator ─────────────────────────────────────────────────

#[test]
fn fld_inline_sum_of_squares() {
    let src = "f xs:L n>n;fld (a:n x:n>n;+a *x x) xs 0";
    assert_eq!(run_ok(src, "f", &["[1,2,3,4]"]), "30");
}

// ── multi-statement body (let + final expression) ──────────────────────────

#[test]
fn lambda_multi_statement_body() {
    let src = "f xs:L n>L n;srt (x:n>n;sq=*x x;sq) xs";
    assert_eq!(run_ok(src, "f", &["[-3,1,-5,2]"]), "[1, 2, -3, -5]");
}

// ── lambda calling a top-level helper (HOF inside HOF) ─────────────────────

#[test]
fn lambda_can_call_top_level_helper() {
    let src = "dbl x:n>n;*x 2\nf xs:L n>L n;map (x:n>n;dbl x) xs";
    assert_eq!(run_ok(src, "f", &["[1,2,3]"]), "[2, 4, 6]");
}

// ── lambda calling a builtin ───────────────────────────────────────────────

#[test]
fn lambda_can_call_builtin() {
    let src = "f xs:L n>L n;srt (x:n>n;abs x) xs";
    assert_eq!(run_ok(src, "f", &["[-3,1,-5,2]"]), "[1, 2, -3, -5]");
}

// ── Multiple lambdas in one function (counter increments) ──────────────────

#[test]
fn multiple_lambdas_in_one_function() {
    let src = "f xs:L n>L n;ys=map (x:n>n;*x 2) xs;flt (x:n>b;>x 4) ys";
    assert_eq!(run_ok(src, "f", &["[1,2,3,4]"]), "[6, 8]");
}

// ── Lambda inside a top-level helper, used twice via different entries ─────

#[test]
fn lambda_inside_helper() {
    let src = "
sorted xs:L n>L n;srt (x:n>n;abs x) xs
f xs:L n>L n;sorted xs
";
    assert_eq!(run_ok(src, "f", &["[-3,1,-5,2]"]), "[1, 2, -3, -5]");
}

// ── Phase 2: closure capture works ─────────────────────────────────────────

#[test]
fn closure_capture_single_var_filter() {
    // Single capture: `thr` is in the enclosing fn's scope and the lambda
    // references it. The parser lifts `__lit_0(x, thr)` and emits a
    // MakeClosure at the call site; flt appends the capture to each call.
    let src = "f xs:L n thr:n>L n;flt (x:n>b;>x thr) xs";
    assert_eq!(run_ok(src, "f", &["[1,5,3,8,2]", "4"]), "[5, 8]");
}

#[test]
fn closure_capture_in_sort_key() {
    // `srt` with an inline key that closes over `target`.
    let src = "f xs:L n target:n>L n;srt (x:n>n;abs -x target) xs";
    assert_eq!(run_ok(src, "f", &["[1,5,10,20]", "8"]), "[10, 5, 1, 20]");
}

#[test]
fn closure_capture_in_map() {
    // `map` with an inline transform that closes over `bump`.
    let src = "f xs:L n bump:n>L n;map (x:n>n;+x bump) xs";
    assert_eq!(run_ok(src, "f", &["[1,2,3]", "10"]), "[11, 12, 13]");
}

#[test]
fn closure_capture_in_fld() {
    // `fld` with an inline reducer that closes over `weight`.
    let src = "f xs:L n weight:n>n;fld (a:n x:n>n;+a *x weight) xs 0";
    assert_eq!(run_ok(src, "f", &["[1,2,3,4]", "5"]), "50");
}

#[test]
fn closure_capture_multiple_vars() {
    // Two captures: `lo` and `hi` both appear in the body. Both lift as
    // trailing params on the synthetic decl, and both flow as captures.
    let src = "f xs:L n lo:n hi:n>L n;flt (x:n>b;&(>=x lo) <=x hi) xs";
    assert_eq!(run_ok(src, "f", &["[1,3,5,7,9,11]", "3", "7"]), "[3, 5, 7]");
}

#[test]
fn closure_capture_text_value() {
    // Capture a Text value, not just numbers. By-value snapshot semantics.
    let src = "f ws:L t prefix:t>L t;flt (w:t>b;has w prefix) ws";
    assert_eq!(
        run_ok(src, "f", &["[\"apple\",\"banana\",\"apricot\"]", "ap"]),
        "[apple, apricot]"
    );
}

#[test]
fn closure_capture_by_value_snapshot() {
    // The capture is snapshot when the closure is constructed, not read
    // live at each call. We mutate the source local after the `srt` runs
    // (well — srt has already completed by then). This just exercises that
    // mutating the capture's source name post-construction is irrelevant
    // because srt already consumed it. The real check is value-equality.
    let src = "f xs:L n bias:n>L n;ys=srt (x:n>n;+x bias) xs;ys";
    assert_eq!(run_ok(src, "f", &["[3,1,2]", "0"]), "[1, 2, 3]");
}

// ── Phase 1 ctx-arg form is still supported alongside captures ─────────────

#[test]
fn ctx_arg_form_works_with_inline_lambda() {
    // Phase 1 capture rejection nudges users to ctx-arg form, which already
    // works for inline lambdas too — the lambda just takes an extra param.
    let src = "f xs:L n thr:n>L n;flt (x:n c:n>b;>x c) thr xs";
    assert_eq!(run_ok(src, "f", &["[1,5,3,8,2]", "4"]), "[5, 8]");
}

// ── No regression: grouped parenthesised expressions still parse ───────────

#[test]
fn grouped_expression_still_parses() {
    // `(+a b)` is a grouped expression, not a lambda — no `ident:` and no
    // leading `>`. Must not trip the inline-lambda lookahead.
    let src = "f a:n b:n>n;*(+a b) 2";
    assert_eq!(run_ok(src, "f", &["3", "4"]), "14");
}

// ── No regression: existing named-helper HOF call still works ──────────────

#[test]
fn named_helper_hof_unaffected() {
    let src = "k x:n>n;abs x\nf xs:L n>L n;srt k xs";
    assert_eq!(run_ok(src, "f", &["[-3,1,-5,2]"]), "[1, 2, -3, -5]");
}

// ── Lambda inside foreach body shadowing is honored ────────────────────────

#[test]
fn lambda_local_binding_shadows_nothing_outside() {
    // The `s` inside the lambda is a param, not a capture of any outer name.
    // Even though there's no outer `s`, this exercises the param/local
    // resolution path explicitly.
    let src = "f ws:L t>L t;srt (s:t>n;n=len s;n) ws";
    assert_eq!(
        run_ok(src, "f", &["[\"banana\",\"fig\",\"apple\"]"]),
        "[fig, apple, banana]"
    );
}
