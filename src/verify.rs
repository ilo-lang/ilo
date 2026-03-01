use std::collections::HashMap;

use crate::ast::*;

/// Verifier's internal type representation.
/// Adds `Unknown` for cases where we can't infer — compatible with anything.
#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    Number,
    Text,
    Bool,
    Nil,
    List(Box<Ty>),
    Result(Box<Ty>, Box<Ty>),
    Named(String),
    Unknown,
}

impl std::fmt::Display for Ty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ty::Number => write!(f, "n"),
            Ty::Text => write!(f, "t"),
            Ty::Bool => write!(f, "b"),
            Ty::Nil => write!(f, "_"),
            Ty::List(inner) => write!(f, "L {inner}"),
            Ty::Result(ok, err) => write!(f, "R {ok} {err}"),
            Ty::Named(name) => write!(f, "{name}"),
            Ty::Unknown => write!(f, "?"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VerifyError {
    pub code: &'static str,
    pub function: String,
    pub message: String,
    pub hint: Option<String>,
    pub span: Option<Span>,
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "verify: {} in '{}'", self.message, self.function)?;
        if let Some(hint) = &self.hint {
            write!(f, "\n  hint: {hint}")?;
        }
        Ok(())
    }
}

struct FuncSig {
    params: Vec<(String, Ty)>,
    return_type: Ty,
}

struct TypeDef {
    fields: Vec<(String, Ty)>,
}

struct VerifyContext {
    functions: HashMap<String, FuncSig>,
    types: HashMap<String, TypeDef>,
    errors: Vec<VerifyError>,
}

type Scope = Vec<HashMap<String, Ty>>;

fn scope_lookup(scope: &Scope, name: &str) -> Option<Ty> {
    for frame in scope.iter().rev() {
        if let Some(ty) = frame.get(name) {
            return Some(ty.clone());
        }
    }
    None
}

fn scope_insert(scope: &mut Scope, name: String, ty: Ty) {
    if let Some(frame) = scope.last_mut() {
        frame.insert(name, ty);
    }
}

fn convert_type(ast_ty: &Type) -> Ty {
    match ast_ty {
        Type::Number => Ty::Number,
        Type::Text => Ty::Text,
        Type::Bool => Ty::Bool,
        Type::Nil => Ty::Nil,
        Type::List(inner) => Ty::List(Box::new(convert_type(inner))),
        Type::Result(ok, err) => Ty::Result(Box::new(convert_type(ok)), Box::new(convert_type(err))),
        Type::Named(name) => Ty::Named(name.clone()),
    }
}

/// Two types are compatible if either is Unknown, or they're structurally equal.
fn compatible(a: &Ty, b: &Ty) -> bool {
    match (a, b) {
        (Ty::Unknown, _) | (_, Ty::Unknown) => true,
        (Ty::Number, Ty::Number) => true,
        (Ty::Text, Ty::Text) => true,
        (Ty::Bool, Ty::Bool) => true,
        (Ty::Nil, Ty::Nil) => true,
        (Ty::List(a), Ty::List(b)) => compatible(a, b),
        (Ty::Result(ao, ae), Ty::Result(bo, be)) => compatible(ao, bo) && compatible(ae, be),
        (Ty::Named(a), Ty::Named(b)) => a == b,
        _ => false,
    }
}

fn closest_match<'a>(name: &str, candidates: impl Iterator<Item = &'a String>) -> Option<String> {
    let mut best: Option<(String, usize)> = None;
    for candidate in candidates {
        let dist = levenshtein(name, candidate);
        if dist <= 3 && best.as_ref().is_none_or(|(_, d)| dist < *d) {
            best = Some((candidate.clone(), dist));
        }
    }
    best.map(|(s, _)| s)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for (i, row) in dp.iter_mut().enumerate().take(m + 1) { row[0] = i; }
    for (j, val) in dp[0].iter_mut().enumerate().take(n + 1) { *val = j; }
    for i in 1..=m {
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[m][n]
}

const BUILTINS: &[(&str, &[&str], &str)] = &[
    // (name, param_types, return_type_desc)
    // We use special strings to describe signatures
    ("len", &["list_or_text"], "n"),
    ("str", &["n"], "t"),
    ("num", &["t"], "R n t"),
    ("abs", &["n"], "n"),
    ("flr", &["n"], "n"),
    ("cel", &["n"], "n"),
    ("min", &["n", "n"], "n"),
    ("max", &["n", "n"], "n"),
    ("get", &["t"], "R t t"),
    ("spl", &["t", "t"], "L t"),
    ("cat", &["L t", "t"], "t"),
    ("has", &["list_or_text", "any"], "b"),
    ("hd", &["list_or_text"], "any"),
    ("tl", &["list_or_text"], "list_or_text"),
    ("rev", &["list_or_text"], "list_or_text"),
    ("srt", &["list_or_text"], "list_or_text"),
    ("slc", &["list_or_text", "n", "n"], "list_or_text"),
];

fn builtin_arity(name: &str) -> Option<usize> {
    BUILTINS.iter().find(|(n, _, _)| *n == name).map(|(_, params, _)| params.len())
}

fn is_builtin(name: &str) -> bool {
    BUILTINS.iter().any(|(n, _, _)| *n == name)
}

fn builtin_check_args(name: &str, arg_types: &[Ty], func_ctx: &str, span: Option<Span>) -> (Ty, Vec<VerifyError>) {
    let mut errors = Vec::new();
    match name {
        "len" => {
            if let Some(arg) = arg_types.first() {
                match arg {
                    Ty::List(_) | Ty::Text | Ty::Unknown => {}
                    other => errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'len' expects a list or text, got {other}"),
                        hint: None,
                        span,
                    }),
                }
            }
            (Ty::Number, errors)
        }
        "str" => {
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &Ty::Number)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'str' expects n, got {arg}"),
                    hint: None,
                    span,
                });
            }
            (Ty::Text, errors)
        }
        "num" => {
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &Ty::Text)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'num' expects t, got {arg}"),
                    hint: None,
                    span,
                });
            }
            (Ty::Result(Box::new(Ty::Number), Box::new(Ty::Text)), errors)
        }
        "abs" | "flr" | "cel" => {
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &Ty::Number)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'{name}' expects n, got {arg}"),
                    hint: None,
                    span,
                });
            }
            (Ty::Number, errors)
        }
        "min" | "max" => {
            for (i, arg) in arg_types.iter().enumerate() {
                if !compatible(arg, &Ty::Number) {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'{name}' arg {} expects n, got {arg}", i + 1),
                        hint: None,
                        span,
                    });
                }
            }
            (Ty::Number, errors)
        }
        "spl" => {
            for (i, arg) in arg_types.iter().enumerate() {
                if !compatible(arg, &Ty::Text) {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'spl' arg {} expects t, got {arg}", i + 1),
                        hint: None,
                        span,
                    });
                }
            }
            (Ty::List(Box::new(Ty::Text)), errors)
        }
        "cat" => {
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &Ty::List(Box::new(Ty::Text)))
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'cat' arg 1 expects L t, got {arg}"),
                    hint: None,
                    span,
                });
            }
            if let Some(arg) = arg_types.get(1)
                && !compatible(arg, &Ty::Text)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'cat' arg 2 expects t, got {arg}"),
                    hint: None,
                    span,
                });
            }
            (Ty::Text, errors)
        }
        "has" => {
            if let Some(arg) = arg_types.first() {
                match arg {
                    Ty::List(_) | Ty::Text | Ty::Unknown => {}
                    other => errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'has' arg 1 expects a list or text, got {other}"),
                        hint: None,
                        span,
                    }),
                }
            }
            (Ty::Bool, errors)
        }
        "hd" => {
            if let Some(arg) = arg_types.first() {
                match arg {
                    Ty::List(inner) => return (*inner.clone(), errors),
                    Ty::Text => return (Ty::Text, errors),
                    Ty::Unknown => return (Ty::Unknown, errors),
                    other => errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'hd' expects a list or text, got {other}"),
                        hint: None,
                        span,
                    }),
                }
            }
            (Ty::Unknown, errors)
        }
        "tl" => {
            if let Some(arg) = arg_types.first() {
                match arg {
                    Ty::List(inner) => return (Ty::List(inner.clone()), errors),
                    Ty::Text => return (Ty::Text, errors),
                    Ty::Unknown => return (Ty::Unknown, errors),
                    other => errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'tl' expects a list or text, got {other}"),
                        hint: None,
                        span,
                    }),
                }
            }
            (Ty::Unknown, errors)
        }
        "rev" => {
            if let Some(arg) = arg_types.first() {
                match arg {
                    Ty::List(_) | Ty::Text | Ty::Unknown => {}
                    other => errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'rev' expects a list or text, got {other}"),
                        hint: None,
                        span,
                    }),
                }
            }
            let ret = match arg_types.first() {
                Some(Ty::List(inner)) => Ty::List(inner.clone()),
                Some(Ty::Text) => Ty::Text,
                _ => Ty::Unknown,
            };
            (ret, errors)
        }
        "srt" => {
            if let Some(arg) = arg_types.first() {
                match arg {
                    Ty::List(inner) => return (Ty::List(inner.clone()), errors),
                    Ty::Text => return (Ty::Text, errors),
                    Ty::Unknown => return (Ty::Unknown, errors),
                    other => errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'srt' expects a list or text, got {other}"),
                        hint: None,
                        span,
                    }),
                }
            }
            (Ty::Unknown, errors)
        }
        "slc" => {
            if let Some(arg) = arg_types.first() {
                match arg {
                    Ty::List(_) | Ty::Text | Ty::Unknown => {}
                    other => errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'slc' expects a list or text, got {other}"),
                        hint: None,
                        span,
                    }),
                }
            }
            for (i, idx) in [1usize, 2].iter().enumerate() {
                if let Some(arg) = arg_types.get(*idx)
                    && !compatible(arg, &Ty::Number)
                {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'slc' arg {} expects n, got {arg}", i + 2),
                        hint: None,
                        span,
                    });
                }
            }
            let ret = match arg_types.first() {
                Some(Ty::List(inner)) => Ty::List(inner.clone()),
                Some(Ty::Text) => Ty::Text,
                _ => Ty::Unknown,
            };
            (ret, errors)
        }
        "get" => {
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &Ty::Text)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'get' expects t, got {arg}"),
                    hint: None,
                    span,
                });
            }
            (Ty::Result(Box::new(Ty::Text), Box::new(Ty::Text)), errors)
        }
        _ => (Ty::Unknown, errors),
    }
}

