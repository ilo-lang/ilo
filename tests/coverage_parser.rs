//! Coverage-oriented tests for `src/parser/mod.rs`.
//!
//! These exercise the broad set of parse rules, error-recovery arms,
//! prefix/infix operator edges, lambda forms, record/with/match patterns,
//! and type-decl shapes that the regression suite skips. The goal is
//! coverage, not behaviour assertions — most cases just confirm that the
//! parser either accepts the input or returns the expected diagnostic code
//! without panicking.

use ilo::ast::Span;
use ilo::lexer;
use ilo::parser::{self, ParseError};

fn lex_to_pairs(src: &str) -> Vec<(lexer::Token, Span)> {
    let tokens = lexer::lex(src).expect("lex failed");
    tokens
        .into_iter()
        .map(|(t, r)| {
            (
                t,
                Span {
                    start: r.start,
                    end: r.end,
                },
            )
        })
        .collect()
}

fn try_parse(src: &str) -> (usize, Vec<ParseError>) {
    let pairs = lex_to_pairs(src);
    let (prog, errs) = parser::parse(pairs);
    (prog.declarations.len(), errs)
}

fn ok(src: &str) {
    let (_n, errs) = try_parse(src);
    assert!(errs.is_empty(), "expected ok for {src:?}, got {errs:?}");
}

fn fail(src: &str) -> Vec<ParseError> {
    let (_n, errs) = try_parse(src);
    assert!(!errs.is_empty(), "expected error for {src:?}");
    errs
}

