use crate::ast::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FmtMode {
    /// Compact wire format: single line per declaration, minimal whitespace.
    /// Suitable for LLM I/O.
    Dense,
    /// Human-readable: multi-line, 2-space indentation.
    /// Suitable for code review and diffs.
    Expanded,
}

const INDENT: &str = "  ";

pub fn format(program: &Program, mode: FmtMode) -> String {
    let decls: Vec<&Decl> =
        program.declarations.iter().filter(|d| !matches!(d, Decl::Error { .. })).collect();
    let sep = if mode == FmtMode::Expanded { "\n\n" } else { "\n" };
    let mut parts = Vec::with_capacity(decls.len());
    for decl in decls {
        let mut out = String::new();
        fmt_decl(&mut out, decl, mode);
        parts.push(out);
    }
    parts.join(sep)
}

// ---- Declarations ----

fn fmt_decl(out: &mut String, decl: &Decl, mode: FmtMode) {
    match decl {
        Decl::Function { name, params, return_type, body, .. } => {
            let params_str = fmt_params(params);
            match mode {
                FmtMode::Dense => {
                    out.push_str(name);
                    if !params.is_empty() {
                        out.push(' ');
                        out.push_str(&params_str);
                    }
                    out.push('>');
                    out.push_str(&fmt_type(return_type));
                    if !body.is_empty() {
                        out.push(';');
                        out.push_str(&fmt_body_dense(body));
                    }
                }
                FmtMode::Expanded => {
                    out.push_str(name);
                    if !params.is_empty() {
                        out.push(' ');
                        out.push_str(&params_str);
                    }
                    out.push_str(" > ");
                    out.push_str(&fmt_type(return_type));
                    out.push('\n');
                    fmt_body_expanded(out, body, 1);
                }
            }
        }

        Decl::TypeDef { name, fields, .. } => match mode {
            FmtMode::Dense => {
                out.push_str("type ");
                out.push_str(name);
                out.push('{');
                for (i, f) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push(';');
                    }
                    out.push_str(&f.name);
                    out.push(':');
                    out.push_str(&fmt_type(&f.ty));
                }
                out.push('}');
            }
            FmtMode::Expanded => {
                out.push_str("type ");
                out.push_str(name);
                out.push_str(" {\n");
                for f in fields {
                    out.push_str(INDENT);
                    out.push_str(&f.name);
                    out.push_str(": ");
                    out.push_str(&fmt_type(&f.ty));
                    out.push('\n');
                }
                out.push('}');
            }
        },

        Decl::Tool { name, description, params, return_type, timeout, retry, .. } => {
            let params_str = fmt_params(params);
            let desc = escape_text(description);
            match mode {
                FmtMode::Dense => {
                    out.push_str("tool ");
                    out.push_str(name);
                    out.push('"');
                    out.push_str(&desc);
                    out.push('"');
                    if !params.is_empty() {
                        out.push(' ');
                        out.push_str(&params_str);
                    }
                    out.push('>');
                    out.push_str(&fmt_type(return_type));
                    let mut opts: Vec<String> = Vec::new();
                    if let Some(t) = timeout {
                        opts.push(format!("timeout:{}", fmt_num(*t)));
                    }
                    if let Some(r) = retry {
                        opts.push(format!("retry:{}", fmt_num(*r)));
                    }
                    if !opts.is_empty() {
                        out.push(' ');
                        out.push_str(&opts.join(","));
                    }
                }
                FmtMode::Expanded => {
                    out.push_str("tool ");
                    out.push_str(name);
                    out.push_str(" \"");
                    out.push_str(&desc);
                    out.push('"');
                    out.push('\n');
                    out.push_str(INDENT);
                    if !params.is_empty() {
                        out.push_str(&params_str);
                        out.push(' ');
                    }
                    out.push_str("> ");
                    out.push_str(&fmt_type(return_type));
                    let mut opts: Vec<String> = Vec::new();
                    if let Some(t) = timeout {
                        opts.push(format!("timeout: {}", fmt_num(*t)));
                    }
                    if let Some(r) = retry {
                        opts.push(format!("retry: {}", fmt_num(*r)));
                    }
                    if !opts.is_empty() {
                        out.push('\n');
                        out.push_str(INDENT);
                        out.push_str(&opts.join(", "));
                    }
                }
            }
        }

        Decl::Error { .. } => {} // poison node — skip
    }
}