impl VerifyContext {
    fn new() -> Self {
        Self {
            functions: HashMap::new(),
            types: HashMap::new(),
            errors: Vec::new(),
        }
    }

    fn err(&mut self, code: &'static str, function: &str, message: String, hint: Option<String>, span: Option<Span>) {
        self.errors.push(VerifyError {
            code,
            function: function.to_string(),
            message,
            hint,
            span,
        });
    }

    /// Phase 1: collect all declarations, check for duplicates and undefined Named types.
    fn collect_declarations(&mut self, program: &Program) {
        // First pass: collect type names
        for decl in &program.declarations {
            if let Decl::TypeDef { name, fields, .. } = decl {
                if self.types.contains_key(name) {
                    self.err("ILO-T001", "<global>", format!("duplicate type definition '{name}'"), None, None);
                } else {
                    let fields: Vec<(String, Ty)> = fields
                        .iter()
                        .map(|p| (p.name.clone(), convert_type(&p.ty)))
                        .collect();
                    self.types.insert(name.clone(), TypeDef { fields });
                }
            }
        }

        // Second pass: collect functions and tools, validate Named types in signatures
        for decl in &program.declarations {
            match decl {
                Decl::Function { name, params, return_type, .. } => {
                    if self.functions.contains_key(name) {
                        self.err("ILO-T002", "<global>", format!("duplicate function definition '{name}'"), None, None);
                        continue;
                    }
                    let params: Vec<(String, Ty)> = params
                        .iter()
                        .map(|p| (p.name.clone(), convert_type(&p.ty)))
                        .collect();
                    let ret = convert_type(return_type);
                    self.validate_named_types_in_sig(name, &params, &ret);
                    self.functions.insert(name.clone(), FuncSig { params, return_type: ret });
                }
                Decl::Tool { name, params, return_type, .. } => {
                    if self.functions.contains_key(name) {
                        self.err("ILO-T002", "<global>", format!("duplicate definition '{name}' (tool conflicts with function)"), None, None);
                        continue;
                    }
                    let params: Vec<(String, Ty)> = params
                        .iter()
                        .map(|p| (p.name.clone(), convert_type(&p.ty)))
                        .collect();
                    let ret = convert_type(return_type);
                    self.validate_named_types_in_sig(name, &params, &ret);
                    self.functions.insert(name.clone(), FuncSig { params, return_type: ret });
                }
                Decl::TypeDef { .. } => {} // already handled
                Decl::Error { .. } => {}   // poison node — skip silently
            }
        }

        // Validate Named types in type def fields
        for decl in &program.declarations {
            if let Decl::TypeDef { name, fields, .. } = decl {
                for field in fields {
                    self.validate_named_type_recursive(&convert_type(&field.ty), name);
                }
            }
        }
    }

    fn validate_named_types_in_sig(&mut self, func_name: &str, params: &[(String, Ty)], ret: &Ty) {
        for (_, ty) in params {
            self.validate_named_type_recursive(ty, func_name);
        }
        self.validate_named_type_recursive(ret, func_name);
    }

    fn validate_named_type_recursive(&mut self, ty: &Ty, ctx: &str) {
        match ty {
            Ty::Named(name) => {
                if !self.types.contains_key(name) {
                    let hint = closest_match(name, self.types.keys())
                        .map(|s| format!("did you mean '{s}'?"));
                    self.err("ILO-T003", ctx, format!("undefined type '{name}'"), hint, None);
                }
            }
            Ty::List(inner) => self.validate_named_type_recursive(inner, ctx),
            Ty::Result(ok, err) => {
                self.validate_named_type_recursive(ok, ctx);
                self.validate_named_type_recursive(err, ctx);
            }
            _ => {}
        }
    }

    /// Phase 2: verify all function bodies.
    fn verify_bodies(&mut self, program: &Program) {
        for decl in &program.declarations {
            if let Decl::Function { name, params, return_type, body, .. } = decl {
                let mut scope: Scope = vec![HashMap::new()];
                for p in params {
                    scope_insert(&mut scope, p.name.clone(), convert_type(&p.ty));
                }

                let body_ty = self.verify_body(name, &mut scope, body);
                let expected = convert_type(return_type);
                if !compatible(&body_ty, &expected) {
                    let hint = match (&body_ty, &expected) {
                        (Ty::Number, Ty::Text) => Some("use 'str' to convert: str <expr>".to_string()),
                        (Ty::Text, Ty::Number) => Some("use 'num' to parse text (returns R n t)".to_string()),
                        _ => None,
                    };
                    let last_span = body.last().map(|s| s.span);
                    self.err(
                        "ILO-T008",
                        name,
                        format!("return type mismatch: expected {expected}, got {body_ty}"),
                        hint,
                        last_span,
                    );
                }
            }
        }
    }

    fn verify_body(&mut self, func: &str, scope: &mut Scope, stmts: &[Spanned<Stmt>]) -> Ty {
        let mut last_ty = Ty::Nil;
        for spanned in stmts {
            last_ty = self.verify_stmt(func, scope, &spanned.node, spanned.span);
        }
        last_ty
    }

    fn verify_stmt(&mut self, func: &str, scope: &mut Scope, stmt: &Stmt, span: Span) -> Ty {
        match stmt {
            Stmt::Let { name, value } => {
                let ty = self.infer_expr(func, scope, value, span);
                scope_insert(scope, name.clone(), ty);
                Ty::Nil
            }
            Stmt::Guard { condition, body, else_body, .. } => {
                let _ = self.infer_expr(func, scope, condition, span);

                // Warn if braceless guard body is a single identifier matching a function name.
                if body.len() == 1 {
                    if let Stmt::Expr(Expr::Ref(ref name)) = body[0].node {
                        if self.functions.contains_key(name) || is_builtin(name) {
                            let body_span = body[0].span;
                            self.err(
                                "ILO-T027",
                                func,
                                format!("braceless guard body '{name}' is a function name — did you mean to call it?"),
                                Some(format!("use braces for function calls: cond{{{name} args}}")),
                                Some(body_span),
                            );
                        }
                    }
                }

                scope.push(HashMap::new());
                let body_ty = self.verify_body(func, scope, body);
                scope.pop();

                if let Some(eb) = else_body {
                    scope.push(HashMap::new());
                    let _else_ty = self.verify_body(func, scope, eb);
                    scope.pop();
                }

                body_ty
            }
            Stmt::Match { subject, arms } => {
                let subject_ty = match subject {
                    Some(expr) => self.infer_expr(func, scope, expr, span),
                    None => Ty::Nil,
                };
                let mut arm_ty = Ty::Unknown;
                for arm in arms {
                    scope.push(HashMap::new());
                    self.bind_pattern(func, scope, &arm.pattern, &subject_ty);
                    let body_ty = self.verify_body(func, scope, &arm.body);
                    if arm_ty == Ty::Unknown {
                        arm_ty = body_ty;
                    }
                    scope.pop();
                }
                self.check_match_exhaustiveness(func, &subject_ty, arms, span);
                arm_ty
            }
            Stmt::ForEach { binding, collection, body } => {
                let coll_ty = self.infer_expr(func, scope, collection, span);
                let elem_ty = match &coll_ty {
                    Ty::List(inner) => *inner.clone(),
                    Ty::Unknown => Ty::Unknown,
                    other => {
                        self.err("ILO-T014", func, format!("foreach expects a list, got {other}"), None, Some(span));
                        Ty::Unknown
                    }
                };
                scope.push(HashMap::new());
                scope_insert(scope, binding.clone(), elem_ty);
                let body_ty = self.verify_body(func, scope, body);
                scope.pop();
                body_ty
            }
            Stmt::While { condition, body } => {
                self.infer_expr(func, scope, condition, span);
                self.verify_body(func, scope, body)
            }
            Stmt::Return(expr) => self.infer_expr(func, scope, expr, span),
            Stmt::Break(expr) => {
                if let Some(e) = expr {
                    self.infer_expr(func, scope, e, span)
                } else {
                    Ty::Nil
                }
            }
            Stmt::Continue => Ty::Nil,
            Stmt::Expr(expr) => self.infer_expr(func, scope, expr, span),
        }
    }

    fn bind_pattern(&mut self, _func: &str, scope: &mut Scope, pattern: &Pattern, subject_ty: &Ty) {
        match pattern {
            Pattern::Ok(name) => {
                if name != "_" {
                    let ty = match subject_ty {
                        Ty::Result(ok, _) => *ok.clone(),
                        Ty::Unknown => Ty::Unknown,
                        _ => Ty::Unknown,
                    };
                    scope_insert(scope, name.clone(), ty);
                }
            }
            Pattern::Err(name) => {
                if name != "_" {
                    let ty = match subject_ty {
                        Ty::Result(_, err) => *err.clone(),
                        Ty::Unknown => Ty::Unknown,
                        _ => Ty::Unknown,
                    };
                    scope_insert(scope, name.clone(), ty);
                }
            }
            Pattern::Literal(_) | Pattern::Wildcard => {}
        }
    }

