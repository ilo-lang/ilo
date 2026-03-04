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

pub fn explain(program: &Program) -> String {
    let source = program.source.as_deref().unwrap_or("");
    let mut out = String::new();
    let mut first = true;

    for decl in &program.declarations {
        if let Decl::Error { .. } = decl { continue; }
        if !first { out.push('\n'); }
        first = false;

        match decl {
            Decl::Function { name, params, return_type, body, .. } => {
                let sig = if params.is_empty() {
                    format!("{}>{}", name, fmt_type(return_type))
                } else {
                    format!("{} {}>{}", name, fmt_params_sig(params), fmt_type(return_type))
                };
                out.push_str(&annotate_line(&sig, "fn start", 0));

                let n = body.len();
                for (i, spanned) in body.iter().enumerate() {
                    let is_last = i == n - 1;
                    let src = extract(source, spanned.span.start, spanned.span.end);
                    let role = role_of(&spanned.node, is_last);
                    out.push_str(&annotate_line(src, &role, 3));
                }
            }

            Decl::TypeDef { name, fields, .. } => {
                let fields_str = fields.iter()
                    .map(|f| format!("{}:{}", f.name, fmt_type(&f.ty)))
                    .collect::<Vec<_>>()
                    .join("; ");
                out.push_str(&annotate_line(&format!("type {name} {{{fields_str}}}"), "type def", 0));
            }

            Decl::Tool { name, params, return_type, .. } => {
                let sig = format!("@{} {}>{}", name, fmt_params_sig(params), fmt_type(return_type));
                out.push_str(&annotate_line(&sig, "tool", 0));
            }

            Decl::Alias { name, target, .. } => {
                out.push_str(&annotate_line(&format!("alias {name}={}", fmt_type(target)), "alias", 0));
            }

            Decl::Error { .. } => {}
        }
    }

    out
}

/// Format one annotated line: `{indent}{code:<col_width}  {role}`
fn annotate_line(code: &str, role: &str, indent: usize) -> String {
    const CODE_COL: usize = 20;
    let padded_code = indent + code.chars().count();
    // Use at least CODE_COL total width for the code+indent column
    let pad = if padded_code < CODE_COL { CODE_COL - padded_code } else { 2 };
    format!("{}{}{}-- {}\n",
        " ".repeat(indent),
        code,
        " ".repeat(pad),
        role,
    )
}

fn role_of(stmt: &Stmt, is_last: bool) -> String {
    match stmt {
        Stmt::Let { name, .. }        => format!("bind → {name}"),
        Stmt::Guard { negated, else_body, .. } => {
            if else_body.is_some() {
                if *negated { "ternary !".into() } else { "ternary".into() }
            } else {
                if *negated { "guard !".into() } else { "guard".into() }
            }
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

fn fmt_type(ty: &Type) -> String {
    match ty {
        Type::Number          => "n".into(),
        Type::Text            => "t".into(),
        Type::Bool            => "b".into(),
        Type::Nil             => "_".into(),
        Type::List(inner)     => format!("L {}", fmt_type(inner)),
        Type::Result(ok, err) => format!("R {} {}", fmt_type(ok), fmt_type(err)),
        Type::Named(name)     => name.clone(),
    }
}

fn extract(source: &str, start: usize, end: usize) -> &str {
    source.get(start..end).unwrap_or("?").trim()
}
