/// --explain / -x: annotate a program showing the expanded (indented) code
/// with structural roles on the right.
///
/// Output format:
///
///   fac n:n>n              fn start
///      <=n 1 1             guard
///      r=fac -n 1          bind → r
///      *n r                return
///
///   fib n:n>n              fn start
///      <=n 1 n             guard
///      a=fib -n 1          bind → a
///      b=fib -n 2          bind → b
///      +a b                return
use crate::ast::{Decl, Param, Program, Stmt, Type};

pub fn explain(program: &Program, filename: Option<&str>) -> String {
    let source = program.source.as_deref().unwrap_or("");
    let mut out = String::new();
    if let Some(name) = filename {
        out.push_str(&format!("file: {name}\n\n"));
    }
    let mut first = true;

    for decl in &program.declarations {
        // Compute the snippet to append for this declaration, or None to skip it.
        let snippet: Option<String> = match decl {
            // Resolved before codegen / poison nodes — skip silently
            Decl::Use { .. } | Decl::Error { .. } => None,

            Decl::Function { name, params, return_type, body, .. } => {
                let sig = if params.is_empty() {
                    format!("{}>{}", name, fmt_type(return_type))
                } else {
                    format!("{} {}>{}", name, fmt_params_sig(params), fmt_type(return_type))
                };

                // Collect all (code, role, indent) lines so we can compute a shared column
                let mut lines: Vec<(String, String, usize)> = Vec::new();
                lines.push((sig, "fn start".into(), 0));
                for p in params {
                    lines.push((format!("{}:{}", p.name, fmt_type(&p.ty)), format!("param → {}", fmt_type_long(&p.ty)), 3));
                }
                lines.push((format!(">{}", fmt_type(return_type)), format!("returns {}", fmt_type_long(return_type)), 3));
                let n = body.len();
                for (i, spanned) in body.iter().enumerate() {
                    let is_last = i == n - 1;
                    let src = extract(source, spanned.span.start, spanned.span.end).to_string();
                    let role = role_of(&spanned.node, is_last);
                    lines.push((src, role, 3));
                }

                // Comment column = max(indent + code_len) + 2 gap, minimum 22
                let col = lines.iter()
                    .map(|(code, _, indent)| indent + code.chars().count())
                    .max()
                    .unwrap_or(0)
                    .max(20) + 2;

                let mut s = String::new();
                for (code, role, indent) in &lines {
                    s.push_str(&annotate_line_col(code, role, *indent, col));
                }
                Some(s)
            }

            Decl::TypeDef { name, fields, .. } => {
                let fields_str = fields.iter()
                    .map(|f| format!("{}:{}", f.name, fmt_type(&f.ty)))
                    .collect::<Vec<_>>()
                    .join("; ");
                Some(annotate_line(&format!("type {name} {{{fields_str}}}"), "type def", 0))
            }

            Decl::Tool { name, params, return_type, .. } => {
                let sig = format!("@{} {}>{}", name, fmt_params_sig(params), fmt_type(return_type));
                Some(annotate_line(&sig, "tool", 0))
            }

            Decl::Alias { name, target, .. } => {
                Some(annotate_line(&format!("alias {name}={}", fmt_type(target)), "alias", 0))
            }
        };

        if let Some(s) = snippet {
            if !first { out.push('\n'); }
            first = false;
            out.push_str(&s);
        }
    }

    out
}

/// Format one annotated line with an explicit comment column.
fn annotate_line_col(code: &str, role: &str, indent: usize, col: usize) -> String {
    let used = indent + code.chars().count();
    let pad = if used < col { col - used } else { 1 };
    format!("{}{}{}-- {}\n", " ".repeat(indent), code, " ".repeat(pad), role)
}

/// Format a single-line decl with auto column.
fn annotate_line(code: &str, role: &str, indent: usize) -> String {
    let col = (indent + code.chars().count()).max(20) + 2;
    annotate_line_col(code, role, indent, col)
}