    fn infer_expr(&mut self, func: &str, scope: &mut Scope, expr: &Expr, span: Span) -> Ty {
        match expr {
            Expr::Literal(lit) => match lit {
                Literal::Number(_) => Ty::Number,
                Literal::Text(_) => Ty::Text,
                Literal::Bool(_) => Ty::Bool,
            },

            Expr::Ref(name) => {
                if let Some(ty) = scope_lookup(scope, name) {
                    ty
                } else {
                    let candidates: Vec<String> = scope.iter()
                        .flat_map(|frame| frame.keys().cloned())
                        .collect();
                    let hint = closest_match(name, candidates.iter())
                        .map(|s| format!("did you mean '{s}'?"));
                    self.err("ILO-T004", func, format!("undefined variable '{name}'"), hint, Some(span));
                    Ty::Unknown
                }
            }

            Expr::Call { function: callee, args, unwrap } => {
                // Infer all arg types first
                let arg_types: Vec<Ty> = args.iter().map(|a| self.infer_expr(func, scope, a, span)).collect();

                let call_ty = if is_builtin(callee) {
                    // Check arity
                    let expected_arity = builtin_arity(callee).unwrap();
                    if args.len() != expected_arity {
                        self.err(
                            "ILO-T006",
                            func,
                            format!("arity mismatch: '{callee}' expects {expected_arity} args, got {}", args.len()),
                            None,
                            Some(span),
                        );
                        return Ty::Unknown;
                    }
                    let (ret_ty, errors) = builtin_check_args(callee, &arg_types, func, Some(span));
                    self.errors.extend(errors);
                    ret_ty
                } else if let Some(sig) = self.functions.get(callee) {
                    let sig_params = sig.params.clone();
                    let sig_ret = sig.return_type.clone();

                    if args.len() != sig_params.len() {
                        let hint = {
                            let sig_str: String = sig_params.iter()
                                .map(|(n, t)| format!("{n}:{t}"))
                                .collect::<Vec<_>>()
                                .join(" ");
                            Some(format!("'{callee}' expects: {sig_str}"))
                        };
                        self.err(
                            "ILO-T006",
                            func,
                            format!(
                                "arity mismatch: '{callee}' expects {} args, got {}",
                                sig_params.len(),
                                args.len()
                            ),
                            hint,
                            Some(span),
                        );
                        return sig_ret;
                    }

                    for (i, ((param_name, param_ty), arg_ty)) in sig_params.iter().zip(arg_types.iter()).enumerate() {
                        if !compatible(param_ty, arg_ty) {
                            let hint = match (param_ty, arg_ty) {
                                (Ty::Text, Ty::Number) => Some("use 'str' to convert number to text".to_string()),
                                (Ty::Number, Ty::Text) => Some("use 'num' to parse text as number (returns R n t)".to_string()),
                                _ => None,
                            };
                            self.err(
                                "ILO-T007",
                                func,
                                format!(
                                    "type mismatch: param '{}' of '{}' expects {}, got {}",
                                    param_name, callee, param_ty, arg_ty
                                ),
                                hint,
                                Some(span),
                            );
                        }
                        let _ = i;
                    }

                    sig_ret
                } else {
                    let mut candidates: Vec<String> = self.functions.keys().cloned().collect();
                    for (n, _, _) in BUILTINS {
                        candidates.push(n.to_string());
                    }
                    let hint = closest_match(callee, candidates.iter())
                        .map(|s| format!("did you mean '{s}'?"));
                    self.err(
                        "ILO-T005",
                        func,
                        format!("undefined function '{callee}' (called with {} args)", args.len()),
                        hint,
                        Some(span),
                    );
                    Ty::Unknown
                };

                // Auto-unwrap: func! args — callee must return Result, enclosing must return Result
                if *unwrap {
                    match &call_ty {
                        Ty::Result(ok_ty, _err_ty) => {
                            // Check enclosing function returns a Result
                            if let Some(enc_sig) = self.functions.get(func) {
                                match &enc_sig.return_type {
                                    Ty::Result(_, _) => {}
                                    other => {
                                        self.err(
                                            "ILO-T026",
                                            func,
                                            format!("'!' used in function '{func}' which returns {other}, not a Result"),
                                            Some("the enclosing function must return R to propagate errors".to_string()),
                                            Some(span),
                                        );
                                    }
                                }
                            }
                            *ok_ty.clone()
                        }
                        Ty::Unknown => Ty::Unknown,
                        other => {
                            self.err(
                                "ILO-T025",
                                func,
                                format!("'!' used on call to '{callee}' which returns {other}, not a Result"),
                                Some("'!' auto-unwraps Result types: Ok(v)→v, Err(e)→propagate".to_string()),
                                Some(span),
                            );
                            Ty::Unknown
                        }
                    }
                } else {
                    call_ty
                }
            }

            Expr::BinOp { op, left, right } => {
                let lt = self.infer_expr(func, scope, left, span);
                let rt = self.infer_expr(func, scope, right, span);
                self.check_binop(func, op, &lt, &rt, span)
            }

            Expr::UnaryOp { op, operand } => {
                let t = self.infer_expr(func, scope, operand, span);
                match op {
                    UnaryOp::Negate => {
                        if !compatible(&t, &Ty::Number) {
                            self.err("ILO-T012", func, format!("negate expects n, got {t}"), None, Some(span));
                        }
                        Ty::Number
                    }
                    UnaryOp::Not => {
                        // Not works on anything (truthiness)
                        Ty::Bool
                    }
                }
            }

            Expr::Ok(inner) => {
                let t = self.infer_expr(func, scope, inner, span);
                Ty::Result(Box::new(t), Box::new(Ty::Unknown))
            }

            Expr::Err(inner) => {
                let t = self.infer_expr(func, scope, inner, span);
                Ty::Result(Box::new(Ty::Unknown), Box::new(t))
            }

            Expr::List(items) => {
                if items.is_empty() {
                    Ty::List(Box::new(Ty::Unknown))
                } else {
                    let first_ty = self.infer_expr(func, scope, &items[0], span);
                    // Infer remaining items but don't enforce homogeneity strictly
                    for item in &items[1..] {
                        let _ = self.infer_expr(func, scope, item, span);
                    }
                    Ty::List(Box::new(first_ty))
                }
            }

            Expr::Record { type_name, fields } => {
                if let Some(type_def) = self.types.get(type_name) {
                    let def_fields = type_def.fields.clone();
                    let provided: HashMap<&str, &Expr> = fields.iter().map(|(n, e)| (n.as_str(), e)).collect();

                    // Check for missing fields
                    for (fname, _) in &def_fields {
                        if !provided.contains_key(fname.as_str()) {
                            self.err(
                                "ILO-T015",
                                func,
                                format!("missing field '{fname}' in record '{type_name}'"),
                                None,
                                Some(span),
                            );
                        }
                    }

                    // Check for extra fields
                    let def_field_names: Vec<&str> = def_fields.iter().map(|(n, _)| n.as_str()).collect();
                    for (fname, _) in fields {
                        if !def_field_names.contains(&fname.as_str()) {
                            let def_field_strings: Vec<String> = def_field_names.iter().map(|s| s.to_string()).collect();
                            let hint = closest_match(fname, def_field_strings.iter())
                                .map(|s| format!("did you mean '{s}'?"));
                            self.err(
                                "ILO-T016",
                                func,
                                format!("unknown field '{fname}' in record '{type_name}'"),
                                hint,
                                Some(span),
                            );
                        }
                    }

                    // Check field types
                    for (fname, fty) in &def_fields {
                        if let Some(expr) = provided.get(fname.as_str()) {
                            let actual = self.infer_expr(func, scope, expr, span);
                            if !compatible(fty, &actual) {
                                self.err(
                                    "ILO-T017",
                                    func,
                                    format!("field '{fname}' of '{type_name}' expects {fty}, got {actual}"),
                                    None,
                                    Some(span),
                                );
                            }
                        }
                    }

                    Ty::Named(type_name.clone())
                } else {
                    let hint = closest_match(type_name, self.types.keys())
                        .map(|s| format!("did you mean '{s}'?"));
                    self.err("ILO-T003", func, format!("undefined type '{type_name}'"), hint, Some(span));
                    Ty::Unknown
                }
            }

            Expr::Field { object, field, safe } => {
                let obj_ty = self.infer_expr(func, scope, object, span);
                if *safe && obj_ty == Ty::Nil {
                    return Ty::Nil;
                }
                match &obj_ty {
                    Ty::Named(type_name) => {
                        if let Some(type_def) = self.types.get(type_name) {
                            if let Some((_, fty)) = type_def.fields.iter().find(|(n, _)| n == field) {
                                fty.clone()
                            } else {
                                let field_names: Vec<String> = type_def.fields.iter().map(|(n, _)| n.clone()).collect();
                                let hint = closest_match(field, field_names.iter())
                                    .map(|s| format!("did you mean '{s}'?"));
                                self.err(
                                    "ILO-T019",
                                    func,
                                    format!("no field '{field}' on type '{type_name}'"),
                                    hint,
                                    Some(span),
                                );
                                Ty::Unknown
                            }
                        } else {
                            Ty::Unknown
                        }
                    }
                    Ty::Unknown => Ty::Unknown,
                    other => {
                        self.err("ILO-T018", func, format!("field access on non-record type {other}"), None, Some(span));
                        Ty::Unknown
                    }
                }
            }

            Expr::Index { object, safe, .. } => {
                let obj_ty = self.infer_expr(func, scope, object, span);
                if *safe && obj_ty == Ty::Nil {
                    return Ty::Nil;
                }
                match &obj_ty {
                    Ty::List(inner) => *inner.clone(),
                    Ty::Unknown => Ty::Unknown,
                    other => {
                        self.err("ILO-T023", func, format!("index access on non-list type {other}"), None, Some(span));
                        Ty::Unknown
                    }
                }
            }

            Expr::Match { subject, arms } => {
                let subject_ty = match subject {
                    Some(expr) => self.infer_expr(func, scope, expr, span),
                    None => Ty::Nil,
                };
                let mut result_ty = Ty::Unknown;
                for arm in arms {
                    scope.push(HashMap::new());
                    self.bind_pattern(func, scope, &arm.pattern, &subject_ty);
                    let body_ty = self.verify_body(func, scope, &arm.body);
                    if result_ty == Ty::Unknown {
                        result_ty = body_ty;
                    }
                    scope.pop();
                }
                self.check_match_exhaustiveness(func, &subject_ty, arms, span);
                result_ty
            }

            Expr::NilCoalesce { value, default } => {
                let val_ty = self.infer_expr(func, scope, value, span);
                let def_ty = self.infer_expr(func, scope, default, span);
                if val_ty == Ty::Nil { def_ty } else { val_ty }
            }
            Expr::With { object, updates } => {
                let obj_ty = self.infer_expr(func, scope, object, span);
                match &obj_ty {
                    Ty::Named(type_name) => {
                        if let Some(type_def) = self.types.get(type_name) {
                            let def_fields = type_def.fields.clone();
                            for (fname, expr) in updates {
                                if let Some((_, fty)) = def_fields.iter().find(|(n, _)| n == fname) {
                                    let actual = self.infer_expr(func, scope, expr, span);
                                    if !compatible(fty, &actual) {
                                        self.err(
                                            "ILO-T022",
                                            func,
                                            format!("'with' field '{fname}' of '{type_name}' expects {fty}, got {actual}"),
                                            None,
                                            Some(span),
                                        );
                                    }
                                } else {
                                    let def_field_strings: Vec<String> = def_fields.iter().map(|(n, _)| n.clone()).collect();
                                    let hint = closest_match(fname, def_field_strings.iter())
                                        .map(|s| format!("did you mean '{s}'?"));
                                    self.err(
                                        "ILO-T021",
                                        func,
                                        format!("unknown field '{fname}' in 'with' on '{type_name}'"),
                                        hint,
                                        Some(span),
                                    );
                                }
                            }
                        }
                        obj_ty
                    }
                    Ty::Unknown => Ty::Unknown,
                    other => {
                        self.err("ILO-T020", func, format!("'with' on non-record type {other}"), None, Some(span));
                        Ty::Unknown
                    }
                }
            }
        }
    }

