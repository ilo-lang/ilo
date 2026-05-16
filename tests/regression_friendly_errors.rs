use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_err(src: &str) -> String {
    let out = ilo().arg(src).output().expect("failed to run ilo");
    assert!(!out.status.success(), "expected failure for {src:?}");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn run_ok(src: &str) {
    // These regressions pin parser/verifier acceptance. Since inline
    // single-fn snippets now auto-run (and would fail arity without
    // positional args), inspect via `--ast` instead to keep the
    // parse-acceptance signal cleanly decoupled from runtime semantics.
    let out = ilo()
        .args(["--ast", src])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "expected success for {src:?}, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// ---- Reserved keywords as LHS of a binding ----

fn assert_reserved_kw(src: &str, word: &str) {
    let err = run_err(src);
    assert!(err.contains("ILO-P011"), "{word}: stderr: {err}");
    assert!(
        err.contains(&format!("`{word}` is a reserved word")),
        "{word}: stderr: {err}"
    );
}

#[test]
fn friendly_var_binding() {
    assert_reserved_kw("var=5", "var");
}

#[test]
fn friendly_let_binding() {
    assert_reserved_kw("let=5", "let");
}

#[test]
fn friendly_fn_binding() {
    assert_reserved_kw("fn=5", "fn");
}

#[test]
fn friendly_const_binding() {
    assert_reserved_kw("const=5", "const");
}

#[test]
fn friendly_if_binding() {
    assert_reserved_kw("if=5", "if");
}

#[test]
fn friendly_return_binding() {
    assert_reserved_kw("return=5", "return");
}

#[test]
fn friendly_def_binding() {
    assert_reserved_kw("def=5", "def");
}

// ---- cnt / brk used as identifiers ----

#[test]
fn friendly_cnt_binding() {
    let err = run_err("cnt=5");
    assert!(err.contains("ILO-P011"), "stderr: {err}");
    assert!(
        err.contains("`cnt` is reserved for continue"),
        "stderr: {err}"
    );
    assert!(err.contains("count"), "hint should suggest `count`: {err}");
    // Should not cascade through to the verifier's ILO-T028.
    assert!(!err.contains("ILO-T028"), "cascade leaked: {err}");
}

#[test]
fn friendly_brk_binding() {
    let err = run_err("brk=5");
    assert!(err.contains("ILO-P011"), "stderr: {err}");
    assert!(err.contains("`brk` is reserved for break"), "stderr: {err}");
    assert!(!err.contains("ILO-T028"), "cascade leaked: {err}");
}

#[test]
fn friendly_fld_binding() {
    let err = run_err("fld=5");
    assert!(err.contains("ILO-P011"), "stderr: {err}");
    assert!(
        err.contains("`fld` is reserved for the fold builtin"),
        "stderr: {err}"
    );
    assert!(err.contains("field"), "hint should suggest `field`: {err}");
    // Should not cascade through to the verifier's misleading arity error.
    assert!(!err.contains("ILO-T006"), "arity cascade leaked: {err}");
    assert!(
        !err.contains("arity mismatch"),
        "arity cascade leaked: {err}"
    );
}

// ---- Underscore mid-identifier ----

#[test]
fn friendly_underscore_in_ident() {
    let err = run_err("rev_ps=5");
    assert!(err.contains("ILO-L002"), "stderr: {err}");
    assert!(err.contains("underscores are not allowed"), "stderr: {err}");
    assert!(err.contains("rev-ps"), "should suggest hyphen form: {err}");
}

// ---- Uppercase mid-identifier ----

#[test]
fn friendly_uppercase_in_ident() {
    let err = run_err("isAgg=5");
    assert!(err.contains("ILO-L003"), "stderr: {err}");
    assert!(err.contains("lowercase"), "stderr: {err}");
    assert!(err.contains("isAgg"), "should echo offender: {err}");
    assert!(
        err.contains("is-agg") || err.contains("isagg"),
        "should suggest hyphen/lowercase form: {err}"
    );
}

// ---- Negative regressions ----

#[test]
fn normal_binding_still_works() {
    run_ok("f>n;count=5;count");
}

#[test]
fn type_sigil_list_still_works() {
    run_ok("f x:L n>n;0");
}

#[test]
fn type_sigil_result_still_works() {
    run_ok("f x:R t t>n;0");
}

#[test]
fn type_sigil_fn_still_works() {
    run_ok("f x:F n n>n;0");
}

#[test]
fn type_sigil_opt_still_works() {
    run_ok("f x:O n>n;0");
}

#[test]
fn type_sigil_map_still_works() {
    run_ok("f x:M t n>n;0");
}

#[test]
fn type_sigil_sum_still_works() {
    run_ok("f x:S n t>n;0");
}

// ---- Reserved keywords as binding LHS *inside* a function body ----
// These must produce ILO-P011, not the cryptic ILO-P009 from parse_atom.

fn assert_reserved_in_body(src: &str, word: &str) {
    let err = run_err(src);
    assert!(err.contains("ILO-P011"), "{word} in body: stderr: {err}");
    assert!(
        err.contains(&format!("`{word}` is a reserved word")),
        "{word} in body: stderr: {err}"
    );
    assert!(
        !err.contains("ILO-P009"),
        "{word} in body should not cascade to ILO-P009: {err}"
    );
}

#[test]
fn friendly_var_binding_in_body() {
    assert_reserved_in_body("f>n;var=5;var", "var");
}

#[test]
fn friendly_let_binding_in_body() {
    assert_reserved_in_body("f>n;let=5;let", "let");
}

#[test]
fn friendly_fn_binding_in_body() {
    assert_reserved_in_body("f>n;fn=5;fn", "fn");
}

#[test]
fn friendly_const_binding_in_body() {
    assert_reserved_in_body("f>n;const=5;const", "const");
}

#[test]
fn friendly_if_binding_in_body() {
    assert_reserved_in_body("f>n;if=5;if", "if");
}

#[test]
fn friendly_return_binding_in_body() {
    assert_reserved_in_body("f>n;return=5;return", "return");
}

#[test]
fn friendly_def_binding_in_body() {
    assert_reserved_in_body("f>n;def=5;def", "def");
}

// ---- Wildcard `_` in match/destructure patterns must still work ----

#[test]
fn bare_underscore_wildcard_still_works() {
    // `_` as the match-all arm of a `?` match expression — a real,
    // common ilo idiom. Should not trip the underscore-in-ident lexer
    // heuristic (ILO-L002).
    run_ok("desc n:n>t;?n{0:\"zero\";1:\"one\";_:\"many\"}");
}

// ---- Type sigils in *return-type* position ----

#[test]
fn type_sigil_list_return_still_works() {
    run_ok("f x:n>L n;[]");
}

#[test]
fn type_sigil_result_return_still_works() {
    run_ok("g x:n>R t t;~\"ok\"");
}

// ---- Names starting with reserved prefixes must still bind ----

#[test]
fn names_starting_with_reserved_prefixes_bind() {
    // `letter`, `variable`, `iffy`, `constant`, `function` should all
    // lex as Ident, not as `KwLet`/`KwVar`/`KwIf`/`KwConst`/`KwFn`.
    run_ok(
        "f>n;letter=1;variable=2;iffy=3;constant=4;function=5;+ + + + letter variable iffy constant function",
    );
}

// ---- Uppercase mid-identifier with hyphenated tail ----

#[test]
fn uppercase_mid_ident_includes_hyphenated_tail() {
    let err = run_err("isHello-world=5");
    assert!(err.contains("ILO-L003"), "stderr: {err}");
    assert!(
        err.contains("isHello-world"),
        "should echo full hyphenated offender: {err}"
    );
    assert!(
        err.contains("is-hello-world"),
        "should suggest fully hyphenated form: {err}"
    );
}

// ---- Camel-case rename cascade (per-file scan in a single pass) ----
//
// When the lexer rejects the first camelCase identifier, the suggestion now
// lists every other distinct camelCase offender in the file so the user can
// fix them all in one pass instead of N-1 retry rounds.

#[test]
fn camel_cascade_lists_other_offenders() {
    let err = run_err("go>n;fooBar=1;bazQux=2;helloWorld=3;fooBar");
    assert!(err.contains("ILO-L003"), "stderr: {err}");
    assert!(
        err.contains("Also found in this file"),
        "should list extras: {err}"
    );
    assert!(err.contains("bazQux"), "should list bazQux: {err}");
    assert!(err.contains("helloWorld"), "should list helloWorld: {err}");
}

#[test]
fn camel_cascade_dedupes_current_offender() {
    // `fooBar` appears twice in the source; the extras list must not
    // re-echo it — only distinct other offenders.
    let err = run_err("go>n;fooBar=1;fooBar");
    assert!(err.contains("ILO-L003"), "stderr: {err}");
    assert!(
        !err.contains("Also found in this file"),
        "single distinct offender should not produce a list: {err}"
    );
}

#[test]
fn camel_cascade_truncates_long_lists() {
    let err = run_err("go>n;aA=1;bB=2;cC=3;dD=4;eE=5;fF=6;gG=7;hH=8;aA");
    assert!(err.contains("ILO-L003"), "stderr: {err}");
    assert!(
        err.contains("more"),
        "should truncate with `+N more`: {err}"
    );
}

// ---- `?cond{body}` bare-bool match-vs-conditional confusion ----

#[test]
fn match_bare_bool_with_let_body_suggests_eq_true_form() {
    let err = run_err("go>n;hit=true;errs=0;?hit{errs=+errs 1};errs");
    assert!(err.contains("ILO-P011"), "stderr: {err}");
    assert!(
        err.contains("match syntax"),
        "should explain that ?expr{{}} is match: {err}"
    );
    assert!(
        err.contains("=hit true{body}"),
        "should suggest =hit true{{body}}: {err}"
    );
}

#[test]
fn match_bare_bool_does_not_fire_on_real_pattern() {
    // `~v:body` is a real Ok-pattern arm — must not trigger the hint.
    run_ok("go>R n t;r=~5;?r{~v:v;^e:0}");
}

#[test]
fn match_bare_bool_does_not_fire_on_literal_pattern() {
    run_ok("go>n;x=2;?x{1:10;2:20;_:0}");
}

// ---- `name={...}` map-literal hint ----

#[test]
fn map_literal_text_key_hints_mset_mmap() {
    let err = run_err("go>n;m={\"a\" 1};0");
    assert!(err.contains("ILO-P009"), "stderr: {err}");
    assert!(
        err.contains("map literal syntax"),
        "should explain no map literal: {err}"
    );
    assert!(err.contains("mset mmap"), "should suggest mset mmap: {err}");
}

#[test]
fn map_literal_number_key_hints_mset_mmap() {
    let err = run_err("go>n;m={1 \"a\"};0");
    assert!(err.contains("ILO-P009"), "stderr: {err}");
    assert!(err.contains("mset mmap"), "stderr: {err}");
}

#[test]
fn map_literal_empty_braces_hints_mset_mmap() {
    let err = run_err("go>n;m={};0");
    assert!(err.contains("ILO-P009"), "stderr: {err}");
    assert!(err.contains("mset mmap"), "stderr: {err}");
}

#[test]
fn let_with_normal_rhs_still_parses() {
    run_ok("go>n;x=5;y=+x 1;y");
}
