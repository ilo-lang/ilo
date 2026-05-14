use crate::ast::*;

fn type_to_py(ty: &Type) -> &'static str {
    match ty {
        Type::Number => "int | float",
        Type::Text => "str",
        Type::Bool => "bool",
        Type::List(_) => "list",
        Type::Map(_, _) => "dict",
        Type::Optional(_) => "object | None",
        Type::Sum(_) => "str",
        _ => "object",
    }
}

const RD_HELPER: &str = r#"def _ilo_rd(path, fmt=None):
    import os, json, csv, io
    if not os.path.exists(path):
        return ("err", f"{path}: no such file")
    try:
        raw = open(path).read()
        if fmt is None:
            ext = os.path.splitext(path)[1].lstrip('.').lower()
        else:
            ext = fmt
        return ("ok", _ilo_parse_fmt(raw, ext))
    except Exception as e:
        return ("err", str(e))

def _ilo_rdb(s, fmt):
    try:
        return ("ok", _ilo_parse_fmt(s, fmt))
    except Exception as e:
        return ("err", str(e))

def _ilo_parse_fmt(s, fmt):
    import json, csv, io
    if fmt in ("csv", "tsv"):
        sep = '\t' if fmt == "tsv" else ','
        return [row for row in csv.reader(io.StringIO(s), delimiter=sep)]
    if fmt == "json":
        return json.loads(s)
    return s

"#;

pub fn emit(program: &Program) -> String {
    let mut out = String::new();
    if uses_unwrap(program) {
        out.push_str("def _ilo_unwrap(r):\n    if r[0] == \"ok\":\n        return r[1]\n    raise RuntimeError(r[1])\n\n");
    }
    if uses_rd(program) {
        out.push_str(RD_HELPER);
    }
    for decl in &program.declarations {
        emit_decl(&mut out, decl, 0);
        out.push('\n');
    }
    out.trim_end().to_string()
}

fn uses_rd(program: &Program) -> bool {
    program.declarations.iter().any(|d| match d {
        Decl::Function { body, .. } => body.iter().any(|s| stmt_uses_rd(&s.node)),
        _ => false,
    })
}

fn stmt_uses_rd(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Let { value, .. } => expr_uses_rd(value),
        Stmt::Guard {
            condition, body, ..
        } => expr_uses_rd(condition) || body.iter().any(|s| stmt_uses_rd(&s.node)),
        Stmt::Match { subject, arms } => {
            subject.as_ref().is_some_and(expr_uses_rd)
                || arms
                    .iter()
                    .any(|a| a.body.iter().any(|s| stmt_uses_rd(&s.node)))
        }
        Stmt::ForEach {
            collection, body, ..
        } => expr_uses_rd(collection) || body.iter().any(|s| stmt_uses_rd(&s.node)),
        Stmt::ForRange {
            start, end, body, ..
        } => expr_uses_rd(start) || expr_uses_rd(end) || body.iter().any(|s| stmt_uses_rd(&s.node)),
        Stmt::While { condition, body } => {
            expr_uses_rd(condition) || body.iter().any(|s| stmt_uses_rd(&s.node))
        }
        Stmt::Return(e) => expr_uses_rd(e),
        Stmt::Break(Some(e)) => expr_uses_rd(e),
        Stmt::Break(None) | Stmt::Continue => false,
        Stmt::Destructure { value, .. } => expr_uses_rd(value),
        Stmt::Expr(e) => expr_uses_rd(e),
    }
}

fn expr_uses_rd(expr: &Expr) -> bool {
    match expr {
        Expr::Call { function, args, .. } => {
            matches!(function.as_str(), "rd" | "rdb") || args.iter().any(expr_uses_rd)
        }
        Expr::BinOp { left, right, .. } => expr_uses_rd(left) || expr_uses_rd(right),
        Expr::UnaryOp { operand, .. } => expr_uses_rd(operand),
        Expr::Ok(e) | Expr::Err(e) => expr_uses_rd(e),
        Expr::Field { object, .. } | Expr::Index { object, .. } => expr_uses_rd(object),
        Expr::List(items) => items.iter().any(expr_uses_rd),
        Expr::Record { fields, .. } => fields.iter().any(|(_, e)| expr_uses_rd(e)),
        Expr::Match { subject, arms } => {
            subject.as_ref().is_some_and(|s| expr_uses_rd(s))
                || arms
                    .iter()
                    .any(|a| a.body.iter().any(|s| stmt_uses_rd(&s.node)))
        }
        Expr::NilCoalesce { value, default } => expr_uses_rd(value) || expr_uses_rd(default),
        _ => false,
    }
}

fn uses_unwrap(program: &Program) -> bool {
    program.declarations.iter().any(|d| match d {
        Decl::Function { body, .. } => body.iter().any(|s| stmt_uses_unwrap(&s.node)),
        _ => false,
    })
}

fn stmt_uses_unwrap(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Let { value, .. } => expr_uses_unwrap(value),
        Stmt::Guard {
            condition, body, ..
        } => expr_uses_unwrap(condition) || body.iter().any(|s| stmt_uses_unwrap(&s.node)),
        Stmt::Match { subject, arms } => {
            subject.as_ref().is_some_and(expr_uses_unwrap)
                || arms
                    .iter()
                    .any(|a| a.body.iter().any(|s| stmt_uses_unwrap(&s.node)))
        }
        Stmt::ForEach {
            collection, body, ..
        } => expr_uses_unwrap(collection) || body.iter().any(|s| stmt_uses_unwrap(&s.node)),
        Stmt::ForRange {
            start, end, body, ..
        } => {
            expr_uses_unwrap(start)
                || expr_uses_unwrap(end)
                || body.iter().any(|s| stmt_uses_unwrap(&s.node))
        }
        Stmt::While { condition, body } => {
            expr_uses_unwrap(condition) || body.iter().any(|s| stmt_uses_unwrap(&s.node))
        }
        Stmt::Return(e) => expr_uses_unwrap(e),
        Stmt::Break(Some(e)) => expr_uses_unwrap(e),
        Stmt::Break(None) => false,
        Stmt::Continue => false,
        Stmt::Destructure { value, .. } => expr_uses_unwrap(value),
        Stmt::Expr(e) => expr_uses_unwrap(e),
    }
}

fn expr_uses_unwrap(expr: &Expr) -> bool {
    match expr {
        Expr::Call { unwrap, args, .. } => *unwrap || args.iter().any(expr_uses_unwrap),
        Expr::BinOp { left, right, .. } => expr_uses_unwrap(left) || expr_uses_unwrap(right),
        Expr::UnaryOp { operand, .. } => expr_uses_unwrap(operand),
        Expr::Ok(e) | Expr::Err(e) => expr_uses_unwrap(e),
        Expr::Field { object, .. } | Expr::Index { object, .. } => expr_uses_unwrap(object),
        Expr::List(items) => items.iter().any(expr_uses_unwrap),
        Expr::Record { fields, .. } => fields.iter().any(|(_, e)| expr_uses_unwrap(e)),
        Expr::Match { subject, arms } => {
            subject.as_ref().is_some_and(|s| expr_uses_unwrap(s))
                || arms
                    .iter()
                    .any(|a| a.body.iter().any(|s| stmt_uses_unwrap(&s.node)))
        }
        Expr::NilCoalesce { value, default } => {
            expr_uses_unwrap(value) || expr_uses_unwrap(default)
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_uses_unwrap(condition)
                || expr_uses_unwrap(then_expr)
                || expr_uses_unwrap(else_expr)
        }
        Expr::With { object, updates } => {
            expr_uses_unwrap(object) || updates.iter().any(|(_, e)| expr_uses_unwrap(e))
        }
        _ => false,
    }
}

fn indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("    ");
    }
}

fn emit_decl(out: &mut String, decl: &Decl, level: usize) {
    match decl {
        Decl::Function {
            name,
            params,
            return_type,
            body,
            ..
        } => {
            indent(out, level);
            out.push_str(&format!("def {}(", py_name(name)));
            for (i, p) in params.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&format!("{}: {}", py_name(&p.name), emit_type(&p.ty)));
            }
            out.push_str(&format!(") -> {}:\n", emit_type(return_type)));
            emit_body(out, body, level + 1, true);
        }
        Decl::TypeDef { name, fields, .. } => {
            indent(out, level);
            out.push_str(&format!("# type {} = {{", name));
            for (i, f) in fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&format!("{}: {}", f.name, emit_type(&f.ty)));
            }
            out.push_str("}\n");
        }
        Decl::Tool {
            name,
            description,
            params,
            return_type,
            ..
        } => {
            indent(out, level);
            out.push_str(&format!("def {}(", py_name(name)));
            for (i, p) in params.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&format!("{}: {}", py_name(&p.name), emit_type(&p.ty)));
            }
            out.push_str(&format!(") -> {}:\n", emit_type(return_type)));
            indent(out, level + 1);
            out.push_str(&format!("\"\"\"{}\"\"\"", description));
            out.push('\n');
            indent(out, level + 1);
            out.push_str("raise NotImplementedError\n");
        }
        Decl::Alias { name, target, .. } => {
            indent(out, level);
            out.push_str(&format!("# alias {} = {}\n", name, emit_type(target)));
        }
        Decl::Use { .. } => {}   // resolved before codegen — skip
        Decl::Error { .. } => {} // poison node — skip
    }
}