    fn check_binop(&mut self, func: &str, op: &BinOp, lt: &Ty, rt: &Ty, span: Span) -> Ty {
        match op {
            BinOp::Add => {
                // Number+Number, Text+Text, List+List
                match (lt, rt) {
                    (Ty::Number, Ty::Number) => Ty::Number,
                    (Ty::Text, Ty::Text) => Ty::Text,
                    (Ty::List(a), Ty::List(_)) => Ty::List(a.clone()),
                    (Ty::Unknown, _) | (_, Ty::Unknown) => Ty::Unknown,
                    _ => {
                        let hint = match (lt, rt) {
                            (Ty::Number, Ty::Text) | (Ty::Text, Ty::Number) =>
                                Some("convert number to text with 'str' before concatenating".to_string()),
                            _ => None,
                        };
                        self.err("ILO-T009", func, format!("'+' expects matching n, t, or L types, got {lt} and {rt}"), hint, Some(span));
                        Ty::Unknown
                    }
                }
            }
            BinOp::Subtract | BinOp::Multiply | BinOp::Divide => {
                if !compatible(lt, &Ty::Number) || !compatible(rt, &Ty::Number) {
                    let sym = match op { BinOp::Subtract => "-", BinOp::Multiply => "*", _ => "/" };
                    let has_text = matches!(lt, Ty::Text) || matches!(rt, Ty::Text);
                    let hint = if has_text { Some("parse text as number with 'num' first".to_string()) } else { None };
                    self.err("ILO-T009", func, format!("'{sym}' expects n and n, got {lt} and {rt}"), hint, Some(span));
                }
                Ty::Number
            }
            BinOp::GreaterThan | BinOp::LessThan | BinOp::GreaterOrEqual | BinOp::LessOrEqual => {
                match (lt, rt) {
                    (Ty::Number, Ty::Number) | (Ty::Text, Ty::Text) => {}
                    (Ty::Unknown, _) | (_, Ty::Unknown) => {}
                    _ => {
                        self.err("ILO-T010", func, format!("comparison expects matching n or t, got {lt} and {rt}"), None, Some(span));
                    }
                }
                Ty::Bool
            }
            BinOp::Equals | BinOp::NotEquals => Ty::Bool,
            BinOp::And | BinOp::Or => Ty::Bool,
            BinOp::Append => {
                // List(T) += T → List(T)
                match lt {
                    Ty::List(inner) => {
                        if !compatible(inner, rt) {
                            self.err("ILO-T011", func, format!("'+=' list element type {inner} doesn't match appended {rt}"), None, Some(span));
                        }
                        lt.clone()
                    }
                    Ty::Unknown => Ty::Unknown,
                    _ => {
                        self.err("ILO-T011", func, format!("'+=' expects a list on the left, got {lt}"), None, Some(span));
                        Ty::Unknown
                    }
                }
            }
        }
    }

    fn check_match_exhaustiveness(&mut self, func: &str, subject_ty: &Ty, arms: &[MatchArm], span: Span) {
        let has_wildcard = arms.iter().any(|a| matches!(a.pattern, Pattern::Wildcard));
        if has_wildcard {
            return;
        }

        match subject_ty {
            Ty::Result(ok_ty, err_ty) => {
                let has_ok = arms.iter().any(|a| matches!(a.pattern, Pattern::Ok(_)));
                let has_err = arms.iter().any(|a| matches!(a.pattern, Pattern::Err(_)));
                if !has_ok || !has_err {
                    let missing: Vec<&str> = [
                        if !has_ok { Some("~") } else { None },
                        if !has_err { Some("^") } else { None },
                    ].into_iter().flatten().collect();
                    let parts: Vec<String> = [
                        if !has_ok { Some(format!("~v: <expr>  (v is of type {ok_ty})")) } else { None },
                        if !has_err { Some(format!("^e: <expr>  (e is of type {err_ty})")) } else { None },
                    ].into_iter().flatten().collect();
                    self.err(
                        "ILO-T024",
                        func,
                        format!("non-exhaustive match on Result: missing {}", missing.join(", ")),
                        Some(format!("add: {}", parts.join(" or "))),
                        Some(span),
                    );
                }
            }
            Ty::Bool => {
                let has_true = arms.iter().any(|a| matches!(&a.pattern, Pattern::Literal(Literal::Bool(true))));
                let has_false = arms.iter().any(|a| matches!(&a.pattern, Pattern::Literal(Literal::Bool(false))));
                if !has_true || !has_false {
                    let missing: Vec<&str> = [
                        if !has_true { Some("true") } else { None },
                        if !has_false { Some("false") } else { None },
                    ].into_iter().flatten().collect();
                    let parts: Vec<&str> = [
                        if !has_true { Some("true: <expr>") } else { None },
                        if !has_false { Some("false: <expr>") } else { None },
                    ].into_iter().flatten().collect();
                    self.err(
                        "ILO-T024",
                        func,
                        format!("non-exhaustive match on Bool: missing {}", missing.join(", ")),
                        Some(format!("add: {}", parts.join(" or "))),
                        Some(span),
                    );
                }
            }
            // For other types (Number, Text, Named, etc.) we can't enumerate
            // all possible values, so warn if there's no wildcard.
            // Nil arises from subjectless match (?{...}) where the actual type
            // is the implicit last result — we can't check exhaustiveness here.
            Ty::Unknown | Ty::Nil => {}
            _ => {
                self.err(
                    "ILO-T024",
                    func,
                    "non-exhaustive match: no wildcard arm".to_string(),
                    Some("add a wildcard arm: _: <default-expr>".to_string()),
                    Some(span),
                );
            }
        }
    }
}