fn role_of(stmt: &Stmt, is_last: bool) -> String {
    match stmt {
        Stmt::Let { name, .. }        => format!("bind → {name}"),
        Stmt::Guard { negated, else_body, .. } => {
            if else_body.is_some() {
                if *negated { "ternary !".into() } else { "ternary".into() }
            } else if *negated { "guard !".into() } else { "guard".into() }
        }
        Stmt::Match { .. }            => "match".into(),
        Stmt::ForEach { binding, .. } => format!("foreach → {binding}"),
        Stmt::ForRange { binding, .. }=> format!("for range → {binding}"),
        Stmt::While { .. }            => "while".into(),
        Stmt::Return(_)               => "ret".into(),
        Stmt::Break(Some(_))          => "break (value)".into(),
        Stmt::Break(None)             => "break".into(),
        Stmt::Continue                => "continue".into(),
        Stmt::Destructure { bindings, .. } => format!("destructure → {}", bindings.join(", ")),
        Stmt::Expr(_) => {
            if is_last { "return".into() } else { "expr".into() }
        }
    }
}

fn fmt_params_sig(params: &[Param]) -> String {
    params.iter()
        .map(|p| format!("{}:{}", p.name, fmt_type(&p.ty)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn fmt_type_long(ty: &Type) -> String {
    match ty {
        Type::Number          => "number".into(),
        Type::Text            => "text".into(),
        Type::Bool            => "bool".into(),
        Type::Any             => "any".into(),
        Type::Optional(inner) => format!("optional {}", fmt_type_long(inner)),
        Type::List(inner)     => format!("list of {}", fmt_type_long(inner)),
        Type::Map(k, v)       => format!("map of {} to {}", fmt_type_long(k), fmt_type_long(v)),
        Type::Sum(vs)         => format!("one of: {}", vs.join(", ")),
        Type::Result(ok, err) => format!("Result ok={} err={}", fmt_type_long(ok), fmt_type_long(err)),
        Type::Fn(params, ret) => {
            let ps: Vec<_> = params.iter().map(fmt_type_long).collect();
            format!("fn({}) → {}", ps.join(", "), fmt_type_long(ret))
        }
        Type::Named(name)     => name.clone(),
    }
}

fn fmt_type(ty: &Type) -> String {
    match ty {
        Type::Number          => "n".into(),
        Type::Text            => "t".into(),
        Type::Bool            => "b".into(),
        Type::Any             => "_".into(),
        Type::Optional(inner) => format!("O {}", fmt_type(inner)),
        Type::List(inner)     => format!("L {}", fmt_type(inner)),
        Type::Map(k, v)       => format!("M {} {}", fmt_type(k), fmt_type(v)),
        Type::Sum(vs)         => format!("S {}", vs.join(" ")),
        Type::Result(ok, err) => format!("R {} {}", fmt_type(ok), fmt_type(err)),
        Type::Fn(params, ret) => {
            let mut s = "F".to_string();
            for p in params { s.push(' '); s.push_str(&fmt_type(p)); }
            s.push(' '); s.push_str(&fmt_type(ret));
            s
        }
        Type::Named(name)     => name.clone(),
    }
}

fn extract(source: &str, start: usize, end: usize) -> &str {
    source.get(start..end).unwrap_or("?").trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_prog(src: &str) -> Program {
        let tokens = crate::lexer::lex(src).unwrap();
        let token_spans: Vec<(crate::lexer::Token, crate::ast::Span)> =
            tokens.into_iter().map(|(t, r)| (t, crate::ast::Span { start: r.start, end: r.end })).collect();
        let (mut prog, _) = crate::parser::parse(token_spans);
        prog.source = Some(src.to_string());
        prog
    }

    #[test]
    fn explain_fn_start_annotation() {
        let prog = parse_prog("f x:n>n;+x 1");
        let out = explain(&prog, None);
        assert!(out.contains("fn start"), "missing 'fn start': {out}");
    }

    #[test]
    fn explain_param_annotation() {
        let prog = parse_prog("f x:n>n;+x 1");
        let out = explain(&prog, None);
        assert!(out.contains("param →"), "missing 'param →': {out}");
    }

    #[test]
    fn explain_returns_annotation() {
        let prog = parse_prog("f x:n>n;+x 1");
        let out = explain(&prog, None);
        assert!(out.contains("returns"), "missing 'returns': {out}");
    }

    #[test]
    fn explain_last_stmt_is_return() {
        let prog = parse_prog("f x:n>n;+x 1");
        let out = explain(&prog, None);
        assert!(out.contains("-- return"), "last stmt should be 'return': {out}");
    }

    #[test]
    fn explain_let_bind_annotation() {
        let prog = parse_prog("f x:n>n;y=+x 1;y");
        let out = explain(&prog, None);
        assert!(out.contains("bind → y"), "missing 'bind → y': {out}");
    }

    #[test]
    fn explain_guard_annotation() {
        let prog = parse_prog("f x:n>n;<=x 0{x};+x 1");
        let out = explain(&prog, None);
        assert!(out.contains("guard"), "missing 'guard': {out}");
    }

    #[test]
    fn explain_with_filename_prefix() {
        let prog = parse_prog("f x:n>n;x");
        let out = explain(&prog, Some("my.ilo"));
        assert!(out.starts_with("file: my.ilo\n"), "missing filename prefix: {out}");
    }

    #[test]
    fn explain_no_filename_no_prefix() {
        let prog = parse_prog("f x:n>n;x");
        let out = explain(&prog, None);
        assert!(!out.starts_with("file:"), "unexpected filename prefix: {out}");
    }

    #[test]
    fn explain_typedef_annotation() {
        let prog = parse_prog("type point{x:n;y:n}");
        let out = explain(&prog, None);
        assert!(out.contains("type def"), "missing 'type def': {out}");
    }

    #[test]
    fn explain_alias_annotation() {
        let prog = parse_prog("alias id n");
        let out = explain(&prog, None);
        assert!(out.contains("alias"), "missing 'alias': {out}");
    }

    #[test]
    fn explain_multiple_functions_separated() {
        let prog = parse_prog("f x:n>n;+x 1 g x:n>n;*x 2");
        let out = explain(&prog, None);
        // Two fn start lines
        assert_eq!(out.matches("fn start").count(), 2, "expected 2 'fn start' annotations: {out}");
    }

    #[test]
    fn explain_no_params_function() {
        let prog = parse_prog("f>n;42");
        let out = explain(&prog, None);
        assert!(out.contains("fn start"), "missing 'fn start': {out}");
        assert!(!out.contains("param →"), "unexpected param for 0-param fn: {out}");
    }

    #[test]
    fn explain_foreach_annotation() {
        let prog = parse_prog("f xs:L n>n;s=0;@x xs{s=+s x};s");
        let out = explain(&prog, None);
        assert!(out.contains("foreach →"), "missing 'foreach →': {out}");
    }

    #[test]
    fn explain_for_range_annotation() {
        let prog = parse_prog("f>n;s=0;@i 0..3{s=+s i};s");
        let out = explain(&prog, None);
        assert!(out.contains("for range →"), "missing 'for range →': {out}");
    }

    #[test]
    fn explain_while_annotation() {
        let prog = parse_prog("f x:n>n;wh >x 0{x=-x 1};x");
        let out = explain(&prog, None);
        assert!(out.contains("while"), "missing 'while': {out}");
    }

    #[test]
    fn explain_match_annotation() {
        let prog = parse_prog("f x:n>t;?x{1:\"one\";_:\"other\"}");
        let out = explain(&prog, None);
        assert!(out.contains("match"), "missing 'match': {out}");
    }

    #[test]
    fn explain_ret_annotation() {
        let prog = parse_prog("f x:n>n;ret x");
        let out = explain(&prog, None);
        assert!(out.contains("-- ret"), "missing '-- ret': {out}");
    }

    #[test]
    fn explain_non_last_expr_is_expr() {
        // Two expr stmts — first is "expr", last is "return"
        let prog = parse_prog("f x:n>n;prnt x;+x 1");
        let out = explain(&prog, None);
        assert!(out.contains("-- expr"), "expected '-- expr' for non-last stmt: {out}");
        assert!(out.contains("-- return"), "expected '-- return' for last stmt: {out}");
    }

    #[test]
    fn explain_negated_guard_annotation() {
        let prog = parse_prog("f x:n>n;!>x 0{x};+x 1");
        let out = explain(&prog, None);
        assert!(out.contains("guard !"), "missing 'guard !': {out}");
    }

    #[test]
    fn explain_break_no_value() {
        // brk as top-level stmt (explain only parses, doesn't verify)
        let prog = parse_prog("f>n;brk");
        let out = explain(&prog, None);
        assert!(out.contains("break"), "missing 'break': {out}");
    }

    #[test]
    fn explain_break_with_value() {
        // brk with value as top-level stmt
        let prog = parse_prog("f x:n>n;brk x");
        let out = explain(&prog, None);
        assert!(out.contains("break (value)"), "missing 'break (value)': {out}");
    }

    #[test]
    fn explain_continue_annotation() {
        // cnt as top-level stmt (explain only parses)
        let prog = parse_prog("f>n;cnt;0");
        let out = explain(&prog, None);
        assert!(out.contains("continue"), "missing 'continue': {out}");
    }

    #[test]
    fn explain_destructure_annotation() {
        let prog = parse_prog("type pt{x:n;y:n} f p:pt>n;{x;y}=p;+x y");
        let out = explain(&prog, None);
        assert!(out.contains("destructure →"), "missing 'destructure →': {out}");
    }

    #[test]
    fn explain_tool_annotation() {
        let prog = parse_prog(r#"tool fetch"Fetch a URL" url:t>R t t"#);
        let out = explain(&prog, None);
        assert!(out.contains("tool"), "missing 'tool': {out}");
        assert!(out.contains("@fetch"), "missing '@fetch': {out}");
    }

    #[test]
    fn explain_ternary_guard_annotation() {
        // Guard with else body is a ternary
        let prog = parse_prog("f x:n>n;<=x 0{1}{x}");
        let out = explain(&prog, None);
        assert!(out.contains("ternary"), "missing 'ternary': {out}");
    }

    #[test]
    fn explain_fmt_type_optional() {
        let prog = parse_prog("f x:O n>O n;x");
        let out = explain(&prog, None);
        assert!(out.contains("O n"), "expected 'O n' in output: {out}");
        assert!(out.contains("optional number"), "expected 'optional number' in output: {out}");
    }

    #[test]
    fn explain_fmt_type_result() {
        let prog = parse_prog("f x:R t t>R t t;x");
        let out = explain(&prog, None);
        assert!(out.contains("R t t"), "expected 'R t t' in output: {out}");
        assert!(out.contains("Result ok=text err=text"), "expected 'Result ok=text err=text': {out}");
    }

    #[test]
    fn explain_fmt_type_map() {
        let prog = parse_prog("f m:M t n>M t n;m");
        let out = explain(&prog, None);
        assert!(out.contains("M t n"), "expected 'M t n' in output: {out}");
        assert!(out.contains("map of text to number"), "expected 'map of text to number': {out}");
    }

    #[test]
    fn explain_fmt_type_sum() {
        let prog = parse_prog("f x:S a b>S a b;x");
        let out = explain(&prog, None);
        assert!(out.contains("S a b"), "expected 'S a b' in output: {out}");
        assert!(out.contains("one of: a, b"), "expected 'one of: a, b': {out}");
    }

    #[test]
    fn explain_fmt_type_bool_and_nil() {
        // bool and nil params hit fmt_type "b"/"_" and fmt_type_long "bool"/"nil"
        let prog = parse_prog("f x:b>_;x");
        let out = explain(&prog, None);
        assert!(out.contains("b"), "expected 'b' in output: {out}");
    }

    #[test]
    fn explain_fmt_type_fn_param() {
        // Fn-typed param exercises fmt_type "F ..." and fmt_type_long "fn(...)"
        let prog = parse_prog("f cb:F n n>n;cb 1");
        let out = explain(&prog, None);
        assert!(out.contains("F"), "expected 'F' type in output: {out}");
    }

    #[test]
    fn explain_use_and_error_decls_skipped() {
        // Inject Use and Error decls directly — explain() must skip them silently
        use crate::ast::{Decl, Span};
        let mut prog = parse_prog("f>n;42");
        prog.declarations.push(Decl::Use { path: "x.ilo".into(), only: None, span: Span::UNKNOWN });
        prog.declarations.push(Decl::Error { span: Span::UNKNOWN });
        // Should not panic, and shouldn't add any output for those nodes
        let out = explain(&prog, None);
        assert!(out.contains("fn start"), "expected function output: {out}");
    }
}