fn emit_body(out: &mut String, stmts: &[Spanned<Stmt>], level: usize, is_fn_body: bool) {
    if stmts.is_empty() {
        indent(out, level);
        out.push_str("pass\n");
        return;
    }
    for (i, spanned) in stmts.iter().enumerate() {
        let is_last = i == stmts.len() - 1;
        emit_stmt(out, &spanned.node, level, is_fn_body && is_last);
    }
}

fn emit_stmt(out: &mut String, stmt: &Stmt, level: usize, implicit_return: bool) {
    match stmt {
        Stmt::Let { name, value } => {
            let val = emit_expr(out, level, value);
            indent(out, level);
            out.push_str(&format!("{} = {}\n", py_name(name), val));
        }
        Stmt::Destructure { bindings, value } => {
            let record = emit_expr(out, level, value);
            for binding in bindings {
                indent(out, level);
                out.push_str(&format!(
                    "{} = {}[\"{}\"]\n",
                    py_name(binding),
                    record,
                    binding
                ));
            }
        }
        Stmt::Guard {
            condition,
            negated,
            body,
            else_body,
            ..
        } => {
            let cond = emit_expr(out, level, condition);
            indent(out, level);
            if *negated {
                out.push_str(&format!("if not ({}):\n", cond));
            } else {
                out.push_str(&format!("if {}:\n", cond));
            }
            let is_ternary = else_body.is_some();
            emit_body(out, body, level + 1, !is_ternary);
            if let Some(eb) = else_body {
                indent(out, level);
                out.push_str("else:\n");
                emit_body(out, eb, level + 1, false);
            }
        }
        Stmt::Match { subject, arms } => {
            emit_match_stmt(out, subject, arms, level);
        }
        Stmt::ForEach {
            binding,
            collection,
            body,
        } => {
            let coll = emit_expr(out, level, collection);
            indent(out, level);
            out.push_str(&format!("for {} in {}:\n", py_name(binding), coll));
            emit_body(out, body, level + 1, false);
        }
        Stmt::ForRange {
            binding,
            start,
            end,
            body,
        } => {
            let s = emit_expr(out, level, start);
            let e = emit_expr(out, level, end);
            indent(out, level);
            out.push_str(&format!(
                "for {} in range(int({}), int({})):\n",
                py_name(binding),
                s,
                e
            ));
            emit_body(out, body, level + 1, false);
        }
        Stmt::While { condition, body } => {
            let cond = emit_expr(out, level, condition);
            indent(out, level);
            out.push_str(&format!("while {}:\n", cond));
            emit_body(out, body, level + 1, false);
        }
        Stmt::Return(expr) => {
            let val = emit_expr(out, level, expr);
            indent(out, level);
            out.push_str(&format!("return {}\n", val));
        }
        Stmt::Break(Some(expr)) => {
            let val = emit_expr(out, level, expr);
            indent(out, level);
            out.push_str(&format!("__break_val = {}\n", val));
            indent(out, level);
            out.push_str("break\n");
        }
        Stmt::Break(None) => {
            indent(out, level);
            out.push_str("break\n");
        }
        Stmt::Continue => {
            indent(out, level);
            out.push_str("continue\n");
        }
        Stmt::Expr(expr) => {
            let val = emit_expr(out, level, expr);
            indent(out, level);
            if implicit_return {
                out.push_str(&format!("return {}\n", val));
            } else {
                out.push_str(&format!("{}\n", val));
            }
        }
    }
}

fn emit_match_stmt(out: &mut String, subject: &Option<Expr>, arms: &[MatchArm], level: usize) {
    let subj_str = match subject {
        Some(e) => emit_expr(out, level, e),
        None => "_subject".to_string(),
    };

    // Use if/elif chain for pattern matching
    for (i, arm) in arms.iter().enumerate() {
        indent(out, level);
        let keyword = if i == 0 { "if" } else { "elif" };
        match &arm.pattern {
            Pattern::Wildcard => {
                if i == 0 {
                    // Wildcard as first arm — just emit body
                    emit_body(out, &arm.body, level, true);
                    return;
                }
                out.push_str("else:\n");
            }
            Pattern::Ok(binding) => {
                out.push_str(&format!(
                    "{} isinstance({}, tuple) and {}[0] == \"ok\":\n",
                    keyword, subj_str, subj_str
                ));
                // `_` is bound like any other name (SPEC.md line 1069); Python
                // accepts `_` as a regular identifier so no special-casing.
                indent(out, level + 1);
                out.push_str(&format!("{} = {}[1]\n", py_name(binding), subj_str));
            }
            Pattern::Err(binding) => {
                out.push_str(&format!(
                    "{} isinstance({}, tuple) and {}[0] == \"err\":\n",
                    keyword, subj_str, subj_str
                ));
                indent(out, level + 1);
                out.push_str(&format!("{} = {}[1]\n", py_name(binding), subj_str));
            }
            Pattern::Literal(lit) => {
                out.push_str(&format!(
                    "{} {} == {}:\n",
                    keyword,
                    subj_str,
                    emit_literal(lit)
                ));
            }
            Pattern::TypeIs { ty, binding } => {
                out.push_str(&format!(
                    "{} isinstance({}, {}):\n",
                    keyword,
                    subj_str,
                    type_to_py(ty)
                ));
                indent(out, level + 1);
                out.push_str(&format!("{} = {}\n", py_name(binding), subj_str));
            }
        }
        emit_body(out, &arm.body, level + 1, true);
    }
}

/// Returns true if the match arm needs statement-level codegen (can't be a simple ternary value).
fn arm_needs_statements(arm: &MatchArm) -> bool {
    // `_` is bound like any other name (SPEC.md line 1069), so wildcard-style
    // bindings need statement-level codegen just like named ones — the body may
    // reference `_`. Plain wildcards also bind `_` to the subject, but the
    // ternary path special-cases that by inlining the subject expression.
    match &arm.pattern {
        Pattern::Ok(_) | Pattern::Err(_) | Pattern::TypeIs { .. } => return true,
        Pattern::Wildcard if body_refs_underscore(&arm.body) => return true,
        _ => {}
    }
    arm.body.len() > 1
        || arm
            .body
            .first()
            .is_some_and(|s| matches!(s.node, Stmt::Let { .. }))
}