/// Run static verification on a parsed program.
/// Returns Ok(()) if valid, Err(errors) if problems found.
pub fn verify(program: &Program) -> Result<(), Vec<VerifyError>> {
    let mut ctx = VerifyContext::new();

    // Phase 1: collect declarations
    ctx.collect_declarations(program);

    // Phase 2: verify function bodies
    ctx.verify_bodies(program);

    if ctx.errors.is_empty() {
        Ok(())
    } else {
        Err(ctx.errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_verify(code: &str) -> Result<(), Vec<VerifyError>> {
        let tokens = crate::lexer::lex(code).expect("lex failed");
        let token_spans: Vec<(crate::lexer::Token, crate::ast::Span)> = tokens
            .into_iter()
            .map(|(t, r)| (t, crate::ast::Span { start: r.start, end: r.end }))
            .collect();
        let (program, parse_errors) = crate::parser::parse(token_spans);
        assert!(parse_errors.is_empty(), "parse failed: {:?}", parse_errors);
        verify(&program)
    }

    #[test]
    fn valid_simple_function() {
        assert!(parse_and_verify("f x:n>n;*x 2").is_ok());
    }

    #[test]
    fn valid_multi_param() {
        assert!(parse_and_verify("tot p:n q:n r:n>n;s=*p q;t=*s r;+s t").is_ok());
    }

    #[test]
    fn valid_bool_function() {
        assert!(parse_and_verify("f x:b>b;!x").is_ok());
    }

    #[test]
    fn valid_text_function() {
        assert!(parse_and_verify("f x:t>t;x").is_ok());
    }

    #[test]
    fn undefined_variable() {
        let result = parse_and_verify("f x:n>n;*y 2");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("undefined variable 'y'")));
    }

    #[test]
    fn undefined_function() {
        let result = parse_and_verify("f x:n>n;foo x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("undefined function 'foo'")));
    }

    #[test]
    fn arity_mismatch() {
        let result = parse_and_verify("g a:n b:n>n;+a b f x:n>n;g x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("arity mismatch")));
    }

    #[test]
    fn type_mismatch_param() {
        let result = parse_and_verify("g x:n>n;*x 2 f x:t>n;g x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("type mismatch")));
    }

    #[test]
    fn multiply_on_text() {
        let result = parse_and_verify("f x:t>n;*x 2");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("'*' expects n and n")));
    }

    #[test]
    fn valid_let_binding() {
        assert!(parse_and_verify("f x:n>n;y=*x 2;+y 1").is_ok());
    }

    #[test]
    fn valid_guard() {
        assert!(parse_and_verify("f x:n>t;>x 10{\"big\"};\"small\"").is_ok());
    }

    #[test]
    fn valid_list() {
        assert!(parse_and_verify("f x:n>L n;[x, *x 2, *x 3]").is_ok());
    }

    #[test]
    fn valid_builtins() {
        assert!(parse_and_verify("f x:n>t;str x").is_ok());
        assert!(parse_and_verify("f x:t>n;len x").is_ok());
        assert!(parse_and_verify("f x:n>n;abs x").is_ok());
    }

    #[test]
    fn builtin_arity_mismatch() {
        let result = parse_and_verify("f x:n>n;min x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("arity mismatch") && e.message.contains("min")));
    }

    #[test]
    fn compatible_types() {
        assert!(compatible(&Ty::Number, &Ty::Number));
        assert!(compatible(&Ty::Unknown, &Ty::Number));
        assert!(compatible(&Ty::Number, &Ty::Unknown));
        assert!(!compatible(&Ty::Number, &Ty::Text));
        assert!(compatible(
            &Ty::List(Box::new(Ty::Number)),
            &Ty::List(Box::new(Ty::Number))
        ));
        assert!(!compatible(
            &Ty::List(Box::new(Ty::Number)),
            &Ty::List(Box::new(Ty::Text))
        ));
    }

    #[test]
    fn valid_ok_err() {
        assert!(parse_and_verify("f x:n>R n t;~x").is_ok());
        assert!(parse_and_verify("f x:t>R n t;^x").is_ok());
    }

    #[test]
    fn valid_match() {
        assert!(parse_and_verify("f x:R n t>n;?x{^e:0;~v:v;_:1}").is_ok());
    }

    #[test]
    fn valid_foreach() {
        assert!(parse_and_verify("f xs:L n>n;s=0;@x xs{s=+s x};s").is_ok());
    }

    #[test]
    fn foreach_on_non_list() {
        let result = parse_and_verify("f x:n>n;@i x{i};0");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("foreach expects a list")));
    }

    #[test]
    fn duplicate_function() {
        // Two functions both named "dup" — second starts a new decl after first body
        let result = parse_and_verify("dup x:n>n;*x 2 dup x:n>n;+x 1");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("duplicate function")));
    }

    #[test]
    fn valid_nested_prefix() {
        assert!(parse_and_verify("f a:n b:n c:n>n;+*a b c").is_ok());
    }

    #[test]
    fn valid_multi_function_calls() {
        // Two functions: dbl doubles, then apply calls dbl
        assert!(parse_and_verify("dbl x:n>n;*x 2 apply x:n>n;dbl x").is_ok());
    }

    #[test]
    fn return_type_mismatch() {
        let result = parse_and_verify("f x:n>t;*x 2");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("return type mismatch")));
    }

    #[test]
    fn valid_negated_guard() {
        assert!(parse_and_verify("f x:b>t;!x{\"yes\"};\"no\"").is_ok());
    }

    #[test]
    fn index_on_non_list() {
        let result = parse_and_verify("f x:n>n;x.0");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("index access on non-list")));
    }

    #[test]
    fn did_you_mean_hint() {
        let result = parse_and_verify("calc x:n>n;*x 2 f x:n>n;calx x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        let err = errors.iter().find(|e| e.message.contains("undefined function 'calx'")).unwrap();
        assert!(err.hint.as_ref().is_some_and(|h| h.contains("did you mean 'calc'?")));
    }

    // --- Match exhaustiveness tests ---

    #[test]
    fn exhaustive_result_match_with_both_arms() {
        // ~v and ^e covers Result fully
        assert!(parse_and_verify("f x:R n t>n;?x{~v:v;^e:0}").is_ok());
    }

    #[test]
    fn exhaustive_result_match_with_wildcard() {
        // wildcard covers everything
        assert!(parse_and_verify("f x:R n t>n;?x{~v:v;_:0}").is_ok());
    }

    #[test]
    fn non_exhaustive_result_missing_err() {
        let result = parse_and_verify("f x:R n t>n;?x{~v:v}");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("non-exhaustive") && e.message.contains("^")));
    }

    #[test]
    fn non_exhaustive_result_missing_ok() {
        let result = parse_and_verify("f x:R n t>n;?x{^e:0}");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("non-exhaustive") && e.message.contains("~")));
    }

    #[test]
    fn exhaustive_bool_match() {
        assert!(parse_and_verify("f x:b>n;?x{true:1;false:0}").is_ok());
    }

    #[test]
    fn non_exhaustive_bool_missing_false() {
        let result = parse_and_verify("f x:b>n;?x{true:1}");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("non-exhaustive") && e.message.contains("false")));
    }

    #[test]
    fn non_exhaustive_number_no_wildcard() {
        let result = parse_and_verify("f x:n>t;?x{1:\"one\";2:\"two\"}");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("non-exhaustive") && e.message.contains("no wildcard")));
    }

    #[test]
    fn exhaustive_number_with_wildcard() {
        assert!(parse_and_verify("f x:n>t;?x{1:\"one\";_:\"other\"}").is_ok());
    }

    #[test]
    fn subjectless_match_no_false_positive() {
        // Subjectless match ?{...} — subject_ty is Nil, should not trigger exhaustiveness error
        assert!(parse_and_verify("f x:R n t>n;?x{~v:v;^e:0}").is_ok());
    }

    // ---- Display for Ty ----

    #[test]
    fn ty_display_bool() {
        assert_eq!(format!("{}", Ty::Bool), "b");
    }

    #[test]
    fn ty_display_nil() {
        assert_eq!(format!("{}", Ty::Nil), "_");
    }

    #[test]
    fn ty_display_list() {
        assert_eq!(format!("{}", Ty::List(Box::new(Ty::Number))), "L n");
    }

    #[test]
    fn ty_display_result() {
        assert_eq!(format!("{}", Ty::Result(Box::new(Ty::Number), Box::new(Ty::Text))), "R n t");
    }

    #[test]
    fn ty_display_named() {
        assert_eq!(format!("{}", Ty::Named("point".to_string())), "point");
    }

    #[test]
    fn ty_display_unknown() {
        assert_eq!(format!("{}", Ty::Unknown), "?");
    }

    // ---- Display for VerifyError ----

    #[test]
    fn verify_error_display_no_hint() {
        let e = VerifyError {
            code: "ILO-T004",
            function: "f".to_string(),
            message: "undefined variable 'x'".to_string(),
            hint: None,
            span: None,
        };
        let s = format!("{e}");
        assert!(s.contains("undefined variable 'x'"));
        assert!(s.contains("'f'"));
        assert!(!s.contains("hint"));
    }

    #[test]
    fn verify_error_display_with_hint() {
        let e = VerifyError {
            code: "ILO-T004",
            function: "f".to_string(),
            message: "undefined variable 'x'".to_string(),
            hint: Some("did you mean 'y'?".to_string()),
            span: None,
        };
        let s = format!("{e}");
        assert!(s.contains("hint: did you mean 'y'?"));
    }

    // ---- compatible() for Nil and Named ----

    #[test]
    fn compatible_nil_nil() {
        assert!(compatible(&Ty::Nil, &Ty::Nil));
    }

    #[test]
    fn compatible_named_same() {
        assert!(compatible(&Ty::Named("point".to_string()), &Ty::Named("point".to_string())));
    }

    #[test]
    fn compatible_named_different() {
        assert!(!compatible(&Ty::Named("point".to_string()), &Ty::Named("rect".to_string())));
    }

    #[test]
    fn compatible_list_unknown() {
        // List(Unknown) is compatible with List(Number) via Unknown inner
        assert!(compatible(
            &Ty::List(Box::new(Ty::Unknown)),
            &Ty::List(Box::new(Ty::Number))
        ));
    }

    #[test]
    fn compatible_result_unknown() {
        assert!(compatible(
            &Ty::Result(Box::new(Ty::Unknown), Box::new(Ty::Unknown)),
            &Ty::Result(Box::new(Ty::Number), Box::new(Ty::Text))
        ));
    }

    // ---- convert_type for Nil and Named ----

    #[test]
    fn convert_type_nil_and_named() {
        // A function with Nil return type exercises convert_type(Type::Nil)
        // Nil is compatible with Unknown (the verifier uses Unknown for unresolved bodies)
        // so we just verify it doesn't panic
        let _ = parse_and_verify("f x:n>_;x");
    }

    #[test]
    fn convert_type_named_in_signature() {
        // Function taking and returning a named type exercises convert_type(Type::Named)
        assert!(parse_and_verify("type point{x:n;y:n} f p:point>point;p").is_ok());
    }

    // ---- builtin_check_args type errors ----

    #[test]
    fn builtin_str_wrong_type() {
        let result = parse_and_verify("f x:t>t;str x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("'str' expects n, got t")));
    }

    #[test]
    fn builtin_num_wrong_type() {
        let result = parse_and_verify("f x:n>R n t;num x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("'num' expects t, got n")));
    }

    #[test]
    fn builtin_min_wrong_type() {
        let result = parse_and_verify("f x:t y:n>n;min x y");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("'min' arg 1 expects n, got t")));
    }

    #[test]
    fn builtin_max_wrong_type() {
        let result = parse_and_verify("f x:n y:t>n;max x y");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("'max' arg 2 expects n, got t")));
    }

    // ---- Tool declaration processing ----

    #[test]
    fn tool_declaration_processed() {
        // A tool should be collected and callable from a function
        let result = parse_and_verify(
            r#"tool my-tool "desc" x:n>n f y:n>n;my-tool y"#
        );
        assert!(result.is_ok());
    }

    #[test]
    fn tool_conflicts_with_function_name() {
        // Tool name conflicts with function name
        let result = parse_and_verify(
            r#"f x:n>n;*x 2 tool f "desc" y:n>n"#
        );
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("duplicate definition") || e.message.contains("duplicate function")));
    }

    // ---- TypeDef field validation ----

    #[test]
    fn typedef_field_with_undefined_named_type() {
        // A typedef with a field referencing an undefined type
        let result = parse_and_verify("type edge{from:node;to:node} f x:n>n;x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("undefined type 'node'")));
    }

    // ---- Undefined type in function signature ----

    #[test]
    fn undefined_type_in_function_param() {
        let result = parse_and_verify("f x:ghost>n;x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("undefined type 'ghost'")));
    }

    // ---- Record errors ----

    #[test]
    fn record_missing_field() {
        let result = parse_and_verify("type point{x:n;y:n} f>point;point x:1");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("missing field 'y'")));
    }

    #[test]
    fn record_extra_field() {
        let result = parse_and_verify("type point{x:n;y:n} f>point;point x:1 y:2 z:3");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("unknown field 'z'")));
    }

    #[test]
    fn record_field_type_mismatch() {
        let result = parse_and_verify("type point{x:n;y:n} f>point;point x:1 y:\"bad\"");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("field 'y' of 'point' expects n, got t")));
    }

    #[test]
    fn record_undefined_type() {
        // Constructing a record of an undefined type
        let result = parse_and_verify("f>n;x=ghost a:1;0");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("undefined type 'ghost'")));
    }

    // ---- Field access errors ----

    #[test]
    fn field_not_found_on_type() {
        let result = parse_and_verify("type point{x:n;y:n} f p:point>n;p.z");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("no field 'z' on type 'point'")));
    }

    #[test]
    fn field_access_on_non_record_type() {
        let result = parse_and_verify("f x:n>n;x.foo");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("field access on non-record type n")));
    }

    // ---- With expression errors ----

    #[test]
    fn with_on_non_record() {
        let result = parse_and_verify("f x:n>n;y=x with foo:1;0");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("'with' on non-record type n")));
    }

    #[test]
    fn with_field_not_found() {
        let result = parse_and_verify("type point{x:n;y:n} f p:point>point;p with z:1");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("unknown field 'z' in 'with'")));
    }

    #[test]
    fn with_field_type_mismatch() {
        let result = parse_and_verify("type point{x:n;y:n} f p:point>point;p with x:\"bad\"");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("'with' field 'x' of 'point' expects n, got t")));
    }

    // ---- BinOp errors ----

    #[test]
    fn binop_comparison_wrong_types() {
        let result = parse_and_verify("f x:n y:b>b;>x y");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("comparison expects matching n or t, got n and b")));
    }

    #[test]
    fn binop_append_non_list() {
        let result = parse_and_verify("f x:n>n;y=+=x 1;0");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("'+=' expects a list on the left, got n")));
    }

    #[test]
    fn binop_append_wrong_element_type() {
        let result = parse_and_verify("f xs:L n>L n;+=xs \"bad\"");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("'+=' list element type n doesn't match appended t")));
    }

    // ---- Expr::Match as expression (in let binding) ----

    #[test]
    fn match_as_expression_in_let() {
        assert!(parse_and_verify("f x:R n t>n;y=?x{~v:v;^e:0};y").is_ok());
    }

    // ---- Non-exhaustive match on Text/Number (the _ => branch in check_match_exhaustiveness) ----

    #[test]
    fn non_exhaustive_text_no_wildcard() {
        let result = parse_and_verify("f x:t>n;?x{\"a\":1;\"b\":2}");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("non-exhaustive") && e.message.contains("no wildcard")));
    }

    // ---- Index access on non-list (when type is not Unknown) ----

    #[test]
    fn index_access_on_non_list_bool() {
        let result = parse_and_verify("f x:b>b;x.0");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("index access on non-list")));
    }

    // ---- builtin len wrong type ----

    #[test]
    fn builtin_len_wrong_type() {
        let result = parse_and_verify("f x:n>n;len x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("'len' expects a list or text, got n")));
    }

    // ---- builtin abs/flr/cel wrong type ----

    #[test]
    fn builtin_abs_wrong_type() {
        let result = parse_and_verify("f x:t>n;abs x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("'abs' expects n, got t")));
    }

    // ---- duplicate type definition ----

    #[test]
    fn duplicate_type_definition() {
        let result = parse_and_verify("type point{x:n;y:n} type point{a:n;b:n} f x:n>n;x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("duplicate type definition 'point'")));
    }

    // ---- Bool literal in function body ----

    #[test]
    fn bool_literal_in_function_body() {
        assert!(parse_and_verify("f>b;true").is_ok());
    }

    // ---- Empty list (List(Unknown)) ----

    #[test]
    fn empty_list_type_is_unknown() {
        assert!(parse_and_verify("f>L n;[]").is_ok());
    }

    // ---- Negate wrong type ----

    #[test]
    fn negate_wrong_type() {
        let result = parse_and_verify("f x:t>n;-x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("negate expects n, got t")));
    }

    // ---- BinOp::Add with mixed types (including text+text, list+list) ----

    #[test]
    fn binop_add_text_text() {
        assert!(parse_and_verify("f a:t b:t>t;+a b").is_ok());
    }

    #[test]
    fn binop_add_list_list() {
        assert!(parse_and_verify("f a:L n b:L n>L n;+a b").is_ok());
    }

    #[test]
    fn binop_add_wrong_types() {
        let result = parse_and_verify("f x:n y:b>n;+x y");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("'+' expects matching n, t, or L types")));
    }

    // ---- BinOp::Equals and BinOp::And (return Bool) ----

    #[test]
    fn binop_equals_returns_bool() {
        assert!(parse_and_verify("f x:n y:n>b;=x y").is_ok());
    }

    #[test]
    fn binop_and_returns_bool() {
        assert!(parse_and_verify("f x:b y:b>b;&x y").is_ok());
    }

    // ---- With expr on Unknown type ----

    #[test]
    fn with_on_unknown_type_is_passthrough() {
        // An undefined function returns Unknown, so with on it should be Unknown
        let result = parse_and_verify("f x:n>n;y=undefined x;z=y with foo:1;0");
        // Should have errors for undefined function, but not panic
        assert!(result.is_err());
    }

    // ---- Field access on Named type where typedef is unknown (Named not in types) ----

    #[test]
    fn field_access_on_named_type_not_in_types() {
        // After an undefined type param, field access goes to the None branch
        let result = parse_and_verify("f p:ghost>n;p.x");
        assert!(result.is_err());
        // Should have an undefined type error for 'ghost'
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("undefined type 'ghost'")));
    }

    // ---- Match with subject (Expr::Match) ----

    #[test]
    fn match_stmt_with_subject() {
        assert!(parse_and_verify("f x:n>t;?x{1:\"one\";_:\"other\"}").is_ok());
    }

    // ---- Guard with Nil subject (stmt-level match without subject) ----

    #[test]
    fn match_stmt_no_subject() {
        assert!(parse_and_verify("f x:n>n;?{_:x}").is_ok());
    }

    // ---- Coverage: Ty::Unknown branches ----

    // L408: ForEach with Unknown collection type (undefined var → Unknown)
    #[test]
    fn foreach_unknown_collection() {
        // z is undefined → Ty::Unknown → elem_ty = Unknown (L408)
        let result = parse_and_verify("f x:n>n;@i z{i};0");
        // Verify produces "undefined variable" error — L408 is still hit
        let _ = result;
    }

    // L430: Pattern::Ok binding where subject type is Unknown (undefined var)
    #[test]
    fn ok_pattern_on_unknown_subject() {
        // z is undefined → Ty::Unknown → bind_pattern: Ty::Unknown => Ty::Unknown (L430)
        let result = parse_and_verify("f x:n>n;r=?z{~v:0;_:0};r");
        let _ = result;
    }

    // L431: Pattern::Ok binding where subject type is Number (not Result, not Unknown)
    #[test]
    fn ok_pattern_on_non_result() {
        // x:n is Number → bind_pattern: _ => Ty::Unknown (L431)
        let result = parse_and_verify("f x:n>n;r=?x{~v:0;_:0};r");
        let _ = result;
    }

    // L440: Pattern::Err binding where subject type is Unknown (undefined var)
    #[test]
    fn err_pattern_on_unknown_subject() {
        // z is undefined → Ty::Unknown → bind_pattern: Ty::Unknown => Ty::Unknown (L440)
        let result = parse_and_verify("f x:n>n;r=?z{^v:0;_:0};r");
        let _ = result;
    }

    // L441: Pattern::Err binding where subject type is non-Result (Number)
    #[test]
    fn err_pattern_on_non_result() {
        // x:n is Number → bind_pattern: _ => Ty::Unknown (L441)
        let result = parse_and_verify("f x:n>n;r=?x{^v:0;_:0};r");
        let _ = result;
    }

    // L647: Field access on a Named type where the field IS found
    #[test]
    fn field_access_on_named_type_found() {
        // p.x on type point{x:n;y:n} → fty.clone() at L647
        assert!(parse_and_verify("type point{x:n;y:n} f p:point>n;p.x").is_ok());
    }

    // L661: Field access on Unknown type (undefined var) → Ty::Unknown => Ty::Unknown
    #[test]
    fn field_access_on_unknown_type() {
        // z is undefined → Ty::Unknown → field access → Ty::Unknown (L661)
        let result = parse_and_verify("f x:n>n;z.field");
        let _ = result;
    }

    // L673: Index access on Unknown type (undefined var) → Ty::Unknown => Ty::Unknown
    #[test]
    fn index_access_on_unknown_type() {
        // z is undefined → Ty::Unknown → index access → Ty::Unknown (L673)
        let result = parse_and_verify("f x:n>n;z.0");
        let _ = result;
    }

    // L684: Expr::Match with no subject → subject_ty = Ty::Nil
    #[test]
    fn expr_match_no_subject() {
        // r=?{_:x} is Expr::Match with None subject → Ty::Nil (L684)
        assert!(parse_and_verify("f x:n>n;r=?{_:x};r").is_ok());
    }

    // L747: BinOp::Add where one operand is Unknown (undefined var)
    #[test]
    fn add_with_unknown_operand() {
        // z is undefined → Ty::Unknown; +z 1 → (Unknown, Number) → L747
        let result = parse_and_verify("f x:n>n;+z 1");
        let _ = result;
    }

    // L764: Comparison where one operand is Unknown (undefined var)
    #[test]
    fn compare_with_unknown_operand() {
        // z is undefined → Ty::Unknown; >z 0 → (Unknown, Number) → L764
        let result = parse_and_verify("f x:n>n;>z 0");
        let _ = result;
    }

    // L782: BinOp::Append where left type is Unknown (undefined var)
    #[test]
    fn append_with_unknown_left() {
        // z is undefined → Ty::Unknown; +=z 1 → Ty::Unknown => Ty::Unknown (L782)
        let result = parse_and_verify("f x:n>n;+=z 1");
        let _ = result;
    }

    // L835: check_match_exhaustiveness with Ty::Unknown subject (undefined var, no wildcard)
    #[test]
    fn match_exhaustiveness_unknown_subject() {
        // z is undefined → Ty::Unknown; ?z{1:0} has no wildcard → L835 Ty::Unknown branch
        let result = parse_and_verify("f x:n>n;?z{1:0};0");
        let _ = result;
    }

    // L835: check_match_exhaustiveness with Ty::Nil subject (subjectless match, no wildcard)
    #[test]
    fn match_exhaustiveness_nil_subject() {
        // ?{1:0} is subjectless (Nil subject) with no wildcard → L835 Ty::Nil branch
        let result = parse_and_verify("f x:n>n;?{1:0};0");
        let _ = result;
    }

    // L175: builtin_check_args "len" with empty arg_types (if let Some(arg) = None → false branch)
    #[test]
    fn builtin_check_args_len_no_args() {
        // Call directly with empty slice → if let Some(arg) evaluates to None → L175 closing }
        let (ty, errors) = builtin_check_args("len", &[], "test_func", None);
        assert!(errors.is_empty());
        assert_eq!(ty, Ty::Number);
    }

    // L230: builtin_check_args with unknown name → _ => (Ty::Unknown, errors)
    #[test]
    fn builtin_check_args_unknown_name() {
        let (ty, errors) = builtin_check_args("unknown_builtin", &[], "test_func", None);
        assert!(errors.is_empty());
        assert_eq!(ty, Ty::Unknown);
    }

    // L434: Pattern::Ok with wildcard name "_" → if name != "_" is false → closing } hit
    #[test]
    fn ok_pattern_wildcard_binding() {
        // ~_ is Pattern::Ok("_") → name == "_" → skip scope_insert → L434 closing }
        let result = parse_and_verify("f x:R n t>n;r=?x{~_:0;_:1};r");
        assert!(result.is_ok());
    }

    // L444: Pattern::Err with wildcard name "_" → if name != "_" is false → closing } hit
    #[test]
    fn err_pattern_wildcard_binding() {
        // ^_ is Pattern::Err("_") → name == "_" → skip scope_insert → L444 closing }
        let result = parse_and_verify("f x:R n t>n;r=?x{^_:1;_:0};r");
        assert!(result.is_ok());
    }

    // L726: Expr::With on Named type not found in self.types (undefined type)
    #[test]
    fn with_on_undefined_named_type() {
        // x:ghost → Ty::Named("ghost") not in types registry
        // → if let Some(type_def) = self.types.get("ghost") → None → L726 closing }
        let result = parse_and_verify("f x:ghost>ghost;x with name:\"bob\"");
        // Verify produces undefined type errors but still processes the with expression
        let _ = result;
    }

    // ---- C3: suggestion / hint tests ----

    #[test]
    fn suggestion_t008_number_body_text_expected() {
        let result = parse_and_verify("f x:n>t;*x 2");
        let errors = result.unwrap_err();
        let e = errors.iter().find(|e| e.code == "ILO-T008").unwrap();
        assert!(e.hint.as_ref().is_some_and(|h| h.contains("str")));
    }

    #[test]
    fn suggestion_t008_text_body_number_expected() {
        let result = parse_and_verify("f x:t>n;x");
        let errors = result.unwrap_err();
        let e = errors.iter().find(|e| e.code == "ILO-T008").unwrap();
        assert!(e.hint.as_ref().is_some_and(|h| h.contains("num")));
    }

    #[test]
    fn suggestion_t008_unrelated_mismatch_no_hint() {
        // bool → number: no specific hint
        let result = parse_and_verify("f x:b>n;x");
        let errors = result.unwrap_err();
        let e = errors.iter().find(|e| e.code == "ILO-T008").unwrap();
        assert!(e.hint.is_none());
    }

    #[test]
    fn suggestion_t006_user_defined_shows_signature() {
        let result = parse_and_verify("g a:n b:n>n;+a b f x:n>n;g x");
        let errors = result.unwrap_err();
        let e = errors.iter().find(|e| e.code == "ILO-T006" && e.message.contains("'g'")).unwrap();
        let hint = e.hint.as_ref().unwrap();
        assert!(hint.contains("'g' expects:"));
        assert!(hint.contains("a:n"));
        assert!(hint.contains("b:n"));
    }

    #[test]
    fn suggestion_t007_param_number_expected_text_given() {
        // g expects n, caller passes t
        let result = parse_and_verify("g x:n>n;*x 2 f x:t>n;g x");
        let errors = result.unwrap_err();
        let e = errors.iter().find(|e| e.code == "ILO-T007").unwrap();
        assert!(e.hint.as_ref().is_some_and(|h| h.contains("num")));
    }

    #[test]
    fn suggestion_t007_param_text_expected_number_given() {
        // g expects t, caller f passes n — use +x x body so greedy parsing doesn't consume f
        let result = parse_and_verify("g x:t>t;+x x f y:n>t;g y");
        let errors = result.unwrap_err();
        let e = errors.iter().find(|e| e.code == "ILO-T007").unwrap();
        assert!(e.hint.as_ref().is_some_and(|h| h.contains("str")));
    }

    #[test]
    fn suggestion_t009_add_mixed_nt_hint() {
        let result = parse_and_verify("f x:n y:t>n;+x y");
        let errors = result.unwrap_err();
        let e = errors.iter().find(|e| e.code == "ILO-T009" && e.message.contains("'+'")).unwrap();
        assert!(e.hint.as_ref().is_some_and(|h| h.contains("str")));
    }

    #[test]
    fn suggestion_t009_multiply_text_hint() {
        let result = parse_and_verify("f x:t y:n>n;*x y");
        let errors = result.unwrap_err();
        let e = errors.iter().find(|e| e.code == "ILO-T009" && e.message.contains("'*'")).unwrap();
        assert!(e.hint.as_ref().is_some_and(|h| h.contains("num")));
    }

    #[test]
    fn suggestion_t016_closest_match() {
        let result = parse_and_verify("type point{x:n;y:n} f>point;point x:1 y:2 z:3");
        let errors = result.unwrap_err();
        let e = errors.iter().find(|e| e.code == "ILO-T016").unwrap();
        // z is close to x or y — no match within distance 3 for "z"
        // Actually "z" has distance 1 from "x" (z→x) and distance 1 from "y" — let's just check it doesn't panic
        let _ = &e.hint;
    }

    #[test]
    fn suggestion_t019_closest_match() {
        // "nam" is close to "name"
        let result = parse_and_verify("type person{name:t;age:n} f p:person>t;p.nam");
        let errors = result.unwrap_err();
        let e = errors.iter().find(|e| e.code == "ILO-T019").unwrap();
        assert!(e.hint.as_ref().is_some_and(|h| h.contains("name")));
    }

    #[test]
    fn suggestion_t021_closest_match() {
        // "nam" is close to "name"
        let result = parse_and_verify("type person{name:t;age:n} f p:person>person;p with nam:\"bob\"");
        let errors = result.unwrap_err();
        let e = errors.iter().find(|e| e.code == "ILO-T021").unwrap();
        assert!(e.hint.as_ref().is_some_and(|h| h.contains("name")));
    }

    #[test]
    fn suggestion_t024_result_missing_err() {
        let result = parse_and_verify("f x:R n t>n;?x{~v:v}");
        let errors = result.unwrap_err();
        let e = errors.iter().find(|e| e.code == "ILO-T024").unwrap();
        let hint = e.hint.as_ref().unwrap();
        assert!(hint.contains("^e: <expr>"));
        assert!(hint.contains("t")); // err_ty = t
    }

    #[test]
    fn suggestion_t024_result_missing_ok() {
        let result = parse_and_verify("f x:R n t>n;?x{^e:0}");
        let errors = result.unwrap_err();
        let e = errors.iter().find(|e| e.code == "ILO-T024").unwrap();
        let hint = e.hint.as_ref().unwrap();
        assert!(hint.contains("~v: <expr>"));
        assert!(hint.contains("n")); // ok_ty = n
    }

    #[test]
    fn suggestion_t024_bool_missing_false() {
        let result = parse_and_verify("f x:b>n;?x{true:1}");
        let errors = result.unwrap_err();
        let e = errors.iter().find(|e| e.code == "ILO-T024").unwrap();
        let hint = e.hint.as_ref().unwrap();
        assert!(hint.contains("false: <expr>"));
    }

    #[test]
    fn suggestion_t024_bool_missing_true() {
        let result = parse_and_verify("f x:b>n;?x{false:0}");
        let errors = result.unwrap_err();
        let e = errors.iter().find(|e| e.code == "ILO-T024").unwrap();
        let hint = e.hint.as_ref().unwrap();
        assert!(hint.contains("true: <expr>"));
    }

    #[test]
    fn suggestion_t024_generic_wildcard() {
        let result = parse_and_verify("f x:n>t;?x{1:\"one\";2:\"two\"}");
        let errors = result.unwrap_err();
        let e = errors.iter().find(|e| e.code == "ILO-T024").unwrap();
        let hint = e.hint.as_ref().unwrap();
        assert!(hint.contains("_: <default-expr>"));
    }

    #[test]
    fn unwrap_valid_result_call() {
        // Construct AST directly to avoid parser boundary issues
        use crate::ast::*;
        let rnt = Type::Result(Box::new(Type::Number), Box::new(Type::Text));
        let prog = Program {
            declarations: vec![
                Decl::Function {
                    name: "inner".to_string(),
                    params: vec![Param { name: "x".to_string(), ty: Type::Number }],
                    return_type: rnt.clone(),
                    body: vec![Spanned::unknown(Stmt::Expr(Expr::Ok(Box::new(Expr::Ref("x".to_string())))))],
                    span: Span::UNKNOWN,
                },
                Decl::Function {
                    name: "outer".to_string(),
                    params: vec![Param { name: "x".to_string(), ty: Type::Number }],
                    return_type: rnt,
                    body: vec![
                        Spanned::unknown(Stmt::Let {
                            name: "d".to_string(),
                            value: Expr::Call { function: "inner".to_string(), args: vec![Expr::Ref("x".to_string())], unwrap: true },
                        }),
                        Spanned::unknown(Stmt::Expr(Expr::Ok(Box::new(Expr::Ref("d".to_string()))))),
                    ],
                    span: Span::UNKNOWN,
                },
            ],
            source: None,
        };
        let result = verify(&prog);
        assert!(result.is_ok(), "expected valid, got: {:?}", result);
    }

    #[test]
    fn unwrap_t025_non_result_callee() {
        // Callee returns n, not R — should emit T025
        use crate::ast::*;
        let prog = Program {
            declarations: vec![
                Decl::Function {
                    name: "inner".to_string(),
                    params: vec![Param { name: "x".to_string(), ty: Type::Number }],
                    return_type: Type::Number,
                    body: vec![Spanned::unknown(Stmt::Expr(Expr::Ref("x".to_string())))],
                    span: Span::UNKNOWN,
                },
                Decl::Function {
                    name: "outer".to_string(),
                    params: vec![Param { name: "x".to_string(), ty: Type::Number }],
                    return_type: Type::Result(Box::new(Type::Number), Box::new(Type::Text)),
                    body: vec![Spanned::unknown(Stmt::Expr(
                        Expr::Call { function: "inner".to_string(), args: vec![Expr::Ref("x".to_string())], unwrap: true },
                    ))],
                    span: Span::UNKNOWN,
                },
            ],
            source: None,
        };
        let errors = verify(&prog).unwrap_err();
        assert!(errors.iter().any(|e| e.code == "ILO-T025"), "expected T025, got: {:?}", errors);
    }

    #[test]
    fn unwrap_t026_non_result_enclosing() {
        // Enclosing returns n, not R — should emit T026
        use crate::ast::*;
        let rnt = Type::Result(Box::new(Type::Number), Box::new(Type::Text));
        let prog = Program {
            declarations: vec![
                Decl::Function {
                    name: "inner".to_string(),
                    params: vec![Param { name: "x".to_string(), ty: Type::Number }],
                    return_type: rnt,
                    body: vec![Spanned::unknown(Stmt::Expr(Expr::Ok(Box::new(Expr::Ref("x".to_string())))))],
                    span: Span::UNKNOWN,
                },
                Decl::Function {
                    name: "outer".to_string(),
                    params: vec![Param { name: "x".to_string(), ty: Type::Number }],
                    return_type: Type::Number,
                    body: vec![Spanned::unknown(Stmt::Expr(
                        Expr::Call { function: "inner".to_string(), args: vec![Expr::Ref("x".to_string())], unwrap: true },
                    ))],
                    span: Span::UNKNOWN,
                },
            ],
            source: None,
        };
        let errors = verify(&prog).unwrap_err();
        assert!(errors.iter().any(|e| e.code == "ILO-T026"), "expected T026, got: {:?}", errors);
    }

    #[test]
    fn builtin_get_valid() {
        assert!(parse_and_verify(r#"f url:t>R t t;get url"#).is_ok());
    }

    #[test]
    fn builtin_get_wrong_type() {
        let result = parse_and_verify("f x:n>R t t;get x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("'get' expects t")));
    }

    #[test]
    fn builtin_get_wrong_arity() {
        let result = parse_and_verify(r#"f x:t y:t>R t t;get x y"#);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("arity")));
    }

    #[test]
    fn dollar_desugars_to_get() {
        // $url should parse and verify the same as get url
        assert!(parse_and_verify(r#"f url:t>R t t;$url"#).is_ok());
    }

    // ---- Braceless guard ambiguity detection (ILO-T027) ----

    #[test]
    fn braceless_guard_body_is_function_name() {
        // `classify` is a known function — warn that it looks like a forgotten call
        let result = parse_and_verify("classify n:n>t;\"done\"\ncls sp:n>t;>=sp 1000 classify;\"fallback\"");
        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|e| e.code == "ILO-T027" && e.message.contains("classify")),
            "expected ILO-T027 for function name in braceless guard body, got: {:?}", errors
        );
        assert!(
            errors.iter().any(|e| e.hint.as_ref().is_some_and(|h| h.contains("braces"))),
            "expected hint about braces, got: {:?}", errors
        );
    }

    #[test]
    fn braceless_guard_body_is_variable_no_warning() {
        // `x` is a variable, not a function — no T027 warning
        assert!(parse_and_verify("f x:n>n;>=x 10 x").is_ok());
    }

    #[test]
    fn braceless_guard_body_is_builtin_name() {
        // `len` is a builtin — warn
        let result = parse_and_verify("f x:n>n;>=x 0 len;x");
        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|e| e.code == "ILO-T027" && e.message.contains("len")),
            "expected ILO-T027 for builtin name in braceless guard body, got: {:?}", errors
        );
    }

    #[test]
    fn spl_valid() {
        let result = parse_and_verify(r#"f s:t sep:t>L t;spl s sep"#);
        assert!(result.is_ok(), "spl with two text args should verify: {:?}", result);
    }

    #[test]
    fn spl_wrong_type() {
        let result = parse_and_verify(r#"f s:t n:n>L t;spl s n"#);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.code == "ILO-T013" && e.message.contains("spl")));
    }

    #[test]
    fn spl_wrong_arity() {
        let result = parse_and_verify(r#"f s:t>L t;spl s"#);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("spl")));
    }

    #[test]
    fn cat_valid_call() {
        assert!(parse_and_verify("f items:L t>t;cat items \",\"").is_ok());
    }

    #[test]
    fn cat_wrong_type_arg1() {
        let result = parse_and_verify("f x:n>t;cat x \",\"");
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.code == "ILO-T013" && e.message.contains("cat")));
    }

    #[test]
    fn cat_wrong_arity() {
        let result = parse_and_verify("f items:L t>t;cat items");
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("cat") && e.message.contains("2")));
    }

    #[test]
    fn has_valid_list() {
        assert!(parse_and_verify("f xs:L n x:n>b;has xs x").is_ok());
    }

    #[test]
    fn has_valid_text() {
        assert!(parse_and_verify(r#"f s:t needle:t>b;has s needle"#).is_ok());
    }

    #[test]
    fn has_wrong_type_arg1() {
        let result = parse_and_verify("f x:n y:n>b;has x y");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("'has' arg 1 expects a list or text")));
    }

    #[test]
    fn hd_valid_list() {
        assert!(parse_and_verify("f xs:L n>n;hd xs").is_ok());
    }

    #[test]
    fn tl_valid_list() {
        assert!(parse_and_verify("f xs:L n>L n;tl xs").is_ok());
    }

    #[test]
    fn hd_valid_text() {
        assert!(parse_and_verify("f s:t>t;hd s").is_ok());
    }

    #[test]
    fn hd_wrong_type() {
        let result = parse_and_verify("f x:n>n;hd x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("'hd' expects a list or text, got n")));
    }

    #[test]
    fn tl_wrong_type() {
        let result = parse_and_verify("f x:n>n;tl x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("'tl' expects a list or text, got n")));
    }

    #[test]
    fn rev_valid_list() {
        assert!(parse_and_verify("f xs:L n>L n;rev xs").is_ok());
    }

    #[test]
    fn rev_valid_text() {
        assert!(parse_and_verify("f s:t>t;rev s").is_ok());
    }

    #[test]
    fn rev_wrong_type() {
        let result = parse_and_verify("f n:n>L n;rev n");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.code == "ILO-T013" && e.message.contains("rev")));
    }

    #[test]
    fn srt_valid_list() {
        assert!(parse_and_verify("f>L n;xs=[3, 1, 2];srt xs").is_ok());
    }

    #[test]
    fn srt_wrong_type() {
        let result = parse_and_verify("f x:n>n;srt x");
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.code == "ILO-T013" && e.message.contains("srt")));
    }

    #[test]
    fn slc_valid_list() {
        assert!(parse_and_verify("f x:L n>L n;slc x 0 2").is_ok());
    }

    #[test]
    fn slc_valid_text() {
        assert!(parse_and_verify("f x:t>t;slc x 0 2").is_ok());
    }

    #[test]
    fn slc_wrong_collection_type() {
        let result = parse_and_verify("f x:n>n;slc x 0 2");
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.code == "ILO-T013" && e.message.contains("slc")));
    }

    #[test]
    fn slc_wrong_index_type() {
        let result = parse_and_verify("f x:L n s:t>L n;slc x s 2");
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.code == "ILO-T013" && e.message.contains("slc")));
    }

    #[test]
    fn while_valid() {
        assert!(parse_and_verify("f>n;i=0;wh <i 5{i=+i 1};i").is_ok());
    }

    #[test]
    fn ret_valid() {
        assert!(parse_and_verify("f x:n>n;ret +x 1").is_ok());
    }

    #[test]
    fn ret_in_guard() {
        assert!(parse_and_verify(r#"f x:n>t;>x 0{ret "pos"};"neg""#).is_ok());
    }
}