fn fail_code(src: &str, code: &str) {
    let errs = fail(src);
    assert!(
        errs.iter().any(|e| e.code == code),
        "expected {code} for {src:?}, got {:?}",
        errs.iter().map(|e| e.code).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Decls and headers
// ---------------------------------------------------------------------------

#[test]
fn type_decl_with_multiple_fields() {
    ok("type point{x:n;y:n;label:t}");
}

#[test]
fn type_decl_empty_body() {
    ok("type empty{}");
}

#[test]
fn tool_decl_basic() {
    ok("tool fetch \"fetch url\" url:t>R t t");
}

#[test]
fn tool_decl_with_timeout_and_retry() {
    ok("tool fetch \"d\" url:t>R t t timeout:30,retry:3");
}

#[test]
fn tool_decl_with_only_timeout() {
    ok("tool fetch \"d\" url:t>R t t timeout:30");
}

#[test]
fn tool_decl_missing_description() {
    fail_code("tool fetch url:t>R t t", "ILO-P015");
}

#[test]
fn alias_decl() {
    ok("alias age n");
}

#[test]
fn use_decl_plain() {
    ok("use \"lib/foo.ilo\"");
}

#[test]
fn use_decl_with_names() {
    ok("use \"lib/foo.ilo\" [a b c]");
}

#[test]
fn use_decl_with_empty_names_fails() {
    fail_code("use \"lib/foo.ilo\" []", "ILO-P016");
}

#[test]
fn use_decl_missing_path() {
    fail_code("use 42", "ILO-P016");
}

#[test]
fn use_decl_eof_after_use() {
    fail_code("use", "ILO-P016");
}

#[test]
fn reserved_keyword_as_decl_var() {
    fail_code("var=5", "ILO-P011");
}

#[test]
fn reserved_keyword_as_decl_let() {
    fail_code("let=5", "ILO-P011");
}

#[test]
fn reserved_keyword_as_decl_const() {
    fail_code("const=5", "ILO-P011");
}

#[test]
fn reserved_keyword_as_decl_if() {
    fail_code("if=5", "ILO-P011");
}

#[test]
fn reserved_keyword_as_decl_fn() {
    fail_code("fn=5", "ILO-P011");
}

#[test]
fn reserved_keyword_as_decl_def() {
    fail_code("def=5", "ILO-P011");
}

#[test]
fn reserved_keyword_as_decl_return() {
    fail_code("return=5", "ILO-P011");
}

#[test]
fn cnt_as_decl_name() {
    fail_code("cnt=5", "ILO-P011");
}

#[test]
fn brk_as_decl_name() {
    fail_code("brk=5", "ILO-P011");
}

#[test]
fn fld_as_decl_name() {
    fail_code("fld=5", "ILO-P011");
}

#[test]
fn builtin_as_decl_name() {
    fail_code("map=5", "ILO-P011");
}

#[test]
fn builtin_as_fn_name() {
    fail_code("len x:t>n;42", "ILO-P011");
}

#[test]
fn ident_keyword_function_hint() {
    fail_code("function foo>n;42", "ILO-P001");
}

#[test]
fn ident_keyword_def_hint() {
    fail_code("def foo>n;42", "ILO-P001");
}

#[test]
fn ident_keyword_let_hint() {
    fail_code("let foo=5", "ILO-P001");
}

#[test]
fn ident_keyword_return_hint() {
    fail_code("return foo", "ILO-P001");
}

#[test]
fn ident_keyword_if_hint() {
    fail_code("if foo", "ILO-P001");
}

#[test]
fn decl_starts_with_operator_plus() {
    fail_code("+1 2", "ILO-P001");
}

#[test]
fn decl_starts_with_operator_minus() {
    fail_code("-1 2", "ILO-P001");
}

#[test]
fn decl_starts_with_kw_token_fn() {
    fail_code("fn x:n>n;42", "ILO-P001");
}

#[test]
fn decl_eof() {
    // Empty program should produce no decls and no errors.
    let (n, errs) = try_parse("");
    assert_eq!(n, 0);
    assert!(errs.is_empty());
}

#[test]
fn fn_decl_no_params() {
    ok("main>n;42");
}

#[test]
fn fn_decl_colon_before_greater_hint() {
    fail_code("main:>n;42", "ILO-P003");
}

#[test]
fn fn_decl_brace_body() {
    ok("main>n;{42}");
}

#[test]
fn fn_decl_brace_body_multiple_stmts() {
    ok("main>n;{x=1;y=2;+x y}");
}

#[test]
fn fn_decl_dash_arrow_hint() {
    fail_code("main->n;42", "ILO-P003");
}

#[test]
fn fn_decl_indented_body_hint() {
    // `;` instead of `{` after a foreach head — should suggest brace/single-line
    fail_code("main>n;@x [1 2 3];+x 1", "ILO-P003");
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[test]
fn type_named_user_type() {
    ok("type p{x:n}\nf x:p>n;42");
}

#[test]
fn type_optional() {
    ok("f x:O n>n;42");
}

#[test]
fn type_list() {
    ok("f x:Ln>n;42");
}

#[test]
fn type_map() {
    ok("f x:Mt n>n;42");
}

#[test]
fn type_result() {
    ok("f>R n t;~42");
}

#[test]
fn type_sum() {
    ok("f x:S a b c>n;42");
}

#[test]
fn type_fn() {
    ok("f x:F n n>n;42");
}

#[test]
fn type_paren_grouped() {
    ok("f x:(n)>n;42");
}

#[test]
fn type_underscore() {
    ok("f x:_>n;42");
}

#[test]
fn type_sum_empty_fails() {
    fail_code("f x:S>n;42", "ILO-P010");
}

#[test]
fn type_fn_no_return_fails() {
    fail_code("f x:F>n;42", "ILO-P009");
}

#[test]
fn type_unknown_token() {
    // Token in type position that isn't a type
    fail_code("f x:>n;42", "ILO-P007");
}

#[test]
fn type_eof_in_type() {
    fail_code("f x:", "ILO-P008");
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

#[test]
fn stmt_let_simple() {
    ok("f>n;x=42;x");
}

#[test]
fn stmt_let_map_literal_attempt() {
    fail_code("f>n;x={};x", "ILO-P009");
}

#[test]
fn stmt_let_map_literal_with_text() {
    fail_code("f>n;x={\"k\" 1};x", "ILO-P009");
}

#[test]
fn stmt_let_ternary_brace_brace() {
    ok("f x:n>n;y=>x 0{1}{2};y");
}

#[test]
fn stmt_let_conditional_brace() {
    ok("f x:n>n;y=>x 0{42};y");
}

#[test]
fn stmt_destructure() {
    ok("type p{a:n;b:n}\nf x:p>n;{a;b}=x;+a b");
}

#[test]
fn stmt_match_no_subject() {
    ok("f>n;?{~v:v;^e:0;_:1}");
}

#[test]
fn stmt_match_with_subject() {
    ok("f x:n>n;?x{0:1;_:2}");
}

#[test]
fn stmt_match_bare_bool_ternary_brace() {
    ok("f h:b>n;?h{1}{0}");
}

#[test]
fn stmt_match_bare_bool_prefix_ternary() {
    ok("f h:b>n;?h 1 0");
}

#[test]
fn stmt_match_brace_body_hint() {
    fail_code("f h:b>n;?h{x=1;+x 2}", "ILO-P011");
}

#[test]
fn stmt_foreach() {
    ok("f xs:Ln>n;@x xs{+x 1};0");
}

#[test]
fn stmt_for_range() {
    ok("f>n;@i 0..10{+i 1};0");
}

#[test]
fn stmt_while() {
    ok("f>n;x=0;wh <x 10{x=+x 1};x");
}

#[test]
fn stmt_bang_negated_guard() {
    ok("f x:n>n;!=x 0{1};x");
}

#[test]
fn stmt_bang_negated_guard_else() {
    ok("f x:n>n;!=x 0{1}{2}");
}

#[test]
fn stmt_caret_err_stmt() {
    ok("f>R n t;^\"oops\"");
}

#[test]
fn stmt_ret() {
    ok("f x:n>n;ret +x 1");
}

#[test]
fn stmt_brk_no_value() {
    ok("f>n;@i 0..10{brk};0");
}

#[test]
fn stmt_brk_with_value() {
    ok("f>n;@i 0..10{brk i};0");
}

#[test]
fn stmt_brk_eq_rejected() {
    fail_code("f>n;brk=5;brk", "ILO-P011");
}

#[test]
fn stmt_cnt_eq_rejected() {
    fail_code("f>n;cnt=5;cnt", "ILO-P011");
}

#[test]
fn stmt_fld_eq_rejected() {
    fail_code("f>n;fld=5;fld", "ILO-P011");
}

#[test]
fn stmt_cnt_in_loop() {
    ok("f>n;@i 0..10{cnt};0");
}

#[test]
fn stmt_builtin_let_rejected() {
    fail_code("f>n;flat=5;flat", "ILO-P011");
}

#[test]
fn stmt_var_in_body_rejected() {
    fail_code("f>n;var=5;var", "ILO-P011");
}

#[test]
fn stmt_guard_braced() {
    ok("f x:n>n;>x 0{1};x");
}

#[test]
fn stmt_guard_else() {
    ok("f x:n>n;>x 0{1}{2}");
}

#[test]
fn stmt_braceless_guard() {
    ok("f x:n>n;>=x 10 \"big\";\"small\"");
}

#[test]
fn stmt_braceless_negated_guard() {
    ok("f x:n>n;!>=x 10 \"small\";\"big\"");
}

#[test]
fn stmt_braceless_guard_dangling_token() {
    // `>=x 10 call extra` shape should surface dangling-token error
    fail_code("f x:n>n;>=x 10 classify x;0", "ILO-P016");
}

// ---------------------------------------------------------------------------
// Patterns
// ---------------------------------------------------------------------------

#[test]
fn pattern_err_bind() {
    ok("f x:R n t>t;?x{~v:\"ok\";^e:e}");
}

#[test]
fn pattern_err_wildcard() {
    ok("f x:R n t>t;?x{~v:\"ok\";^_:\"err\"}");
}

#[test]
fn pattern_ok_wildcard() {
    ok("f x:R n t>n;?x{~_:1;^e:0}");
}

#[test]
fn pattern_wildcard() {
    ok("f x:n>n;?x{_:1}");
}

#[test]
fn pattern_number_literal() {
    ok("f x:n>n;?x{0:1;_:2}");
}

#[test]
fn pattern_text_literal() {
    ok("f x:t>n;?x{\"hi\":1;_:0}");
}

#[test]
fn pattern_bool_true() {
    ok("f x:b>n;?x{true:1;false:0}");
}

#[test]
fn pattern_nil() {
    ok("f x:O n>n;?x{nil:0;n v:v}");
}

#[test]
fn pattern_typeis_text() {
    ok("f x:_>n;?x{t s:1;_:0}");
}

#[test]
fn pattern_typeis_bool() {
    ok("f x:_>n;?x{b v:1;_:0}");
}

#[test]
fn pattern_typeis_list() {
    ok("f x:_>n;?x{l v:1;_:0}");
}

#[test]
fn pattern_typeis_wildcard_binding() {
    ok("f x:_>n;?x{n _:1;_:0}");
}

#[test]
fn pattern_unknown_token() {
    fail_code("f x:n>n;?x{>:1}", "ILO-P011");
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

#[test]
fn expr_literal_number() {
    ok("f>n;42");
}

#[test]
fn expr_literal_text() {
    ok("f>t;\"hi\"");
}

#[test]
fn expr_literal_bool() {
    ok("f>b;true");
    ok("f>b;false");
}

#[test]
fn expr_nil() {
    ok("f>O n;nil");
}

#[test]
fn expr_underscore_ref() {
    ok("f x:n>n;_=x;42");
}

#[test]
fn expr_paren_group() {
    ok("f x:n>n;(+x 1)");
}

#[test]
fn expr_list_whitespace() {
    ok("f>Ln;[1 2 3]");
}

#[test]
fn expr_list_comma_separated() {
    ok("f>Ln;[1, 2, 3]");
}

#[test]
fn expr_list_with_calls_comma() {
    ok("f x:n>Ln;[flr x, cel x]");
}

#[test]
fn expr_list_empty() {
    ok("f>Ln;[]");
}

#[test]
fn expr_list_semi_inside_fails() {
    fail_code("f>Ln;[1;2;3]", "ILO-P009");
}

#[test]
fn expr_zero_arg_call() {
    ok("g>n;42;f>n;g()");
}

#[test]
fn expr_zero_arg_call_bang() {
    ok("g>R n t;~42;f>n;g!()");
}

#[test]
fn expr_zero_arg_call_bangbang() {
    ok("g>R n t;~42;f>n;g!!()");
}

#[test]
fn expr_field_access() {
    ok("type p{x:n}\nf q:p>n;q.x");
}

#[test]
fn expr_safe_field_access() {
    ok("type p{x:n}\nf q:O p>O n;q.?x");
}

#[test]
fn expr_field_dot_paren_hint() {
    fail_code("f xs:Ln i:n>n;xs.(+i 1)", "ILO-P005");
}

#[test]
fn expr_index_numeric() {
    ok("f xs:Ln>n;xs.0");
}

#[test]
fn expr_unary_negate() {
    ok("f x:n>n;-x");
}

#[test]
fn expr_binary_subtract() {
    ok("f a:n b:n>n;-a b");
}

#[test]
fn expr_double_minus_trap() {
    fail_code("f gl:n s:n b:n om:n>n;- -*gl s *b om", "ILO-P021");
}

#[test]
fn expr_prefix_plus() {
    ok("f a:n b:n>n;+a b");
}

#[test]
fn expr_prefix_times() {
    ok("f a:n b:n>n;*a b");
}

#[test]
fn expr_prefix_divide() {
    ok("f a:n b:n>n;/a b");
}

#[test]
fn expr_prefix_and() {
    ok("f a:b b:b>b;&a b");
}

#[test]
fn expr_prefix_or() {
    ok("f a:b b:b>b;|a b");
}

#[test]
fn expr_prefix_neq() {
    ok("f a:n b:n>b;!=a b");
}

#[test]
fn expr_prefix_append() {
    ok("f xs:Ln ys:Ln>Ln;+=xs ys");
}

#[test]
fn expr_prefix_amp_amp_rejected() {
    fail_code("f a:b b:b>b;&&a b", "ILO-P003");
}

#[test]
fn expr_prefix_pipe_pipe_rejected() {
    fail_code("f a:b b:b>b;||a b", "ILO-P003");
}

#[test]
fn expr_compound_eq_less_hint() {
    fail_code("f d:n>n;=<d 0{1};0", "ILO-P003");
}

#[test]
fn expr_compound_eq_greater_hint() {
    fail_code("f d:n>n;=>d 0{1};0", "ILO-P003");
}

#[test]
fn expr_prefix_chain_arity() {
    fail_code("f a:n b:n c:n>n;+++a b c", "ILO-P003");
}

#[test]
fn expr_nested_prefix_chain() {
    ok("f a:n b:n c:n>n;+*a b c");
}

#[test]
fn expr_infix_chain() {
    ok("f a:n b:n c:n>n;a+b*c");
}

#[test]
fn expr_infix_and_or() {
    ok("f a:b b:b c:b>b;a&b|c");
}

#[test]
fn expr_infix_comparison() {
    ok("f a:n b:n>b;a<b");
}

#[test]
fn expr_nil_coalesce_infix() {
    ok("f x:O n>n;x??0");
}

#[test]
fn expr_nil_coalesce_prefix() {
    ok("f m:Mt n k:t>n;??mget m k 0");
}

#[test]
fn expr_with_update() {
    ok("type p{x:n;y:n}\nf q:p>p;q with x:1");
}

#[test]
fn expr_pipe() {
    ok("f xs:Ln>n;xs >> sum");
}

#[test]
fn expr_pipe_with_args() {
    ok("dbl x:n>n;*x 2;f xs:Ln>Ln;xs >> map dbl");
}

#[test]
fn expr_dollar_get() {
    ok("f m:M t n k:t>O n;mget m k");
}

#[test]
fn expr_dollar_bang() {
    // Post-0.12.0 `$` is rebound to `run` (2-arg), not `get` (1-arg).
    ok(r#"f>M t t;$!"echo" ["hi"]"#);
}

#[test]
fn expr_ok_wrap() {
    ok("f>R n t;~42");
}

#[test]
fn expr_err_wrap() {
    ok("f>R n t;^\"oops\"");
}

#[test]
fn expr_ternary_question_eq() {
    ok("f x:n>n;?=x 0 1 2");
}

#[test]
fn expr_match_in_expr_position() {
    ok("f x:n>n;y=?x{0:1;_:2};y");
}

#[test]
fn expr_match_with_braces_no_subject() {
    ok("f x:R n t>n;y=?{~v:v;^_:0};y");
}

#[test]
fn expr_call_with_known_arity() {
    ok("f xs:Ln>n;len xs");
}

#[test]
fn expr_record_construction() {
    ok("type p{x:n;y:n}\nf>p;p x:1 y:2");
}

#[test]
fn expr_inline_lambda() {
    ok("f xs:Ln>Ln;map (x:n>n;+x 1) xs");
}

#[test]
fn expr_inline_lambda_zero_param() {
    ok("f>F n;(>n;42)");
}

#[test]
fn expr_inline_lambda_with_capture() {
    ok("f xs:Ln k:n>Ln;map (x:n>n;+x k) xs");
}

#[test]
fn expr_inline_lambda_dangling_idents_hint() {
    let errs = fail("f xs:Ln>Ln;map (a:n>n;+a len xs body x) xs");
    assert!(
        errs.iter().any(|e| e.code == "ILO-P003"),
        "expected ILO-P003, got {:?}",
        errs.iter().map(|e| e.code).collect::<Vec<_>>()
    );
}

#[test]
fn expr_fmt_in_wrong_slot_rejected() {
    fail_code("f x:n>t;wr fmt \"x={}\" x \"file.txt\"", "ILO-P018");
}

#[test]
fn expr_fn_kw_in_expr_rejected() {
    fail_code("f>n;x=fn;x", "ILO-P009");
}

#[test]
fn expr_def_kw_in_expr_rejected() {
    fail_code("f>n;x=def;x", "ILO-P009");
}

#[test]
fn expr_unexpected_token_in_expr() {
    fail_code("f>n;x=;x", "ILO-P009");
}

#[test]
fn expr_eof_mid_expression() {
    fail_code("f>n;x=", "ILO-P010");
}

#[test]
fn expr_call_chain_with_bang() {
    ok("f x:O n>n;x!");
}

#[test]
fn expr_call_chain_with_bangbang() {
    ok("f x:O n>n;x!!");
}

#[test]
fn expr_paren_call() {
    ok("f x:n>n;(+x 1)");
}

#[test]
fn expr_paren_unclosed() {
    fail_code("f>n;(1", "ILO-P004");
}

#[test]
fn expr_list_unclosed() {
    fail_code("f>Ln;[1 2", "ILO-P010");
}

// ---------------------------------------------------------------------------
// Error recovery / sync
// ---------------------------------------------------------------------------

#[test]
fn multiple_decls_error_recovery() {
    let (_n, errs) = try_parse("foo:>n;42\nbar>n;42");
    assert!(!errs.is_empty(), "expected first decl error");
}

#[test]
fn stray_brace_top_level() {
    let (_n, errs) = try_parse("}");
    assert!(!errs.is_empty());
}

#[test]
fn many_errors_capped() {
    // Generate junk to verify the parser caps errors and doesn't panic.
    let mut src = String::new();
    for _ in 0..40 {
        src.push_str("var=5\n");
    }
    let (_n, errs) = try_parse(&src);
    assert!(!errs.is_empty());
    assert!(errs.len() <= 30);
}

#[test]
fn type_decl_field_missing_colon() {
    fail("type p{x;y:n}");
}

#[test]
fn fn_decl_eof_in_header() {
    fail_code("foo a:n", "ILO-P020");
}

#[test]
fn fn_decl_eof_mid_params() {
    fail("foo a:");
}

// ---------------------------------------------------------------------------
// parse_tokens (test helper) — keep coverage on this thin wrapper too.
// ---------------------------------------------------------------------------

#[test]
fn parse_tokens_ok_round_trip() {
    let pairs = lex_to_pairs("main>n;42");
    let (prog, errs) = parser::parse(pairs);
    assert!(errs.is_empty());
    assert_eq!(prog.declarations.len(), 1);
}

// ---------------------------------------------------------------------------
// Misc: zero-arg builtins, dot keywords, fld in arg position
// ---------------------------------------------------------------------------

#[test]
fn zero_arg_builtin_now() {
    ok("f>n;now");
}

#[test]
fn zero_arg_builtin_rnd() {
    ok("f>n;rnd");
}

#[test]
fn zero_arg_builtin_mmap_in_arg() {
    ok("f>Mt n;mset mmap \"k\" 1");
}

#[test]
fn fld_as_arg_position() {
    ok("f xs:Ln>n;fld (a:n x:n>n;+a x) xs 0");
}

// ---------------------------------------------------------------------------
// More edge cases
// ---------------------------------------------------------------------------

#[test]
fn match_expr_prefix_ternary_greater() {
    ok("f x:n>n;?>x 0 1 2");
}

#[test]
fn match_expr_prefix_ternary_less() {
    ok("f x:n>n;?<x 0 1 2");
}

#[test]
fn match_expr_prefix_ternary_neq() {
    ok("f x:n>n;?!=x 0 1 2");
}

#[test]
fn match_expr_prefix_ternary_geq() {
    ok("f x:n>n;?>=x 0 1 2");
}

#[test]
fn match_expr_prefix_ternary_leq() {
    ok("f x:n>n;?<=x 0 1 2");
}

#[test]
fn match_brace_ternary_in_expr_position() {
    ok("f h:b>n;y=?h{1}{0};y");
}

#[test]
fn use_decl_unclosed_brackets() {
    fail_code("use \"x.ilo\" [a b", "ILO-P016");
}

#[test]
fn expr_safe_field_chain() {
    ok("type p{x:n}\nf q:O p>O n;q.?x");
}

#[test]
fn expr_field_chain_two_deep() {
    ok("type a{x:n}\ntype b{a:a}\nf q:b>n;q.a.x");
}

#[test]
fn fn_decl_with_optional_result_param() {
    ok("f x:O n>O n;x");
}

#[test]
fn type_nested_list_optional() {
    ok("f x:L (O n)>O n;hd x");
}

#[test]
fn fn_decl_underscore_type() {
    ok("f x:_>_;x");
}

#[test]
fn fn_decl_paren_type() {
    ok("f x:(Ln)>n;len x");
}

#[test]
fn expr_ok_in_expr() {
    ok("f x:n>R n t;y=~x;y");
}

#[test]
fn expr_err_in_expr() {
    ok("f>R n t;y=^\"e\";y");
}

#[test]
fn expr_with_zero_updates() {
    ok("type p{x:n}\nf q:p>p;q with");
}

#[test]
fn expr_with_multiple_updates() {
    ok("type p{x:n;y:n}\nf q:p>p;q with x:1 y:2");
}

#[test]
fn expr_nil_coalesce_chained() {
    ok("f a:O n b:O n>O n;a??b??0");
}

#[test]
fn expr_pipe_chained() {
    ok("f xs:Ln>n;xs >> rev >> sum");
}

#[test]
fn expr_call_then_infix() {
    ok("f xs:Ln>n;+sum xs 1");
}

#[test]
fn expr_record_in_record() {
    ok("type a{x:n}\ntype b{inner:a}\nf>b;b inner:(a x:1)");
}

#[test]
fn expr_list_with_paren_call() {
    ok("dbl x:n>n;*x 2;f xs:Ln>Ln;[(dbl 1) (dbl 2)]");
}

#[test]
fn expr_list_of_known_calls_whitespace() {
    ok("dbl x:n>n;*x 2;f xs:Ln>Ln;[dbl 1 dbl 2]");
}

#[test]
fn stmt_destructure_one_field() {
    ok("type p{a:n}\nf x:p>n;{a}=x;a");
}

#[test]
fn stmt_match_with_brace_arm_body() {
    ok("f x:n>n;?x{0:{a=1;a};_:{b=2;b}}");
}

#[test]
fn stmt_match_typeis_arm_body_inline() {
    ok("f x:_>n;?x{n v:v;t s:0;_:0}");
}

#[test]
fn expr_unary_negate_in_call() {
    ok("f x:n>n;abs -x");
}

#[test]
fn expr_paren_call_nested() {
    ok("f x:n>n;abs (+x 1)");
}

#[test]
fn expr_call_with_neg_number_literal() {
    ok("f>n;abs -5");
}

#[test]
fn type_in_nested_f_chain() {
    ok("f g:F n n>F n n;g");
}

#[test]
fn fn_decl_body_with_match_arms_brace_form() {
    ok("f x:n>n;?x{0:1;_:2}");
}

#[test]
fn caret_in_expr_position() {
    ok("f>R n t;e=^\"x\";e");
}

#[test]
fn tilde_in_expr_position() {
    ok("f x:n>R n t;~x");
}

#[test]
fn brk_in_loop_with_complex_value() {
    ok("f>n;@i 0..10{>=i 5{brk +i 1}};0");
}

#[test]
fn while_with_braceless_body_in_brace() {
    ok("f>n;x=0;wh <x 5{x=+x 1;x}");
}

#[test]
fn ret_with_complex_expr() {
    ok("f x:n>n;ret +x *x 2");
}

#[test]
fn deep_nested_match() {
    ok("f x:n y:n>n;?x{0:?y{0:1;_:2};_:0}");
}

#[test]
fn lambda_with_multiline_body() {
    ok("f xs:Ln>Ln;map (x:n>n;y=+x 1;*y 2) xs");
}

#[test]
fn lambda_with_match_in_body() {
    ok("f xs:Ln>Ln;map (x:n>n;?x{0:1;_:x}) xs");
}

#[test]
fn lambda_with_let_then_use() {
    ok("f xs:Ln>Ln;map (x:n>n;y=+x 1;y) xs");
}

#[test]
fn lambda_with_foreach_inside() {
    ok("f xs:LLn>Ln;map (ys:Ln>n;s=0;@y ys{s=+s y};s) xs");
}

#[test]
fn lambda_with_forrange_inside() {
    ok("f xs:Ln>Ln;map (n:n>n;s=0;@i 0..n{s=+s i};s) xs");
}

#[test]
fn lambda_with_while_inside() {
    ok("f xs:Ln>Ln;map (n:n>n;s=0;@i 0..n{s=+s i};s) xs");
}

#[test]
fn lambda_with_destructure() {
    ok("type p{a:n;b:n}\nf xs:L p>L n;map (q:p>n;{a;b}=q;+a b) xs");
}

#[test]
fn lambda_with_break_value() {
    ok("f xs:Ln>Ln;map (n:n>n;s=0;@i 0..n{>=i 5{brk s};s=+s i};s) xs");
}

#[test]
fn lambda_with_continue() {
    ok("f xs:Ln>Ln;map (n:n>n;s=0;@i 0..n{>=i 5{cnt};s=+s i};s) xs");
}

#[test]
fn lambda_with_return() {
    ok("f xs:Ln>Ln;map (n:n>n;>=n 5{ret 0};n) xs");
}

#[test]
fn nested_lambdas_capture() {
    ok("f xs:LLn k:n>LLn;map (ys:Ln>Ln;map (y:n>n;+y k) ys) xs");
}

#[test]
fn pattern_text_arm() {
    ok("f x:t>n;?x{\"a\":1;\"b\":2;_:0}");
}

#[test]
fn pattern_bool_false_arm() {
    ok("f x:b>n;?x{false:0;true:1}");
}

#[test]
fn expr_ternary_in_let() {
    ok("f x:n>n;y=>x 0{1}{2};y");
}

#[test]
fn negated_braceless_guard_call_dangling() {
    fail_code("f x:n>n;!=x 0 classify x;0", "ILO-P016");
}

#[test]
fn expr_negated_unary_not() {
    ok("f x:b>b;!x");
}

#[test]
fn expr_double_not() {
    // `!!` lexes as a single BangBang panic-unwrap token, so write `! !x` with a space.
    ok("f x:b>b;! !x");
}

#[test]
fn expr_ok_then_pipe() {
    ok("f x:n>R n t;~x >> sum");
}

#[test]
fn expr_call_with_text_arg() {
    ok("f>n;len \"hi\"");
}

#[test]
fn use_decl_after_use_path_extra_tokens_ok() {
    ok("use \"x.ilo\"\nmain>n;42");
}

#[test]
fn fn_decl_recursive_arity_registered() {
    ok("fac n:n>n;?=n 0 1 (*n fac -n 1)");
}

#[test]
fn match_arm_no_subject_with_typeis() {
    ok("f x:_>n;?{n v:v;t _:0;_:1}");
}

#[test]
fn match_with_ok_typed_arm() {
    ok("f x:R n t>n;?x{~v:v;^_:0}");
}

#[test]
fn paren_inline_lambda_in_pipe_target() {
    ok("f xs:Ln>Ln;xs >> map (x:n>n;+x 1)");
}

#[test]
fn deep_prefix_with_mixed_ops() {
    ok("f a:n b:n c:n d:n>n;+-a b *c d");
}

#[test]
fn list_with_negative_numbers() {
    ok("f>Ln;[-1 -2 -3]");
}

#[test]
fn tool_decl_bad_timeout_value() {
    fail_code("tool f \"d\" url:t>R t t timeout:abc", "ILO-P013");
}

#[test]
fn tool_decl_eof_after_timeout_colon() {
    fail_code("tool f \"d\" url:t>R t t timeout:", "ILO-P014");
}

#[test]
fn fn_decl_with_optional_decl_in_body() {
    // Two decls separated by a top-level newline. Parser should accept.
    let (_, errs) = try_parse("g x:n>O n;x\nf y:n>n;g y");
    assert!(errs.is_empty(), "errors: {errs:?}");
}

#[test]
fn while_loop_with_brk_value() {
    ok("f>n;x=0;wh <x 10{>=x 5{brk x};x=+x 1};x");
}

#[test]
fn nested_braced_guards() {
    ok("f x:n>n;>x 0{>x 10{1}{2}}{3}");
}

#[test]
fn many_consecutive_dots() {
    ok("type a{b:_}\ntype c{a:a}\nf q:c>_;q.a.b");
}

#[test]
fn paren_in_argument() {
    ok("f x:n>n;abs (+x 1)");
}

#[test]
fn record_in_arg_position() {
    ok("type p{x:n}\nid q:p>p;q\nf>p;id (p x:1)");
}