/// Emit an expression, potentially writing preamble statements to `out`.
/// Returns the inline expression string.
fn emit_expr(out: &mut String, level: usize, expr: &Expr) -> String {
    match expr {
        Expr::Literal(lit) => emit_literal(lit),
        Expr::Ref(name) => py_name(name),
        Expr::Field {
            object,
            field,
            safe,
        } => {
            let obj = emit_expr(out, level, object);
            if *safe {
                // `.?field` returns None when the object is None *or* the field
                // is missing on a present record. Matches the tree/VM/Cranelift
                // semantics — heterogeneous JSON records (jpar output) commonly
                // lack optional keys.
                format!(
                    "({0}.get(\"{1}\") if {0} is not None else None)",
                    obj, field
                )
            } else {
                format!("{}[\"{}\"]", obj, field)
            }
        }
        Expr::Index {
            object,
            index,
            safe,
        } => {
            let obj = emit_expr(out, level, object);
            if *safe {
                format!("({0}[{1}] if {0} is not None else None)", obj, index)
            } else {
                format!("{}[{}]", obj, index)
            }
        }
        Expr::Call {
            function,
            args,
            unwrap,
        } => {
            if function == "num" && args.len() == 1 {
                let arg = emit_expr(out, level, &args[0]);
                let call = format!(
                    "(lambda s: (\"ok\", float(s)) if s.replace('.','',1).replace('-','',1).isdigit() else (\"err\", s))({})",
                    arg
                );
                return if *unwrap {
                    format!("_ilo_unwrap({})", call)
                } else {
                    call
                };
            }
            if function == "now" && args.is_empty() {
                return "(__import__('time').time())".to_string();
            }
            if function == "sleep" && args.len() == 1 {
                // sleep ms — emit a Python expression that blocks for ms
                // milliseconds and evaluates to None (the ilo Nil sentinel).
                // `__import__('time').sleep` takes seconds, so we divide by
                // 1000 and clamp negatives to 0 to mirror the tree semantics.
                let arg = emit_expr(out, level, &args[0]);
                return format!(
                    "(lambda _ms: (__import__('time').sleep(max(0.0, float(_ms)) / 1000.0), None)[1])({})",
                    arg
                );
            }
            if function == "jpth" && args.len() == 2 {
                let json_arg = emit_expr(out, level, &args[0]);
                let path_arg = emit_expr(out, level, &args[1]);
                let call = format!(
                    "(lambda j, p: (lambda c: (\"ok\", str(c) if not isinstance(c, str) else c))((__import__('functools').reduce(lambda c, k: c[int(k)] if isinstance(c, list) and k.isdigit() else c[k], p.split('.'), __import__('json').loads(j)))) if True else None)({}, {})",
                    json_arg, path_arg
                );
                let call = format!("(lambda: {})()", call);
                return if *unwrap {
                    format!("_ilo_unwrap({})", call)
                } else {
                    call
                };
            }
            if function == "prnt" && args.len() == 1 {
                let arg = emit_expr(out, level, &args[0]);
                // passthrough: print and return the value
                return format!("(lambda _v: (print(_v), _v)[1])({})", arg);
            }
            if function == "rd" && args.len() == 1 {
                let arg = emit_expr(out, level, &args[0]);
                let call = format!("_ilo_rd({})", arg);
                return if *unwrap {
                    format!("_ilo_unwrap({})", call)
                } else {
                    call
                };
            }
            if function == "rd" && args.len() == 2 {
                let path = emit_expr(out, level, &args[0]);
                let fmt = emit_expr(out, level, &args[1]);
                let call = format!("_ilo_rd({}, {})", path, fmt);
                return if *unwrap {
                    format!("_ilo_unwrap({})", call)
                } else {
                    call
                };
            }
            if function == "rdb" && args.len() == 2 {
                let s = emit_expr(out, level, &args[0]);
                let fmt = emit_expr(out, level, &args[1]);
                let call = format!("_ilo_rdb({}, {})", s, fmt);
                return if *unwrap {
                    format!("_ilo_unwrap({})", call)
                } else {
                    call
                };
            }
            if function == "rdl" && args.len() == 1 {
                let arg = emit_expr(out, level, &args[0]);
                let call = format!(
                    "(lambda p: (\"ok\", open(p).read().splitlines()) if __import__('os.path', fromlist=['']).exists(p) else (\"err\", f\"{{p}}: no such file\"))({})",
                    arg
                );
                return if *unwrap {
                    format!("_ilo_unwrap({})", call)
                } else {
                    call
                };
            }
            if function == "wr" && args.len() == 2 {
                let pa = emit_expr(out, level, &args[0]);
                let content = emit_expr(out, level, &args[1]);
                let call = format!(
                    "(lambda p, c: (open(p, 'w').write(c), (\"ok\", p))[1])({}, {})",
                    pa, content
                );
                return if *unwrap {
                    format!("_ilo_unwrap({})", call)
                } else {
                    call
                };
            }
            if function == "wrl" && args.len() == 2 {
                let pa = emit_expr(out, level, &args[0]);
                let lines = emit_expr(out, level, &args[1]);
                let call = format!(
                    "(lambda p, ls: (open(p, 'w').write('\\n'.join(ls) + '\\n'), (\"ok\", p))[1])({}, {})",
                    pa, lines
                );
                return if *unwrap {
                    format!("_ilo_unwrap({})", call)
                } else {
                    call
                };
            }
            if function == "jdmp" && args.len() == 1 {
                let arg = emit_expr(out, level, &args[0]);
                return format!("__import__('json').dumps({})", arg);
            }
            if function == "jpar" && args.len() == 1 {
                let arg = emit_expr(out, level, &args[0]);
                let call = format!("(lambda s: (\"ok\", __import__('json').loads(s)))({})", arg);
                return if *unwrap {
                    format!("_ilo_unwrap({})", call)
                } else {
                    call
                };
            }

            if function == "env" && args.len() == 1 {
                let arg = emit_expr(out, level, &args[0]);
                let call = format!(
                    "(lambda k: (\"ok\", __import__('os').environ[k]) if k in __import__('os').environ else (\"err\", f\"env var '{{k}}' not set\"))({})",
                    arg
                );
                return if *unwrap {
                    format!("_ilo_unwrap({})", call)
                } else {
                    call
                };
            }
            if function == "rnd" && args.is_empty() {
                return "(__import__('random').random())".to_string();
            }
            if function == "rnd" && args.len() == 2 {
                return format!(
                    "float(__import__('random').randint({}, {}))",
                    emit_expr(out, level, &args[0]),
                    emit_expr(out, level, &args[1])
                );
            }
            if function == "rndn" && args.len() == 2 {
                return format!(
                    "float(__import__('random').gauss({}, {}))",
                    emit_expr(out, level, &args[0]),
                    emit_expr(out, level, &args[1])
                );
            }
            if function == "flr" && args.len() == 1 {
                return format!(
                    "float(__import__('math').floor({}))",
                    emit_expr(out, level, &args[0])
                );
            }
            if function == "cel" && args.len() == 1 {
                return format!(
                    "float(__import__('math').ceil({}))",
                    emit_expr(out, level, &args[0])
                );
            }
            if function == "rou" && args.len() == 1 {
                return format!("float(round({}))", emit_expr(out, level, &args[0]));
            }
            if function == "srt" && args.len() == 2 {
                let key_fn = emit_expr(out, level, &args[0]);
                let xs = emit_expr(out, level, &args[1]);
                return format!("sorted({}, key={})", xs, key_fn);
            }
            if function == "trm" && args.len() == 1 {
                return format!("{}.strip()", emit_expr(out, level, &args[0]));
            }
            if function == "unq" && args.len() == 1 {
                let xs = emit_expr(out, level, &args[0]);
                return format!("list(dict.fromkeys({}))", xs);
            }
            if function == "fmt" && !args.is_empty() {
                let tmpl = emit_expr(out, level, &args[0]);
                let rest: Vec<String> =
                    args[1..].iter().map(|a| emit_expr(out, level, a)).collect();
                if rest.is_empty() {
                    return tmpl;
                }
                return format!("({}).format({})", tmpl, rest.join(", "));
            }
            let args_str: Vec<String> = args.iter().map(|a| emit_expr(out, level, a)).collect();
            let call = format!("{}({})", py_name(function), args_str.join(", "));
            if *unwrap {
                format!("_ilo_unwrap({})", call)
            } else {
                call
            }
        }
        Expr::BinOp { op, left, right } => {
            let op_str = match op {
                BinOp::Add => "+",
                BinOp::Subtract => "-",
                BinOp::Multiply => "*",
                BinOp::Divide => "/",
                BinOp::Equals => "==",
                BinOp::NotEquals => "!=",
                BinOp::GreaterThan => ">",
                BinOp::LessThan => "<",
                BinOp::GreaterOrEqual => ">=",
                BinOp::LessOrEqual => "<=",
                BinOp::And => "and",
                BinOp::Or => "or",
                BinOp::Append => {
                    let l = emit_expr(out, level, left);
                    let r = emit_expr(out, level, right);
                    return format!("({} + [{}])", l, r);
                }
            };
            let l = emit_expr(out, level, left);
            let r = emit_expr(out, level, right);
            format!("({} {} {})", l, op_str, r)
        }
        Expr::UnaryOp { op, operand } => {
            let val = emit_expr(out, level, operand);
            match op {
                UnaryOp::Not => format!("(not {})", val),
                UnaryOp::Negate => format!("(-{})", val),
            }
        }
        Expr::Ok(inner) => format!("(\"ok\", {})", emit_expr(out, level, inner)),
        Expr::Err(inner) => format!("(\"err\", {})", emit_expr(out, level, inner)),
        Expr::List(items) => {
            let items_str: Vec<String> = items.iter().map(|i| emit_expr(out, level, i)).collect();
            format!("[{}]", items_str.join(", "))
        }
        Expr::Record { type_name, fields } => {
            let mut parts = vec![format!("\"_type\": \"{}\"", type_name)];
            for (name, val) in fields {
                parts.push(format!("\"{}\": {}", name, emit_expr(out, level, val)));
            }
            format!("{{{}}}", parts.join(", "))
        }
        Expr::Match { subject, arms } => emit_match_expr(out, level, subject, arms),
        Expr::NilCoalesce { value, default } => {
            let v = emit_expr(out, level, value);
            let d = emit_expr(out, level, default);
            format!("({v} if {v} is not None else {d})")
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            let c = emit_expr(out, level, condition);
            let t = emit_expr(out, level, then_expr);
            let e = emit_expr(out, level, else_expr);
            format!("({t} if {c} else {e})")
        }
        Expr::With { object, updates } => {
            let obj = emit_expr(out, level, object);
            let mut parts = vec![format!("**{}", obj)];
            for (name, val) in updates {
                parts.push(format!("\"{}\": {}", name, emit_expr(out, level, val)));
            }
            format!("{{{}}}", parts.join(", "))
        }
        Expr::MakeClosure { fn_name, captures } => {
            // Emit as `lambda *a: fn_name(*a, c1, c2, ...)` so Python sees a
            // callable that prepends the per-item args and appends captures,
            // matching the tree interpreter's call shape.
            let caps: Vec<String> = captures.iter().map(|c| emit_expr(out, level, c)).collect();
            let cap_str = if caps.is_empty() {
                String::new()
            } else {
                format!(", {}", caps.join(", "))
            };
            format!("(lambda *_a: {}(*_a{}))", py_name(fn_name), cap_str)
        }
    }
}