// ---- Params and types ----

fn fmt_params(params: &[Param]) -> String {
    params
        .iter()
        .map(|p| format!("{}:{}", p.name, fmt_type(&p.ty)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn fmt_type(ty: &Type) -> String {
    match ty {
        Type::Number => "n".to_string(),
        Type::Text => "t".to_string(),
        Type::Bool => "b".to_string(),
        Type::Nil => "_".to_string(),
        Type::List(inner) => format!("L {}", fmt_type(inner)),
        Type::Result(ok, err) => format!("R {} {}", fmt_type(ok), fmt_type(err)),
        Type::Named(name) => name.clone(),
    }
}

// ---- Dense body / statement formatting ----

fn fmt_body_dense(stmts: &[Spanned<Stmt>]) -> String {
    stmts.iter().map(|s| fmt_stmt_dense(&s.node)).collect::<Vec<_>>().join(";")
}

fn fmt_stmt_dense(stmt: &Stmt) -> String {
    match stmt {
        Stmt::Let { name, value } => format!("{}={}", name, fmt_expr(value, FmtMode::Dense)),
        Stmt::Guard { condition, negated, body, else_body } => {
            let prefix = if *negated { "!" } else { "" };
            let main = format!("{}{}{{{}}}", prefix, fmt_expr(condition, FmtMode::Dense), fmt_body_dense(body));
            if let Some(eb) = else_body {
                format!("{}{{{}}}", main, fmt_body_dense(eb))
            } else {
                main
            }
        }
        Stmt::Match { subject, arms } => {
            let subj = subject.as_ref().map(|e| fmt_expr(e, FmtMode::Dense)).unwrap_or_default();
            format!("?{}{{{}}}", subj, fmt_arms_dense(arms))
        }
        Stmt::ForEach { binding, collection, body } => {
            format!("@{} {}{{{}}}", binding, fmt_expr(collection, FmtMode::Dense), fmt_body_dense(body))
        }
        Stmt::While { condition, body } => {
            format!("wh {}{{{}}}", fmt_expr(condition, FmtMode::Dense), fmt_body_dense(body))
        }
        Stmt::Return(e) => format!("ret {}", fmt_expr(e, FmtMode::Dense)),
        Stmt::Break(Some(e)) => format!("brk {}", fmt_expr(e, FmtMode::Dense)),
        Stmt::Break(None) => "brk".to_string(),
        Stmt::Continue => "cnt".to_string(),
        Stmt::Expr(e) => fmt_expr(e, FmtMode::Dense),
    }
}

fn fmt_arms_dense(arms: &[MatchArm]) -> String {
    arms.iter()
        .map(|arm| {
            let body = fmt_body_dense(&arm.body);
            format!("{}:{}", fmt_pattern(&arm.pattern), body)
        })
        .collect::<Vec<_>>()
        .join(";")
}

// ---- Expanded body / statement formatting ----

fn fmt_body_expanded(out: &mut String, stmts: &[Spanned<Stmt>], indent_level: usize) {
    for s in stmts {
        fmt_stmt_expanded(out, &s.node, indent_level);
    }
}

fn fmt_stmt_expanded(out: &mut String, stmt: &Stmt, indent_level: usize) {
    let ind = INDENT.repeat(indent_level);
    match stmt {
        Stmt::Let { name, value } => {
            out.push_str(&ind);
            out.push_str(name);
            out.push_str(" = ");
            out.push_str(&fmt_expr(value, FmtMode::Expanded));
            out.push('\n');
        }
        Stmt::Guard { condition, negated, body, else_body } => {
            let prefix = if *negated { "!" } else { "" };
            out.push_str(&ind);
            out.push_str(prefix);
            out.push_str(&fmt_expr(condition, FmtMode::Expanded));
            out.push_str(" {\n");
            fmt_body_expanded(out, body, indent_level + 1);
            out.push_str(&ind);
            out.push('}');
            if let Some(eb) = else_body {
                out.push_str(" {\n");
                fmt_body_expanded(out, eb, indent_level + 1);
                out.push_str(&ind);
                out.push_str("}\n");
            } else {
                out.push('\n');
            }
        }
        Stmt::Match { subject, arms } => {
            out.push_str(&ind);
            out.push('?');
            if let Some(e) = subject {
                out.push(' ');
                out.push_str(&fmt_expr(e, FmtMode::Expanded));
                out.push(' ');
            } else {
                out.push(' ');
            }
            out.push_str("{\n");
            for arm in arms {
                fmt_arm_expanded(out, arm, indent_level + 1);
            }
            out.push_str(&ind);
            out.push_str("}\n");
        }
        Stmt::ForEach { binding, collection, body } => {
            out.push_str(&ind);
            out.push('@');
            out.push(' ');
            out.push_str(binding);
            out.push(' ');
            out.push_str(&fmt_expr(collection, FmtMode::Expanded));
            out.push_str(" {\n");
            fmt_body_expanded(out, body, indent_level + 1);
            out.push_str(&ind);
            out.push_str("}\n");
        }
        Stmt::While { condition, body } => {
            out.push_str(&ind);
            out.push_str("wh ");
            out.push_str(&fmt_expr(condition, FmtMode::Expanded));
            out.push_str(" {\n");
            fmt_body_expanded(out, body, indent_level + 1);
            out.push_str(&ind);
            out.push_str("}\n");
        }
        Stmt::Return(e) => {
            out.push_str(&ind);
            out.push_str("ret ");
            out.push_str(&fmt_expr(e, FmtMode::Expanded));
            out.push('\n');
        }
        Stmt::Break(Some(e)) => {
            out.push_str(&ind);
            out.push_str("brk ");
            out.push_str(&fmt_expr(e, FmtMode::Expanded));
            out.push('\n');
        }
        Stmt::Break(None) => {
            out.push_str(&ind);
            out.push_str("brk\n");
        }
        Stmt::Continue => {
            out.push_str(&ind);
            out.push_str("cnt\n");
        }
        Stmt::Expr(e) => {
            out.push_str(&ind);
            out.push_str(&fmt_expr(e, FmtMode::Expanded));
            out.push('\n');
        }
    }
}

fn fmt_arm_expanded(out: &mut String, arm: &MatchArm, indent_level: usize) {
    let ind = INDENT.repeat(indent_level);
    out.push_str(&ind);
    out.push_str(&fmt_pattern(&arm.pattern));
    out.push_str(":\n");
    fmt_body_expanded(out, &arm.body, indent_level + 1);
}

// ---- Expressions ----
// In Dense mode: operators glue to first operand (e.g. `>=sp 1000`)
// In Expanded mode: space between operator and operand (e.g. `>= sp 1000`)

fn fmt_expr(expr: &Expr, mode: FmtMode) -> String {
    match expr {
        Expr::Literal(lit) => fmt_literal(lit),
        Expr::Ref(name) => name.clone(),
        Expr::Field { object, field, safe } => {
            let dot = if *safe { ".?" } else { "." };
            format!("{}{}{}", fmt_expr(object, mode), dot, field)
        }
        Expr::Index { object, index, safe } => {
            let dot = if *safe { ".?" } else { "." };
            format!("{}{}{}", fmt_expr(object, mode), dot, index)
        }
        Expr::Call { function, args, unwrap } => {
            let bang = if *unwrap { "!" } else { "" };
            if args.is_empty() {
                format!("{}{}()", function, bang)
            } else {
                let args_str: Vec<String> = args.iter().map(|a| fmt_expr(a, mode)).collect();
                format!("{}{} {}", function, bang, args_str.join(" "))
            }
        }
        Expr::BinOp { op, left, right } => match mode {
            FmtMode::Dense => {
                format!("{}{} {}", fmt_binop(op), fmt_expr(left, mode), fmt_expr(right, mode))
            }
            FmtMode::Expanded => {
                format!("{} {} {}", fmt_binop(op), fmt_expr(left, mode), fmt_expr(right, mode))
            }
        },
        Expr::UnaryOp { op, operand } => {
            let inner = fmt_expr(operand, mode);
            match (op, mode) {
                // Add a space to avoid "--" being lexed as a comment token.
                (UnaryOp::Negate, _) => format!("- {}", inner),
                (UnaryOp::Not, FmtMode::Dense) => format!("!{}", inner),
                (UnaryOp::Not, FmtMode::Expanded) => format!("! {}", inner),
            }
        }
        Expr::Ok(inner) => match mode {
            FmtMode::Dense => format!("~{}", fmt_expr(inner, mode)),
            FmtMode::Expanded => format!("~ {}", fmt_expr(inner, mode)),
        },
        Expr::Err(inner) => match mode {
            FmtMode::Dense => format!("^{}", fmt_expr(inner, mode)),
            FmtMode::Expanded => format!("^ {}", fmt_expr(inner, mode)),
        },
        Expr::List(items) => {
            let items_str: Vec<String> = items.iter().map(|i| fmt_expr(i, mode)).collect();
            format!("[{}]", items_str.join(", "))
        }
        Expr::Record { type_name, fields } => {
            if fields.is_empty() {
                return type_name.clone();
            }
            let fields_str: Vec<String> =
                fields.iter().map(|(n, v)| format!("{}:{}", n, fmt_expr(v, mode))).collect();
            format!("{} {}", type_name, fields_str.join(" "))
        }
        // Match expressions stay dense — they appear in expression position.
        Expr::Match { subject, arms } => {
            let subj = subject.as_ref().map(|e| fmt_expr(e, mode)).unwrap_or_default();
            format!("?{}{{{}}}", subj, fmt_arms_dense(arms))
        }
        Expr::NilCoalesce { value, default } => {
            format!("{}??{}", fmt_expr(value, mode), fmt_expr(default, mode))
        }
        Expr::With { object, updates } => {
            let updates_str: Vec<String> =
                updates.iter().map(|(n, v)| format!("{}:{}", n, fmt_expr(v, mode))).collect();
            format!("{} with {}", fmt_expr(object, mode), updates_str.join(" "))
        }
    }
}

// ---- Patterns and literals ----

fn fmt_pattern(pat: &Pattern) -> String {
    match pat {
        Pattern::Wildcard => "_".to_string(),
        Pattern::Ok(binding) => format!("~{}", binding),
        Pattern::Err(binding) => format!("^{}", binding),
        Pattern::Literal(lit) => fmt_literal(lit),
    }
}

fn fmt_literal(lit: &Literal) -> String {
    match lit {
        Literal::Number(n) => fmt_num(*n),
        Literal::Text(s) => format!("\"{}\"", escape_text(s)),
        Literal::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
    }
}

fn fmt_num(n: f64) -> String {
    if n == (n as i64) as f64 && n.is_finite() {
        format!("{}", n as i64)
    } else {
        format!("{}", n)
    }
}

fn escape_text(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "\\r")
}

fn fmt_binop(op: &BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Subtract => "-",
        BinOp::Multiply => "*",
        BinOp::Divide => "/",
        BinOp::Equals => "=",
        BinOp::NotEquals => "!=",
        BinOp::GreaterThan => ">",
        BinOp::LessThan => "<",
        BinOp::GreaterOrEqual => ">=",
        BinOp::LessOrEqual => "<=",
        BinOp::And => "&",
        BinOp::Or => "|",
        BinOp::Append => "+=",
    }
}

// ---- Tests ----

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer, parser};

    fn parse(source: &str) -> Program {
        let tokens = lexer::lex(source).unwrap();
        let token_spans = tokens
            .into_iter()
            .map(|(t, r)| (t, crate::ast::Span { start: r.start, end: r.end }))
            .collect();
        let (mut prog, errs) = parser::parse(token_spans);
        assert!(errs.is_empty(), "parse errors in test: {:?}", errs);
        prog.source = Some(source.to_string());
        prog
    }

    fn dense(source: &str) -> String {
        format(&parse(source), FmtMode::Dense)
    }

    fn expanded(source: &str) -> String {
        format(&parse(source), FmtMode::Expanded)
    }

    // Round-trip: dense(parse(dense(parse(src)))) == dense(parse(src))
    // Compares formatted strings rather than AST nodes, which avoids span differences.
    fn assert_round_trip(source: &str) {
        let prog = parse(source);
        let formatted = format(&prog, FmtMode::Dense);
        let prog2 = parse(&formatted);
        let formatted2 = format(&prog2, FmtMode::Dense);
        assert_eq!(
            formatted, formatted2,
            "round-trip mismatch\n  original:  {source}\n  formatted: {formatted}\n  re-formatted: {formatted2}"
        );
    }

    // Idempotency: dense(parse(dense(parse(src)))) == dense(parse(src))
    fn assert_idempotent(source: &str) {
        let first = dense(source);
        let second = dense(&first);
        assert_eq!(first, second, "formatter not idempotent for: {source}");
    }

    #[test]
    fn dense_simple_function() {
        let s = dense("tot p:n q:n r:n>n;s=*p q;t=*s r;+s t");
        assert_eq!(s, "tot p:n q:n r:n>n;s=*p q;t=*s r;+s t");
    }

    #[test]
    fn dense_zero_arg_call() {
        let s = dense("f>t;make-id()");
        assert_eq!(s, "f>t;make-id()");
    }

    #[test]
    fn dense_guard() {
        let s = dense(r#"cls sp:n>t;>=sp 1000{"gold"};>=sp 500{"silver"};"bronze""#);
        assert!(s.contains(r#">= sp 1000{"gold"}"#) || s.contains(r#">=sp 1000{"gold"}"#));
    }

    #[test]
    fn dense_negated_guard() {
        let s = dense(r#"f x:b>t;!x{"nope"};"ok""#);
        assert!(s.contains(r#"!x{"nope"}"#));
    }

    #[test]
    fn dense_match_stmt() {
        let s = dense(r#"f x:t>n;?x{"a":1;"b":2;_:0}"#);
        assert!(s.contains(r#"?x{"a":1;"b":2;_:0}"#));
    }

    #[test]
    fn dense_foreach() {
        let s = dense("f xs:L n>L n;@x xs{+x 1}");
        assert!(s.contains("@x xs{"));
    }

    #[test]
    fn dense_type_def() {
        let s = dense("type point{x:n;y:n}");
        assert_eq!(s, "type point{x:n;y:n}");
    }

    #[test]
    fn dense_tool() {
        let s = dense(r#"tool send-email"Send an email" to:t body:t>R _ t timeout:30,retry:3"#);
        assert!(s.contains(r#"tool send-email"Send an email""#));
        assert!(s.contains("timeout:30"));
        assert!(s.contains("retry:3"));
    }

    #[test]
    fn dense_ok_err() {
        let s = dense("f x:n>R n t;~x");
        assert!(s.contains("~x"));
        let s = dense(r#"f x:n>R n t;^"bad""#);
        assert!(s.contains(r#"^"bad""#));
    }

    #[test]
    fn dense_list_literal() {
        let s = dense("f>L n;[1, 2, 3]");
        assert!(s.contains("[1, 2, 3]"));
    }

    #[test]
    fn dense_record() {
        let s = dense("f x:n>point;point x:x y:10");
        assert!(s.contains("point x:x y:10"));
    }

    #[test]
    fn dense_with_expr() {
        let s = dense("f x:order>order;x with total:100");
        assert!(s.contains("x with total:100"));
    }

    #[test]
    fn dense_logical_ops() {
        let s = dense("f a:b b:b>b;&a b");
        assert!(s.contains("&a b"));
        let s = dense("f a:b b:b>b;|a b");
        assert!(s.contains("|a b"));
    }

    #[test]
    fn dense_list_append() {
        let s = dense("f xs:L n>L n;+=xs 1");
        assert!(s.contains("+=xs 1"));
    }

    #[test]
    fn dense_complex_types() {
        let s = dense("f x:L n>R n t;~x.0");
        assert!(s.contains("L n"));
        assert!(s.contains("R n t"));
    }

    #[test]
    fn dense_not_equals() {
        let s = dense("f a:n b:n>b;!=a b");
        assert!(s.contains("!=a b"));
    }

    #[test]
    fn dense_bool_literals() {
        let s = dense("f>b;true");
        assert!(s.contains("true"));
        let s = dense("f>b;false");
        assert!(s.contains("false"));
    }

    #[test]
    fn dense_float_literal() {
        let s = dense("f>n;3.14");
        assert!(s.contains("3.14"));
    }

    #[test]
    fn dense_nested_prefix_ops() {
        let s = dense("f a:n b:n c:n>n;+*a b c");
        assert!(s.contains("+*a b c"), "got: {s}");
    }

    #[test]
    fn dense_unary_negate() {
        let s = dense("f x:n>n;-x");
        // May have a space: "- x" to avoid "--"
        let parsed_back = parse(&s);
        assert!(matches!(
            parsed_back.declarations[0],
            Decl::Function { .. }
        ));
    }

    #[test]
    fn dense_logical_not() {
        let s = dense("f x:b>b;!x");
        assert!(s.contains("!x"));
    }

    // ---- Round-trip tests ----

    #[test]
    fn round_trip_simple() {
        assert_round_trip("tot p:n q:n r:n>n;s=*p q;t=*s r;+s t");
    }

    #[test]
    fn round_trip_guard() {
        assert_round_trip(r#"cls sp:n>t;>=sp 1000{"gold"};>=sp 500{"silver"};"bronze""#);
    }

    #[test]
    fn round_trip_match() {
        assert_round_trip(r#"f x:t>n;?x{"a":1;"b":2;_:0}"#);
    }

    #[test]
    fn round_trip_foreach() {
        assert_round_trip("f xs:L n>L n;@x xs{+x 1}");
    }

    #[test]
    fn round_trip_ok_err() {
        assert_round_trip("f x:n>R n t;~x");
        assert_round_trip(r#"f x:n>R n t;^"bad""#);
    }

    #[test]
    fn round_trip_record_with() {
        assert_round_trip("f x:order>order;x with total:100");
    }

    #[test]
    fn round_trip_typedef() {
        assert_round_trip("type point{x:n;y:n}");
    }

    #[test]
    fn round_trip_tool() {
        assert_round_trip(
            r#"tool send-email"Send an email" to:t body:t>R _ t timeout:30,retry:3"#,
        );
    }

    #[test]
    fn round_trip_example_01() {
        assert_round_trip(
            &std::fs::read_to_string(
                "research/explorations/idea9-ultra-dense-short/01-simple-function.ilo",
            )
            .unwrap(),
        );
    }

    #[test]
    fn round_trip_example_02() {
        assert_round_trip(
            &std::fs::read_to_string(
                "research/explorations/idea9-ultra-dense-short/02-with-dependencies.ilo",
            )
            .unwrap(),
        );
    }

    #[test]
    fn round_trip_example_03() {
        assert_round_trip(
            &std::fs::read_to_string(
                "research/explorations/idea9-ultra-dense-short/03-data-transform.ilo",
            )
            .unwrap(),
        );
    }

    #[test]
    fn round_trip_example_04() {
        assert_round_trip(
            &std::fs::read_to_string(
                "research/explorations/idea9-ultra-dense-short/04-tool-interaction.ilo",
            )
            .unwrap(),
        );
    }

    #[test]
    fn round_trip_example_05() {
        assert_round_trip(
            &std::fs::read_to_string(
                "research/explorations/idea9-ultra-dense-short/05-workflow.ilo",
            )
            .unwrap(),
        );
    }

    // ---- Idempotency tests ----

    #[test]
    fn idempotent_simple() {
        assert_idempotent("tot p:n q:n r:n>n;s=*p q;t=*s r;+s t");
    }

    #[test]
    fn idempotent_guard() {
        assert_idempotent(r#"cls sp:n>t;>=sp 1000{"gold"};>=sp 500{"silver"};"bronze""#);
    }

    #[test]
    fn idempotent_example_04() {
        assert_idempotent(
            &std::fs::read_to_string(
                "research/explorations/idea9-ultra-dense-short/04-tool-interaction.ilo",
            )
            .unwrap(),
        );
    }

    #[test]
    fn idempotent_example_05() {
        assert_idempotent(
            &std::fs::read_to_string(
                "research/explorations/idea9-ultra-dense-short/05-workflow.ilo",
            )
            .unwrap(),
        );
    }

    // ---- Expanded format structure tests ----

    #[test]
    fn expanded_simple_function() {
        let s = expanded("tot p:n q:n r:n>n;s=*p q;t=*s r;+s t");
        assert!(s.starts_with("tot p:n q:n r:n > n\n"), "got: {s}");
        assert!(s.contains("  s = * p q\n"), "got: {s}");
        assert!(s.contains("  t = * s r\n"), "got: {s}");
        assert!(s.contains("  + s t\n"), "got: {s}");
    }

    #[test]
    fn expanded_guard() {
        let s = expanded(r#"cls sp:n>t;>=sp 1000{"gold"};>=sp 500{"silver"};"bronze""#);
        assert!(s.contains("> t\n"), "got: {s}");
        assert!(s.contains(">= sp 1000 {\n"), "got: {s}");
        assert!(s.contains(r#"    "gold""#), "got: {s}");
    }

    #[test]
    fn expanded_match() {
        let s = expanded(r#"f x:t>n;?x{"a":1;"b":2;_:0}"#);
        assert!(s.contains("  ? x {\n"), "got: {s}");
        assert!(s.contains("  \"a\":\n"), "got: {s}");
        assert!(s.contains("    1\n"), "got: {s}");
        assert!(s.contains("  _:\n"), "got: {s}");
    }

    #[test]
    fn expanded_foreach() {
        let s = expanded("f xs:L n>L n;@x xs{+x 1}");
        assert!(s.contains("  @ x xs {\n"), "got: {s}");
        assert!(s.contains("    + x 1\n"), "got: {s}");
    }

    #[test]
    fn expanded_typedef() {
        let s = expanded("type point{x:n;y:n}");
        assert!(s.starts_with("type point {\n"), "got: {s}");
        assert!(s.contains("  x: n\n"), "got: {s}");
        assert!(s.contains("  y: n\n"), "got: {s}");
    }

    #[test]
    fn expanded_tool() {
        let s = expanded(r#"tool send-email"Send an email" to:t body:t>R _ t timeout:30,retry:3"#);
        assert!(s.contains("tool send-email \"Send an email\"\n"), "got: {s}");
        assert!(s.contains("> R _ t\n"), "got: {s}");
        assert!(s.contains("timeout: 30"), "got: {s}");
        assert!(s.contains("retry: 3"), "got: {s}");
    }

    #[test]
    fn expanded_multiple_decls_separated_by_blank_line() {
        let s = expanded(
            &std::fs::read_to_string(
                "research/explorations/idea9-ultra-dense-short/03-data-transform.ilo",
            )
            .unwrap(),
        );
        // Two declarations should be separated by a blank line in expanded mode.
        assert!(s.contains("\n\n"), "expected blank line between decls, got: {s}");
    }

    #[test]
    fn expanded_no_params_function() {
        let s = expanded("f>n;42");
        assert!(s.starts_with("f > n\n"), "got: {s}");
    }

    #[test]
    fn expanded_workflow() {
        let s = expanded(
            &std::fs::read_to_string(
                "research/explorations/idea9-ultra-dense-short/05-workflow.ilo",
            )
            .unwrap(),
        );
        assert!(s.contains("chk"), "got: {s}");
        assert!(s.contains("  ? {\n"), "expected expanded match arms, got: {s}");
        // Nested match arms are indented further
        assert!(s.contains("      ? {\n") || s.contains("    ? {\n"), "got: {s}");
    }

    #[test]
    fn error_decl_skipped() {
        use crate::ast::{Decl, Span};
        let mut prog = parse("f x:n>n;*x 2");
        prog.declarations.push(Decl::Error { span: Span::UNKNOWN });
        let s = format(&prog, FmtMode::Dense);
        assert!(s.contains("f x:n>n;*x 2"), "got: {s}");
        // Error node produces no output
        assert!(!s.contains("Error"), "got: {s}");
    }

    // ---- Braceless guards ----

    #[test]
    fn braceless_guard_normalizes_to_braced() {
        // Braceless input should normalize to braced in dense format
        let prog = parse(r#"cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze""#);
        let s = format(&prog, FmtMode::Dense);
        assert_eq!(s, r#"cls sp:n>t;>=sp 1000{"gold"};>=sp 500{"silver"};"bronze""#,
            "braceless guard should normalize to braced: {s}");
    }

    #[test]
    fn braceless_guard_round_trip() {
        // Braceless → format(braced) → parse → format(braced) should be stable
        let prog = parse(r#"cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze""#);
        let formatted = format(&prog, FmtMode::Dense);
        let prog2 = parse(&formatted);
        let formatted2 = format(&prog2, FmtMode::Dense);
        assert_eq!(formatted, formatted2,
            "braceless guard round-trip mismatch:\n  formatted: {formatted}\n  re-formatted: {formatted2}");
    }

    #[test]
    fn braceless_guard_expanded() {
        let prog = parse(r#"cls sp:n>t;>=sp 1000 "gold";"bronze""#);
        let s = format(&prog, FmtMode::Expanded);
        assert!(s.contains(">= sp 1000 {"), "expanded should have braces: {s}");
        assert!(s.contains("\"gold\""), "expanded should contain body: {s}");
    }

    #[test]
    fn dense_while() {
        let s = dense("f>n;wh true{42}");
        assert!(s.contains("wh true{42}"), "got: {s}");
    }

    #[test]
    fn expanded_while() {
        let s = expanded("f>n;i=0;wh <i 5{i=+i 1}");
        assert!(s.contains("wh < i 5 {\n"), "got: {s}");
    }

    #[test]
    fn round_trip_while() {
        assert_round_trip("f>n;i=0;wh <i 5{i=+i 1};i");
    }

    #[test]
    fn dense_ret() {
        let s = dense("f x:n>n;ret +x 1");
        assert!(s.contains("ret +x 1"), "got: {s}");
    }

    #[test]
    fn expanded_ret() {
        let s = expanded("f x:n>n;ret +x 1");
        assert!(s.contains("  ret + x 1\n"), "got: {s}");
    }

    #[test]
    fn round_trip_ret() {
        assert_round_trip("f x:n>n;>x 0{ret x};0");
    }
}