fn emit_match_expr(
    out: &mut String,
    level: usize,
    subject: &Option<Box<Expr>>,
    arms: &[MatchArm],
) -> String {
    let needs_statements = arms.iter().any(arm_needs_statements);

    if needs_statements {
        return emit_match_expr_complex(out, level, subject, arms);
    }

    // Simple path: emit as a chained ternary expression
    let subj = match subject {
        Some(e) => emit_expr(out, level, e),
        None => "_subject".to_string(),
    };

    let mut parts: Vec<String> = Vec::new();
    let mut default = "None".to_string();

    for arm in arms {
        let arm_val = emit_arm_value(out, level, &arm.body);
        match &arm.pattern {
            Pattern::Wildcard => {
                default = arm_val;
            }
            Pattern::Literal(lit) => {
                parts.push(format!(
                    "{} if {} == {} else",
                    arm_val,
                    subj,
                    emit_literal(lit)
                ));
            }
            Pattern::Ok(_) => {
                parts.push(format!(
                    "{} if isinstance({}, tuple) and {}[0] == \"ok\" else",
                    arm_val, subj, subj
                ));
            }
            Pattern::Err(_) => {
                parts.push(format!(
                    "{} if isinstance({}, tuple) and {}[0] == \"err\" else",
                    arm_val, subj, subj
                ));
            }
            Pattern::TypeIs { ty, .. } => {
                parts.push(format!(
                    "{} if isinstance({}, {}) else",
                    arm_val,
                    subj,
                    type_to_py(ty)
                ));
            }
        }
    }

    if parts.is_empty() {
        return default;
    }

    // Build: val1 if cond1 else val2 if cond2 else default
    format!("({} {})", parts.join(" "), default)
}

/// Emit a complex match expression using if/elif chain with a temp variable.
/// Writes statements to `out` and returns the temp variable name.
fn emit_match_expr_complex(
    out: &mut String,
    level: usize,
    subject: &Option<Box<Expr>>,
    arms: &[MatchArm],
) -> String {
    let subj_str = match subject {
        Some(e) => emit_expr(out, level, e),
        None => "_subject".to_string(),
    };
    let tmp = "_m".to_string();

    for (i, arm) in arms.iter().enumerate() {
        indent(out, level);
        let keyword = if i == 0 { "if" } else { "elif" };
        match &arm.pattern {
            Pattern::Wildcard => {
                if i == 0 {
                    // Wildcard as first arm — emit body and assign last expr to tmp
                    emit_match_arm_body_to_tmp(out, &arm.body, level, &tmp);
                    return tmp;
                }
                out.push_str("else:\n");
            }
            Pattern::Ok(binding) => {
                out.push_str(&format!(
                    "{} isinstance({}, tuple) and {}[0] == \"ok\":\n",
                    keyword, subj_str, subj_str
                ));
                // `_` is bound like any other name (SPEC.md line 1069); Python
                // accepts `_` as a regular identifier so no special-casing.
                indent(out, level + 1);
                out.push_str(&format!("{} = {}[1]\n", py_name(binding), subj_str));
            }
            Pattern::Err(binding) => {
                out.push_str(&format!(
                    "{} isinstance({}, tuple) and {}[0] == \"err\":\n",
                    keyword, subj_str, subj_str
                ));
                indent(out, level + 1);
                out.push_str(&format!("{} = {}[1]\n", py_name(binding), subj_str));
            }
            Pattern::Literal(lit) => {
                out.push_str(&format!(
                    "{} {} == {}:\n",
                    keyword,
                    subj_str,
                    emit_literal(lit)
                ));
            }
            Pattern::TypeIs { ty, binding } => {
                out.push_str(&format!(
                    "{} isinstance({}, {}):\n",
                    keyword,
                    subj_str,
                    type_to_py(ty)
                ));
                indent(out, level + 1);
                out.push_str(&format!("{} = {}\n", py_name(binding), subj_str));
            }
        }
        emit_match_arm_body_to_tmp(out, &arm.body, level + 1, &tmp);
    }

    tmp
}

/// Emit a match arm body, assigning the last expression to a temp variable.
fn emit_match_arm_body_to_tmp(out: &mut String, body: &[Spanned<Stmt>], level: usize, tmp: &str) {
    if body.is_empty() {
        indent(out, level);
        out.push_str(&format!("{} = None\n", tmp));
        return;
    }
    for (i, spanned) in body.iter().enumerate() {
        let is_last = i == body.len() - 1;
        let stmt = &spanned.node;
        if is_last {
            // Last statement: assign its value to tmp instead of emitting as-is
            match stmt {
                Stmt::Expr(expr) => {
                    let val = emit_expr(out, level, expr);
                    indent(out, level);
                    out.push_str(&format!("{} = {}\n", tmp, val));
                }
                _ => {
                    // Non-expression last stmt (e.g. Let) — emit it, then assign None
                    emit_stmt(out, stmt, level, false);
                    indent(out, level);
                    out.push_str(&format!("{} = None\n", tmp));
                }
            }
        } else {
            emit_stmt(out, stmt, level, false);
        }
    }
}

fn emit_arm_value(out: &mut String, level: usize, body: &[Spanned<Stmt>]) -> String {
    if let Some(last) = body.last() {
        match &last.node {
            Stmt::Expr(e) => emit_expr(out, level, e),
            _ => "None".to_string(),
        }
    } else {
        "None".to_string()
    }
}

fn emit_literal(lit: &Literal) -> String {
    match lit {
        Literal::Number(n) => {
            if *n == (*n as i64) as f64 {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        }
        Literal::Text(s) => {
            let escaped = s
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\r', "\\r");
            format!("\"{}\"", escaped)
        }
        Literal::Bool(b) => {
            if *b {
                "True".to_string()
            } else {
                "False".to_string()
            }
        }
        Literal::Nil => "None".to_string(),
    }
}

fn emit_type(ty: &Type) -> String {
    match ty {
        Type::Number => "float".to_string(),
        Type::Text => "str".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Any => "Any".to_string(),
        Type::Optional(inner) => format!("{} | None", emit_type(inner)),
        Type::List(inner) => format!("list[{}]", emit_type(inner)),
        Type::Map(k, v) => format!("dict[{}, {}]", emit_type(k), emit_type(v)),
        Type::Sum(_) => "str".to_string(),
        Type::Result(ok, err) => format!("tuple[str, {} | {}]", emit_type(ok), emit_type(err)),
        Type::Fn(params, ret) => {
            let ps: Vec<_> = params.iter().map(emit_type).collect();
            format!("Callable[[{}], {}]", ps.join(", "), emit_type(ret))
        }
        Type::Named(_name) => "dict".to_string(),
    }
}

/// Convert ilo names (kebab-case) to Python (snake_case)
fn py_name(name: &str) -> String {
    name.replace('-', "_")
}

/// True if any stmt in the body references the ilo wildcard binding `_`.
/// Used by the Python ternary fast-path to fall back to statement codegen
/// when a `_:body` arm actually reads the subject via `_`.
fn body_refs_underscore(body: &[Spanned<Stmt>]) -> bool {
    body.iter().any(|s| stmt_refs_underscore(&s.node))
}

fn stmt_refs_underscore(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Let { value, .. } => expr_refs_underscore(value),
        Stmt::Expr(e) => expr_refs_underscore(e),
        _ => false,
    }
}

fn expr_refs_underscore(expr: &Expr) -> bool {
    match expr {
        Expr::Ref(name) => name == "_",
        Expr::Literal(_) => false,
        Expr::BinOp { left, right, .. } => {
            expr_refs_underscore(left) || expr_refs_underscore(right)
        }
        Expr::UnaryOp { operand, .. } => expr_refs_underscore(operand),
        Expr::Call { args, .. } => args.iter().any(expr_refs_underscore),
        Expr::Ok(e) | Expr::Err(e) => expr_refs_underscore(e),
        Expr::Field { object, .. } => expr_refs_underscore(object),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_refs_underscore(condition)
                || expr_refs_underscore(then_expr)
                || expr_refs_underscore(else_expr)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer;
    use crate::parser;

    fn parse_and_emit(source: &str) -> String {
        let tokens: Vec<crate::lexer::Token> = lexer::lex(source)
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let program = parser::parse_tokens(tokens).unwrap();
        emit(&program)
    }

    fn parse_file_and_emit(path: &str) -> String {
        let source = std::fs::read_to_string(path).unwrap();
        parse_and_emit(&source)
    }

    #[test]
    fn emit_simple_function() {
        let py = parse_and_emit("tot p:n q:n r:n>n;s=*p q;t=*s r;+s t");
        assert!(py.contains("def tot(p: float, q: float, r: float) -> float:"));
        assert!(py.contains("s = (p * q)"));
        assert!(py.contains("t = (s * r)"));
        assert!(py.contains("return (s + t)"));
    }

    #[test]
    fn emit_guard() {
        let py = parse_and_emit(r#"cls sp:n>t;>=sp 1000{"gold"};>=sp 500{"silver"};"bronze""#);
        assert!(py.contains("def cls(sp: float) -> str:"));
        assert!(py.contains("if (sp >= 1000):"));
        assert!(py.contains("return \"gold\""));
        assert!(py.contains("return \"bronze\""));
    }

    #[test]
    fn emit_ok_err() {
        let py = parse_and_emit("f x:n>R n t;~x");
        assert!(py.contains("return (\"ok\", x)"));
    }

    #[test]
    fn emit_err_expr() {
        let py = parse_and_emit(r#"f x:n>R n t;^"bad""#);
        assert!(py.contains("return (\"err\", \"bad\")"));
    }

    #[test]
    fn emit_let_binding() {
        let py = parse_and_emit("f x:n>n;y=+x 1;y");
        assert!(py.contains("y = (x + 1)"));
        assert!(py.contains("return y"));
    }

    #[test]
    fn emit_foreach() {
        let py = parse_and_emit("f xs:L n>n;@x xs{+x 1}");
        assert!(py.contains("for x in xs:"));
    }

    #[test]
    fn emit_record() {
        let py = parse_and_emit("f x:n>point;point x:x y:10");
        assert!(py.contains("\"_type\": \"point\""));
        assert!(py.contains("\"x\": x"));
        assert!(py.contains("\"y\": 10"));
    }

    #[test]
    fn emit_with() {
        let py = parse_and_emit("f x:order>order;x with total:100");
        assert!(py.contains("**x"));
        assert!(py.contains("\"total\": 100"));
    }

    #[test]
    fn emit_field_access() {
        let py = parse_and_emit("f x:order>n;x.total");
        assert!(py.contains("x[\"total\"]"));
    }

    #[test]
    fn emit_type_def() {
        let py = parse_and_emit("type point{x:n;y:n}");
        assert!(py.contains("# type point = {x: float, y: float}"));
    }

    #[test]
    fn emit_tool() {
        let py = parse_and_emit(
            r#"tool send-email"Send an email" to:t body:t>R _ t timeout:30,retry:3"#,
        );
        assert!(py.contains("def send_email(to: str, body: str)"));
        assert!(py.contains("Send an email"));
        assert!(py.contains("raise NotImplementedError"));
    }

    #[test]
    fn emit_example_01() {
        let py = parse_file_and_emit("examples/01-simple-function.ilo");
        assert!(py.contains("def tot("));
        assert!(py.contains("return (s + t)"));
    }

    #[test]
    fn emit_example_02() {
        let py = parse_file_and_emit("examples/02-with-dependencies.ilo");
        assert!(py.contains("def prc("));
    }

    #[test]
    fn emit_example_03() {
        let py = parse_file_and_emit("examples/03-data-transform.ilo");
        assert!(py.contains("def cls("));
        assert!(py.contains("def sms("));
    }

    #[test]
    fn emit_example_04() {
        let py = parse_file_and_emit("examples/04-tool-interaction.ilo");
        assert!(py.contains("def ntf("));
    }

    #[test]
    fn emit_example_05() {
        let py = parse_file_and_emit("examples/05-workflow.ilo");
        assert!(py.contains("def chk("));
    }

    #[test]
    fn emit_match_stmt() {
        let py = parse_and_emit(r#"f x:t>n;?x{"a":1;"b":2;_:0}"#);
        assert!(py.contains("if x == \"a\":"));
        assert!(py.contains("return 1"));
        assert!(py.contains("elif x == \"b\":"));
        assert!(py.contains("return 2"));
        assert!(py.contains("else:"));
        assert!(py.contains("return 0"));
    }

    #[test]
    fn emit_negated_guard() {
        let py = parse_and_emit(r#"f x:b>t;!x{"nope"};x"#);
        assert!(py.contains("if not (x):"));
        assert!(py.contains("return \"nope\""));
    }

    #[test]
    fn emit_logical_not() {
        let py = parse_and_emit("f x:b>b;!x");
        assert!(py.contains("(not x)"));
    }

    #[test]
    fn emit_kebab_to_snake() {
        let py = parse_and_emit("f>t;make-id()");
        assert!(py.contains("make_id()"));
    }

    #[test]
    fn emit_logical_and_or() {
        let py = parse_and_emit("f a:b b:b>b;&a b");
        assert!(py.contains("(a and b)"));
        let py = parse_and_emit("f a:b b:b>b;|a b");
        assert!(py.contains("(a or b)"));
    }

    #[test]
    fn emit_len_builtin() {
        let py = parse_and_emit(r#"f s:t>n;len s"#);
        assert!(py.contains("len(s)"));
    }

    #[test]
    fn emit_list_append() {
        let py = parse_and_emit("f xs:L n>L n;+=xs 1");
        assert!(py.contains("(xs + [1])"));
    }

    #[test]
    fn emit_index_access() {
        let py = parse_and_emit("f xs:L n>n;xs.0");
        assert!(py.contains("xs[0]"));
    }

    #[test]
    fn emit_str_builtin() {
        let py = parse_and_emit("f n:n>t;str n");
        assert!(py.contains("str(n)"));
    }

    #[test]
    fn emit_num_builtin() {
        let py = parse_and_emit("f s:t>R n t;num s");
        assert!(py.contains("float(s)"));
        assert!(py.contains("\"ok\""));
        assert!(py.contains("\"err\""));
    }

    #[test]
    fn emit_abs_builtin() {
        let py = parse_and_emit("f n:n>n;abs n");
        assert!(py.contains("abs(n)"));
    }

    #[test]
    fn emit_min_max_builtin() {
        let py = parse_and_emit("f a:n b:n>n;min a b");
        assert!(py.contains("min(a, b)"));
        let py = parse_and_emit("f a:n b:n>n;max a b");
        assert!(py.contains("max(a, b)"));
    }

    #[test]
    fn emit_zero_arg_call() {
        let py = parse_and_emit("f>t;make-id()");
        assert!(py.contains("make_id()"));
    }

    #[test]
    fn emit_flr_cel_builtin() {
        let py = parse_and_emit("f n:n>n;flr n");
        assert!(py.contains("__import__('math').floor(n)"));
        let py = parse_and_emit("f n:n>n;cel n");
        assert!(py.contains("__import__('math').ceil(n)"));
    }

    #[test]
    fn emit_env_builtin() {
        let py = parse_and_emit(r#"f k:t>R t t;env k"#);
        assert!(py.contains("os"), "should use os module");
        assert!(py.contains("environ"), "should access environ");
    }

    #[test]
    fn emit_nested_prefix() {
        // +*a b c → (a * b) + c
        let py = parse_and_emit("f a:n b:n c:n>n;+*a b c");
        assert!(py.contains("((a * b) + c)"), "got: {}", py);
    }

    #[test]
    fn emit_binop_divide() {
        let py = parse_and_emit("f a:n b:n>n;/a b");
        assert!(py.contains("(a / b)"), "got: {}", py);
    }

    #[test]
    fn emit_binop_equals() {
        let py = parse_and_emit("f a:n b:n>b;=a b");
        assert!(py.contains("(a == b)"), "got: {}", py);
    }

    #[test]
    fn emit_binop_not_equals() {
        let py = parse_and_emit("f a:n b:n>b;!=a b");
        assert!(py.contains("(a != b)"), "got: {}", py);
    }

    #[test]
    fn emit_binop_greater_than() {
        let py = parse_and_emit("f a:n b:n>b;>a b");
        assert!(py.contains("(a > b)"), "got: {}", py);
    }

    #[test]
    fn emit_binop_less_than() {
        let py = parse_and_emit("f a:n b:n>b;<a b");
        assert!(py.contains("(a < b)"), "got: {}", py);
    }

    #[test]
    fn emit_binop_less_or_equal() {
        let py = parse_and_emit("f a:n b:n>b;<=a b");
        assert!(py.contains("(a <= b)"), "got: {}", py);
    }

    #[test]
    fn emit_unary_negate() {
        let py = parse_and_emit("f x:n>n;-x");
        assert!(py.contains("(-x)"), "got: {}", py);
    }

    #[test]
    fn emit_list_literal() {
        let py = parse_and_emit("f>L n;[1, 2, 3]");
        assert!(py.contains("[1, 2, 3]"), "got: {}", py);
    }

    #[test]
    fn emit_bool_literal() {
        let py = parse_and_emit("f>b;true");
        assert!(py.contains("True"), "got: {}", py);
        let py = parse_and_emit("f>b;false");
        assert!(py.contains("False"), "got: {}", py);
    }

    #[test]
    fn emit_float_literal() {
        let py = parse_and_emit("f>n;3.14");
        assert!(py.contains("3.14"), "got: {}", py);
    }

    #[test]
    fn emit_match_expr_ok_err_patterns() {
        // Match expression (in let binding) with ~v and ^e patterns
        let py = parse_and_emit(r#"f x:R n t>t;y=?x{~v:"ok";^e:e};y"#);
        assert!(py.contains("isinstance(x, tuple)"), "got: {}", py);
        assert!(py.contains(r#"x[0] == "ok""#), "got: {}", py);
        assert!(py.contains(r#"x[0] == "err""#), "got: {}", py);
    }

    #[test]
    fn emit_match_expr_wildcard() {
        // Match expression with wildcard pattern
        let py = parse_and_emit(r#"f x:t>n;y=?x{"a":1;_:0};y"#);
        assert!(py.contains("1 if x == \"a\" else"), "got: {}", py);
        assert!(py.contains(" 0)"), "got: {}", py);
    }

    #[test]
    fn emit_match_expr_subjectless() {
        // Subjectless match expression ?{...}
        let py = parse_and_emit(r#"f>n;y=?{true:1;_:0};y"#);
        assert!(py.contains("_subject"), "got: {}", py);
    }

    #[test]
    fn emit_match_stmt_wildcard_first() {
        let py = parse_and_emit(r#"f x:n>t;?x{_:"always";1:"one"}"#);
        // Wildcard as first arm emits body directly without if/elif
        assert!(!py.contains("if"), "got: {}", py);
        assert!(py.contains("\"always\""), "got: {}", py);
    }

    #[test]
    fn emit_match_expr_ok_binding_used() {
        // Match expr where Ok binding `v` is used in the body
        let py = parse_and_emit(r#"f x:R n t>n;y=?x{~v:v;^e:0};y"#);
        // Should use complex path with if/elif and temp var
        assert!(py.contains("v = x[1]"), "should bind v: got: {}", py);
        assert!(
            py.contains("_m = v"),
            "should assign v to temp: got: {}",
            py
        );
        assert!(
            py.contains("_m = 0"),
            "should assign 0 to temp: got: {}",
            py
        );
        assert!(
            py.contains("y = _m"),
            "should assign temp to y: got: {}",
            py
        );
    }

    #[test]
    fn emit_match_expr_let_in_arm() {
        // Match expr with Let binding inside arm body
        let py = parse_and_emit(r#"f x:R n t>n;y=?x{~v:z=+v 1;z;^e:0};y"#);
        // Should use complex path
        assert!(py.contains("v = x[1]"), "should bind v: got: {}", py);
        assert!(
            py.contains("z = (v + 1)"),
            "should emit let binding: got: {}",
            py
        );
        assert!(
            py.contains("_m = z"),
            "should assign z to temp: got: {}",
            py
        );
        assert!(
            py.contains("y = _m"),
            "should assign temp to y: got: {}",
            py
        );
    }

    #[test]
    fn emit_match_expr_simple_stays_ternary() {
        // Literal-only arms (no Ok/Err/TypeIs bindings) take the ternary path.
        // The previous shape of this test (`~_:1;^_:0`) is no longer covered
        // by the ternary because `_` now binds like any other name — see
        // SPEC.md "In any binding position the name `_` is permitted" — so
        // Ok/Err arms always need statement-level codegen to emit the bind.
        let py = parse_and_emit(r#"f x:n>n;y=?x{1:10;2:20;_:0};y"#);
        assert!(
            py.contains("10 if x == 1"),
            "should use ternary for literal arms: got: {}",
            py
        );
    }

    #[test]
    fn emit_empty_guard_body() {
        // Guard with empty brace body → emit_body with empty stmts → "pass" (L65-67)
        // Also exercises parse_expr_or_guard returning a Guard (parser L621-625)
        let py = parse_and_emit("f x:b>n;x{};0");
        assert!(
            py.contains("pass"),
            "expected 'pass' for empty body in: {py}"
        );
    }

    #[test]
    fn emit_match_expr_wildcard_only() {
        // Match expression with only a wildcard arm → parts.is_empty() → return default (L301)
        let py = parse_and_emit("f>n;x=?{_:42};x");
        assert!(py.contains("42"), "expected 42 as default in: {py}");
    }

    #[test]
    fn emit_match_expr_complex_let_arm() {
        // Match expr with Let as last stmt in arm body → arm_needs_statements=true → complex path
        // Arm 1 body is just `z=2` (Let stmt) → last stmt is Let → _m = None (L379-383)
        // Syntax: arm bodies use `;` not `{}` — `1:z=2` means arm 1 body is [Let{z=2}]
        let py = parse_and_emit("f x:n>n;y=?x{1:z=2;_:0};y");
        assert!(py.contains("_m"), "expected temp var _m in: {py}");
        assert!(py.contains("None"), "expected None assignment in: {py}");
    }

    #[test]
    fn emit_match_expr_complex_literal() {
        // Match expr needing complex emit with a literal pattern → Pattern::Literal in complex (L349-354)
        // Arm body has 2 stmts (Let + Expr) → arm_needs_statements=true → complex path
        let py = parse_and_emit("f x:n>n;y=?x{1:z=1;+z 1;_:0};y");
        assert!(
            py.contains("== 1"),
            "expected literal pattern comparison in: {py}"
        );
    }

    #[test]
    fn emit_match_expr_complex_no_subject() {
        // Match expr with no subject, complex (needs statements) → "_subject" default (L313)
        // Wildcard with multi-stmt body → complex path, no subject
        let py = parse_and_emit("f>n;y=?{_:z=1;+z 1};y");
        assert!(py.contains("_m"), "expected temp var in: {py}");
    }

    #[test]
    fn emit_match_expr_complex_wildcard_first() {
        // Match expr where first arm is wildcard in complex emit path → return tmp early (L322-325)
        // Wildcard with multi-stmt body (needs_statements=true) is arm index 0
        // Note: wildcard must come LAST in actual ilo programs; here we test codegen behavior only
        // We pass AST directly to bypass parser validation of arm order
        use crate::ast::{Expr, Literal, MatchArm, Pattern, Spanned, Stmt};
        let tokens: Vec<crate::lexer::Token> = lexer::lex("f>n;42")
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let mut prog = parser::parse_tokens(tokens).unwrap();
        // Replace the function body with a let binding to a match expr
        // match expr: first arm is wildcard with multi-stmt body
        let match_expr = Expr::Match {
            subject: None,
            arms: vec![MatchArm {
                pattern: Pattern::Wildcard,
                body: vec![
                    Spanned::unknown(Stmt::Let {
                        name: "z".to_string(),
                        value: Expr::Literal(Literal::Number(1.0)),
                    }),
                    Spanned::unknown(Stmt::Expr(Expr::Ref("z".to_string()))),
                ],
            }],
        };
        if let crate::ast::Decl::Function { ref mut body, .. } = prog.declarations[0] {
            *body = vec![Spanned::unknown(Stmt::Expr(match_expr))];
        }
        let py = emit(&prog);
        assert!(py.contains("_m"), "expected temp var in: {py}");
    }

    #[test]
    fn emit_match_arm_body_to_tmp_empty_body() {
        // Cover L365-367: emit_match_arm_body_to_tmp called with empty body.
        // Inject a complex match (Ok pattern → needs_statements=true) with one arm having empty body.
        use crate::ast::{Expr, Literal, MatchArm, Pattern, Spanned, Stmt};
        let tokens: Vec<crate::lexer::Token> = lexer::lex("f>n;42")
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let mut prog = parser::parse_tokens(tokens).unwrap();
        // Match expr: Ok("v") arm with empty body, Wildcard arm with Literal(0)
        let match_expr = Expr::Match {
            subject: Some(Box::new(Expr::Literal(Literal::Number(0.0)))),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Ok("v".to_string()), // named binding → needs_statements=true
                    body: vec![],                          // empty body → L365-367
                },
                MatchArm {
                    pattern: Pattern::Wildcard,
                    body: vec![Spanned::unknown(Stmt::Expr(Expr::Literal(
                        Literal::Number(0.0),
                    )))],
                },
            ],
        };
        if let crate::ast::Decl::Function { ref mut body, .. } = prog.declarations[0] {
            *body = vec![Spanned::unknown(Stmt::Expr(match_expr))];
        }
        let py = emit(&prog);
        // The arm body was empty → `_m = None` was emitted
        assert!(
            py.contains("= None"),
            "expected '= None' for empty arm body in: {py}"
        );
    }

    #[test]
    fn emit_arm_value_non_expr_last_stmt() {
        // Cover L396: emit_arm_value where body.last() is not Stmt::Expr.
        // Inject a simple match arm (arm_needs_statements=false) with body=[Stmt::Guard].
        use crate::ast::{Expr, Literal, MatchArm, Pattern, Spanned, Stmt};
        let tokens: Vec<crate::lexer::Token> = lexer::lex("f>n;42")
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let mut prog = parser::parse_tokens(tokens).unwrap();
        // Match expr: arm with body=[Guard] (len==1, not Let → arm_needs_statements=false → simple path)
        let match_expr = Expr::Match {
            subject: Some(Box::new(Expr::Literal(Literal::Number(0.0)))),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Literal(Literal::Number(1.0)),
                    body: vec![Spanned::unknown(Stmt::Guard {
                        condition: Expr::Literal(Literal::Bool(true)),
                        negated: false,
                        body: vec![],
                        else_body: None,
                        braceless: false,
                    })],
                },
                MatchArm {
                    pattern: Pattern::Wildcard,
                    body: vec![Spanned::unknown(Stmt::Expr(Expr::Literal(
                        Literal::Number(0.0),
                    )))],
                },
            ],
        };
        if let crate::ast::Decl::Function { ref mut body, .. } = prog.declarations[0] {
            *body = vec![Spanned::unknown(Stmt::Expr(match_expr))];
        }
        let py = emit(&prog);
        // The Guard arm returns None from emit_arm_value (L396)
        assert!(
            py.contains("None"),
            "expected None for guard-body arm in: {py}"
        );
    }

    #[test]
    fn emit_arm_value_empty_body() {
        // Cover L399: emit_arm_value where body is empty (body.last() returns None).
        // Inject a simple match arm (arm_needs_statements=false) with body=[].
        use crate::ast::{Expr, Literal, MatchArm, Pattern, Spanned, Stmt};
        let tokens: Vec<crate::lexer::Token> = lexer::lex("f>n;42")
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let mut prog = parser::parse_tokens(tokens).unwrap();
        // Match expr: Literal arm with empty body (len==0, not Ok/Err with binding → simple path)
        let match_expr = Expr::Match {
            subject: Some(Box::new(Expr::Literal(Literal::Number(0.0)))),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Literal(Literal::Number(1.0)),
                    body: vec![], // empty → L399
                },
                MatchArm {
                    pattern: Pattern::Wildcard,
                    body: vec![Spanned::unknown(Stmt::Expr(Expr::Literal(
                        Literal::Number(0.0),
                    )))],
                },
            ],
        };
        if let crate::ast::Decl::Function { ref mut body, .. } = prog.declarations[0] {
            *body = vec![Spanned::unknown(Stmt::Expr(match_expr))];
        }
        let py = emit(&prog);
        // The empty arm returns None from emit_arm_value (L399)
        assert!(
            py.contains("None"),
            "expected None for empty-body arm in: {py}"
        );
    }

    #[test]
    fn emit_while_loop() {
        let py = parse_and_emit("f>n;i=0;wh <i 5{i=+i 1};i");
        assert!(py.contains("while (i < 5):"), "got: {}", py);
    }

    #[test]
    fn emit_ret_statement() {
        let py = parse_and_emit("f x:n>n;ret +x 1");
        assert!(py.contains("return (x + 1)"), "got: {}", py);
    }

    #[test]
    fn emit_ret_in_guard() {
        let py = parse_and_emit(r#"f x:n>t;>x 0{ret "pos"};"neg""#);
        assert!(py.contains("return \"pos\""), "got: {}", py);
        assert!(py.contains("return \"neg\""), "got: {}", py);
    }

    #[test]
    fn emit_error_decl_skipped() {
        // A parse error produces a Decl::Error poison node; emit() should skip it silently.
        // We create a program with a valid function + an error node directly.
        use crate::ast::{Decl, Span};
        let tokens: Vec<crate::lexer::Token> = lexer::lex("f x:n>n;*x 2")
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let mut prog = parser::parse_tokens(tokens).unwrap();
        // Inject a poison node
        prog.declarations.push(Decl::Error {
            span: Span { start: 0, end: 1 },
        });
        let py = emit(&prog);
        // The valid function should appear; the error node should be silently skipped
        assert!(py.contains("def f("), "missing valid function in: {py}");
        // Output should contain the function body, not any error artifacts
        assert!(py.contains("return"), "missing return in: {py}");
    }

    // ---- Coverage tests for newer features ----

    #[test]
    fn emit_for_range() {
        let py = parse_and_emit("f>n;@i 0..5{i}");
        assert!(py.contains("for i in range("), "got: {py}");
    }

    #[test]
    fn emit_nil_coalesce() {
        let py = parse_and_emit("f x:n>n;x??42");
        assert!(py.contains("is not None else"), "got: {py}");
    }

    #[test]
    fn emit_break_no_value() {
        let py = parse_and_emit("f>n;wh true{brk}");
        assert!(py.contains("break"), "got: {py}");
    }

    #[test]
    fn emit_break_with_value() {
        let py = parse_and_emit("f>n;wh true{brk 99}");
        assert!(py.contains("__break_val = 99"), "got: {py}");
        assert!(py.contains("break"), "got: {py}");
    }

    #[test]
    fn emit_continue() {
        let py = parse_and_emit("f>n;wh true{cnt}");
        assert!(py.contains("continue"), "got: {py}");
    }

    #[test]
    fn emit_alias() {
        let py = parse_and_emit("alias res R n t\nf x:n>res;~x");
        assert!(py.contains("# alias res"), "got: {py}");
    }

    #[test]
    fn emit_safe_field() {
        let py = parse_and_emit("f x:n>n;x.?name");
        assert!(py.contains("is not None else None"), "got: {py}");
    }

    #[test]
    fn emit_safe_index() {
        let py = parse_and_emit("f xs:L n>n;xs.?0");
        assert!(py.contains("is not None else None"), "got: {py}");
    }

    #[test]
    fn emit_destructure() {
        let py = parse_and_emit("type pt{x:n;y:n}\nf p:pt>n;{x;y}=p;+x y");
        assert!(py.contains("[\"x\"]"), "got: {py}");
        assert!(py.contains("[\"y\"]"), "got: {py}");
    }

    #[test]
    fn emit_now_builtin() {
        let py = parse_and_emit("f>n;now");
        assert!(py.contains("time"), "got: {py}");
    }

    #[test]
    fn emit_jdmp_builtin() {
        let py = parse_and_emit(r#"f x:n>t;jdmp x"#);
        assert!(py.contains("json"), "got: {py}");
        assert!(py.contains("dumps"), "got: {py}");
    }

    #[test]
    fn emit_jpar_builtin() {
        let py = parse_and_emit(r#"f s:t>R t t;jpar s"#);
        assert!(py.contains("json"), "got: {py}");
        assert!(py.contains("loads"), "got: {py}");
    }

    #[test]
    fn emit_jpth_builtin() {
        let py = parse_and_emit(r#"f s:t>R t t;jpth s "a.b""#);
        assert!(py.contains("reduce"), "got: {py}");
    }

    #[test]
    fn emit_rdl_builtin() {
        let py = parse_and_emit(r#"f p:t>R t t;rdl p"#);
        assert!(py.contains("splitlines"), "got: {py}");
    }

    #[test]
    fn emit_wr_builtin() {
        let py = parse_and_emit(r#"f p:t c:t>R t t;wr p c"#);
        assert!(py.contains("open("), "got: {py}");
        assert!(py.contains("write"), "got: {py}");
    }

    #[test]
    fn emit_wrl_builtin() {
        let py = parse_and_emit(r#"f p:t xs:L t>R t t;wrl p xs"#);
        assert!(py.contains("join"), "got: {py}");
        assert!(py.contains("write"), "got: {py}");
    }

    #[test]
    fn emit_rdb_builtin() {
        let py = parse_and_emit(r#"f s:t>R t t;rdb s "json""#);
        assert!(py.contains("_ilo_rdb"), "got: {py}");
    }

    #[test]
    fn emit_rnd_zero_args() {
        let py = parse_and_emit("f>n;rnd");
        assert!(py.contains("random"), "got: {py}");
    }

    #[test]
    fn emit_rnd_two_args() {
        let py = parse_and_emit("f a:n b:n>n;rnd a b");
        assert!(py.contains("randint"), "got: {py}");
    }

    #[test]
    fn emit_srt_two_args() {
        let py = parse_and_emit("f key:n xs:L n>L n;srt key xs");
        assert!(py.contains("sorted"), "got: {py}");
        assert!(py.contains("key="), "got: {py}");
    }

    #[test]
    fn emit_trm_builtin() {
        let py = parse_and_emit("f s:t>t;trm s");
        assert!(py.contains("strip()"), "got: {py}");
    }

    #[test]
    fn emit_unq_builtin() {
        let py = parse_and_emit("f xs:L n>L n;unq xs");
        assert!(py.contains("dict.fromkeys"), "got: {py}");
    }

    #[test]
    fn emit_fmt_builtin() {
        let py = parse_and_emit(r#"f a:n b:n>t;fmt "{}:{}" a b"#);
        assert!(py.contains("format("), "got: {py}");
    }

    #[test]
    fn emit_fmt_one_arg() {
        // fmt with only template (no substitution args) → returns template as-is
        let py = parse_and_emit(r#"f s:t>t;fmt s"#);
        assert!(py.contains("return s"), "got: {py}");
    }

    #[test]
    fn emit_type_optional() {
        let py = parse_and_emit("f x:O n>O n;x");
        assert!(py.contains("float | None"), "got: {py}");
    }

    #[test]
    fn emit_type_map() {
        let py = parse_and_emit("f m:M t n>M t n;m");
        assert!(py.contains("dict[str, float]"), "got: {py}");
    }

    #[test]
    fn emit_type_sum() {
        let py = parse_and_emit("f x:S a b>S a b;x");
        assert!(py.contains("str"), "got: {py}");
    }

    #[test]
    fn emit_rd_two_args() {
        let py = parse_and_emit(r#"f p:t>R t t;rd p "json""#);
        assert!(py.contains("_ilo_rd"), "got: {py}");
    }

    // ── Uses unwrap helper (line 50) ──────────────────────────────────────────

    #[test]
    fn emit_uses_unwrap_helper_included() {
        // Inject a program with unwrap=true call via AST directly
        use crate::ast::*;
        let prog = Program {
            declarations: vec![Decl::Function {
                name: "f".into(),
                params: vec![Param {
                    name: "s".into(),
                    ty: Type::Text,
                }],
                return_type: Type::Number,
                body: vec![Spanned::unknown(Stmt::Expr(Expr::Call {
                    function: "num".into(),
                    args: vec![Expr::Ref("s".into())],
                    unwrap: true,
                }))],
                span: Span::UNKNOWN,
            }],
            source: None,
        };
        let py = emit(&prog);
        assert!(
            py.contains("def _ilo_unwrap"),
            "unwrap helper should be defined: {py}"
        );
    }

    // ── Guard with else_body (ternary as stmt, lines 267-269) ─────────────────

    #[test]
    fn emit_guard_with_else_body() {
        // Guard with `else` body: `>x 0{x}{0}` emits `if`/`else`
        let py = parse_and_emit("f x:n>n;>x 0{x}{0}");
        assert!(py.contains("if (x > 0):"), "got: {py}");
        assert!(py.contains("else:"), "got: {py}");
    }

    // ── TypeIs pattern in match statement (lines 371-379) ────────────────────

    #[test]
    fn emit_type_is_in_match_stmt() {
        // TypeIs pattern in match statement covers emit_match_arm TypeIs branch
        let py = parse_and_emit(r#"f x:n>t;?x{n v:"number";_:"other"}"#);
        assert!(py.contains("isinstance"), "got: {py}");
    }

    // ── prnt builtin (lines 436, 438) ────────────────────────────────────────

    #[test]
    fn emit_prnt_builtin() {
        let py = parse_and_emit("f x:n>n;prnt x;x");
        assert!(py.contains("print"), "got: {py}");
    }

    // ── rd with 1 arg (lines 441-443) ────────────────────────────────────────

    #[test]
    fn emit_rd_one_arg() {
        let py = parse_and_emit(r#"f p:t>R t t;rd p"#);
        assert!(py.contains("_ilo_rd"), "got: {py}");
    }

    // ── TypeIs in simple match expr ternary (line 625-627) ───────────────────

    #[test]
    fn emit_type_is_in_match_expr_simple() {
        // TypeIs with wildcard binding in match expression → simple ternary path
        let py = parse_and_emit(r#"f x:n>t;y=?x{n _:"num";_:"other"};y"#);
        assert!(py.contains("isinstance"), "got: {py}");
    }

    // ── TypeIs in complex match expr (lines 686-694) ─────────────────────────

    #[test]
    fn emit_type_is_in_match_expr_complex_with_binding() {
        // TypeIs with non-wildcard binding → complex path with binding assignment
        let py = parse_and_emit(r#"f x:n>t;y=?x{n v:str v;_:"other"};y"#);
        assert!(py.contains("isinstance"), "got: {py}");
        assert!(py.contains("_m"), "expected complex path: {py}");
    }

    // ── emit_type for Fn (lines 777-779) ─────────────────────────────────────

    #[test]
    fn emit_type_fn() {
        // Function with Fn-typed param exercises emit_type for Type::Fn
        let py = parse_and_emit("f cb:F n n>n;cb 1");
        assert!(py.contains("Callable"), "expected Callable type: {py}");
    }

    // ── Decl::Use skipped (line 225) ─────────────────────────────────────────

    #[test]
    fn emit_use_decl_skipped() {
        use crate::ast::{Decl, Program, Span};
        let mut prog = Program {
            declarations: vec![],
            source: None,
        };
        prog.declarations.push(Decl::Use {
            path: "x.ilo".into(),
            only: None,
            span: Span::UNKNOWN,
        });
        let py = emit(&prog);
        assert!(!py.contains("use"), "Use should produce no output: {py}");
    }

    // ── type_to_py coverage (L6-12) ─────────────────────────────────────────

    #[test]
    fn type_to_py_text() {
        assert_eq!(type_to_py(&Type::Text), "str");
    }

    #[test]
    fn type_to_py_bool() {
        assert_eq!(type_to_py(&Type::Bool), "bool");
    }

    #[test]
    fn type_to_py_list() {
        assert_eq!(type_to_py(&Type::List(Box::new(Type::Number))), "list");
    }

    #[test]
    fn type_to_py_map() {
        assert_eq!(
            type_to_py(&Type::Map(Box::new(Type::Text), Box::new(Type::Number))),
            "dict"
        );
    }

    #[test]
    fn type_to_py_optional() {
        assert_eq!(
            type_to_py(&Type::Optional(Box::new(Type::Number))),
            "object | None"
        );
    }

    #[test]
    fn type_to_py_sum() {
        assert_eq!(type_to_py(&Type::Sum(vec!["a".into(), "b".into()])), "str");
    }

    #[test]
    fn type_to_py_wildcard() {
        assert_eq!(type_to_py(&Type::Named("custom".into())), "object");
    }

    // ── Ternary codegen (L583-587) ──────────────────────────────────────────

    #[test]
    fn emit_ternary() {
        let py = parse_and_emit("f x:n>n;?=x 0 10 20");
        assert!(py.contains("10 if"), "expected ternary: {py}");
        assert!(py.contains("else 20"), "expected else clause: {py}");
    }

    // ── expr_uses_unwrap for Ternary (L167-168) ────────────────────────────

    #[test]
    fn emit_ternary_with_unwrap() {
        // Directly test expr_uses_unwrap with a synthetic Ternary containing an unwrap call
        let ternary = Expr::Ternary {
            condition: Box::new(Expr::Literal(Literal::Bool(true))),
            then_expr: Box::new(Expr::Call {
                function: "g".into(),
                args: vec![],
                unwrap: true,
            }),
            else_expr: Box::new(Expr::Literal(Literal::Number(0.0))),
        };
        assert!(
            expr_uses_unwrap(&ternary),
            "ternary with unwrap call should be detected"
        );
    }

    // ── rou codegen (L505) ──────────────────────────────────────────────────

    #[test]
    fn emit_rou() {
        let py = parse_and_emit("f x:n>n;rou x");
        assert!(py.contains("float(round("), "expected round call: {py}");
    }

    // ── Literal::Nil codegen (L775) ─────────────────────────────────────────

    #[test]
    fn emit_literal_nil() {
        let py = parse_and_emit("f>O n;nil");
        assert!(py.contains("None"), "expected None for nil: {py}");
    }
}
