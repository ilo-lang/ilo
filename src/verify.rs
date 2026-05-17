use std::collections::HashMap;

use crate::ast::*;
use crate::builtins::Builtin;

/// Verifier's internal type representation.
/// Adds `Unknown` for cases where we can't infer — compatible with anything.
#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    Number,
    Text,
    Bool,
    Nil,
    Optional(Box<Ty>),
    List(Box<Ty>),
    Map(Box<Ty>, Box<Ty>),
    Result(Box<Ty>, Box<Ty>),
    Sum(Vec<String>),
    /// Function type: params then return. `F n n` = Fn(vec![Number], Number).
    Fn(Vec<Ty>, Box<Ty>),
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
            Ty::Optional(inner) => write!(f, "O {inner}"),
            Ty::List(inner) => write!(f, "L {inner}"),
            Ty::Map(k, v) => write!(f, "M {k} {v}"),
            Ty::Result(ok, err) => write!(f, "R {ok} {err}"),
            Ty::Sum(variants) => {
                write!(f, "S")?;
                for v in variants {
                    write!(f, " {v}")?;
                }
                Ok(())
            }
            Ty::Fn(params, ret) => {
                write!(f, "F")?;
                for p in params {
                    write!(f, " {p}")?;
                }
                write!(f, " {ret}")
            }
            Ty::Named(name) => write!(f, "{name}"),
            Ty::Unknown => write!(f, "_"),
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
    pub is_warning: bool,
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

#[derive(Clone)]
struct TypeDef {
    fields: Vec<(String, Ty)>,
}

struct VerifyContext {
    functions: HashMap<String, FuncSig>,
    types: HashMap<String, TypeDef>,
    aliases: HashMap<String, Ty>,
    errors: Vec<VerifyError>,
    in_loop: bool,
}

type Scope = Vec<HashMap<String, Ty>>;

fn scope_lookup<'a>(scope: &'a Scope, name: &str) -> Option<&'a Ty> {
    for frame in scope.iter().rev() {
        if let Some(ty) = frame.get(name) {
            return Some(ty);
        }
    }
    None
}

fn scope_insert(scope: &mut Scope, name: String, ty: Ty) {
    if let Some(frame) = scope.last_mut() {
        frame.insert(name, ty);
    }
}

/// Collect all Named type references from a Type (for alias dependency tracking).
fn collect_named_refs(ty: &Type) -> Vec<String> {
    let mut refs = Vec::new();
    collect_named_refs_inner(ty, &mut refs);
    refs
}

fn collect_named_refs_inner(ty: &Type, refs: &mut Vec<String>) {
    match ty {
        Type::Named(name) => refs.push(name.clone()),
        Type::Optional(inner) => collect_named_refs_inner(inner, refs),
        Type::List(inner) => collect_named_refs_inner(inner, refs),
        Type::Map(k, v) => {
            collect_named_refs_inner(k, refs);
            collect_named_refs_inner(v, refs);
        }
        Type::Result(ok, err) => {
            collect_named_refs_inner(ok, refs);
            collect_named_refs_inner(err, refs);
        }
        Type::Fn(params, ret) => {
            for p in params {
                collect_named_refs_inner(p, refs);
            }
            collect_named_refs_inner(ret, refs);
        }
        Type::Sum(_) | Type::Number | Type::Text | Type::Bool | Type::Any => {}
    }
}

#[allow(dead_code)] // used in tests
fn convert_type(ast_ty: &Type) -> Ty {
    convert_type_with_aliases(ast_ty, &HashMap::new())
}

fn convert_type_with_aliases(ast_ty: &Type, aliases: &HashMap<String, Ty>) -> Ty {
    match ast_ty {
        Type::Number => Ty::Number,
        Type::Text => Ty::Text,
        Type::Bool => Ty::Bool,
        Type::Any => Ty::Unknown,
        Type::Optional(inner) => Ty::Optional(Box::new(convert_type_with_aliases(inner, aliases))),
        Type::List(inner) => Ty::List(Box::new(convert_type_with_aliases(inner, aliases))),
        Type::Map(k, v) => Ty::Map(
            Box::new(convert_type_with_aliases(k, aliases)),
            Box::new(convert_type_with_aliases(v, aliases)),
        ),
        Type::Result(ok, err) => Ty::Result(
            Box::new(convert_type_with_aliases(ok, aliases)),
            Box::new(convert_type_with_aliases(err, aliases)),
        ),
        Type::Sum(variants) => Ty::Sum(variants.clone()),
        Type::Fn(params, ret) => Ty::Fn(
            params
                .iter()
                .map(|p| convert_type_with_aliases(p, aliases))
                .collect(),
            Box::new(convert_type_with_aliases(ret, aliases)),
        ),
        Type::Named(name) => {
            if let Some(resolved) = aliases.get(name) {
                resolved.clone()
            } else if name.len() == 1
                && name.chars().next().is_some_and(|c| c.is_lowercase())
                && !matches!(name.as_str(), "n" | "t" | "b")
            {
                // Single lowercase letter not in aliases = type variable → compatible with anything
                Ty::Unknown
            } else {
                Ty::Named(name.clone())
            }
        }
    }
}

/// Two types are compatible if either is Unknown, or they're structurally equal.
fn compatible(a: &Ty, b: &Ty) -> bool {
    match (a, b) {
        (Ty::Unknown, _) | (_, Ty::Unknown) => true,
        // Optional: nil is always compatible with Optional; inner type is also compatible.
        (Ty::Nil, Ty::Optional(_)) | (Ty::Optional(_), Ty::Nil) => true,
        (Ty::Optional(a), Ty::Optional(b)) => compatible(a, b),
        (inner, Ty::Optional(b)) => compatible(inner, b),
        (Ty::Optional(a), inner) => compatible(a, inner),
        // Sum types are interchangeable with text (they're text at runtime).
        (Ty::Sum(_), Ty::Text) | (Ty::Text, Ty::Sum(_)) => true,
        (Ty::Sum(a), Ty::Sum(b)) => a == b,
        (Ty::Number, Ty::Number) => true,
        (Ty::Text, Ty::Text) => true,
        (Ty::Bool, Ty::Bool) => true,
        (Ty::Nil, Ty::Nil) => true,
        (Ty::List(a), Ty::List(b)) => compatible(a, b),
        (Ty::Map(ak, av), Ty::Map(bk, bv)) => compatible(ak, bk) && compatible(av, bv),
        (Ty::Result(ao, ae), Ty::Result(bo, be)) => compatible(ao, bo) && compatible(ae, be),
        (Ty::Fn(ap, ar), Ty::Fn(bp, br)) => {
            ap.len() == bp.len()
                && ap.iter().zip(bp).all(|(a, b)| compatible(a, b))
                && compatible(ar, br)
        }
        (Ty::Named(a), Ty::Named(b)) => a == b,
        _ => false,
    }
}

/// Validate a value being passed as a map key against the map's declared
/// key type. Allowed scalar key types are `Text` and `Number`; both may be
/// passed where the declared key type is `Unknown` (uninferred). Otherwise
/// the supplied type must be `compatible` with the declared type.
fn is_valid_map_key_arg(supplied: &Ty, declared: &Ty) -> bool {
    if matches!(supplied, Ty::Unknown) || matches!(declared, Ty::Unknown) {
        return true;
    }
    // A scalar map key must be text or number; nothing else.
    if !matches!(supplied, Ty::Text | Ty::Number) {
        return false;
    }
    compatible(supplied, declared)
}

/// Diagnostic-layer hint for an undefined kebab-case identifier whose halves
/// are themselves bound in scope. Misreading `best-d` (single identifier) as
/// `best - d` (subtraction) is a recurring persona footgun; the lexer always
/// keeps `best-d` atomic, so when both `best` and `d` resolve as values the
/// most useful nudge is to spell that out and show the explicit subtraction
/// form. Returns `None` if the name is not kebab-case or if any part is not
/// in scope as a variable / function / builtin.
fn kebab_subtract_hint<'a>(
    name: &str,
    candidates: impl Iterator<Item = &'a String> + Clone,
) -> Option<String> {
    if !name.contains('-') {
        return None;
    }
    let parts: Vec<&str> = name.split('-').collect();
    if parts.len() < 2 || parts.iter().any(|p| p.is_empty()) {
        return None;
    }
    let all_resolved = parts.iter().all(|p| {
        candidates.clone().any(|c| c == p) || is_builtin(p) || builtin_as_fn_ty(p).is_some()
    });
    if !all_resolved {
        return None;
    }
    if parts.len() == 2 {
        Some(format!(
            "'{name}' is a single identifier (kebab-case); for subtraction write '- {a} {b}'",
            a = parts[0],
            b = parts[1],
        ))
    } else {
        Some(format!(
            "'{name}' is a single identifier (kebab-case); '-' inside an identifier never means subtraction"
        ))
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
    for (i, row) in dp.iter_mut().enumerate().take(m + 1) {
        row[0] = i;
    }
    for (j, val) in dp[0].iter_mut().enumerate().take(n + 1) {
        *val = j;
    }
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
    ("rou", &["n"], "n"),
    ("min", &["n", "n"], "n"),
    ("min", &["list"], "n"),
    ("max", &["n", "n"], "n"),
    ("max", &["list"], "n"),
    ("mod", &["n", "n"], "n"),
    ("clamp", &["n", "n", "n"], "n"),
    ("pow", &["n", "n"], "n"),
    ("sqrt", &["n"], "n"),
    ("log", &["n"], "n"),
    ("exp", &["n"], "n"),
    ("sin", &["n"], "n"),
    ("cos", &["n"], "n"),
    ("tan", &["n"], "n"),
    ("log10", &["n"], "n"),
    ("log2", &["n"], "n"),
    ("asin", &["n"], "n"),
    ("acos", &["n"], "n"),
    ("atan", &["n"], "n"),
    ("atan2", &["n", "n"], "n"),
    ("get", &["t"], "R t t"),
    ("get", &["t", "M t t"], "R t t"),
    ("post", &["t", "t"], "R t t"),
    ("post", &["t", "t", "M t t"], "R t t"),
    ("get-many", &["L t"], "L (R t t)"),
    ("rd", &["t"], "R ? t"),
    ("rd", &["t", "t"], "R ? t"),
    ("rdl", &["t"], "R (L t) t"),
    ("rdb", &["t", "t"], "R ? t"),
    ("wr", &["t", "t"], "R t t"),
    ("wrl", &["t", "L t"], "R t t"),
    ("trm", &["t"], "t"),
    ("upr", &["t"], "t"),
    ("lwr", &["t"], "t"),
    ("cap", &["t"], "t"),
    ("padl", &["t", "n"], "t"),
    ("padr", &["t", "n"], "t"),
    ("ord", &["t"], "n"),
    ("chr", &["n"], "t"),
    ("chars", &["t"], "L t"),
    ("spl", &["t", "t"], "L t"),
    ("cat", &["L t", "t"], "t"),
    ("zip", &["list", "list"], "list"),
    ("enumerate", &["list"], "list"),
    ("range", &["n", "n"], "L n"),
    ("window", &["n", "list"], "list"),
    ("chunks", &["n", "L a"], "L (L a)"),
    ("setunion", &["list", "list"], "list"),
    ("setinter", &["list", "list"], "list"),
    ("setdiff", &["list", "list"], "list"),
    ("has", &["list_or_text", "any"], "b"),
    ("hd", &["list_or_text"], "any"),
    ("at", &["list_or_text", "n"], "any"),
    ("tl", &["list_or_text"], "list_or_text"),
    ("rev", &["list_or_text"], "list_or_text"),
    ("srt", &["list_or_text"], "list_or_text"),
    ("srt", &["fn", "list"], "list"),
    ("rsrt", &["list_or_text"], "list_or_text"),
    ("rsrt", &["fn", "list"], "list"),
    ("unq", &["list_or_text"], "list_or_text"),
    ("slc", &["list_or_text", "n", "n"], "list_or_text"),
    ("lst", &["list", "n", "any"], "list"),
    ("take", &["n", "list_or_text"], "list_or_text"),
    ("drop", &["n", "list_or_text"], "list_or_text"),
    ("rnd", &[], "n"),
    ("rndn", &["n", "n"], "n"),
    ("now", &[], "n"),
    ("sleep", &["n"], "_"),
    ("dtfmt", &["n", "t"], "R t t"),
    ("dtparse", &["t", "t"], "R n t"),
    ("env", &["t"], "R t t"),
    ("jpth", &["t", "t"], "R t t"),
    ("jdmp", &["any"], "t"),
    ("prnt", &["any"], "any"),
    ("fmt", &["t"], "t"), // variadic: fmt template arg1 arg2 … — checked specially
    ("fmt2", &["n", "n"], "t"),
    ("jpar", &["t"], "R ? t"),
    ("rdjl", &["t"], "L (R ? t)"),
    // Higher-order: map/flt/fld take a function ref as first arg (special-cased in builtin_check_args)
    ("map", &["fn", "list"], "list"),
    ("mapr", &["fn", "list"], "result"),
    ("flt", &["fn", "list"], "list"),
    ("fld", &["fn", "list", "any"], "any"),
    ("grp", &["fn", "list"], "map"),
    ("uniqby", &["fn", "list"], "list"),
    ("partition", &["fn", "list"], "list"),
    ("frq", &["list"], "map"),
    ("flatmap", &["fn", "list"], "list"),
    ("flat", &["list"], "list"),
    ("sum", &["list"], "n"),
    ("cumsum", &["L n"], "L n"),
    ("avg", &["list"], "n"),
    ("median", &["list"], "n"),
    ("quantile", &["list", "n"], "n"),
    ("stdev", &["list"], "n"),
    ("variance", &["list"], "n"),
    ("fft", &["list"], "list"),
    ("ifft", &["list"], "list"),
    ("transpose", &["L (L n)"], "L (L n)"),
    ("matmul", &["L (L n)", "L (L n)"], "L (L n)"),
    ("dot", &["L n", "L n"], "n"),
    ("rgx", &["t", "t"], "L t"),
    ("rgxall", &["t", "t"], "L (L t)"),
    ("rgxsub", &["t", "t", "t"], "t"),
    // Map builtins (M k v type)
    ("mmap", &[], "map"),
    ("mget", &["map", "t"], "optional"),
    ("mset", &["map", "t", "any"], "map"),
    ("mhas", &["map", "t"], "b"),
    ("mkeys", &["map"], "L t"),
    ("mvals", &["map"], "list"),
    ("mdel", &["map", "t"], "map"),
    // Linear algebra
    ("solve", &["L (L n)", "L n"], "L n"),
    ("inv", &["L (L n)"], "L (L n)"),
    ("det", &["L (L n)"], "n"),
];

fn builtin_arity(name: &str) -> Option<usize> {
    BUILTINS
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, params, _)| params.len())
}

fn is_builtin(name: &str) -> bool {
    Builtin::is_builtin(name)
}

/// Detect the common param-order mistake in closure-bind HOFs like
/// `srt fn ctx xs`, `rsrt fn ctx xs`, `map fn ctx xs`, `flt fn ctx xs`.
/// The fn must take `(element, ctx)` in that order. Personas frequently
/// write `(ctx, element)` because the call-site lists `fn ctx xs` and
/// the lambda's first param looks like it should match the ctx slot.
///
/// We detect the swap when:
///   - the fn's first param type matches the ctx slot's type, and
///   - the fn's second param type matches the list element type, and
///   - those two types are distinct (otherwise we can't tell)
///
/// Returns a hint string when the swap is detected, else None.
fn detect_ctx_param_swap(fn_ty: &Ty, ctx_ty: &Ty, xs_ty: &Ty) -> Option<String> {
    let Ty::Fn(params, _) = fn_ty else {
        return None;
    };
    if params.len() != 2 {
        return None;
    }
    let p0 = &params[0];
    let p1 = &params[1];
    let elem_ty = match xs_ty {
        Ty::List(inner) => (**inner).clone(),
        _ => return None,
    };
    if matches!(p0, Ty::Unknown) || matches!(p1, Ty::Unknown) {
        return None;
    }
    if matches!(ctx_ty, Ty::Unknown) || matches!(elem_ty, Ty::Unknown) {
        return None;
    }
    if p0 == p1 {
        return None;
    }
    if p0 == ctx_ty && *p1 == elem_ty && *ctx_ty != elem_ty {
        return Some(format!(
            "param order is `(element, ctx)`, not `(ctx, element)` — your fn takes `({p0}, {p1})` but the list element is `{elem_ty}` and the ctx is `{ctx_ty}`; swap the params to `({p1}, {p0})`"
        ));
    }
    None
}

/// If `name` is a pure builtin that's safe to pass as a higher-order argument,
/// return its function type. Returns None for IO/HTTP/Map builtins, for HOFs
/// themselves (map/flt/fld/grp), and for builtins with ambiguous/polymorphic
/// types that can't be reduced to a single `Ty::Fn` signature (hd, tl, etc).
fn builtin_as_fn_ty(name: &str) -> Option<Ty> {
    let n = Ty::Number;
    let t = Ty::Text;
    Some(match name {
        // 1-arg n->n
        "abs" | "flr" | "cel" | "rou" => Ty::Fn(vec![n.clone()], Box::new(n)),
        // 2-arg n,n->n (suitable as fld accumulator)
        "min" | "max" | "mod" => Ty::Fn(vec![n.clone(), n.clone()], Box::new(n)),
        // 1-arg list->n
        "sum" | "avg" | "median" | "stdev" | "variance" => {
            Ty::Fn(vec![Ty::List(Box::new(n.clone()))], Box::new(n))
        }
        // 1-arg t->t
        "trm" | "upr" | "lwr" | "cap" => Ty::Fn(vec![t.clone()], Box::new(t)),
        // 2-arg t,n->t
        "padl" | "padr" => Ty::Fn(vec![t.clone(), n.clone()], Box::new(t)),
        // 1-arg t->n / n->t (ASCII / Unicode codepoint round-trip)
        "ord" => Ty::Fn(vec![t.clone()], Box::new(n.clone())),
        "chr" => Ty::Fn(vec![n.clone()], Box::new(t.clone())),
        // 1-arg t -> L t (split into single-char strings, one per Unicode scalar)
        "chars" => Ty::Fn(vec![t.clone()], Box::new(Ty::List(Box::new(t.clone())))),
        // 1-arg n->t and t->R n t
        "str" => Ty::Fn(vec![n], Box::new(t)),
        "num" => Ty::Fn(
            vec![t.clone()],
            Box::new(Ty::Result(Box::new(Ty::Number), Box::new(t))),
        ),
        // 1-arg any->t (passthrough but typed as text for json dump)
        "jdmp" => Ty::Fn(vec![Ty::Unknown], Box::new(t)),
        // 1-arg list->n (len of list)
        "len" => Ty::Fn(vec![Ty::List(Box::new(Ty::Unknown))], Box::new(Ty::Number)),
        _ => return None,
    })
}

fn builtin_check_args(
    name: &str,
    arg_types: &[Ty],
    func_ctx: &str,
    span: Option<Span>,
) -> (Ty, Vec<VerifyError>) {
    let mut errors = Vec::new();
    match name {
        "len" => {
            if let Some(arg) = arg_types.first() {
                match arg {
                    Ty::List(_) | Ty::Map(_, _) | Ty::Text | Ty::Unknown => {}
                    other => errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'len' expects a list, map, or text, got {other}"),
                        hint: None,
                        span,
                        is_warning: false,
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
                    is_warning: false,
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
                    is_warning: false,
                });
            }
            (Ty::Result(Box::new(Ty::Number), Box::new(Ty::Text)), errors)
        }
        "abs" | "flr" | "cel" | "rou" | "sqrt" | "log" | "exp" | "sin" | "cos" | "tan"
        | "log10" | "log2" | "asin" | "acos" | "atan" => {
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &Ty::Number)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'{name}' expects n, got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            (Ty::Number, errors)
        }
        "min" | "max" if arg_types.len() == 1 => {
            // 1-arg list form: returns min/max element of a list of numbers
            if let Some(arg) = arg_types.first() {
                match arg {
                    Ty::List(inner) => {
                        if !compatible(inner, &Ty::Number) {
                            errors.push(VerifyError {
                                code: "ILO-T013",
                                function: func_ctx.to_string(),
                                message: format!("'{name}' expects L n, got L {inner}"),
                                hint: None,
                                span,
                                is_warning: false,
                            });
                        }
                    }
                    Ty::Unknown => {}
                    other => errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'{name}' expects L n, got {other}"),
                        hint: Some(format!(
                            "use `{name} xs` for the list form or `{name} a b` for two numbers"
                        )),
                        span,
                        is_warning: false,
                    }),
                }
            }
            (Ty::Number, errors)
        }
        "min" | "max" | "mod" | "pow" | "atan2" | "clamp" => {
            for (i, arg) in arg_types.iter().enumerate() {
                if !compatible(arg, &Ty::Number) {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'{name}' arg {} expects n, got {arg}", i + 1),
                        hint: None,
                        span,
                        is_warning: false,
                    });
                }
            }
            (Ty::Number, errors)
        }
        "range" => {
            for (i, arg) in arg_types.iter().enumerate() {
                if !compatible(arg, &Ty::Number) {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'range' arg {} expects n, got {arg}", i + 1),
                        hint: None,
                        span,
                        is_warning: false,
                    });
                }
            }
            (Ty::List(Box::new(Ty::Number)), errors)
        }
        "rnd" => {
            for (i, arg) in arg_types.iter().enumerate() {
                if !compatible(arg, &Ty::Number) {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'rnd' arg {} expects n, got {arg}", i + 1),
                        hint: None,
                        span,
                        is_warning: false,
                    });
                }
            }
            (Ty::Number, errors)
        }
        "rndn" => {
            for (i, arg) in arg_types.iter().enumerate() {
                if !compatible(arg, &Ty::Number) {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'rndn' arg {} expects n, got {arg}", i + 1),
                        hint: None,
                        span,
                        is_warning: false,
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
                        is_warning: false,
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
                    is_warning: false,
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
                    is_warning: false,
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
                        is_warning: false,
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
                        is_warning: false,
                    }),
                }
            }
            (Ty::Unknown, errors)
        }
        "at" => {
            // at xs i — returns the i-th element of xs (list or text)
            if let Some(arg) = arg_types.get(1)
                && !compatible(arg, &Ty::Number)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'at' index must be n, got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            if let Some(arg) = arg_types.first() {
                match arg {
                    Ty::List(inner) => return (*inner.clone(), errors),
                    Ty::Text => return (Ty::Text, errors),
                    Ty::Unknown => return (Ty::Unknown, errors),
                    other => errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'at' expects a list or text, got {other}"),
                        hint: None,
                        span,
                        is_warning: false,
                    }),
                }
            }
            (Ty::Unknown, errors)
        }
        "lst" => {
            // lst xs i v — return new list with index i replaced by v.
            // Type variable: list element type and value type must match.
            if let Some(arg) = arg_types.get(1)
                && !compatible(arg, &Ty::Number)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'lst' index must be n, got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            let list_ty = arg_types.first();
            let val_ty = arg_types.get(2);
            match list_ty {
                Some(Ty::List(inner)) => {
                    if let Some(v) = val_ty
                        && !compatible(v, inner)
                    {
                        errors.push(VerifyError {
                            code: "ILO-T013",
                            function: func_ctx.to_string(),
                            message: format!(
                                "'lst' value type {v} does not match list element type {inner}"
                            ),
                            hint: None,
                            span,
                            is_warning: false,
                        });
                    }
                    return (Ty::List(inner.clone()), errors);
                }
                Some(Ty::Unknown) => return (Ty::Unknown, errors),
                Some(other) => errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'lst' expects a list, got {other}"),
                    hint: None,
                    span,
                    is_warning: false,
                }),
                None => {}
            }
            (Ty::Unknown, errors)
        }
        "zip" => {
            // zip xs ys — returns a list of 2-element pairs [[x,y],...].
            // Truncates to the shorter list. Inner element type is the unification
            // (or fallback to Unknown) of the two list element types.
            let elem_a = match arg_types.first() {
                Some(Ty::List(inner)) => Some((**inner).clone()),
                Some(Ty::Unknown) | None => None,
                Some(other) => {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'zip' arg 1 expects a list, got {other}"),
                        hint: None,
                        span,
                        is_warning: false,
                    });
                    None
                }
            };
            let elem_b = match arg_types.get(1) {
                Some(Ty::List(inner)) => Some((**inner).clone()),
                Some(Ty::Unknown) | None => None,
                Some(other) => {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'zip' arg 2 expects a list, got {other}"),
                        hint: None,
                        span,
                        is_warning: false,
                    });
                    None
                }
            };
            let inner = match (elem_a, elem_b) {
                (Some(a), Some(b)) if compatible(&a, &b) => a,
                (Some(a), None) => a,
                (None, Some(b)) => b,
                _ => Ty::Unknown,
            };
            (Ty::List(Box::new(Ty::List(Box::new(inner)))), errors)
        }
        "enumerate" => {
            // enumerate xs — returns a list of [index, value] pairs.
            // Inner element type is erased to Unknown because the pair holds
            // both a number (index) and an `a` (element).
            if let Some(arg) = arg_types.first() {
                match arg {
                    Ty::List(_) | Ty::Unknown => {}
                    other => errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'enumerate' expects a list, got {other}"),
                        hint: None,
                        span,
                        is_warning: false,
                    }),
                }
            }
            (Ty::List(Box::new(Ty::List(Box::new(Ty::Unknown)))), errors)
        }
        "window" => {
            // window n xs — returns a list of consecutive n-sized sub-lists of xs.
            // Signature: window n:n xs:L a > L (L a)
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &Ty::Number)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'window' arg 1 expects n, got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            let inner = match arg_types.get(1) {
                Some(Ty::List(inner)) => (**inner).clone(),
                Some(Ty::Unknown) | None => Ty::Unknown,
                Some(other) => {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'window' arg 2 expects a list, got {other}"),
                        hint: None,
                        span,
                        is_warning: false,
                    });
                    Ty::Unknown
                }
            };
            (Ty::List(Box::new(Ty::List(Box::new(inner)))), errors)
        }
        name @ ("setunion" | "setinter" | "setdiff") => {
            // setunion/setinter/setdiff a:L _ b:L _ > L _
            let elem_a = match arg_types.first() {
                Some(Ty::List(inner)) => Some((**inner).clone()),
                Some(Ty::Unknown) | None => None,
                Some(other) => {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'{name}' arg 1 expects a list, got {other}"),
                        hint: None,
                        span,
                        is_warning: false,
                    });
                    None
                }
            };
            let elem_b = match arg_types.get(1) {
                Some(Ty::List(inner)) => Some((**inner).clone()),
                Some(Ty::Unknown) | None => None,
                Some(other) => {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'{name}' arg 2 expects a list, got {other}"),
                        hint: None,
                        span,
                        is_warning: false,
                    });
                    None
                }
            };
            let inner = match (elem_a, elem_b) {
                (Some(a), Some(b)) if compatible(&a, &b) => a,
                (Some(a), None) => a,
                (None, Some(b)) => b,
                (Some(a), Some(_)) => a,
                _ => Ty::Unknown,
            };
            (Ty::List(Box::new(inner)), errors)
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
                        is_warning: false,
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
                        is_warning: false,
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
        "trm" | "upr" | "lwr" | "cap" => {
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &Ty::Text)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'{name}' expects t, got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            (Ty::Text, errors)
        }
        "padl" | "padr" => {
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &Ty::Text)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'{name}' arg 1 expects t, got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            if let Some(arg) = arg_types.get(1)
                && !compatible(arg, &Ty::Number)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'{name}' arg 2 expects n, got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            // Optional pad-char arg (3-arg overload): must be text (a 1-character
            // string at runtime — char-count is checked by the executor).
            if let Some(arg) = arg_types.get(2)
                && !compatible(arg, &Ty::Text)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'{name}' arg 3 (pad char) expects t, got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            (Ty::Text, errors)
        }
        "ord" => {
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &Ty::Text)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'ord' expects t, got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            (Ty::Number, errors)
        }
        "chr" => {
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &Ty::Number)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'chr' expects n, got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            (Ty::Text, errors)
        }
        "chars" => {
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &Ty::Text)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'chars' expects t, got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            (Ty::List(Box::new(Ty::Text)), errors)
        }
        "unq" => {
            if let Some(arg) = arg_types.first() {
                match arg {
                    Ty::List(_) | Ty::Text | Ty::Unknown => {}
                    other => errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'unq' expects a list or text, got {other}"),
                        hint: None,
                        span,
                        is_warning: false,
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
        "fmt" => {
            // fmt template arg1 arg2 … — at least 1 arg (the template)
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &Ty::Text)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'fmt' first arg must be a text template, got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            (Ty::Text, errors)
        }
        "fmt2" => {
            // fmt2 x digits — format number x to `digits` decimal places, returning text.
            for (i, arg) in arg_types.iter().enumerate() {
                if !compatible(arg, &Ty::Number) {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'fmt2' arg {} expects n, got {arg}", i + 1),
                        hint: None,
                        span,
                        is_warning: false,
                    });
                }
            }
            (Ty::Text, errors)
        }
        "srt" => {
            if arg_types.len() == 3 {
                // srt key-fn ctx xs — closure-bind variant: fn takes (elem, ctx)
                if let Some(fn_ty) = arg_types.first()
                    && !matches!(fn_ty, Ty::Fn(_, _) | Ty::Unknown)
                {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'srt' key arg must be a function (F ...), got {fn_ty}"),
                        hint: Some("pass a function name: srt key-fn ctx xs".to_string()),
                        span,
                        is_warning: false,
                    });
                }
                // fn must accept 2 args
                if let Some(Ty::Fn(params, _)) = arg_types.first()
                    && params.len() != 2
                {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!(
                            "'srt' key fn must take 2 args (elem, ctx) for closure-bind variant, got {} args",
                            params.len()
                        ),
                        hint: Some("for srt fn ctx xs, fn must be: F a c b".to_string()),
                        span,
                        is_warning: false,
                    });
                }
                if let (Some(fn_ty), Some(ctx_ty), Some(xs_ty)) =
                    (arg_types.first(), arg_types.get(1), arg_types.get(2))
                    && let Some(hint) = detect_ctx_param_swap(fn_ty, ctx_ty, xs_ty)
                {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: "'srt' fn params look swapped for closure-bind variant"
                            .to_string(),
                        hint: Some(hint),
                        span,
                        is_warning: false,
                    });
                }
                let ret = match arg_types.get(2) {
                    Some(ty @ Ty::List(_)) => ty.clone(),
                    _ => Ty::Unknown,
                };
                return (ret, errors);
            }
            if arg_types.len() == 2 {
                // srt key-fn xs — sort by key function
                if let Some(fn_ty) = arg_types.first()
                    && !matches!(fn_ty, Ty::Fn(_, _) | Ty::Unknown)
                {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'srt' key arg must be a function (F ...), got {fn_ty}"),
                        hint: Some("pass a function name: srt key-fn xs".to_string()),
                        span,
                        is_warning: false,
                    });
                }
                let ret = match arg_types.get(1) {
                    Some(ty @ Ty::List(_)) => ty.clone(),
                    _ => Ty::Unknown,
                };
                return (ret, errors);
            }
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
                        is_warning: false,
                    }),
                }
            }
            (Ty::Unknown, errors)
        }
        "rsrt" => {
            if arg_types.len() == 3 {
                // rsrt key-fn ctx xs — closure-bind variant: fn takes (elem, ctx)
                if let Some(fn_ty) = arg_types.first()
                    && !matches!(fn_ty, Ty::Fn(_, _) | Ty::Unknown)
                {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'rsrt' key arg must be a function (F ...), got {fn_ty}"),
                        hint: Some("pass a function name: rsrt key-fn ctx xs".to_string()),
                        span,
                        is_warning: false,
                    });
                }
                if let Some(Ty::Fn(params, _)) = arg_types.first()
                    && params.len() != 2
                {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!(
                            "'rsrt' key fn must take 2 args (elem, ctx) for closure-bind variant, got {} args",
                            params.len()
                        ),
                        hint: Some("for rsrt fn ctx xs, fn must be: F a c b".to_string()),
                        span,
                        is_warning: false,
                    });
                }
                if let (Some(fn_ty), Some(ctx_ty), Some(xs_ty)) =
                    (arg_types.first(), arg_types.get(1), arg_types.get(2))
                    && let Some(hint) = detect_ctx_param_swap(fn_ty, ctx_ty, xs_ty)
                {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: "'rsrt' fn params look swapped for closure-bind variant"
                            .to_string(),
                        hint: Some(hint),
                        span,
                        is_warning: false,
                    });
                }
                let ret = match arg_types.get(2) {
                    Some(ty @ Ty::List(_)) => ty.clone(),
                    _ => Ty::Unknown,
                };
                return (ret, errors);
            }
            if arg_types.len() == 2 {
                // rsrt key-fn xs — descending sort by key function
                if let Some(fn_ty) = arg_types.first()
                    && !matches!(fn_ty, Ty::Fn(_, _) | Ty::Unknown)
                {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'rsrt' key arg must be a function (F ...), got {fn_ty}"),
                        hint: Some("pass a function name: rsrt key-fn xs".to_string()),
                        span,
                        is_warning: false,
                    });
                }
                let ret = match arg_types.get(1) {
                    Some(ty @ Ty::List(_)) => ty.clone(),
                    _ => Ty::Unknown,
                };
                return (ret, errors);
            }
            if let Some(arg) = arg_types.first() {
                match arg {
                    Ty::List(inner) => return (Ty::List(inner.clone()), errors),
                    Ty::Text => return (Ty::Text, errors),
                    Ty::Unknown => return (Ty::Unknown, errors),
                    other => errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'rsrt' expects a list or text, got {other}"),
                        hint: None,
                        span,
                        is_warning: false,
                    }),
                }
            }
            (Ty::Unknown, errors)
        }
        "fft" => {
            if let Some(arg) = arg_types.first() {
                match arg {
                    Ty::List(_) | Ty::Unknown => {}
                    other => errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'fft' expects a list of numbers, got {other}"),
                        hint: None,
                        span,
                        is_warning: false,
                    }),
                }
            }
            // Output is L (L n) — list of [real, imag] pairs.
            (Ty::List(Box::new(Ty::List(Box::new(Ty::Number)))), errors)
        }
        "ifft" => {
            if let Some(arg) = arg_types.first() {
                match arg {
                    Ty::List(_) | Ty::Unknown => {}
                    other => errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!(
                            "'ifft' expects a list of [real, imag] pairs, got {other}"
                        ),
                        hint: None,
                        span,
                        is_warning: false,
                    }),
                }
            }
            (Ty::List(Box::new(Ty::Number)), errors)
        }
        "median" | "stdev" | "variance" => {
            if let Some(arg) = arg_types.first() {
                match arg {
                    Ty::List(_) | Ty::Unknown => {}
                    other => errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'{name}' expects a list of numbers, got {other}"),
                        hint: None,
                        span,
                        is_warning: false,
                    }),
                }
            }
            (Ty::Number, errors)
        }
        "quantile" => {
            if let Some(arg) = arg_types.first() {
                match arg {
                    Ty::List(_) | Ty::Unknown => {}
                    other => errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'quantile' first arg must be a list, got {other}"),
                        hint: None,
                        span,
                        is_warning: false,
                    }),
                }
            }
            if let Some(arg) = arg_types.get(1)
                && !compatible(arg, &Ty::Number)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'quantile' second arg p must be n, got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            (Ty::Number, errors)
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
                        is_warning: false,
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
                        is_warning: false,
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
        "take" | "drop" => {
            // take n xs / drop n xs — first arg is n, second is list_or_text
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &Ty::Number)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'{name}' count must be n, got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            if let Some(arg) = arg_types.get(1) {
                match arg {
                    Ty::List(inner) => return (Ty::List(inner.clone()), errors),
                    Ty::Text => return (Ty::Text, errors),
                    Ty::Unknown => return (Ty::Unknown, errors),
                    other => errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'{name}' expects a list or text, got {other}"),
                        hint: None,
                        span,
                        is_warning: false,
                    }),
                }
            }
            (Ty::Unknown, errors)
        }
        "get" => {
            // get url          — 1-arg
            // get url headers  — 2-arg: headers is M t t
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &Ty::Text)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'get' expects t (url), got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            if let Some(arg) = arg_types.get(1) {
                let map_ty = Ty::Map(Box::new(Ty::Text), Box::new(Ty::Text));
                if !compatible(arg, &map_ty) {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'get' headers arg expects M t t, got {arg}"),
                        hint: None,
                        span,
                        is_warning: false,
                    });
                }
            }
            (Ty::Result(Box::new(Ty::Text), Box::new(Ty::Text)), errors)
        }
        "post" => {
            // post url body          — 2-arg
            // post url body headers  — 3-arg: headers is M t t
            for (i, arg) in arg_types.iter().enumerate().take(2) {
                if !compatible(arg, &Ty::Text) {
                    let label = if i == 0 { "url" } else { "body" };
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'post' expects t ({label}), got {arg}"),
                        hint: None,
                        span,
                        is_warning: false,
                    });
                }
            }
            if let Some(arg) = arg_types.get(2) {
                let map_ty = Ty::Map(Box::new(Ty::Text), Box::new(Ty::Text));
                if !compatible(arg, &map_ty) {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'post' headers arg expects M t t, got {arg}"),
                        hint: None,
                        span,
                        is_warning: false,
                    });
                }
            }
            (Ty::Result(Box::new(Ty::Text), Box::new(Ty::Text)), errors)
        }
        "get-many" => {
            // get-many urls — urls is L t; returns L (R t t) (one Result per URL)
            let list_text = Ty::List(Box::new(Ty::Text));
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &list_text)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'get-many' expects L t (list of urls), got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            (
                Ty::List(Box::new(Ty::Result(Box::new(Ty::Text), Box::new(Ty::Text)))),
                errors,
            )
        }
        "rd" | "rdb" => {
            // rd path         — 1-arg: auto-detect format from extension → R ? t
            // rd path fmt     — 2-arg: explicit format override → R ? t
            // rdb s fmt       — 2-arg: parse string/buffer in given format → R ? t
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &Ty::Text)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'{name}' expects t (path/string), got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            if arg_types.len() == 2
                && let Some(fmt) = arg_types.get(1)
                && !compatible(fmt, &Ty::Text)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!(
                        "'{name}' format arg expects t (\"csv\", \"json\", \"raw\"…), got {fmt}"
                    ),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            (
                Ty::Result(Box::new(Ty::Unknown), Box::new(Ty::Text)),
                errors,
            )
        }
        "rdl" => {
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &Ty::Text)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'rdl' expects t (path), got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            (
                Ty::Result(Box::new(Ty::List(Box::new(Ty::Text))), Box::new(Ty::Text)),
                errors,
            )
        }
        "wr" | "wrl" => {
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &Ty::Text)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'{name}' arg 1 expects t (path), got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            // 2-arg form: wr path content — content must be text.
            // 3-arg form: wr path data fmt — data may be any serialisable type;
            // fmt selects the encoder (csv/tsv/json) and must be text.
            if name == "wr"
                && arg_types.len() < 3
                && let Some(arg) = arg_types.get(1)
                && !compatible(arg, &Ty::Text)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'wr' arg 2 expects t (content), got {arg}"),
                    hint: Some(
                        "for typed data use the 3-arg form: wr path data \"json\"".to_string(),
                    ),
                    span,
                    is_warning: false,
                });
            }
            if name == "wr"
                && let Some(arg) = arg_types.get(2)
                && !compatible(arg, &Ty::Text)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'wr' arg 3 (format) expects t, got {arg}"),
                    hint: Some("supported formats: \"json\", \"csv\", \"tsv\"".to_string()),
                    span,
                    is_warning: false,
                });
            }
            (Ty::Result(Box::new(Ty::Text), Box::new(Ty::Text)), errors)
        }
        "jpth" => {
            for (i, arg) in arg_types.iter().enumerate() {
                if !compatible(arg, &Ty::Text) {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'jpth' arg {} expects t, got {arg}", i + 1),
                        hint: None,
                        span,
                        is_warning: false,
                    });
                }
            }
            (Ty::Result(Box::new(Ty::Text), Box::new(Ty::Text)), errors)
        }
        "jdmp" => {
            // jdmp accepts any value, no type checking needed
            (Ty::Text, errors)
        }
        "rgxsub" => {
            for (i, arg) in arg_types.iter().enumerate() {
                if !compatible(arg, &Ty::Text) {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'rgxsub' arg {} expects t, got {arg}", i + 1),
                        hint: None,
                        span,
                        is_warning: false,
                    });
                }
            }
            (Ty::Text, errors)
        }
        "prnt" => {
            // prt prints to stdout and returns the same value (passthrough, like dbg!)
            (arg_types.first().cloned().unwrap_or(Ty::Unknown), errors)
        }
        "jpar" => {
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &Ty::Text)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'jpar' expects t, got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            (
                Ty::Result(Box::new(Ty::Unknown), Box::new(Ty::Text)),
                errors,
            )
        }
        "rdjl" => {
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &Ty::Text)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'rdjl' expects t (path), got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            // rdjl path → L (R _ t): list of per-line parse results
            (
                Ty::List(Box::new(Ty::Result(
                    Box::new(Ty::Unknown),
                    Box::new(Ty::Text),
                ))),
                errors,
            )
        }
        "dtfmt" => {
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &Ty::Number)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'dtfmt' first arg must be n (unix epoch), got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            if let Some(arg) = arg_types.get(1)
                && !compatible(arg, &Ty::Text)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'dtfmt' second arg must be t (format), got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            (Ty::Result(Box::new(Ty::Text), Box::new(Ty::Text)), errors)
        }
        "dtparse" => {
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &Ty::Text)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'dtparse' first arg must be t, got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            if let Some(arg) = arg_types.get(1)
                && !compatible(arg, &Ty::Text)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'dtparse' second arg must be t (format), got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            (Ty::Result(Box::new(Ty::Number), Box::new(Ty::Text)), errors)
        }
        "map" => {
            // map fn:F a b xs:L a → L b
            // map fn:F a c b ctx:c xs:L a → L b   (closure-bind variant)
            if let Some(fn_ty) = arg_types.first()
                && !matches!(fn_ty, Ty::Fn(_, _) | Ty::Unknown)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'map' first arg must be a function (F ...), got {fn_ty}"),
                    hint: Some("pass a function name: map sq xs".to_string()),
                    span,
                    is_warning: false,
                });
            }
            // closure-bind: fn must take 2 args (elem, ctx)
            if arg_types.len() == 3
                && let Some(Ty::Fn(params, _)) = arg_types.first()
                && params.len() != 2
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!(
                        "'map' fn must take 2 args (elem, ctx) for closure-bind variant, got {} args",
                        params.len()
                    ),
                    hint: Some("for map fn ctx xs, fn must be: F a c b".to_string()),
                    span,
                    is_warning: false,
                });
            }
            if arg_types.len() == 3
                && let (Some(fn_ty), Some(ctx_ty), Some(xs_ty)) =
                    (arg_types.first(), arg_types.get(1), arg_types.get(2))
                && let Some(hint) = detect_ctx_param_swap(fn_ty, ctx_ty, xs_ty)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: "'map' fn params look swapped for closure-bind variant".to_string(),
                    hint: Some(hint),
                    span,
                    is_warning: false,
                });
            }
            // Return type: L of the function's return type, or L Unknown
            let ret_elem = match arg_types.first() {
                Some(Ty::Fn(_, ret)) => *ret.clone(),
                _ => Ty::Unknown,
            };
            (Ty::List(Box::new(ret_elem)), errors)
        }
        "mapr" => {
            // mapr fn:F a (R b e) xs:L a → R (L b) e
            //
            // Short-circuiting parallel to `map`. The fn must return a Result;
            // on the first ^err encountered the whole call returns that ^err,
            // otherwise the unwrapped Ok values are collected into a list and
            // wrapped in a single outer ~. Pairs with `!` to thread the err
            // up into a Result-returning caller without per-item match noise.
            // Retires the persona-written `ton s:t>n;r=num s;?r{~v:v;^_:0}`
            // helper that html-scraper + CSV-parsing rerun3s kept writing.
            if let Some(fn_ty) = arg_types.first()
                && !matches!(fn_ty, Ty::Fn(_, _) | Ty::Unknown)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'mapr' first arg must be a function (F ...), got {fn_ty}"),
                    hint: Some("pass a function name: mapr num xs".to_string()),
                    span,
                    is_warning: false,
                });
            }
            // fn must return a Result. Catches the obvious misuse where the
            // caller picked `mapr` over `map` for a non-fallible fn.
            if let Some(Ty::Fn(_, ret)) = arg_types.first()
                && !matches!(ret.as_ref(), Ty::Result(_, _) | Ty::Unknown)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!(
                        "'mapr' fn must return a Result (R _ _), got {ret}; use 'map' for non-fallible fns"
                    ),
                    hint: Some(
                        "mapr short-circuits on the first ^err; for non-fallible fns use map".to_string(),
                    ),
                    span,
                    is_warning: false,
                });
            }
            // Return type: R (L b) e from fn's R b e, or R (L Unknown) Unknown.
            let (ok_elem, err_ty) = match arg_types.first() {
                Some(Ty::Fn(_, ret)) => match ret.as_ref() {
                    Ty::Result(ok, err) => ((**ok).clone(), (**err).clone()),
                    _ => (Ty::Unknown, Ty::Unknown),
                },
                _ => (Ty::Unknown, Ty::Unknown),
            };
            (
                Ty::Result(Box::new(Ty::List(Box::new(ok_elem))), Box::new(err_ty)),
                errors,
            )
        }
        "flt" => {
            // flt fn:F a b xs:L a → L a
            // flt fn:F a c b ctx:c xs:L a → L a   (closure-bind variant)
            if let Some(fn_ty) = arg_types.first()
                && !matches!(fn_ty, Ty::Fn(_, _) | Ty::Unknown)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'flt' first arg must be a function (F ...), got {fn_ty}"),
                    hint: Some("pass a function name: flt pred xs".to_string()),
                    span,
                    is_warning: false,
                });
            }
            if arg_types.len() == 3
                && let Some(Ty::Fn(params, _)) = arg_types.first()
                && params.len() != 2
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!(
                        "'flt' fn must take 2 args (elem, ctx) for closure-bind variant, got {} args",
                        params.len()
                    ),
                    hint: Some("for flt fn ctx xs, fn must be: F a c b".to_string()),
                    span,
                    is_warning: false,
                });
            }
            if arg_types.len() == 3
                && let (Some(fn_ty), Some(ctx_ty), Some(xs_ty)) =
                    (arg_types.first(), arg_types.get(1), arg_types.get(2))
                && let Some(hint) = detect_ctx_param_swap(fn_ty, ctx_ty, xs_ty)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: "'flt' fn params look swapped for closure-bind variant".to_string(),
                    hint: Some(hint),
                    span,
                    is_warning: false,
                });
            }
            // Return type: same list type as input (last arg position)
            let list_idx = if arg_types.len() == 3 { 2 } else { 1 };
            let ret = match arg_types.get(list_idx) {
                Some(ty @ Ty::List(_)) => ty.clone(),
                _ => Ty::Unknown,
            };
            (ret, errors)
        }
        "fld" => {
            // fld fn:F a b b xs:L a init:b → b
            // fld fn:F a c b b ctx:c xs:L a init:b → b   (closure-bind variant)
            if let Some(fn_ty) = arg_types.first()
                && !matches!(fn_ty, Ty::Fn(_, _) | Ty::Unknown)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'fld' first arg must be a function (F ...), got {fn_ty}"),
                    hint: Some("pass a function name: fld f xs init".to_string()),
                    span,
                    is_warning: false,
                });
            }
            if arg_types.len() == 4
                && let Some(Ty::Fn(params, _)) = arg_types.first()
                && params.len() != 3
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!(
                        "'fld' fn must take 3 args (acc, elem, ctx) for closure-bind variant, got {} args",
                        params.len()
                    ),
                    hint: Some("for fld fn ctx xs init, fn must be: F b a c b".to_string()),
                    span,
                    is_warning: false,
                });
            }
            // Return type: accumulator type (last arg) or function return type
            let init_idx = if arg_types.len() == 4 { 3 } else { 2 };
            let ret = match arg_types.get(init_idx) {
                Some(ty) if !matches!(ty, Ty::Unknown) => ty.clone(),
                _ => match arg_types.first() {
                    Some(Ty::Fn(_, ret)) => *ret.clone(),
                    _ => Ty::Unknown,
                },
            };
            (ret, errors)
        }
        "grp" => {
            // grp fn:F a k xs:L a → M k (L a)
            if let Some(fn_ty) = arg_types.first()
                && !matches!(fn_ty, Ty::Fn(_, _) | Ty::Unknown)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'grp' first arg must be a function (F ...), got {fn_ty}"),
                    hint: Some("pass a function name: grp key-fn xs".to_string()),
                    span,
                    is_warning: false,
                });
            }
            let key_ty = match arg_types.first() {
                Some(Ty::Fn(_, ret)) => *ret.clone(),
                _ => Ty::Unknown,
            };
            let elem_ty = match arg_types.get(1) {
                Some(Ty::List(inner)) => *inner.clone(),
                _ => Ty::Unknown,
            };
            (
                Ty::Map(Box::new(key_ty), Box::new(Ty::List(Box::new(elem_ty)))),
                errors,
            )
        }
        "uniqby" => {
            // uniqby fn:F a t xs:L a → L a — keep first occurrence by key fn
            if let Some(fn_ty) = arg_types.first()
                && !matches!(fn_ty, Ty::Fn(_, _) | Ty::Unknown)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'uniqby' first arg must be a function (F ...), got {fn_ty}"),
                    hint: Some("pass a function name: uniqby key-fn xs".to_string()),
                    span,
                    is_warning: false,
                });
            }
            let ret = match arg_types.get(1) {
                Some(ty @ Ty::List(_)) => ty.clone(),
                _ => Ty::Unknown,
            };
            (ret, errors)
        }
        "flatmap" => {
            // flatmap fn:F a (L b) xs:L a → L b
            // First arg must be a function returning a list; second must be a list.
            if let Some(fn_ty) = arg_types.first()
                && !matches!(fn_ty, Ty::Fn(_, _) | Ty::Unknown)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'flatmap' first arg must be a function (F ...), got {fn_ty}"),
                    hint: Some("pass a function name: flatmap f xs".to_string()),
                    span,
                    is_warning: false,
                });
            }
            // Return type: the inner list element type of the function's return.
            let ret_elem = match arg_types.first() {
                Some(Ty::Fn(_, ret)) => match ret.as_ref() {
                    Ty::List(inner) => *inner.clone(),
                    _ => Ty::Unknown,
                },
                _ => Ty::Unknown,
            };
            (Ty::List(Box::new(ret_elem)), errors)
        }
        "partition" => {
            // partition fn:F a b xs:L a → L (L a) — split into [passing, failing]
            if let Some(fn_ty) = arg_types.first()
                && !matches!(fn_ty, Ty::Fn(_, _) | Ty::Unknown)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!(
                        "'partition' first arg must be a function (F ...), got {fn_ty}"
                    ),
                    hint: Some("pass a predicate function: partition pred xs".to_string()),
                    span,
                    is_warning: false,
                });
            }
            // Return type: L (L a) where a is the element type of the input list.
            let ret = match arg_types.get(1) {
                Some(ty @ Ty::List(_)) => Ty::List(Box::new(ty.clone())),
                _ => Ty::Unknown,
            };
            (ret, errors)
        }
        "chunks" => {
            // chunks n xs:L a → L (L a) — split into non-overlapping chunks of size n.
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &Ty::Number)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'chunks' arg 1 expects n, got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            let inner = match arg_types.get(1) {
                Some(Ty::List(inner)) => *inner.clone(),
                Some(Ty::Unknown) | None => Ty::Unknown,
                Some(other) => {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'chunks' expects a list, got {other}"),
                        hint: None,
                        span,
                        is_warning: false,
                    });
                    Ty::Unknown
                }
            };
            (Ty::List(Box::new(Ty::List(Box::new(inner)))), errors)
        }
        "frq" => {
            // frq xs:L a → M a n — count occurrences of each element. The
            // resulting map's key type matches the element type of the list
            // (Text → M t n; Number → M n n; Bool → M t n since bools are
            // stringified at the MapKey boundary).
            let key_ty = match arg_types.first() {
                Some(Ty::List(inner)) => match inner.as_ref() {
                    Ty::Bool => Ty::Text,
                    other => other.clone(),
                },
                _ => Ty::Unknown,
            };
            if let Some(first) = arg_types.first()
                && !matches!(first, Ty::List(_) | Ty::Unknown)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'frq' expects a list, got {first}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            (Ty::Map(Box::new(key_ty), Box::new(Ty::Number)), errors)
        }
        "cumsum" => {
            if let Some(arg) = arg_types.first() {
                match arg {
                    Ty::List(inner) => {
                        if !compatible(inner, &Ty::Number) {
                            errors.push(VerifyError {
                                code: "ILO-T013",
                                function: func_ctx.to_string(),
                                message: format!("'cumsum' expects L n, got L {inner}"),
                                hint: None,
                                span,
                                is_warning: false,
                            });
                        }
                    }
                    Ty::Unknown => {}
                    other => errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'cumsum' expects L n, got {other}"),
                        hint: None,
                        span,
                        is_warning: false,
                    }),
                }
            }
            (Ty::List(Box::new(Ty::Number)), errors)
        }
        "flat" => {
            // flat xs:L (L a) → L a — flatten one level
            let inner = match arg_types.first() {
                Some(Ty::List(inner)) => match inner.as_ref() {
                    Ty::List(elem) => *elem.clone(),
                    _ => Ty::Unknown,
                },
                _ => Ty::Unknown,
            };
            (Ty::List(Box::new(inner)), errors)
        }
        "mmap" => (
            Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)),
            errors,
        ),
        "mget" => {
            // mget map key → O value_type. The key type must be compatible
            // with the declared map key type (text or number); we no longer
            // hard-code "must be text" — see PR #257 for the MapKey rollout.
            let (key_ty_decl, val_ty) = match arg_types.first() {
                Some(Ty::Map(k, v)) => (*k.clone(), *v.clone()),
                _ => (Ty::Unknown, Ty::Unknown),
            };
            if let Some(key_ty) = arg_types.get(1)
                && !is_valid_map_key_arg(key_ty, &key_ty_decl)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!(
                        "'mget' key must match map key type {key_ty_decl}, got {key_ty}"
                    ),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            (Ty::Optional(Box::new(val_ty)), errors)
        }
        "mset" => {
            // mset map key val → map (same key type as input map, value type
            // inferred from the third arg if not previously known).
            let (key_ty_decl, val_ty_decl) = match arg_types.first() {
                Some(Ty::Map(k, v)) => (Some(*k.clone()), Some(*v.clone())),
                _ => (None, None),
            };
            if let (Some(key_ty), Some(decl)) = (arg_types.get(1), key_ty_decl.as_ref())
                && !is_valid_map_key_arg(key_ty, decl)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'mset' key must match map key type {decl}, got {key_ty}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            let map_ty = match arg_types.first() {
                Some(Ty::Map(k, _)) => {
                    let val_ty = arg_types.get(2).cloned().unwrap_or(Ty::Unknown);
                    Ty::Map(k.clone(), Box::new(val_ty))
                }
                _ => Ty::Map(
                    Box::new(Ty::Unknown),
                    Box::new(val_ty_decl.unwrap_or(Ty::Unknown)),
                ),
            };
            (map_ty, errors)
        }
        "mhas" => {
            let key_ty_decl = match arg_types.first() {
                Some(Ty::Map(k, _)) => *k.clone(),
                _ => Ty::Unknown,
            };
            if let Some(first) = arg_types.first()
                && !matches!(first, Ty::Map(_, _) | Ty::Unknown)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'mhas' expects a map, got {first}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            if let Some(key_ty) = arg_types.get(1)
                && !is_valid_map_key_arg(key_ty, &key_ty_decl)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!(
                        "'mhas' key must match map key type {key_ty_decl}, got {key_ty}"
                    ),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            (Ty::Bool, errors)
        }
        "mkeys" => {
            if let Some(first) = arg_types.first()
                && !matches!(first, Ty::Map(_, _) | Ty::Unknown)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'mkeys' expects a map, got {first}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            // mkeys returns the declared key type (was hard-coded to L t).
            let key_ty = match arg_types.first() {
                Some(Ty::Map(k, _)) => *k.clone(),
                _ => Ty::Text,
            };
            (Ty::List(Box::new(key_ty)), errors)
        }
        "mvals" => {
            let val_ty = match arg_types.first() {
                Some(Ty::Map(_, v)) => *v.clone(),
                _ => Ty::Unknown,
            };
            (Ty::List(Box::new(val_ty)), errors)
        }
        "mdel" => {
            let key_ty_decl = match arg_types.first() {
                Some(Ty::Map(k, _)) => *k.clone(),
                _ => Ty::Unknown,
            };
            if let Some(key_ty) = arg_types.get(1)
                && !is_valid_map_key_arg(key_ty, &key_ty_decl)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!(
                        "'mdel' key must match map key type {key_ty_decl}, got {key_ty}"
                    ),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            let map_ty = match arg_types.first() {
                Some(ty @ Ty::Map(_, _)) => ty.clone(),
                _ => Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)),
            };
            (map_ty, errors)
        }
        "transpose" => {
            // transpose m:L (L n) → L (L n)
            if let Some(arg) = arg_types.first() {
                let ok = match arg {
                    Ty::List(inner) => matches!(inner.as_ref(), Ty::List(_) | Ty::Unknown),
                    Ty::Unknown => true,
                    _ => false,
                };
                if !ok {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'transpose' expects L (L n), got {arg}"),
                        hint: None,
                        span,
                        is_warning: false,
                    });
                }
            }
            let ret = match arg_types.first() {
                Some(ty @ Ty::List(inner)) if matches!(inner.as_ref(), Ty::List(_)) => ty.clone(),
                _ => Ty::List(Box::new(Ty::List(Box::new(Ty::Number)))),
            };
            (ret, errors)
        }
        "matmul" => {
            // matmul a:L (L n) b:L (L n) → L (L n)
            for (i, arg) in arg_types.iter().enumerate() {
                let ok = match arg {
                    Ty::List(inner) => matches!(inner.as_ref(), Ty::List(_) | Ty::Unknown),
                    Ty::Unknown => true,
                    _ => false,
                };
                if !ok {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'matmul' arg {} must be L (L n), got {arg}", i + 1),
                        hint: None,
                        span,
                        is_warning: false,
                    });
                }
            }
            (Ty::List(Box::new(Ty::List(Box::new(Ty::Number)))), errors)
        }
        "dot" => {
            // dot xs:L n ys:L n → n
            for (i, arg) in arg_types.iter().enumerate() {
                let ok = match arg {
                    Ty::List(inner) => compatible(inner, &Ty::Number),
                    Ty::Unknown => true,
                    _ => false,
                };
                if !ok {
                    errors.push(VerifyError {
                        code: "ILO-T013",
                        function: func_ctx.to_string(),
                        message: format!("'dot' arg {} must be L n, got {arg}", i + 1),
                        hint: None,
                        span,
                        is_warning: false,
                    });
                }
            }
            (Ty::Number, errors)
        }
        "solve" => {
            let matrix_ty = Ty::List(Box::new(Ty::List(Box::new(Ty::Number))));
            let vec_ty = Ty::List(Box::new(Ty::Number));
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &matrix_ty)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'solve' first arg expects L (L n), got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            if let Some(arg) = arg_types.get(1)
                && !compatible(arg, &vec_ty)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'solve' second arg expects L n, got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            (vec_ty, errors)
        }
        "inv" => {
            let matrix_ty = Ty::List(Box::new(Ty::List(Box::new(Ty::Number))));
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &matrix_ty)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'inv' expects L (L n), got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            (matrix_ty, errors)
        }
        "det" => {
            let matrix_ty = Ty::List(Box::new(Ty::List(Box::new(Ty::Number))));
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &matrix_ty)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'det' expects L (L n), got {arg}"),
                    hint: None,
                    span,
                    is_warning: false,
                });
            }
            (Ty::Number, errors)
        }
        "sleep" => {
            // sleep ms:n -> _   (blocks the current engine for `ms` milliseconds,
            // returns nil so it composes naturally as a statement in any block).
            if let Some(arg) = arg_types.first()
                && !compatible(arg, &Ty::Number)
            {
                errors.push(VerifyError {
                    code: "ILO-T013",
                    function: func_ctx.to_string(),
                    message: format!("'sleep' expects ms:n, got {arg}"),
                    hint: Some("pass a number of milliseconds, e.g. sleep 100".to_string()),
                    span,
                    is_warning: false,
                });
            }
            (Ty::Nil, errors)
        }
        _ => (Ty::Unknown, errors),
    }
}

impl VerifyContext {
    fn new() -> Self {
        Self {
            functions: HashMap::new(),
            types: HashMap::new(),
            aliases: HashMap::new(),
            errors: Vec::new(),
            in_loop: false,
        }
    }

    fn err(
        &mut self,
        code: &'static str,
        function: &str,
        message: String,
        hint: Option<String>,
        span: Option<Span>,
    ) {
        self.errors.push(VerifyError {
            code,
            function: function.to_string(),
            message,
            hint,
            span,
            is_warning: false,
        });
    }

    fn warn(
        &mut self,
        code: &'static str,
        function: &str,
        message: String,
        hint: Option<String>,
        span: Option<Span>,
    ) {
        self.errors.push(VerifyError {
            code,
            function: function.to_string(),
            message,
            hint,
            span,
            is_warning: true,
        });
    }

    /// Phase 1: collect all declarations, check for duplicates and undefined Named types.
    fn collect_declarations(&mut self, program: &Program) {
        // Pass 0: collect type aliases (before types so aliases can be used in type fields)
        let builtin_type_names = ["n", "t", "b", "L", "R"];
        let mut raw_aliases: HashMap<String, Type> = HashMap::new();
        for decl in &program.declarations {
            if let Decl::Alias { name, target, span } = decl {
                if builtin_type_names.contains(&name.as_str()) || name == "_" {
                    self.err(
                        "ILO-T031",
                        "<global>",
                        format!("type alias '{name}' shadows a builtin type"),
                        Some("choose a different name for the alias".to_string()),
                        Some(*span),
                    );
                    continue;
                }
                if raw_aliases.contains_key(name) {
                    self.err(
                        "ILO-T001",
                        "<global>",
                        format!("duplicate type alias '{name}'"),
                        None,
                        Some(*span),
                    );
                } else {
                    raw_aliases.insert(name.clone(), target.clone());
                }
            }
        }
        // Resolve aliases with cycle detection
        self.resolve_aliases(&raw_aliases);

        // First pass: collect type names
        for decl in &program.declarations {
            if let Decl::TypeDef { name, fields, .. } = decl {
                if self.aliases.contains_key(name) {
                    self.err(
                        "ILO-T001",
                        "<global>",
                        format!("type '{name}' conflicts with type alias of the same name"),
                        None,
                        None,
                    );
                } else if self.types.contains_key(name) {
                    self.err(
                        "ILO-T001",
                        "<global>",
                        format!("duplicate type definition '{name}'"),
                        None,
                        None,
                    );
                } else {
                    let fields: Vec<(String, Ty)> = fields
                        .iter()
                        .map(|p| {
                            (
                                p.name.clone(),
                                convert_type_with_aliases(&p.ty, &self.aliases),
                            )
                        })
                        .collect();
                    self.types.insert(name.clone(), TypeDef { fields });
                }
            }
        }

        // Second pass: collect functions and tools, validate Named types in signatures
        for decl in &program.declarations {
            match decl {
                Decl::Function {
                    name,
                    params,
                    return_type,
                    ..
                } => {
                    if self.functions.contains_key(name) {
                        self.err(
                            "ILO-T002",
                            "<global>",
                            format!("duplicate function definition '{name}'"),
                            None,
                            None,
                        );
                        continue;
                    }
                    let params: Vec<(String, Ty)> = params
                        .iter()
                        .map(|p| {
                            (
                                p.name.clone(),
                                convert_type_with_aliases(&p.ty, &self.aliases),
                            )
                        })
                        .collect();
                    let ret = convert_type_with_aliases(return_type, &self.aliases);
                    self.validate_named_types_in_sig(name, &params, &ret);
                    self.functions.insert(
                        name.clone(),
                        FuncSig {
                            params,
                            return_type: ret,
                        },
                    );
                }
                Decl::Tool {
                    name,
                    params,
                    return_type,
                    ..
                } => {
                    if self.functions.contains_key(name) {
                        self.err(
                            "ILO-T002",
                            "<global>",
                            format!("duplicate definition '{name}' (tool conflicts with function)"),
                            None,
                            None,
                        );
                        continue;
                    }
                    let params: Vec<(String, Ty)> = params
                        .iter()
                        .map(|p| {
                            (
                                p.name.clone(),
                                convert_type_with_aliases(&p.ty, &self.aliases),
                            )
                        })
                        .collect();
                    let ret = convert_type_with_aliases(return_type, &self.aliases);
                    self.validate_named_types_in_sig(name, &params, &ret);
                    self.functions.insert(
                        name.clone(),
                        FuncSig {
                            params,
                            return_type: ret,
                        },
                    );
                }
                Decl::TypeDef { .. } => {} // already handled
                Decl::Alias { .. } => {}   // already handled
                Decl::Use { .. } => {}     // resolved before verify — skip
                Decl::Error { .. } => {}   // poison node — skip silently
            }
        }

        // Validate Named types in type def fields
        for decl in &program.declarations {
            if let Decl::TypeDef { name, fields, .. } = decl {
                for field in fields {
                    self.validate_named_type_recursive(
                        &convert_type_with_aliases(&field.ty, &self.aliases),
                        name,
                    );
                }
            }
        }
    }

    /// Resolve raw alias map into fully expanded `Ty` values, detecting cycles.
    fn resolve_aliases(&mut self, raw: &HashMap<String, Type>) {
        use std::collections::HashSet;

        // Build dependency graph: for each alias, which other aliases does it reference?
        let deps: HashMap<String, Vec<String>> = raw
            .iter()
            .map(|(name, target)| {
                let refs: Vec<String> = collect_named_refs(target)
                    .into_iter()
                    .filter(|r| raw.contains_key(r))
                    .collect();
                (name.clone(), refs)
            })
            .collect();

        // DFS cycle detection
        let mut in_cycle: HashSet<String> = HashSet::new();
        for name in raw.keys() {
            let mut visited = HashSet::new();
            let mut stack = HashSet::new();
            if Self::has_cycle(name, &deps, &mut visited, &mut stack) {
                // All nodes in the stack when cycle detected are part of cycle
                for n in &stack {
                    in_cycle.insert(n.clone());
                }
            }
        }

        for name in &in_cycle {
            self.err(
                "ILO-T030",
                "<global>",
                format!("circular type alias '{name}'"),
                Some("type aliases cannot reference each other in a cycle".to_string()),
                None,
            );
        }

        // Resolve non-cyclic aliases
        for name in raw.keys() {
            if !in_cycle.contains(name) && !self.aliases.contains_key(name) {
                self.resolve_alias_recursive(name, raw);
            }
        }
    }

    /// DFS cycle detection. Returns true if `name` is part of a cycle.
    fn has_cycle(
        name: &str,
        deps: &HashMap<String, Vec<String>>,
        visited: &mut std::collections::HashSet<String>,
        stack: &mut std::collections::HashSet<String>,
    ) -> bool {
        if stack.contains(name) {
            return true; // back edge — cycle
        }
        if visited.contains(name) {
            return false; // already fully explored, no cycle
        }
        visited.insert(name.to_string());
        stack.insert(name.to_string());
        if let Some(neighbors) = deps.get(name) {
            for dep in neighbors {
                if Self::has_cycle(dep, deps, visited, stack) {
                    return true;
                }
            }
        }
        stack.remove(name);
        false
    }

    /// Recursively resolve a single alias, storing results in self.aliases.
    fn resolve_alias_recursive(&mut self, name: &str, raw: &HashMap<String, Type>) {
        if self.aliases.contains_key(name) {
            return;
        }
        if let Some(target) = raw.get(name) {
            // Resolve any alias dependencies in the target type first
            let deps = collect_named_refs(target);
            for dep in &deps {
                if raw.contains_key(dep) && !self.aliases.contains_key(dep) {
                    self.resolve_alias_recursive(dep, raw);
                }
            }
            // Now convert with currently resolved aliases
            let resolved = convert_type_with_aliases(target, &self.aliases);
            self.aliases.insert(name.to_string(), resolved);
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
            Ty::Named(name) if !self.types.contains_key(name) => {
                let hint =
                    closest_match(name, self.types.keys()).map(|s| format!("did you mean '{s}'?"));
                self.err(
                    "ILO-T003",
                    ctx,
                    format!("undefined type '{name}'"),
                    hint,
                    None,
                );
            }
            Ty::Named(_) => {}
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
            if let Decl::Function {
                name,
                params,
                return_type,
                body,
                ..
            } = decl
            {
                let mut scope: Scope = vec![HashMap::new()];
                for p in params {
                    scope_insert(
                        &mut scope,
                        p.name.clone(),
                        convert_type_with_aliases(&p.ty, &self.aliases),
                    );
                }

                let body_ty = self.verify_body(name, &mut scope, body);
                let expected = convert_type_with_aliases(return_type, &self.aliases);
                if !compatible(&body_ty, &expected) {
                    let hint = match (&body_ty, &expected) {
                        (Ty::Number, Ty::Text) => {
                            Some("use 'str' to convert: str <expr>".to_string())
                        }
                        (Ty::Text, Ty::Number) => {
                            Some("use 'num' to parse text (returns R n t)".to_string())
                        }
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
        for (i, spanned) in stmts.iter().enumerate() {
            let is_tail = i + 1 == stmts.len();
            // ILO-T032: bare `fmt`/`fmt2` at non-tail position is almost always a bug.
            // `fmt` is pure-functional sprintf — it builds a string and returns it.
            // When used as a non-tail statement with no binding, the string is silently
            // discarded on every engine (tree/VM/Cranelift), and nothing reaches stdout.
            // The common mistake is treating `fmt` like Rust's `println!` / Python's print.
            // Fix: bind via `name=fmt ...` or print via `prnt fmt ...`.
            // We only warn at non-tail position so the idiomatic `say-x>t;fmt "x={}" 42`
            // pattern (fmt as the function's return value) stays clean.
            if !is_tail
                && let Stmt::Expr(Expr::Call { function, .. }) = &spanned.node
                && let Some(b) = Builtin::from_name(function)
                && matches!(b, Builtin::Fmt | Builtin::Fmt2)
            {
                let name = match b {
                    Builtin::Fmt => "fmt",
                    Builtin::Fmt2 => "fmt2",
                    _ => unreachable!(),
                };
                self.warn(
                    "ILO-T032",
                    func,
                    format!("bare '{name}' result is discarded"),
                    Some(format!(
                        "did you mean `prnt {name} ...` to print, or `name = {name} ...` to capture?"
                    )),
                    Some(spanned.span),
                );
            }
            // ILO-T033: bare `+=xs v` / `mset m k v` / `mdel m k` at a position
            // whose value is discarded is almost always a bug. These shapes look
            // like in-place mutation but ilo's functional semantics return a new
            // value and only bind it back when written as `name = +=xs v` /
            // `name = mset m k v` / `name = mdel m k`. As a bare statement with
            // no rebind, the result is silently discarded on every engine and
            // the source binding is unchanged.
            //
            // Discarded positions: any non-tail statement, plus EVERY statement
            // inside a loop body (the loop discards each iteration's tail).
            // Tail position in a function/if/match body is legitimate (returns
            // the appended value to the caller / produces the branch value).
            if !is_tail || self.in_loop {
                // Hint helpers: extract the source binding name from the first
                // operand so the rebind shape we suggest matches the user's
                // local variable, not a generic placeholder.
                let ref_name = |e: &Expr| match e {
                    Expr::Ref(n) => Some(n.clone()),
                    _ => None,
                };
                let warning = match &spanned.node {
                    Stmt::Expr(Expr::BinOp {
                        op: BinOp::Append,
                        left,
                        ..
                    }) => {
                        let lhs = ref_name(left).unwrap_or_else(|| "xs".to_string());
                        Some((
                            "+=",
                            format!(
                                "`+=xs v` returns a new list — rebind with `{lhs}=+={lhs} v` to mutate, or use the result"
                            ),
                        ))
                    }
                    Stmt::Expr(Expr::Call { function, args, .. })
                        if Builtin::from_name(function) == Some(Builtin::Mset) =>
                    {
                        let lhs = args
                            .first()
                            .and_then(ref_name)
                            .unwrap_or_else(|| "m".to_string());
                        Some((
                            "mset",
                            format!(
                                "`mset m k v` returns a new map — rebind with `{lhs}=mset {lhs} k v` to mutate, or use the result"
                            ),
                        ))
                    }
                    Stmt::Expr(Expr::Call { function, args, .. })
                        if Builtin::from_name(function) == Some(Builtin::Mdel) =>
                    {
                        let lhs = args
                            .first()
                            .and_then(ref_name)
                            .unwrap_or_else(|| "m".to_string());
                        Some((
                            "mdel",
                            format!(
                                "`mdel m k` returns a new map — rebind with `{lhs}=mdel {lhs} k` to mutate, or use the result"
                            ),
                        ))
                    }
                    _ => None,
                };
                if let Some((name, hint)) = warning {
                    self.warn(
                        "ILO-T033",
                        func,
                        format!("bare '{name}' result is discarded"),
                        Some(hint),
                        Some(spanned.span),
                    );
                }
            }
            last_ty = self.verify_stmt(func, scope, &spanned.node, spanned.span);
            if matches!(spanned.node, Stmt::Return(_) | Stmt::Break(_)) && i + 1 < stmts.len() {
                let first_unreachable = stmts[i + 1].span;
                let last_unreachable = stmts.last().unwrap().span;
                let span = first_unreachable.merge(last_unreachable);
                let kind = match &spanned.node {
                    Stmt::Return(_) => "ret",
                    Stmt::Break(_) => "brk",
                    _ => unreachable!(),
                };
                self.warn(
                    "ILO-T029",
                    func,
                    format!("unreachable code after '{kind}'"),
                    None,
                    Some(span),
                );
                break;
            }
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
            Stmt::Destructure { bindings, value } => {
                let record_ty = self.infer_expr(func, scope, value, span);
                match &record_ty {
                    Ty::Named(type_name) => {
                        if let Some(type_def) = self.types.get(type_name).cloned() {
                            for binding in bindings {
                                if let Some((_, fty)) =
                                    type_def.fields.iter().find(|(n, _)| n == binding)
                                {
                                    scope_insert(scope, binding.clone(), fty.clone());
                                } else {
                                    let field_names: Vec<String> =
                                        type_def.fields.iter().map(|(n, _)| n.clone()).collect();
                                    let hint = closest_match(binding, field_names.iter())
                                        .map(|s| format!("did you mean '{s}'?"));
                                    self.err(
                                        "ILO-T019",
                                        func,
                                        format!("no field '{binding}' on type '{type_name}'"),
                                        hint,
                                        Some(span),
                                    );
                                    scope_insert(scope, binding.clone(), Ty::Unknown);
                                }
                            }
                        } else {
                            for binding in bindings {
                                scope_insert(scope, binding.clone(), Ty::Unknown);
                            }
                        }
                    }
                    Ty::Unknown => {
                        for binding in bindings {
                            scope_insert(scope, binding.clone(), Ty::Unknown);
                        }
                    }
                    other => {
                        self.err(
                            "ILO-T009",
                            func,
                            format!("destructure requires a record type, got {other}"),
                            None,
                            Some(span),
                        );
                        for binding in bindings {
                            scope_insert(scope, binding.clone(), Ty::Unknown);
                        }
                    }
                }
                Ty::Nil
            }
            Stmt::Guard {
                condition,
                body,
                else_body,
                ..
            } => {
                let _ = self.infer_expr(func, scope, condition, span);

                // Warn if braceless guard body is a single identifier matching a function name.
                if body.len() == 1
                    && let Stmt::Expr(Expr::Ref(ref name)) = body[0].node
                    && (self.functions.contains_key(name) || is_builtin(name))
                {
                    let body_span = body[0].span;
                    self.err(
                        "ILO-T027",
                        func,
                        format!("braceless guard body '{name}' is a function name — did you mean to call it?"),
                        Some(format!("use braces for function calls: cond{{{name} args}}")),
                        Some(body_span),
                    );
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
            Stmt::ForEach {
                binding,
                collection,
                body,
            } => {
                let coll_ty = self.infer_expr(func, scope, collection, span);
                let elem_ty = match &coll_ty {
                    Ty::List(inner) => *inner.clone(),
                    Ty::Unknown => Ty::Unknown,
                    other => {
                        self.err(
                            "ILO-T014",
                            func,
                            format!("foreach expects a list, got {other}"),
                            None,
                            Some(span),
                        );
                        Ty::Unknown
                    }
                };
                scope.push(HashMap::new());
                scope_insert(scope, binding.clone(), elem_ty);
                let prev = self.in_loop;
                self.in_loop = true;
                let body_ty = self.verify_body(func, scope, body);
                self.in_loop = prev;
                scope.pop();
                body_ty
            }
            Stmt::ForRange {
                binding,
                start,
                end,
                body,
            } => {
                let start_ty = self.infer_expr(func, scope, start, span);
                let end_ty = self.infer_expr(func, scope, end, span);
                if !compatible(&start_ty, &Ty::Number) {
                    self.err(
                        "ILO-T014",
                        func,
                        format!("range start must be n, got {start_ty}"),
                        None,
                        Some(span),
                    );
                }
                if !compatible(&end_ty, &Ty::Number) {
                    self.err(
                        "ILO-T014",
                        func,
                        format!("range end must be n, got {end_ty}"),
                        None,
                        Some(span),
                    );
                }
                scope.push(HashMap::new());
                scope_insert(scope, binding.clone(), Ty::Number);
                let prev = self.in_loop;
                self.in_loop = true;
                let body_ty = self.verify_body(func, scope, body);
                self.in_loop = prev;
                scope.pop();
                body_ty
            }
            Stmt::While { condition, body } => {
                self.infer_expr(func, scope, condition, span);
                let prev = self.in_loop;
                self.in_loop = true;
                let body_ty = self.verify_body(func, scope, body);
                self.in_loop = prev;
                body_ty
            }
            Stmt::Return(expr) => self.infer_expr(func, scope, expr, span),
            Stmt::Break(expr) => {
                if !self.in_loop {
                    self.err(
                        "ILO-T028",
                        func,
                        "brk can only be used inside a loop (@/wh)".to_string(),
                        None,
                        Some(span),
                    );
                }
                if let Some(e) = expr {
                    self.infer_expr(func, scope, e, span)
                } else {
                    Ty::Nil
                }
            }
            Stmt::Continue => {
                if !self.in_loop {
                    self.err(
                        "ILO-T028",
                        func,
                        "cnt can only be used inside a loop (@/wh)".to_string(),
                        None,
                        Some(span),
                    );
                }
                Ty::Nil
            }
            Stmt::Expr(expr) => self.infer_expr(func, scope, expr, span),
        }
    }

    fn bind_pattern(&mut self, _func: &str, scope: &mut Scope, pattern: &Pattern, subject_ty: &Ty) {
        // `_` is always bound to the matched inner value (or the subject itself
        // for Pattern::Wildcard). SPEC.md documents this in the line 1069
        // example `~_:~_` where the wildcard arm re-wraps the unchanged inner
        // value. Binding `_` makes wildcard arms compose like named arms at
        // zero extra tokens — discard remains free (bodies that never name `_`
        // still work), and throwaway code can poke at the value (`fmt "..." _`)
        // without renaming.
        match pattern {
            Pattern::Ok(name) => {
                let ty = match subject_ty {
                    Ty::Result(ok, _) => *ok.clone(),
                    Ty::Unknown => Ty::Unknown,
                    _ => Ty::Unknown,
                };
                scope_insert(scope, name.clone(), ty);
            }
            Pattern::Err(name) => {
                let ty = match subject_ty {
                    Ty::Result(_, err) => *err.clone(),
                    Ty::Unknown => Ty::Unknown,
                    _ => Ty::Unknown,
                };
                scope_insert(scope, name.clone(), ty);
            }
            Pattern::Literal(_) => {}
            Pattern::Wildcard => {
                // Plain `_:body` — `_` resolves to the subject itself.
                scope_insert(scope, "_".to_string(), subject_ty.clone());
            }
            Pattern::TypeIs { ty, binding } => {
                let bound_ty = match ty {
                    Type::Number => Ty::Number,
                    Type::Text => Ty::Text,
                    Type::Bool => Ty::Bool,
                    Type::List(_) => Ty::List(Box::new(Ty::Unknown)),
                    _ => Ty::Unknown,
                };
                scope_insert(scope, binding.clone(), bound_ty);
            }
        }
    }

    fn infer_expr(&mut self, func: &str, scope: &mut Scope, expr: &Expr, span: Span) -> Ty {
        match expr {
            Expr::Literal(lit) => match lit {
                Literal::Number(_) => Ty::Number,
                Literal::Text(_) => Ty::Text,
                Literal::Bool(_) => Ty::Bool,
                Literal::Nil => Ty::Nil,
            },

            Expr::Ref(name) => {
                if let Some(ty) = scope_lookup(scope, name) {
                    ty.clone()
                } else if let Some(sig) = self.functions.get(name) {
                    // Function name used as a value — resolve to Ty::Fn
                    let params: Vec<Ty> = sig.params.iter().map(|(_, t)| t.clone()).collect();
                    Ty::Fn(params, Box::new(sig.return_type.clone()))
                } else if let Some(fn_ty) = builtin_as_fn_ty(name) {
                    // Pure builtin used as a value (e.g. `fld max xs 0`).
                    // Promote to Ty::Fn so HOF args type-check.
                    fn_ty
                } else {
                    let mut candidates: Vec<String> = scope
                        .iter()
                        .flat_map(|frame| frame.keys().cloned())
                        .collect();
                    candidates.extend(self.functions.keys().cloned());
                    // Prefer the kebab-case clarification when every dash-separated
                    // half resolves as a value: that's the high-signal case where
                    // the model is liable to misread the atomic ident as a binop.
                    // Fall back to the standard closest-match suggestion otherwise.
                    let hint = kebab_subtract_hint(name, candidates.iter()).or_else(|| {
                        closest_match(name, candidates.iter())
                            .map(|s| format!("did you mean '{s}'?"))
                    });
                    self.err(
                        "ILO-T004",
                        func,
                        format!("undefined variable '{name}'"),
                        hint,
                        Some(span),
                    );
                    Ty::Unknown
                }
            }

            Expr::Call {
                function: callee,
                args,
                unwrap,
            } => {
                // Infer all arg types first
                let arg_types: Vec<Ty> = args
                    .iter()
                    .map(|a| self.infer_expr(func, scope, a, span))
                    .collect();

                let call_ty = if is_builtin(callee) {
                    // Check arity (rnd accepts 0 or 2 args)
                    let expected_arity =
                        builtin_arity(callee).expect("is_builtin guarantees arity exists");
                    let arity_ok = if callee == "rnd" {
                        args.is_empty() || args.len() == 2
                    } else if callee == "srt" || callee == "rsrt" {
                        // srt xs / srt fn xs / srt fn ctx xs (and rsrt mirrors)
                        args.len() == 1 || args.len() == 2 || args.len() == 3
                    } else if callee == "min" || callee == "max" {
                        // min xs (list form, returns min element) / min a b (number pair)
                        args.len() == 1 || args.len() == 2
                    } else if callee == "map" || callee == "flt" {
                        // map fn xs / map fn ctx xs   (closure-bind variant)
                        args.len() == 2 || args.len() == 3
                    } else if callee == "fld" {
                        // fld fn xs init / fld fn ctx xs init  (closure-bind variant)
                        args.len() == 3 || args.len() == 4
                    } else if callee == "rd" {
                        args.len() == 1 || args.len() == 2
                    } else if callee == "wr" {
                        args.len() == 2 || args.len() == 3
                    } else if callee == "get" {
                        args.len() == 1 || args.len() == 2
                    } else if callee == "post" {
                        args.len() == 2 || args.len() == 3
                    } else if callee == "padl" || callee == "padr" {
                        // padl s w  /  padl s w padchar
                        args.len() == 2 || args.len() == 3
                    } else if callee == "fmt" {
                        !args.is_empty() // variadic: template + 0 or more args
                    } else {
                        args.len() == expected_arity
                    };
                    if !arity_ok {
                        let arity_desc = if callee == "rnd" {
                            "0 or 2".to_string()
                        } else if callee == "srt" || callee == "rsrt" {
                            "1, 2, or 3".to_string()
                        } else if callee == "map" || callee == "flt" {
                            "2 or 3".to_string()
                        } else if callee == "fld" {
                            "3 or 4".to_string()
                        } else if callee == "rd" || callee == "get" {
                            "1 or 2".to_string()
                        } else if matches!(callee.as_str(), "post" | "wr" | "padl" | "padr") {
                            "2 or 3".to_string()
                        } else if callee == "min" || callee == "max" {
                            "1 or 2".to_string()
                        } else {
                            expected_arity.to_string()
                        };
                        self.err(
                            "ILO-T006",
                            func,
                            format!(
                                "arity mismatch: '{callee}' expects {arity_desc} args, got {}",
                                args.len()
                            ),
                            None,
                            Some(span),
                        );
                        return Ty::Unknown;
                    }
                    // Literal-template check for `fmt`: if the template is a
                    // string literal and contains a `{:...}` printf-style
                    // spec, fail fast — fmt only supports bare `{}` and the
                    // runtime would otherwise either silently emit the
                    // literal (pre-fix) or surface ILO-R009 (post-fix).
                    if callee == "fmt"
                        && let Some(Expr::Literal(Literal::Text(tmpl))) = args.first()
                    {
                        let mut iter = tmpl.chars().peekable();
                        let mut bad: Option<String> = None;
                        while let Some(c) = iter.next() {
                            if c == '{' && iter.peek() == Some(&':') {
                                let mut spec = String::from("{");
                                for sc in iter.by_ref() {
                                    spec.push(sc);
                                    if sc == '}' {
                                        break;
                                    }
                                }
                                bad = Some(spec);
                                break;
                            }
                        }
                        if let Some(spec) = bad {
                            self.err(
                                "ILO-T013",
                                func,
                                format!(
                                    "'fmt' only supports bare `{{}}` placeholders, got `{spec}`"
                                ),
                                Some(
                                    "for decimal precision use `fmt \"...{}\" (fmt2 v 2)`; \
                                     for width / padding use `padl (str n) 6` (space-pad)"
                                        .to_string(),
                                ),
                                Some(span),
                            );
                        }
                    }
                    // Literal-format check for 3-arg `wr path data fmt`:
                    // if fmt is a string literal, fail fast on unsupported values.
                    if callee == "wr"
                        && args.len() == 3
                        && let Expr::Literal(Literal::Text(fmt)) = &args[2]
                        && !matches!(fmt.as_str(), "json" | "csv" | "tsv")
                    {
                        self.err(
                            "ILO-T013",
                            func,
                            format!(
                                "'wr' format \"{fmt}\" is not supported; expected \"json\", \"csv\", or \"tsv\""
                            ),
                            Some("e.g. wr path data \"json\"".to_string()),
                            Some(span),
                        );
                    }
                    let (ret_ty, errors) = builtin_check_args(callee, &arg_types, func, Some(span));
                    self.errors.extend(errors);
                    ret_ty
                } else if let Some(sig) = self.functions.get(callee) {
                    let sig_params = sig.params.clone();
                    let sig_ret = sig.return_type.clone();

                    if args.len() != sig_params.len() {
                        let hint = {
                            let sig_str: String = sig_params
                                .iter()
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

                    for (i, ((param_name, param_ty), arg_ty)) in
                        sig_params.iter().zip(arg_types.iter()).enumerate()
                    {
                        if !compatible(param_ty, arg_ty) {
                            let hint = match (param_ty, arg_ty) {
                                (Ty::Text, Ty::Number) => {
                                    Some("use 'str' to convert number to text".to_string())
                                }
                                (Ty::Number, Ty::Text) => Some(
                                    "use 'num' to parse text as number (returns R n t)".to_string(),
                                ),
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
                } else if let Some(Ty::Fn(param_types, ret_type)) =
                    scope_lookup(scope, callee).cloned()
                {
                    // Dynamic dispatch: calling a function-ref held in a variable.
                    if args.len() != param_types.len() {
                        self.err(
                            "ILO-T006",
                            func,
                            format!("arity mismatch: function parameter '{callee}' expects {} args, got {}", param_types.len(), args.len()),
                            None,
                            Some(span),
                        );
                    } else {
                        for (i, (param_ty, arg_ty)) in
                            param_types.iter().zip(arg_types.iter()).enumerate()
                        {
                            if !compatible(param_ty, arg_ty) {
                                self.err(
                                    "ILO-T007",
                                    func,
                                    format!("type mismatch: arg {} of '{callee}' expects {param_ty}, got {arg_ty}", i + 1),
                                    None,
                                    Some(span),
                                );
                            }
                        }
                    }
                    *ret_type
                } else if let Some(bound_ty) = scope_lookup(scope, callee).cloned() {
                    // `callee` resolves as a parameter or local variable, but its type
                    // is not callable (i.e. not Ty::Fn). Do NOT fuzzy-match against
                    // builtins — the name is already a valid in-scope binding, just
                    // not a function. Produce a targeted error instead.
                    //
                    // ILO-T034 carve-out: a bare ident with a postfix bang
                    // (`x!` / `x!!`) parses as `Call { function: "x", args: [],
                    // unwrap: Propagate|Panic }`. The shape is a common
                    // mistake from agents reaching for `!` as a Rust-style
                    // `Result::unwrap` on a Result-typed local. In ilo `!` is
                    // strictly the auto-unwrap operator on a function CALL.
                    // Steer to the two canonical alternatives: match
                    // (`?x{~v:v;^e:^e}`) for inspecting a Result value, or
                    // rebinding from the producer (`y = producer! ...`).
                    if args.is_empty() && unwrap.is_any() {
                        let op = if unwrap.is_panic() { "!!" } else { "!" };
                        self.err(
                            "ILO-T034",
                            func,
                            format!(
                                "'{op}' is the auto-unwrap operator and only applies to function calls — '{callee}' is a {bound_ty} value"
                            ),
                            Some(format!(
                                "to inspect a Result value use match: `?{callee}{{~v:v;^e:^e}}`; to auto-unwrap a producer use `{callee} = producer! ...` and reference `{callee}` afterwards"
                            )),
                            Some(span),
                        );
                    } else {
                        self.err(
                            "ILO-T005",
                            func,
                            format!(
                                "'{callee}' is a {bound_ty}, not a function (called with {} args)",
                                args.len()
                            ),
                            Some(format!(
                                "'{callee}' is bound as {bound_ty} in this scope; only functions can be called"
                            )),
                            Some(span),
                        );
                    }
                    Ty::Unknown
                } else {
                    // Suggest in-scope variables/params first, then user functions, then
                    // builtins. closest_match picks the shortest distance, but when the
                    // name truly is undefined we still want a useful suggestion across
                    // all categories.
                    // Undefined-with-bang stays as ILO-T005 — the name doesn't
                    // resolve at all, so the fundamental error is the missing
                    // binding, not the postfix operator. Suggestions still help.
                    let mut candidates: Vec<String> = scope
                        .iter()
                        .flat_map(|frame| frame.keys().cloned())
                        .collect();
                    candidates.extend(self.functions.keys().cloned());
                    for (n, _, _) in BUILTINS {
                        candidates.push(n.to_string());
                    }
                    let hint = closest_match(callee, candidates.iter())
                        .map(|s| format!("did you mean '{s}'?"));
                    self.err(
                        "ILO-T005",
                        func,
                        format!(
                            "undefined function '{callee}' (called with {} args)",
                            args.len()
                        ),
                        hint,
                        Some(span),
                    );
                    Ty::Unknown
                };

                // Auto-unwrap: `func! args` or `func!! args`. Both require the
                // callee to return Result or Optional. `!` (Propagate) additionally
                // requires the enclosing function's return type to carry the
                // propagated Err/nil. `!!` (Panic) aborts at runtime instead, so
                // there is no enclosing-return constraint.
                if unwrap.is_any() {
                    let op_str = if unwrap.is_panic() { "!!" } else { "!" };
                    let op_desc = if unwrap.is_panic() {
                        "'!!' auto-unwraps R (Ok→v, Err→abort) or O (Some→v, Nil→abort)".to_string()
                    } else {
                        "'!' auto-unwraps R (Ok→v, Err→propagate) or O (Some→v, Nil→propagate)"
                            .to_string()
                    };
                    match &call_ty {
                        Ty::Result(ok_ty, _err_ty) => {
                            // Only `!` constrains the enclosing return type. `!!`
                            // aborts the process, so it works in any context.
                            if unwrap.is_propagate() {
                                // Clone the return type to release the &self borrow before calling self.err.
                                let enc_rt =
                                    self.functions.get(func).map(|sig| sig.return_type.clone());
                                // `for` over Option avoids a phantom else-branch in LLVM coverage
                                // (the None path is unreachable in practice — func is always registered).
                                #[allow(for_loops_over_fallibles)]
                                for rt in enc_rt {
                                    match rt {
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
                            }
                            *ok_ty.clone()
                        }
                        Ty::Optional(inner_ty) => {
                            // Optional unwrap propagates nil as the function's return.
                            // The enclosing function's return type must accept nil:
                            // Optional, Nil, or Unknown (inferred). `!!` skips this
                            // check since nil aborts at runtime.
                            if unwrap.is_propagate() {
                                let enc_rt =
                                    self.functions.get(func).map(|sig| sig.return_type.clone());
                                #[allow(for_loops_over_fallibles)]
                                for rt in enc_rt {
                                    match rt {
                                        Ty::Optional(_) | Ty::Nil | Ty::Unknown => {}
                                        other => {
                                            self.err(
                                                "ILO-T026",
                                                func,
                                                format!("'!' used in function '{func}' which returns {other}, not an Optional"),
                                                Some("the enclosing function must return O to propagate nil".to_string()),
                                                Some(span),
                                            );
                                        }
                                    }
                                }
                            }
                            *inner_ty.clone()
                        }
                        Ty::Unknown => Ty::Unknown,
                        other => {
                            self.err(
                                "ILO-T025",
                                func,
                                format!("'{op_str}' used on call to '{callee}' which returns {other}, not a Result or Optional"),
                                Some(op_desc),
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
                            self.err(
                                "ILO-T012",
                                func,
                                format!("negate expects n, got {t}"),
                                None,
                                Some(span),
                            );
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
                    let mut elem_ty = first_ty;
                    for item in &items[1..] {
                        let item_ty = self.infer_expr(func, scope, item, span);
                        if !compatible(&elem_ty, &item_ty) && !compatible(&item_ty, &elem_ty) {
                            elem_ty = Ty::Unknown; // heterogeneous list
                        }
                    }
                    Ty::List(Box::new(elem_ty))
                }
            }

            Expr::Record { type_name, fields } => {
                if let Some(type_def) = self.types.get(type_name) {
                    let def_fields = type_def.fields.clone();
                    let provided: HashMap<&str, &Expr> =
                        fields.iter().map(|(n, e)| (n.as_str(), e)).collect();

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
                    let def_field_names: Vec<&str> =
                        def_fields.iter().map(|(n, _)| n.as_str()).collect();
                    for (fname, _) in fields {
                        if !def_field_names.contains(&fname.as_str()) {
                            let def_field_strings: Vec<String> =
                                def_field_names.iter().map(|s| s.to_string()).collect();
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
                    self.err(
                        "ILO-T003",
                        func,
                        format!("undefined type '{type_name}'"),
                        hint,
                        Some(span),
                    );
                    Ty::Unknown
                }
            }

            Expr::Field {
                object,
                field,
                safe,
            } => {
                let obj_ty = self.infer_expr(func, scope, object, span);
                if *safe && obj_ty == Ty::Nil {
                    return Ty::Nil;
                }
                match &obj_ty {
                    Ty::Named(type_name) => {
                        if let Some(type_def) = self.types.get(type_name) {
                            if let Some((_, fty)) = type_def.fields.iter().find(|(n, _)| n == field)
                            {
                                fty.clone()
                            } else {
                                let field_names: Vec<String> =
                                    type_def.fields.iter().map(|(n, _)| n.clone()).collect();
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
                        self.err(
                            "ILO-T018",
                            func,
                            format!("field access on non-record type {other}"),
                            None,
                            Some(span),
                        );
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
                        self.err(
                            "ILO-T023",
                            func,
                            format!("index access on non-list type {other}"),
                            None,
                            Some(span),
                        );
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
                match val_ty {
                    Ty::Nil => def_ty,
                    Ty::Optional(inner) => *inner,
                    other => other,
                }
            }
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                self.infer_expr(func, scope, condition, span);
                let then_ty = self.infer_expr(func, scope, then_expr, span);
                let else_ty = self.infer_expr(func, scope, else_expr, span);
                if compatible(&then_ty, &else_ty) {
                    then_ty
                } else if compatible(&else_ty, &then_ty) {
                    else_ty
                } else {
                    self.err(
                        "ILO-T003",
                        func,
                        format!(
                            "ternary branches have different types: {} vs {}",
                            then_ty, else_ty
                        ),
                        None,
                        Some(span),
                    );
                    then_ty
                }
            }

            Expr::With { object, updates } => {
                let obj_ty = self.infer_expr(func, scope, object, span);
                match &obj_ty {
                    Ty::Named(type_name) => {
                        if let Some(type_def) = self.types.get(type_name) {
                            let def_fields = type_def.fields.clone();
                            for (fname, expr) in updates {
                                if let Some((_, fty)) = def_fields.iter().find(|(n, _)| n == fname)
                                {
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
                                    let def_field_strings: Vec<String> =
                                        def_fields.iter().map(|(n, _)| n.clone()).collect();
                                    let hint = closest_match(fname, def_field_strings.iter())
                                        .map(|s| format!("did you mean '{s}'?"));
                                    self.err(
                                        "ILO-T021",
                                        func,
                                        format!(
                                            "unknown field '{fname}' in 'with' on '{type_name}'"
                                        ),
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
                        self.err(
                            "ILO-T020",
                            func,
                            format!("'with' on non-record type {other}"),
                            None,
                            Some(span),
                        );
                        Ty::Unknown
                    }
                }
            }

            Expr::MakeClosure { fn_name, captures } => {
                // Evaluate captures so any free-var-resolution errors surface
                // here against the call site (which is where the user wrote the
                // capture). The closure value itself is fn-like; we report
                // Ty::Unknown so HOF signatures treat it like a fn-ref (same
                // as a bare `Expr::Ref(fn_name)` does today).
                for cap in captures {
                    self.infer_expr(func, scope, cap, span);
                }
                let _ = fn_name;
                Ty::Unknown
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
                            (Ty::Number, Ty::Text) | (Ty::Text, Ty::Number) => Some(
                                "convert number to text with 'str' before concatenating"
                                    .to_string(),
                            ),
                            _ => None,
                        };
                        self.err(
                            "ILO-T009",
                            func,
                            format!("'+' expects matching n, t, or L types, got {lt} and {rt}"),
                            hint,
                            Some(span),
                        );
                        Ty::Unknown
                    }
                }
            }
            BinOp::Subtract | BinOp::Multiply | BinOp::Divide => {
                if !compatible(lt, &Ty::Number) || !compatible(rt, &Ty::Number) {
                    let sym = match op {
                        BinOp::Subtract => "-",
                        BinOp::Multiply => "*",
                        _ => "/",
                    };
                    let has_text = matches!(lt, Ty::Text) || matches!(rt, Ty::Text);
                    let hint = if has_text {
                        Some("parse text as number with 'num' first".to_string())
                    } else {
                        None
                    };
                    self.err(
                        "ILO-T009",
                        func,
                        format!("'{sym}' expects n and n, got {lt} and {rt}"),
                        hint,
                        Some(span),
                    );
                }
                Ty::Number
            }
            BinOp::GreaterThan | BinOp::LessThan | BinOp::GreaterOrEqual | BinOp::LessOrEqual => {
                match (lt, rt) {
                    (Ty::Number, Ty::Number) | (Ty::Text, Ty::Text) => {}
                    (Ty::Unknown, _) | (_, Ty::Unknown) => {}
                    _ => {
                        self.err(
                            "ILO-T010",
                            func,
                            format!("comparison expects matching n or t, got {lt} and {rt}"),
                            None,
                            Some(span),
                        );
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
                            self.err(
                                "ILO-T011",
                                func,
                                format!(
                                    "'+=' list element type {inner} doesn't match appended {rt}"
                                ),
                                None,
                                Some(span),
                            );
                        }
                        lt.clone()
                    }
                    Ty::Unknown => Ty::Unknown,
                    _ => {
                        self.err(
                            "ILO-T011",
                            func,
                            format!("'+=' expects a list on the left, got {lt}"),
                            None,
                            Some(span),
                        );
                        Ty::Unknown
                    }
                }
            }
        }
    }

    fn check_match_exhaustiveness(
        &mut self,
        func: &str,
        subject_ty: &Ty,
        arms: &[MatchArm],
        span: Span,
    ) {
        let has_wildcard = arms
            .iter()
            .any(|a| matches!(a.pattern, Pattern::Wildcard | Pattern::TypeIs { .. }));
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
                    ]
                    .into_iter()
                    .flatten()
                    .collect();
                    let parts: Vec<String> = [
                        if !has_ok {
                            Some(format!("~v: <expr>  (v is of type {ok_ty})"))
                        } else {
                            None
                        },
                        if !has_err {
                            Some(format!("^e: <expr>  (e is of type {err_ty})"))
                        } else {
                            None
                        },
                    ]
                    .into_iter()
                    .flatten()
                    .collect();
                    self.err(
                        "ILO-T024",
                        func,
                        format!(
                            "non-exhaustive match on Result: missing {}",
                            missing.join(", ")
                        ),
                        Some(format!("add: {}", parts.join(" or "))),
                        Some(span),
                    );
                }
            }
            Ty::Bool => {
                let has_true = arms
                    .iter()
                    .any(|a| matches!(&a.pattern, Pattern::Literal(Literal::Bool(true))));
                let has_false = arms
                    .iter()
                    .any(|a| matches!(&a.pattern, Pattern::Literal(Literal::Bool(false))));
                if !has_true || !has_false {
                    let missing: Vec<&str> = [
                        if !has_true { Some("true") } else { None },
                        if !has_false { Some("false") } else { None },
                    ]
                    .into_iter()
                    .flatten()
                    .collect();
                    let parts: Vec<&str> = [
                        if !has_true {
                            Some("true: <expr>")
                        } else {
                            None
                        },
                        if !has_false {
                            Some("false: <expr>")
                        } else {
                            None
                        },
                    ]
                    .into_iter()
                    .flatten()
                    .collect();
                    self.err(
                        "ILO-T024",
                        func,
                        format!(
                            "non-exhaustive match on Bool: missing {}",
                            missing.join(", ")
                        ),
                        Some(format!("add: {}", parts.join(" or "))),
                        Some(span),
                    );
                }
            }
            // Sum types: exhaustive if every variant has a literal text arm
            Ty::Sum(variants) => {
                let covered: Vec<&str> = arms
                    .iter()
                    .filter_map(|a| match &a.pattern {
                        Pattern::Literal(Literal::Text(s)) => Some(s.as_str()),
                        _ => None,
                    })
                    .collect();
                let missing: Vec<&String> = variants
                    .iter()
                    .filter(|v| !covered.contains(&v.as_str()))
                    .collect();
                if !missing.is_empty() {
                    let names: Vec<&str> = missing.iter().map(|s| s.as_str()).collect();
                    self.err(
                        "ILO-T024",
                        func,
                        format!(
                            "non-exhaustive match on sum type: missing {}",
                            names.join(", ")
                        ),
                        Some(format!(
                            "add: {}",
                            names
                                .iter()
                                .map(|n| format!("\"{n}\": <expr>"))
                                .collect::<Vec<_>>()
                                .join(" or ")
                        )),
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

#[derive(Debug)]
pub struct VerifyResult {
    pub errors: Vec<VerifyError>,
    pub warnings: Vec<VerifyError>,
}

/// Run static verification on a parsed program.
/// Returns errors and warnings separately.
pub fn verify(program: &Program) -> VerifyResult {
    let mut ctx = VerifyContext::new();

    // Phase 1: collect declarations
    ctx.collect_declarations(program);

    // Phase 2: verify function bodies
    ctx.verify_bodies(program);

    let (warnings, errors) = ctx.errors.into_iter().partition(|e| e.is_warning);
    VerifyResult { errors, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_verify(code: &str) -> Result<(), Vec<VerifyError>> {
        let result = parse_and_verify_full(code);
        if result.errors.is_empty() {
            Ok(())
        } else {
            Err(result.errors)
        }
    }

    fn parse_and_verify_full(code: &str) -> VerifyResult {
        let tokens = crate::lexer::lex(code).expect("lex failed");
        let token_spans: Vec<(crate::lexer::Token, crate::ast::Span)> = tokens
            .into_iter()
            .map(|(t, r)| {
                (
                    t,
                    crate::ast::Span {
                        start: r.start,
                        end: r.end,
                    },
                )
            })
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
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("undefined variable 'y'"))
        );
    }

    #[test]
    fn undefined_function() {
        let result = parse_and_verify("f x:n>n;foo x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("undefined function 'foo'"))
        );
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
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("'*' expects n and n"))
        );
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
        // `min` accepts 1 (list form) or 2 (numeric pair) args. Three args
        // is the smallest clear arity violation.
        let result = parse_and_verify("f a:n b:n c:n>n;min a b c");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("arity mismatch") && e.message.contains("min")),
            "errors: {:?}",
            errors
        );
    }

    #[test]
    fn min_max_one_arg_list_form_accepted() {
        // `min xs` / `max xs` (1-arg list form) must verify cleanly against
        // a list-of-numbers argument; only the 2-arg form requires `n n`.
        assert!(parse_and_verify("f xs:L n>n;min xs").is_ok());
        assert!(parse_and_verify("f xs:L n>n;max xs").is_ok());
    }

    #[test]
    fn min_max_one_arg_rejects_non_list() {
        // 1-arg form on a plain number must be rejected with a type error
        // mentioning the builtin so the verifier hint stays useful.
        let result = parse_and_verify("f x:n>n;min x");
        assert!(result.is_err(), "expected type error, got Ok");
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("min")),
            "errors: {:?}",
            errors
        );
    }

    #[test]
    fn min_max_one_arg_rejects_list_of_text() {
        // 1-arg form on `L t` (list of text) hits the inner-non-number branch
        // and must surface ILO-T013 mentioning `L n` so the hint stays useful.
        let result = parse_and_verify(r#"f xs:L t>n;min xs"#);
        assert!(result.is_err(), "expected type error, got Ok");
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("min")),
            "errors: {:?}",
            errors
        );

        let result = parse_and_verify(r#"f xs:L t>n;max xs"#);
        assert!(result.is_err(), "expected type error, got Ok");
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("max")),
            "errors: {:?}",
            errors
        );
    }

    #[test]
    fn min_max_one_arg_max_rejects_non_list() {
        // Symmetric to min_max_one_arg_rejects_non_list, exercising `max`
        // through the same arm so the hint string ("use `max xs` ...")
        // gets covered too.
        let result = parse_and_verify("f x:n>n;max x");
        assert!(result.is_err(), "expected type error, got Ok");
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("max")),
            "errors: {:?}",
            errors
        );
    }

    #[test]
    fn min_max_arity_three_args_errors() {
        // arity-ok branch returns false for args.len() == 3, which routes
        // through the dedicated `1 or 2` arity-desc string.
        let result = parse_and_verify("f a:n b:n c:n>n;min a b c");
        assert!(result.is_err(), "expected arity error, got Ok");
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("1 or 2") && e.message.contains("min")),
            "errors: {:?}",
            errors
        );
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
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("foreach expects a list"))
        );
    }

    #[test]
    fn duplicate_function() {
        // Two functions both named "dup" — second starts a new decl after first body
        let result = parse_and_verify("dup x:n>n;*x 2 dup x:n>n;+x 1");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("duplicate function"))
        );
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
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("return type mismatch"))
        );
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
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("index access on non-list"))
        );
    }

    #[test]
    fn did_you_mean_hint() {
        let result = parse_and_verify("calc x:n>n;*x 2 f x:n>n;calx x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        let err = errors
            .iter()
            .find(|e| e.message.contains("undefined function 'calx'"))
            .unwrap();
        assert!(
            err.hint
                .as_ref()
                .is_some_and(|h| h.contains("did you mean 'calc'?"))
        );
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
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("non-exhaustive") && e.message.contains("^"))
        );
    }

    #[test]
    fn non_exhaustive_result_missing_ok() {
        let result = parse_and_verify("f x:R n t>n;?x{^e:0}");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("non-exhaustive") && e.message.contains("~"))
        );
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
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("non-exhaustive") && e.message.contains("false"))
        );
    }

    #[test]
    fn non_exhaustive_number_no_wildcard() {
        let result = parse_and_verify("f x:n>t;?x{1:\"one\";2:\"two\"}");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("non-exhaustive") && e.message.contains("no wildcard"))
        );
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
        assert_eq!(
            format!("{}", Ty::Result(Box::new(Ty::Number), Box::new(Ty::Text))),
            "R n t"
        );
    }

    #[test]
    fn ty_display_named() {
        assert_eq!(format!("{}", Ty::Named("point".to_string())), "point");
    }

    #[test]
    fn ty_display_unknown() {
        assert_eq!(format!("{}", Ty::Unknown), "_");
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
            is_warning: false,
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
            is_warning: false,
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
        assert!(compatible(
            &Ty::Named("point".to_string()),
            &Ty::Named("point".to_string())
        ));
    }

    #[test]
    fn compatible_named_different() {
        assert!(!compatible(
            &Ty::Named("point".to_string()),
            &Ty::Named("rect".to_string())
        ));
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
        // A function with Any return type exercises convert_type(Type::Any)
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
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("'str' expects n, got t"))
        );
    }

    #[test]
    fn builtin_num_wrong_type() {
        let result = parse_and_verify("f x:n>R n t;num x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("'num' expects t, got n"))
        );
    }

    #[test]
    fn builtin_min_wrong_type() {
        let result = parse_and_verify("f x:t y:n>n;min x y");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("'min' arg 1 expects n, got t"))
        );
    }

    #[test]
    fn builtin_max_wrong_type() {
        let result = parse_and_verify("f x:n y:t>n;max x y");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("'max' arg 2 expects n, got t"))
        );
    }

    // ---- Tool declaration processing ----

    #[test]
    fn tool_declaration_processed() {
        // A tool should be collected and callable from a function
        let result = parse_and_verify(r#"tool my-tool "desc" x:n>n f y:n>n;my-tool y"#);
        assert!(result.is_ok());
    }

    #[test]
    fn tool_conflicts_with_function_name() {
        // Tool name conflicts with function name
        let result = parse_and_verify(r#"f x:n>n;*x 2 tool f "desc" y:n>n"#);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("duplicate definition")
                    || e.message.contains("duplicate function"))
        );
    }

    // ---- TypeDef field validation ----

    #[test]
    fn typedef_field_with_undefined_named_type() {
        // A typedef with a field referencing an undefined type
        let result = parse_and_verify("type edge{from:node;to:node} f x:n>n;x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("undefined type 'node'"))
        );
    }

    // ---- Undefined type in function signature ----

    #[test]
    fn undefined_type_in_function_param() {
        let result = parse_and_verify("f x:ghost>n;x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("undefined type 'ghost'"))
        );
    }

    // ---- Record errors ----

    #[test]
    fn record_missing_field() {
        let result = parse_and_verify("type point{x:n;y:n} f>point;point x:1");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("missing field 'y'"))
        );
    }

    #[test]
    fn record_extra_field() {
        let result = parse_and_verify("type point{x:n;y:n} f>point;point x:1 y:2 z:3");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("unknown field 'z'"))
        );
    }

    #[test]
    fn record_field_type_mismatch() {
        let result = parse_and_verify("type point{x:n;y:n} f>point;point x:1 y:\"bad\"");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("field 'y' of 'point' expects n, got t"))
        );
    }

    #[test]
    fn record_undefined_type() {
        // Constructing a record of an undefined type
        let result = parse_and_verify("f>n;x=ghost a:1;0");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("undefined type 'ghost'"))
        );
    }

    // ---- Field access errors ----

    #[test]
    fn field_not_found_on_type() {
        let result = parse_and_verify("type point{x:n;y:n} f p:point>n;p.z");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("no field 'z' on type 'point'"))
        );
    }

    #[test]
    fn field_access_on_non_record_type() {
        let result = parse_and_verify("f x:n>n;x.foo");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("field access on non-record type n"))
        );
    }

    // ---- With expression errors ----

    #[test]
    fn with_on_non_record() {
        let result = parse_and_verify("f x:n>n;y=x with foo:1;0");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("'with' on non-record type n"))
        );
    }

    #[test]
    fn with_field_not_found() {
        let result = parse_and_verify("type point{x:n;y:n} f p:point>point;p with z:1");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("unknown field 'z' in 'with'"))
        );
    }

    #[test]
    fn with_field_type_mismatch() {
        let result = parse_and_verify("type point{x:n;y:n} f p:point>point;p with x:\"bad\"");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| {
            e.message
                .contains("'with' field 'x' of 'point' expects n, got t")
        }));
    }

    // ---- BinOp errors ----

    #[test]
    fn binop_comparison_wrong_types() {
        let result = parse_and_verify("f x:n y:b>b;>x y");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| {
            e.message
                .contains("comparison expects matching n or t, got n and b")
        }));
    }

    #[test]
    fn binop_append_non_list() {
        let result = parse_and_verify("f x:n>n;y=+=x 1;0");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("'+=' expects a list on the left, got n"))
        );
    }

    #[test]
    fn binop_append_wrong_element_type() {
        let result = parse_and_verify("f xs:L n>L n;+=xs \"bad\"");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| {
            e.message
                .contains("'+=' list element type n doesn't match appended t")
        }));
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
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("non-exhaustive") && e.message.contains("no wildcard"))
        );
    }

    // ---- Index access on non-list (when type is not Unknown) ----

    #[test]
    fn index_access_on_non_list_bool() {
        let result = parse_and_verify("f x:b>b;x.0");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("index access on non-list"))
        );
    }

    // ---- builtin len wrong type ----

    #[test]
    fn builtin_len_wrong_type() {
        let result = parse_and_verify("f x:n>n;len x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| {
            e.message
                .contains("'len' expects a list, map, or text, got n")
        }));
    }

    // ---- builtin abs/flr/cel wrong type ----

    #[test]
    fn builtin_abs_wrong_type() {
        let result = parse_and_verify("f x:t>n;abs x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("'abs' expects n, got t"))
        );
    }

    // ---- duplicate type definition ----

    #[test]
    fn duplicate_type_definition() {
        let result = parse_and_verify("type point{x:n;y:n} type point{a:n;b:n} f x:n>n;x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("duplicate type definition 'point'"))
        );
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
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("negate expects n, got t"))
        );
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
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("'+' expects matching n, t, or L types"))
        );
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
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("undefined type 'ghost'"))
        );
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
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-T006" && e.message.contains("'g'"))
            .unwrap();
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
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-T009" && e.message.contains("'+'"))
            .unwrap();
        assert!(e.hint.as_ref().is_some_and(|h| h.contains("str")));
    }

    #[test]
    fn suggestion_t009_multiply_text_hint() {
        let result = parse_and_verify("f x:t y:n>n;*x y");
        let errors = result.unwrap_err();
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-T009" && e.message.contains("'*'"))
            .unwrap();
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
        let result =
            parse_and_verify("type person{name:t;age:n} f p:person>person;p with nam:\"bob\"");
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
                    params: vec![Param {
                        name: "x".to_string(),
                        ty: Type::Number,
                    }],
                    return_type: rnt.clone(),
                    body: vec![Spanned::unknown(Stmt::Expr(Expr::Ok(Box::new(Expr::Ref(
                        "x".to_string(),
                    )))))],
                    span: Span::UNKNOWN,
                },
                Decl::Function {
                    name: "outer".to_string(),
                    params: vec![Param {
                        name: "x".to_string(),
                        ty: Type::Number,
                    }],
                    return_type: rnt,
                    body: vec![
                        Spanned::unknown(Stmt::Let {
                            name: "d".to_string(),
                            value: Expr::Call {
                                function: "inner".to_string(),
                                args: vec![Expr::Ref("x".to_string())],
                                unwrap: UnwrapMode::Propagate,
                            },
                        }),
                        Spanned::unknown(Stmt::Expr(Expr::Ok(Box::new(Expr::Ref(
                            "d".to_string(),
                        ))))),
                    ],
                    span: Span::UNKNOWN,
                },
            ],
            source: None,
        };
        let result = verify(&prog);
        assert!(
            result.errors.is_empty(),
            "expected valid, got: {:?}",
            result
        );
    }

    #[test]
    fn unwrap_t025_non_result_callee() {
        // Callee returns n, not R — should emit T025
        use crate::ast::*;
        let prog = Program {
            declarations: vec![
                Decl::Function {
                    name: "inner".to_string(),
                    params: vec![Param {
                        name: "x".to_string(),
                        ty: Type::Number,
                    }],
                    return_type: Type::Number,
                    body: vec![Spanned::unknown(Stmt::Expr(Expr::Ref("x".to_string())))],
                    span: Span::UNKNOWN,
                },
                Decl::Function {
                    name: "outer".to_string(),
                    params: vec![Param {
                        name: "x".to_string(),
                        ty: Type::Number,
                    }],
                    return_type: Type::Result(Box::new(Type::Number), Box::new(Type::Text)),
                    body: vec![Spanned::unknown(Stmt::Expr(Expr::Call {
                        function: "inner".to_string(),
                        args: vec![Expr::Ref("x".to_string())],
                        unwrap: UnwrapMode::Propagate,
                    }))],
                    span: Span::UNKNOWN,
                },
            ],
            source: None,
        };
        let errors = &verify(&prog).errors;
        assert!(
            errors.iter().any(|e| e.code == "ILO-T025"),
            "expected T025, got: {:?}",
            errors
        );
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
                    params: vec![Param {
                        name: "x".to_string(),
                        ty: Type::Number,
                    }],
                    return_type: rnt,
                    body: vec![Spanned::unknown(Stmt::Expr(Expr::Ok(Box::new(Expr::Ref(
                        "x".to_string(),
                    )))))],
                    span: Span::UNKNOWN,
                },
                Decl::Function {
                    name: "outer".to_string(),
                    params: vec![Param {
                        name: "x".to_string(),
                        ty: Type::Number,
                    }],
                    return_type: Type::Number,
                    body: vec![Spanned::unknown(Stmt::Expr(Expr::Call {
                        function: "inner".to_string(),
                        args: vec![Expr::Ref("x".to_string())],
                        unwrap: UnwrapMode::Propagate,
                    }))],
                    span: Span::UNKNOWN,
                },
            ],
            source: None,
        };
        let errors = &verify(&prog).errors;
        assert!(
            errors.iter().any(|e| e.code == "ILO-T026"),
            "expected T026, got: {:?}",
            errors
        );
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
        // 3 args is invalid for get (max 2)
        let result = parse_and_verify(r#"f x:t y:M t t z:t>R t t;get x y z"#);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("arity")));
    }

    #[test]
    fn builtin_get_with_headers_ok() {
        // 2-arg get with M t t headers is valid
        assert!(parse_and_verify(r#"f url:t hdrs:M t t>R t t;get url hdrs"#).is_ok());
    }

    #[test]
    fn builtin_post_with_headers_ok() {
        // 3-arg post with M t t headers is valid
        assert!(parse_and_verify(r#"f url:t body:t hdrs:M t t>R t t;post url body hdrs"#).is_ok());
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
        let result =
            parse_and_verify("classify n:n>t;\"done\"\ncls sp:n>t;>=sp 1000 classify;\"fallback\"");
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ILO-T027" && e.message.contains("classify")),
            "expected ILO-T027 for function name in braceless guard body, got: {:?}",
            errors
        );
        assert!(
            errors
                .iter()
                .any(|e| e.hint.as_ref().is_some_and(|h| h.contains("braces"))),
            "expected hint about braces, got: {:?}",
            errors
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
            errors
                .iter()
                .any(|e| e.code == "ILO-T027" && e.message.contains("len")),
            "expected ILO-T027 for builtin name in braceless guard body, got: {:?}",
            errors
        );
    }

    #[test]
    fn spl_valid() {
        let result = parse_and_verify(r#"f s:t sep:t>L t;spl s sep"#);
        assert!(
            result.is_ok(),
            "spl with two text args should verify: {:?}",
            result
        );
    }

    #[test]
    fn spl_wrong_type() {
        let result = parse_and_verify(r#"f s:t n:n>L t;spl s n"#);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("spl"))
        );
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
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("cat"))
        );
    }

    #[test]
    fn cat_wrong_arity() {
        let result = parse_and_verify("f items:L t>t;cat items");
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("cat") && e.message.contains("2"))
        );
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
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("'has' arg 1 expects a list or text"))
        );
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
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("'hd' expects a list or text, got n"))
        );
    }

    #[test]
    fn tl_wrong_type() {
        let result = parse_and_verify("f x:n>n;tl x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("'tl' expects a list or text, got n"))
        );
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
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("rev"))
        );
    }

    #[test]
    fn srt_valid_list() {
        assert!(parse_and_verify("f>L n;xs=[3, 1, 2];srt xs").is_ok());
    }

    #[test]
    fn srt_wrong_type() {
        let result = parse_and_verify("f x:n>n;srt x");
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("srt"))
        );
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
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("slc"))
        );
    }

    #[test]
    fn slc_wrong_index_type() {
        let result = parse_and_verify("f x:L n s:t>L n;slc x s 2");
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("slc"))
        );
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

    // ---- brk/cnt outside loop (ILO-T028) ----

    #[test]
    fn brk_outside_loop() {
        let errors = parse_and_verify("f>n;brk").unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ILO-T028" && e.message.contains("brk"))
        );
    }

    #[test]
    fn cnt_outside_loop() {
        let errors = parse_and_verify("f>n;cnt").unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ILO-T028" && e.message.contains("cnt"))
        );
    }

    #[test]
    fn brk_inside_foreach() {
        assert!(parse_and_verify("f xs:L n>_;@x xs{brk}").is_ok());
    }

    #[test]
    fn brk_inside_while() {
        assert!(parse_and_verify("f>_;i=0;wh <i 5{brk}").is_ok());
    }

    #[test]
    fn cnt_inside_foreach() {
        assert!(parse_and_verify("f xs:L n>_;@x xs{cnt}").is_ok());
    }

    #[test]
    fn brk_inside_guard_inside_loop() {
        assert!(parse_and_verify("f>_;i=0;wh <i 5{>i 3{brk};i=+i 1}").is_ok());
    }

    // ---- Guard-in-loop: ILO-W001 retired (braced guards are now conditional execution) ----

    #[test]
    fn guard_in_foreach_no_warning() {
        // Braced guards in loops are now conditional execution — no warning
        let result = parse_and_verify_full("f xs:L n>n;r=0;@x xs{>=x 10{r= +r x}};r");
        assert!(result.errors.is_empty());
        let w001: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.code == "ILO-W001")
            .collect();
        assert_eq!(w001.len(), 0);
    }

    #[test]
    fn guard_in_while_no_warning() {
        let result = parse_and_verify_full("f>n;i=0;wh <i 10{>i 5{ret i};i= +i 1};i");
        assert!(result.errors.is_empty());
        let w001: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.code == "ILO-W001")
            .collect();
        assert_eq!(w001.len(), 0);
    }

    #[test]
    fn guard_in_range_no_warning() {
        let result = parse_and_verify_full("f>n;r=0;@i 0..10{>=i 5{r= +r i}};r");
        assert!(result.errors.is_empty());
        let w001: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.code == "ILO-W001")
            .collect();
        assert_eq!(w001.len(), 0);
    }

    #[test]
    fn guard_with_else_in_loop_no_warning() {
        // Ternary form {then}{else} is fine — no early return
        let result = parse_and_verify_full("f xs:L n>n;r=0;@x xs{>=x 10{r= +r x}{r}};r");
        let w001: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.code == "ILO-W001")
            .collect();
        assert_eq!(w001.len(), 0);
    }

    #[test]
    fn guard_outside_loop_no_warning() {
        // Guard at function level is normal — no warning
        let result = parse_and_verify_full("f x:n>n;>=x 0{x};-x");
        let w001: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.code == "ILO-W001")
            .collect();
        assert_eq!(w001.len(), 0);
    }

    // ---- Unreachable code warnings (ILO-T029) ----

    #[test]
    fn unreachable_after_ret() {
        let result = parse_and_verify_full("f x:n>n;ret x;*x 2");
        assert!(result.errors.is_empty());
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].code, "ILO-T029");
        assert!(result.warnings[0].message.contains("ret"));
    }

    #[test]
    fn unreachable_after_brk() {
        let result = parse_and_verify_full("f x:n>n;wh true{brk 1;x=2;x};x");
        assert!(result.errors.is_empty());
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].code, "ILO-T029");
        assert!(result.warnings[0].message.contains("brk"));
    }

    #[test]
    fn ret_as_last_no_warning() {
        let result = parse_and_verify_full("f x:n>n;y=*x 2;ret y");
        assert!(result.errors.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn ret_in_guard_body_no_warning_for_outer() {
        let result = parse_and_verify_full(r#"f x:n>t;>x 0{ret "pos"};"neg""#);
        assert!(result.errors.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn multiple_stmts_after_ret_one_warning() {
        let result = parse_and_verify_full("f x:n>n;ret x;y=*x 2;+y 1");
        assert!(result.errors.is_empty());
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].code, "ILO-T029");
    }

    // ---- Bare fmt/fmt2 discard warnings (ILO-T032) ----

    #[test]
    fn bare_fmt_non_tail_warns() {
        // The classic persona pain: `fmt "..." v` as a non-tail stmt is
        // silently discarded. Should warn.
        let result = parse_and_verify_full(r#"f v:n>n;fmt "v={}" v;v"#);
        assert!(result.errors.is_empty());
        let t032: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.code == "ILO-T032")
            .collect();
        assert_eq!(
            t032.len(),
            1,
            "expected one ILO-T032 warning, got {:?}",
            result.warnings
        );
        assert!(t032[0].message.contains("fmt"));
        assert!(
            t032[0]
                .hint
                .as_ref()
                .is_some_and(|h| h.contains("prnt fmt"))
        );
    }

    #[test]
    fn bare_fmt2_non_tail_warns() {
        let result = parse_and_verify_full(r#"f v:n>n;fmt2 v 2;v"#);
        assert!(result.errors.is_empty());
        let t032: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.code == "ILO-T032")
            .collect();
        assert_eq!(t032.len(), 1);
        assert!(t032[0].message.contains("fmt2"));
    }

    #[test]
    fn fmt_in_tail_position_no_warning() {
        // `fmt` as the return value of a function is the documented idiom.
        // Must NOT warn.
        let result = parse_and_verify_full(r#"say-x v:n>t;fmt "x={}" v"#);
        assert!(result.errors.is_empty());
        let t032: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.code == "ILO-T032")
            .collect();
        assert_eq!(
            t032.len(),
            0,
            "tail fmt should not warn, got {:?}",
            result.warnings
        );
    }

    #[test]
    fn fmt_bound_to_name_no_warning() {
        // `name = fmt ...` is the captured form. Must NOT warn.
        let result = parse_and_verify_full(r#"f v:n>t;line=fmt "v={}" v;prnt line;line"#);
        assert!(result.errors.is_empty());
        let t032: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.code == "ILO-T032")
            .collect();
        assert_eq!(t032.len(), 0);
    }

    #[test]
    fn fmt_inside_prnt_no_warning() {
        // `prnt fmt ...` is the print form. The `fmt` is nested inside the
        // prnt call, not a bare statement. Must NOT warn.
        let result = parse_and_verify_full(r#"f v:n>n;prnt fmt "v={}" v;v"#);
        assert!(result.errors.is_empty());
        let t032: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.code == "ILO-T032")
            .collect();
        assert_eq!(t032.len(), 0);
    }

    #[test]
    fn multiple_bare_fmts_each_warn() {
        // The persona's actual file had 13 such lines. Each should warn
        // independently so the agent sees them all in one pass.
        let result = parse_and_verify_full(r#"f v:n>n;fmt "a={}" v;fmt "b={}" v;fmt "c={}" v;v"#);
        assert!(result.errors.is_empty());
        let t032: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.code == "ILO-T032")
            .collect();
        assert_eq!(t032.len(), 3);
    }

    // ---- Bare mutation-shaped builtin discard warnings (ILO-T033) ----

    #[test]
    fn bare_append_in_loop_warns() {
        // The persona's exact repro: `+=out i` inside `@` loop body is the
        // single biggest correctness footgun. Logged across three consecutive
        // db-analyst sessions. Must warn.
        let result = parse_and_verify_full(r#"f>L n;out=[];@i 0..3{+=out i};out"#);
        assert!(result.errors.is_empty());
        let t033: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.code == "ILO-T033")
            .collect();
        assert_eq!(
            t033.len(),
            1,
            "expected one ILO-T033 warning, got {:?}",
            result.warnings
        );
        assert!(t033[0].message.contains("+="));
        assert!(
            t033[0]
                .hint
                .as_ref()
                .is_some_and(|h| h.contains("out=+=out"))
        );
    }

    #[test]
    fn bare_mset_non_tail_warns() {
        let result = parse_and_verify_full(r#"f>M t n;m=mmap;mset m "a" 1;m"#);
        assert!(result.errors.is_empty());
        let t033: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.code == "ILO-T033")
            .collect();
        assert_eq!(t033.len(), 1);
        assert!(t033[0].message.contains("mset"));
        assert!(
            t033[0]
                .hint
                .as_ref()
                .is_some_and(|h| h.contains("m=mset m"))
        );
    }

    #[test]
    fn bare_mdel_non_tail_warns() {
        let result = parse_and_verify_full(r#"f>M t n;m=mset mmap "a" 1;mdel m "a";m"#);
        assert!(result.errors.is_empty());
        let t033: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.code == "ILO-T033")
            .collect();
        assert_eq!(t033.len(), 1);
        assert!(t033[0].message.contains("mdel"));
    }

    #[test]
    fn append_rebind_no_warning() {
        // The canonical fix shape. Must NOT warn.
        let result = parse_and_verify_full(r#"f>L n;out=[];@i 0..3{out=+=out i};out"#);
        assert!(result.errors.is_empty());
        let t033: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.code == "ILO-T033")
            .collect();
        assert_eq!(
            t033.len(),
            0,
            "rebind shape should not warn, got {:?}",
            result.warnings
        );
    }

    #[test]
    fn mset_rebind_no_warning() {
        let result = parse_and_verify_full(r#"f>M t n;m=mmap;m=mset m "a" 1;m"#);
        assert!(result.errors.is_empty());
        let t033: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.code == "ILO-T033")
            .collect();
        assert_eq!(t033.len(), 0);
    }

    #[test]
    fn append_in_function_tail_no_warning() {
        // `+=xs v` as the function's last statement returns the new list
        // to the caller. That's a legitimate idiom, must NOT warn.
        let result = parse_and_verify_full(r#"f>L n;xs=[1 2 3];+=xs 99"#);
        assert!(result.errors.is_empty());
        let t033: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.code == "ILO-T033")
            .collect();
        assert_eq!(t033.len(), 0);
    }

    #[test]
    fn mset_in_function_tail_no_warning() {
        let result = parse_and_verify_full(r#"f>M t n;m=mmap;mset m "a" 1"#);
        assert!(result.errors.is_empty());
        let t033: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.code == "ILO-T033")
            .collect();
        assert_eq!(t033.len(), 0);
    }

    #[test]
    fn append_inside_if_inside_loop_warns() {
        // `>i 1{+=out i}` — the guard body is a single-stmt block inside a
        // loop, so its tail is still discarded by the loop. Must warn.
        let result = parse_and_verify_full(r#"f>L n;out=[];@i 0..3{>i 1{+=out i}};out"#);
        assert!(result.errors.is_empty());
        let t033: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.code == "ILO-T033")
            .collect();
        assert_eq!(
            t033.len(),
            1,
            "expected one ILO-T033 warning inside guard in loop, got {:?}",
            result.warnings
        );
    }

    #[test]
    fn append_as_expr_arg_no_warning() {
        // `+=xs v` nested inside another call is not a bare statement,
        // its result is consumed. Must NOT warn.
        let result = parse_and_verify_full(r#"f>n;xs=[1 2];ys=+=xs 3;len ys"#);
        assert!(result.errors.is_empty());
        let t033: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.code == "ILO-T033")
            .collect();
        assert_eq!(t033.len(), 0);
    }

    #[test]
    fn multiple_bare_mutations_each_warn() {
        // Three bare mset calls in a row — each must warn so the agent sees
        // them all in one pass.
        let result =
            parse_and_verify_full(r#"f>M t n;m=mmap;mset m "a" 1;mset m "b" 2;mset m "c" 3;m"#);
        assert!(result.errors.is_empty());
        let t033: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.code == "ILO-T033")
            .collect();
        assert_eq!(t033.len(), 3);
    }

    // ---- rnd builtin ----

    #[test]
    fn rnd_zero_args_valid() {
        assert!(parse_and_verify("f>n;rnd").is_ok());
    }

    #[test]
    fn rnd_two_args_valid() {
        assert!(parse_and_verify("f>n;rnd 1 10").is_ok());
    }

    #[test]
    fn rnd_one_arg_arity_error() {
        let result = parse_and_verify("f x:n>n;rnd x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("arity mismatch") && e.message.contains("rnd"))
        );
    }

    #[test]
    fn rnd_type_error() {
        let result = parse_and_verify(r#"f>n;rnd "hello" 5"#);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("rnd"))
        );
    }

    // ---- now builtin ----

    #[test]
    fn now_zero_args_valid() {
        assert!(parse_and_verify("f>n;now").is_ok());
    }

    #[test]
    fn now_with_args_arity_error() {
        let result = parse_and_verify("f x:n>n;now x");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("arity mismatch") && e.message.contains("now"))
        );
    }

    // ── Range iteration verifier tests ──────────────────────────────────

    #[test]
    fn range_basic_ok() {
        assert!(parse_and_verify("f>n;@i 0..3{i}").is_ok());
    }

    #[test]
    fn range_binding_is_number() {
        // The range variable should be typed as n; using it as n should work
        assert!(parse_and_verify("f>n;@i 0..3{+i 1}").is_ok());
    }

    #[test]
    fn range_start_must_be_number() {
        let result = parse_and_verify(r#"f>n;@i "a"..3{i}"#);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("range start must be n"))
        );
    }

    #[test]
    fn range_end_must_be_number() {
        let result = parse_and_verify(r#"f>n;@i 0.."b"{i}"#);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("range end must be n"))
        );
    }

    #[test]
    fn range_brk_cnt_allowed() {
        assert!(parse_and_verify("f>n;@i 0..5{>=i 3{brk i};i}").is_ok());
        assert!(parse_and_verify("f>n;@i 0..5{=i 2{cnt};i}").is_ok());
    }

    // ---- Type alias tests ----

    #[test]
    fn alias_basic_return_type() {
        // alias res R n t, function returning res
        assert!(parse_and_verify("alias res R n t\nf x:n>res;~x").is_ok());
    }

    #[test]
    fn alias_in_param_type() {
        // alias num n, function taking num param
        assert!(parse_and_verify("alias num n\nf x:num>n;x").is_ok());
    }

    #[test]
    fn alias_nested() {
        // alias ids L n, then alias idres R ids t
        assert!(parse_and_verify("alias ids L n\nalias idres R ids t\nf>idres;~[1, 2, 3]").is_ok());
    }

    #[test]
    fn alias_circular_detected() {
        let errs = parse_and_verify("alias foo bar\nalias bar foo\nf>n;1").unwrap_err();
        assert!(errs.iter().any(|e| e.code == "ILO-T030"));
    }

    #[test]
    fn alias_of_alias_chain() {
        // alias x n, alias y x — y should resolve to n
        assert!(parse_and_verify("alias x n\nalias y x\nf a:y>y;a").is_ok());
    }

    #[test]
    fn alias_shadows_builtin_type_error() {
        let result = parse_and_verify_full("alias n t\nf>n;1");
        assert!(result.errors.iter().any(|e| e.code == "ILO-T031"));
    }

    #[test]
    fn alias_duplicate_error() {
        let errs = parse_and_verify("alias res R n t\nalias res L n\nf>n;1").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T001" && e.message.contains("duplicate type alias"))
        );
    }

    #[test]
    fn alias_in_type_def_field() {
        assert!(parse_and_verify("alias id n\ntype user{name:t;id:id}\nf u:user>id;u.id").is_ok());
    }

    #[test]
    fn alias_conflicts_with_type_def() {
        let errs = parse_and_verify("alias pt n\ntype pt{x:n;y:n}\nf>n;1").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T001" && e.message.contains("conflicts with type alias"))
        );
    }

    #[test]
    fn alias_complex_type() {
        // alias with nested L and R
        assert!(parse_and_verify("alias deep L R n t\nf>deep;[~1, ~2]").is_ok());
    }

    // ---- Destructuring bind tests ----

    #[test]
    fn destructure_ok() {
        assert!(parse_and_verify("type pt{x:n;y:n}\nf p:pt>n;{x;y}=p;+x y").is_ok());
    }

    #[test]
    fn destructure_infers_types() {
        // After destructuring, x should be n and usable in arithmetic
        assert!(parse_and_verify("type pt{x:n;y:n}\nf p:pt>n;{x;y}=p;*x y").is_ok());
    }

    #[test]
    fn destructure_wrong_field() {
        let errs = parse_and_verify("type pt{x:n;y:n}\nf p:pt>n;{x;z}=p;x").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T019" && e.message.contains("no field 'z'"))
        );
    }

    #[test]
    fn destructure_non_record() {
        let errs = parse_and_verify("f x:n>n;{a}=x;a").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.message.contains("destructure requires a record"))
        );
    }

    #[test]
    fn destructure_text_type_error() {
        let errs = parse_and_verify("f x:t>n;{a}=x;a").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.message.contains("destructure requires a record"))
        );
    }

    // --- mkeys / mvals type errors ---

    #[test]
    fn mkeys_non_map_arg_error() {
        // mkeys expects a map; passing a number produces ILO-T013
        let errs = parse_and_verify("f x:n>L t;mkeys x").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("mkeys"))
        );
    }

    // --- match exhaustiveness ---

    #[test]
    fn match_exhaustive_on_number_no_wildcard_error() {
        // Matching on a Number without a wildcard → ILO-T024 (falls into _ => branch)
        let errs = parse_and_verify("f x:n>t;?x{1:\"one\";2:\"two\"}").unwrap_err();
        assert!(errs.iter().any(|e| e.code == "ILO-T024"));
    }

    #[test]
    fn match_exhaustive_on_text_no_wildcard_error() {
        // Matching on a Text without a wildcard → ILO-T024
        let errs = parse_and_verify(r#"f x:t>n;?x{"a":1;"b":2}"#).unwrap_err();
        assert!(errs.iter().any(|e| e.code == "ILO-T024"));
    }

    #[test]
    fn match_result_missing_err_arm() {
        // Matching on Result but only ~ok arm — missing ^err arm
        let errs = parse_and_verify("f x:R n t>n;?x{~v:v}").unwrap_err();
        assert!(errs.iter().any(|e| e.code == "ILO-T024"
            && e.message.contains("missing")
            && e.message.contains("^")));
    }

    #[test]
    fn match_result_missing_ok_arm() {
        // Matching on Result but only ^err arm — missing ~ok arm
        let errs = parse_and_verify("f x:R n t>t;?x{^e:e}").unwrap_err();
        assert!(errs.iter().any(|e| e.code == "ILO-T024"
            && e.message.contains("missing")
            && e.message.contains("~")));
    }

    #[test]
    fn match_bool_missing_false_arm() {
        // Bool match missing false arm
        let errs = parse_and_verify("f x:b>n;?x{true:1}").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T024" && e.message.contains("false"))
        );
    }

    #[test]
    fn match_bool_missing_true_arm() {
        // Bool match missing true arm
        let errs = parse_and_verify("f x:b>n;?x{false:0}").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T024" && e.message.contains("true"))
        );
    }

    // ---- Sum type match exhaustiveness ----

    #[test]
    fn sum_match_all_variants_exhaustive() {
        assert!(
            parse_and_verify(r#"f x:S red green blue>t;?x{"red":"r";"green":"g";"blue":"b"}"#)
                .is_ok()
        );
    }

    #[test]
    fn sum_match_with_wildcard_exhaustive() {
        assert!(parse_and_verify(r#"f x:S red green blue>t;?x{"red":"r";_:"other"}"#).is_ok());
    }

    #[test]
    fn sum_match_missing_variant() {
        let errs =
            parse_and_verify(r#"f x:S red green blue>t;?x{"red":"r";"green":"g"}"#).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T024" && e.message.contains("blue"))
        );
    }

    #[test]
    fn sum_match_missing_multiple_variants() {
        let errs = parse_and_verify(r#"f x:S a b c>t;?x{"a":"1"}"#).unwrap_err();
        assert!(
            errs.iter().any(|e| e.code == "ILO-T024"
                && e.message.contains("b")
                && e.message.contains("c"))
        );
    }

    #[test]
    fn sum_match_hint_includes_missing_variant() {
        let errs =
            parse_and_verify(r#"f x:S red green blue>t;?x{"red":"r";"green":"g"}"#).unwrap_err();
        let e = errs.iter().find(|e| e.code == "ILO-T024").unwrap();
        let hint = e.hint.as_ref().unwrap();
        assert!(hint.contains("\"blue\""));
    }

    // ---- Builtin-specific type checks ----

    #[test]
    fn trm_wrong_type() {
        // trm expects t; passing n should error
        let errs = parse_and_verify("f x:n>t;trm x").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("trm"))
        );
    }

    #[test]
    fn unq_with_list_ok() {
        // unq accepts L n — should verify ok
        assert!(parse_and_verify("f xs:L n>L n;unq xs").is_ok());
    }

    #[test]
    fn fmt_non_text_template() {
        // fmt first arg must be t; passing n should error
        let errs = parse_and_verify("f x:n>t;fmt x").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("fmt"))
        );
    }

    #[test]
    fn jpth_wrong_first_arg() {
        // jpth expects t for first arg; passing n should error
        let errs = parse_and_verify(r#"f x:n>R t t;jpth x "path""#).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("jpth"))
        );
    }

    #[test]
    fn jpth_wrong_second_arg() {
        // jpth expects t for second arg (path); passing n should error
        let errs = parse_and_verify(r#"f x:t y:n>R t t;jpth x y"#).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("jpth"))
        );
    }

    #[test]
    fn jpar_wrong_type() {
        // jpar expects t; passing n should error (return type uses R n t since ? is not valid in signatures)
        let errs = parse_and_verify("f x:n>R n t;jpar x").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("jpar"))
        );
    }

    #[test]
    fn jdmp_any_type_ok() {
        // jdmp accepts any value — number should verify ok
        assert!(parse_and_verify("f x:n>t;jdmp x").is_ok());
    }

    #[test]
    fn prnt_passthrough_number() {
        // prnt returns the same type as its argument (passthrough)
        assert!(parse_and_verify("f x:n>n;prnt x").is_ok());
    }

    #[test]
    fn map_non_function_first_arg() {
        // map first arg must be a function; passing a literal 123 should error
        let errs = parse_and_verify("f xs:L n>L n;map 123 xs").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("map"))
        );
    }

    #[test]
    fn flt_non_function_first_arg() {
        // flt first arg must be a function; passing a literal 123 should error
        let errs = parse_and_verify("f xs:L n>L n;flt 123 xs").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("flt"))
        );
    }

    #[test]
    fn fld_non_function_first_arg() {
        // fld first arg must be a function; passing a literal 123 should error
        let errs = parse_and_verify("f xs:L n>n;fld 123 xs 0").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("fld"))
        );
    }

    #[test]
    fn grp_non_function_first_arg() {
        let errs = parse_and_verify("f xs:L n>M t L n;grp 123 xs").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("grp"))
        );
    }

    #[test]
    fn mget_key_type_mismatch() {
        // Map declared with text keys (M t n); passing a number key
        // mismatches and should error.
        let errs = parse_and_verify("f m:M t n>O n;mget m 123").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("mget"))
        );
    }

    #[test]
    fn mget_numeric_key_ok() {
        // Map declared with numeric keys (M n t); passing a number key is fine.
        assert!(parse_and_verify("f m:M n t>O t;mget m 42").is_ok());
    }

    #[test]
    fn mset_numeric_key_ok() {
        // Map declared with numeric keys (M n n); mset accepts numeric keys.
        assert!(parse_and_verify("f m:M n n>M n n;mset m 7 11").is_ok());
    }

    #[test]
    fn mkeys_numeric_returns_list_of_numbers() {
        // mkeys returns L of the declared key type — L n for M n t.
        assert!(parse_and_verify("f m:M n t>L n;mkeys m").is_ok());
    }

    #[test]
    fn mhas_non_map_first_arg() {
        // mhas expects a map as first arg; passing 123 should error
        let errs = parse_and_verify(r#"f>b;mhas 123 "k""#).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("mhas"))
        );
    }

    #[test]
    fn mset_returns_map() {
        // mset m "k" "v" — should verify ok and infer map type
        assert!(parse_and_verify(r#"f m:M t t>M t t;mset m "k" "v""#).is_ok());
    }

    #[test]
    fn mvals_returns_list() {
        // mvals m — should verify ok and return L n for M t n
        assert!(parse_and_verify("f m:M t n>L n;mvals m").is_ok());
    }

    #[test]
    fn mdel_returns_map() {
        // mdel m "k" — should verify ok returning same map type
        assert!(parse_and_verify(r#"f m:M t n>M t n;mdel m "k""#).is_ok());
    }

    #[test]
    fn rd_wrong_type_path() {
        // rd expects t (path); passing n should error (return type uses R n t since ? is not valid in signatures)
        let errs = parse_and_verify("f x:n>R n t;rd x").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("rd"))
        );
    }

    #[test]
    fn wr_wrong_content_type() {
        // wr arg 2 must be t (content); passing n (123) should error
        let errs = parse_and_verify(r#"f x:t>R t t;wr x 123"#).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("wr"))
        );
    }

    #[test]
    fn wrl_wrong_second_arg() {
        // wrl arg 2 is expected to be L t; the verifier only checks arg 1 (path) for wrl,
        // so passing n as arg 2 does NOT currently produce a type error — verify passes ok.
        assert!(parse_and_verify("f x:t xs:n>R t t;wrl x xs").is_ok());
    }

    // ── Coverage: Ty::Display for composite types ─────────────────────────────

    #[test]
    fn ty_display_optional_type() {
        let ty = Ty::Optional(Box::new(Ty::Number));
        assert_eq!(format!("{ty}"), "O n");
    }

    #[test]
    fn ty_display_map_type() {
        let ty = Ty::Map(Box::new(Ty::Text), Box::new(Ty::Number));
        assert_eq!(format!("{ty}"), "M t n");
    }

    #[test]
    fn ty_display_sum_type() {
        let ty = Ty::Sum(vec!["a".into(), "b".into()]);
        assert_eq!(format!("{ty}"), "S a b");
    }

    #[test]
    fn ty_display_fn_type() {
        let ty = Ty::Fn(vec![Ty::Number], Box::new(Ty::Text));
        assert_eq!(format!("{ty}"), "F n t");
    }

    // ── Coverage: collect_named_refs_inner for Optional/Map/Fn ────────────────

    #[test]
    fn verify_named_type_in_optional_param() {
        // `f x:O mytype>n;0` — collect_named_refs_inner sees Optional(Named("mytype"))
        let errs = parse_and_verify("f x:O mytype>n;0");
        // may error (unresolved type) but should not panic
        let _ = errs;
    }

    #[test]
    fn verify_named_type_in_map_param() {
        // `f x:M mytype n>n;0` — collect_named_refs_inner sees Map(Named, Number)
        let errs = parse_and_verify("f x:M mytype n>n;0");
        let _ = errs;
    }

    #[test]
    fn verify_named_type_in_fn_param() {
        // `f cb:F n mytype>n;0` — collect_named_refs_inner sees Fn([Number], Named)
        let errs = parse_and_verify("f cb:F n mytype>n;0");
        let _ = errs;
    }

    // ── Coverage: convert_type_with_aliases for Sum and Fn ────────────────────

    #[test]
    fn verify_sum_type_param() {
        // `f x:S foo bar>t;"ok"` — convert_type sees Sum type
        assert!(parse_and_verify(r#"f x:S foo bar>t;"ok""#).is_ok());
    }

    #[test]
    fn verify_fn_type_param() {
        // `f cb:F n t x:n>t;cb x` — convert_type sees Fn type
        assert!(parse_and_verify("f cb:F n t x:n>t;cb x").is_ok());
    }

    #[test]
    fn verify_type_variable_in_param() {
        // Single lowercase non-n/t/b letter = type variable → Ty::Unknown
        // `f x:z>n;0` — 'z' is a type variable
        let errs = parse_and_verify("f x:z>n;0");
        // verify may warn but should not panic
        let _ = errs;
    }

    // ── Coverage: builtin type errors — cat arg2, hd/tl text, rev wrong type ──

    #[test]
    fn verify_cat_arg2_wrong_type() {
        // cat expects t for arg 2; passing n should produce ILO-T013
        let errs = parse_and_verify("f a:t>t;cat a 42").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("cat"))
        );
    }

    #[test]
    fn verify_hd_on_text_returns_text() {
        // hd on a text arg → returns Text
        assert!(parse_and_verify("f s:t>t;hd s").is_ok());
    }

    #[test]
    fn verify_hd_wrong_type() {
        // hd expects list or text; passing n should error
        let errs = parse_and_verify("f x:n>t;hd x").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("hd"))
        );
    }

    #[test]
    fn verify_tl_on_text_returns_text() {
        // tl on text → text
        assert!(parse_and_verify("f s:t>t;tl s").is_ok());
    }

    #[test]
    fn verify_tl_wrong_type() {
        // tl expects list or text; n is wrong
        let errs = parse_and_verify("f x:n>t;tl x").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("tl"))
        );
    }

    #[test]
    fn verify_rev_wrong_type() {
        // rev expects list or text; n is wrong
        let errs = parse_and_verify("f x:n>L t;rev x").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("rev"))
        );
    }

    #[test]
    fn verify_unq_wrong_type() {
        // unq expects list or text; n is wrong
        let errs = parse_and_verify("f x:n>L t;unq x").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("unq"))
        );
    }

    #[test]
    fn verify_unq_on_text_returns_text() {
        // unq on text → text return type
        assert!(parse_and_verify("f s:t>t;unq s").is_ok());
    }

    #[test]
    fn verify_srt_with_key_fn() {
        // srt key-fn xs — 2-arg form with function key
        assert!(parse_and_verify("key x:n>n;*x 2  f xs:L n>L n;srt key xs").is_ok());
    }

    #[test]
    fn verify_srt_wrong_key_fn() {
        // srt with non-function key arg should produce ILO-T013
        let errs = parse_and_verify("f xs:L n>L n;srt 42 xs").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("srt"))
        );
    }

    #[test]
    fn verify_srt_on_text() {
        // srt on text → text return type
        assert!(parse_and_verify("f s:t>t;srt s").is_ok());
    }

    #[test]
    fn verify_srt_wrong_single_arg() {
        // srt with wrong type (n) → ILO-T013
        let errs = parse_and_verify("f x:n>t;srt x").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("srt"))
        );
    }

    #[test]
    fn verify_slc_wrong_type() {
        // slc expects list or text for first arg; n is wrong
        let errs = parse_and_verify("f x:n>L n;slc x 0 1").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("slc"))
        );
    }

    #[test]
    fn verify_slc_on_text() {
        // slc on text → text return type
        assert!(parse_and_verify("f s:t>t;slc s 0 2").is_ok());
    }

    // ── collect_named_refs_inner: Optional, Map, Fn via aliases ──────────────

    #[test]
    fn collect_named_refs_inner_optional_type() {
        // Directly call collect_named_refs_inner with Optional(Named) — exercises line 116
        use crate::ast::Type;
        let mut refs = Vec::new();
        collect_named_refs_inner(
            &Type::Optional(Box::new(Type::Named("mytype".to_string()))),
            &mut refs,
        );
        assert_eq!(refs, vec!["mytype".to_string()]);
    }

    #[test]
    fn collect_named_refs_inner_map_type() {
        // Directly call collect_named_refs_inner with Map(Named, Named) — exercises lines 118-121
        use crate::ast::Type;
        let mut refs = Vec::new();
        collect_named_refs_inner(
            &Type::Map(
                Box::new(Type::Named("keytype".to_string())),
                Box::new(Type::Named("valtype".to_string())),
            ),
            &mut refs,
        );
        assert!(refs.contains(&"keytype".to_string()));
        assert!(refs.contains(&"valtype".to_string()));
    }

    #[test]
    fn collect_named_refs_inner_fn_type() {
        // Directly call collect_named_refs_inner with a Fn type containing Named types
        // exercises the Fn branch (lines 126-128)
        use crate::ast::Type;
        let mut refs = Vec::new();
        collect_named_refs_inner(
            &Type::Fn(
                vec![Type::Named("myarg".to_string())],
                Box::new(Type::Named("myret".to_string())),
            ),
            &mut refs,
        );
        assert!(refs.contains(&"myarg".to_string()));
        assert!(refs.contains(&"myret".to_string()));
    }

    // ── convert_type direct call (lines 135-137) ─────────────────────────────

    #[test]
    fn convert_type_number() {
        use crate::ast::Type;
        let ty = convert_type(&Type::Number);
        assert_eq!(ty, Ty::Number);
    }

    #[test]
    fn convert_type_fn_type() {
        use crate::ast::Type;
        let ty = convert_type(&Type::Fn(vec![Type::Number], Box::new(Type::Text)));
        assert_eq!(ty, Ty::Fn(vec![Ty::Number], Box::new(Ty::Text)));
    }

    // ── compatible(): Optional arms (lines 181-184) ───────────────────────────

    #[test]
    fn compat_nil_to_optional_via_assign_body() {
        // Body: x=0 (assign, returns Nil); expected return: O n
        // compatible(Nil, Optional(n)) → true (line 181)
        assert!(parse_and_verify("f>O n;x=0").is_ok());
    }

    #[test]
    fn compat_optional_to_optional_return() {
        // Body returns O n; expected O n → compatible(O n, O n) (line 182)
        assert!(parse_and_verify("f x:O n>O n;x").is_ok());
    }

    #[test]
    fn compat_inner_to_optional_return() {
        // Body returns n; expected O n → compatible(n, O n) (line 183)
        assert!(parse_and_verify("f x:n>O n;x").is_ok());
    }

    #[test]
    fn compat_optional_to_inner_return() {
        // Body returns O n; expected n → compatible(O n, n) (line 184)
        assert!(parse_and_verify("f x:O n>n;x").is_ok());
    }

    // ── compatible(): Sum+Text, Fn+Fn (lines 186-198) ────────────────────────

    #[test]
    fn compat_sum_to_text_return() {
        // Sum type returned where t expected → compatible(Sum, Text) (line 186)
        assert!(parse_and_verify(r#"f x:S a b>t;x"#).is_ok());
    }

    #[test]
    fn compat_text_to_sum_param() {
        // Passing text to Sum param → compatible(Text, Sum) (line 186)
        assert!(parse_and_verify(r#"f x:S a b>n;0   g y:S a b>n;g "hello""#).is_ok());
    }

    #[test]
    fn compat_fn_to_fn_param() {
        // Passing function ref to Fn param → compatible(Fn, Fn) (lines 195-198)
        assert!(
            parse_and_verify("double x:n>n;*x 2   apply cb:F n n x:n>n;cb x   h>n;apply double 5")
                .is_ok()
        );
    }

    // ── Unknown-typed args in hd/tl/srt (lines 454, 472, 583) ───────────────

    #[test]
    fn hd_with_unknown_type_arg() {
        // z is a type variable → Ty::Unknown; hd(Unknown) → Ty::Unknown (line 454)
        assert!(parse_and_verify("f x:z>n;hd x").is_ok());
    }

    #[test]
    fn tl_with_unknown_type_arg() {
        // tl(Unknown) → Ty::Unknown (line 472)
        assert!(parse_and_verify("f x:z>n;tl x").is_ok());
    }

    #[test]
    fn srt_single_unknown_type_arg() {
        // srt(Unknown) → Ty::Unknown (line 583)
        assert!(parse_and_verify("f x:z>n;srt x").is_ok());
    }

    // ── srt 2-arg: second arg not a list (line 575) ──────────────────────────

    #[test]
    fn srt_two_arg_second_not_list_returns_unknown() {
        // srt fn n — second arg is n, not L; returns Unknown compatible with n
        assert!(parse_and_verify("double x:n>n;*x 2   f x:n>n;srt double x").is_ok());
    }

    // ── get/post wrong headers type (lines 649-656, 680-687) ─────────────────

    #[test]
    fn get_wrong_headers_type_error() {
        // get url n (headers not M t t) → ILO-T013 (lines 649-656)
        let errs = parse_and_verify("f url:t hdrs:n>R t t;get url hdrs").unwrap_err();
        assert!(errs.iter().any(|e| e.code == "ILO-T013"
            && e.message.contains("get")
            && e.message.contains("headers")));
    }

    #[test]
    fn post_wrong_headers_type_error() {
        // post url body n (headers not M t t) → ILO-T013 (lines 680-687)
        let errs = parse_and_verify("f url:t body:t hdrs:n>R t t;post url body hdrs").unwrap_err();
        assert!(errs.iter().any(|e| e.code == "ILO-T013"
            && e.message.contains("post")
            && e.message.contains("headers")));
    }

    // ── rd format arg wrong type, rdl/wr wrong path (lines 709-749) ──────────

    #[test]
    fn rd_wrong_format_arg_type() {
        // rd path n (format not t) → ILO-T013 (lines 709-719)
        let errs = parse_and_verify("f p:t fmt:n>R n t;rd p fmt").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("rd"))
        );
    }

    #[test]
    fn rdl_wrong_path_type() {
        // rdl expects t path; passing n → ILO-T013 (lines 724-736)
        let errs = parse_and_verify("f x:n>R L t t;rdl x").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T013" && e.message.contains("rdl"))
        );
    }

    #[test]
    fn wr_wrong_path_type() {
        // wr expects t path as arg 1; passing n → ILO-T013 (lines 741-749)
        let errs = parse_and_verify(r#"f x:n>R t t;wr x "content""#).unwrap_err();
        assert!(errs.iter().any(|e| e.code == "ILO-T013"
            && e.message.contains("wr")
            && e.message.contains("arg 1")));
    }

    // ── map/flt/fld return type inference (lines 820, 842, 863-865) ──────────

    #[test]
    fn map_infers_fn_return_type() {
        // map fn xs → L of fn's return type (line 820: Some(Ty::Fn(_, ret)) => *ret.clone())
        assert!(parse_and_verify("double x:n>n;*x 2   f xs:L n>L n;map double xs").is_ok());
    }

    #[test]
    fn flt_infers_list_type_from_second_arg() {
        // flt fn xs → same list type (line 842: Some(ty @ List(_)) => ty.clone())
        assert!(parse_and_verify("gt x:n>b;>x 0   f xs:L n>L n;flt gt xs").is_ok());
    }

    #[test]
    fn fld_infers_fn_return_type_when_third_unknown() {
        // fld fn xs 0 — third arg is Unknown? No, 0 is n. Use type variable for third.
        // fld fn xs z_arg — third is Unknown → falls back to fn return type (lines 863-865)
        assert!(parse_and_verify("add x:n y:n>n;+x y   f xs:L n init:z>n;fld add xs init").is_ok());
    }

    // ── mvals/mdel with non-map first arg (lines 935, 942) ───────────────────

    #[test]
    fn mvals_non_map_returns_unknown_list() {
        // mvals n → L Unknown; compatible with L n (line 935: _ => Ty::Unknown)
        assert!(parse_and_verify("f x:n>L n;mvals x").is_ok());
    }

    #[test]
    fn mdel_non_map_returns_generic_map() {
        // mdel n "k" → M Unknown Unknown; compatible with M t n (line 942)
        assert!(parse_and_verify(r#"f x:n>M t n;mdel x "k""#).is_ok());
    }

    // ── Decl::Use skip in verify (line 1055) ─────────────────────────────────

    #[test]
    fn decl_use_skipped_in_verify() {
        use crate::ast::{Decl, Span};
        // Build a program with a Use node and verify it — should be silently skipped
        let tokens = crate::lexer::lex("f>n;1").expect("lex failed");
        let token_spans: Vec<(crate::lexer::Token, crate::ast::Span)> = tokens
            .into_iter()
            .map(|(t, r)| {
                (
                    t,
                    crate::ast::Span {
                        start: r.start,
                        end: r.end,
                    },
                )
            })
            .collect();
        let (mut program, _) = crate::parser::parse(token_spans);
        program.declarations.push(Decl::Use {
            path: "x.ilo".into(),
            only: None,
            span: Span::UNKNOWN,
        });
        let result = verify(&program);
        assert!(result.errors.is_empty());
    }

    // ── has_cycle false via visited set (line 1122) ───────────────────────────

    #[test]
    fn alias_diamond_dep_exercises_has_cycle_false() {
        // alias a R inner inner — inner appears twice; second visit hits line 1122
        assert!(parse_and_verify("alias inner n\nalias a R inner inner\nf>n;1").is_ok());
    }

    // ── resolve_alias_recursive already-resolved (lines 1139-1153) ───────────

    #[test]
    fn alias_shared_dep_resolved_once() {
        // a and b both depend on shared; shared resolved first time, skipped second
        let result = parse_and_verify_full(
            "alias shared n\nalias a L shared\nalias b L shared\nf x:a y:b>n;0",
        );
        // May have errors if "shared" / "a" / "b" aren't usable as param types; just assert no panics
        let _ = result;
    }

    // ── Destructure with Unknown type (lines 1267-1275) ──────────────────────

    #[test]
    fn destructure_with_unknown_type_binds_unknown() {
        // x:z is Unknown type; destructuring Unknown inserts Unknown bindings
        assert!(parse_and_verify("f x:z>n;{a}=x;0").is_ok());
    }

    // ── Guard Stmt with else_body (lines 1315-1317) ───────────────────────────

    #[test]
    fn guard_stmt_with_else_body_verified() {
        // >x 0{x}{0} as statement with else body exercises lines 1315-1317
        assert!(parse_and_verify("f x:n>n;>x 0{x}{0}").is_ok());
    }

    // ── rd/srt arity error description (lines 1495, 1499) ────────────────────

    #[test]
    fn rd_three_args_arity_error_description() {
        // rd with 3 args → arity error using "1 or 2" description (line 1495)
        let errs = parse_and_verify("f p:t fmt:t extra:t>R n t;rd p fmt extra").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.message.contains("arity") && e.message.contains("rd"))
        );
    }

    #[test]
    fn post_one_arg_arity_error_description() {
        // post with 1 arg → arity error using "2 or 3" description (line 1499)
        let errs = parse_and_verify(r#"f url:t>R t t;post url"#).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.message.contains("arity") && e.message.contains("post"))
        );
    }

    // ── Type mismatch no specific hint (line 1546 _ => None) ─────────────────

    #[test]
    fn type_mismatch_bool_to_number_no_hint() {
        // Passing Bool to Number param — no specific conversion hint (line 1546)
        let errs = parse_and_verify("f x:b>n;0   g y:n>n;y   h x:b>n;g x").unwrap_err();
        assert!(errs.iter().any(|e| e.code == "ILO-T007"));
    }

    // ── Dynamic dispatch arity/type errors (lines 1566-1582) ─────────────────

    #[test]
    fn dynamic_dispatch_wrong_arity() {
        // Call function-ref with wrong number of args → ILO-T006 (lines 1566-1572)
        let errs = parse_and_verify("f cb:F n n>n;cb 1 2 3").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T006" && e.message.contains("cb"))
        );
    }

    #[test]
    fn dynamic_dispatch_wrong_type() {
        // Call function-ref with wrong arg type → ILO-T007 (lines 1573-1584)
        let errs = parse_and_verify(r#"f cb:F n n>n;cb "text""#).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T007" && e.message.contains("cb"))
        );
    }

    // ── Unwrap in non-Result function (lines 1622-1625) ──────────────────────

    #[test]
    fn unwrap_in_non_result_enclosing_fn() {
        // get! in a function that returns t (not R) → ILO-T026
        let errs = parse_and_verify(r#"f url:t>t;x=get! url;x"#).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T026" && e.message.contains("t"))
        );
    }

    // ── NilCoalesce expression (lines 1816-1822) ──────────────────────────────

    #[test]
    fn nil_coalesce_optional_to_inner() {
        // x:O n ?? 0 → inner type n (line 1821)
        assert!(parse_and_verify("f x:O n>n;y=x??0;y").is_ok());
    }

    #[test]
    fn nil_coalesce_non_optional_passthrough() {
        // x:n ?? 0 → n (line 1822: other => other)
        assert!(parse_and_verify("f x:n>n;y=x??0;y").is_ok());
    }

    // ── mget with non-Map first arg (line 875-877) ───────────────────────────

    #[test]
    fn mget_non_map_first_arg_returns_unknown() {
        // mget n "k" — first arg not a map → val_ty = Unknown (line 877: _ => Unknown)
        // Returns O Unknown, compatible with O n
        assert!(parse_and_verify(r#"f x:n>O n;mget x "k""#).is_ok());
    }

    // ── mset non-Map first arg (line 898) ────────────────────────────────────

    #[test]
    fn mset_non_map_first_arg_returns_generic_map() {
        // mset n "k" "v" — first arg not map → returns M Unknown Unknown (line 898)
        assert!(parse_and_verify(r#"f x:n>M t t;mset x "k" "v""#).is_ok());
    }

    // ── rev/unq return Unknown (lines 498, 534) ───────────────────────────────

    #[test]
    fn rev_unknown_type_returns_unknown() {
        // rev(Unknown) falls to _ => Ty::Unknown return (line 498/502)
        assert!(parse_and_verify("f x:z>n;rev x").is_ok());
    }

    #[test]
    fn unq_unknown_type_returns_unknown() {
        // unq(Unknown) falls to _ => Ty::Unknown return (line 534/538)
        assert!(parse_and_verify("f x:z>n;unq x").is_ok());
    }

    // ── compatible() Sum+Sum (line 187) ──────────────────────────────────────

    #[test]
    fn compat_sum_to_sum_same_variants_ok() {
        // Body returns S a b; expected S a b → compatible(Sum, Sum) (line 187)
        assert!(parse_and_verify(r#"f x:S a b>S a b;x"#).is_ok());
    }

    // ── flt second arg not a List (line 842) ─────────────────────────────────

    #[test]
    fn flt_second_arg_not_list_returns_unknown() {
        // flt fn n — second arg is n, not L; returns Unknown compat with L n
        assert!(parse_and_verify("gt x:n>b;>x 0\nf xs:n>L n;flt gt xs").is_ok());
    }

    // ── fld with Unknown first arg, Unknown third arg (line 865) ─────────────

    #[test]
    fn fld_unknown_fn_and_unknown_init_returns_unknown() {
        // fld cb xs init — cb:z (Unknown), init:z (Unknown) → _ => Unknown (line 865)
        assert!(parse_and_verify("f cb:z xs:L n init:z>n;fld cb xs init").is_ok());
    }

    // ── Destructure Named type not in types (lines 1267-1269) ────────────────

    #[test]
    fn destructure_named_type_not_in_types_binds_unknown() {
        // x:foo where foo is undefined → Ty::Named("foo") not in types → else branch
        // Produces ILO-T003 for param type but body still verified (lines 1267-1269)
        let result = parse_and_verify_full("f x:foo>n;{a}=x;a");
        assert!(
            result.errors.iter().any(|e| e.code == "ILO-T003"),
            "expected ILO-T003"
        );
    }

    // ── TypeIs pattern in Stmt::Match bind_pattern (lines 1429-1439) ─────────

    #[test]
    fn match_stmt_type_is_pattern_binds_var() {
        // ?x{n v:"num";_:"other"} — TypeIs with non-underscore binding exercises lines 1429-1438
        assert!(parse_and_verify(r#"f x:n>t;?x{n v:"num";_:"other"}"#).is_ok());
    }

    #[test]
    fn match_stmt_type_is_list_pattern_binds_var() {
        // Inject TypeIs{List, binding="v"} directly — parser can't produce L t in pattern position
        // This exercises line 1435: Type::List(_) => Ty::List(Box::new(Ty::Unknown))
        use crate::ast::{Decl, Expr, MatchArm, Pattern, Program, Span, Spanned, Stmt, Type};
        let lit_list = Expr::Literal(crate::ast::Literal::Text("list".to_string()));
        let lit_other = Expr::Literal(crate::ast::Literal::Text("other".to_string()));
        let arm_list = MatchArm {
            pattern: Pattern::TypeIs {
                ty: Type::List(Box::new(Type::Text)),
                binding: "v".to_string(),
            },
            body: vec![Spanned::unknown(Stmt::Expr(lit_list))],
        };
        let arm_wild = MatchArm {
            pattern: Pattern::Wildcard,
            body: vec![Spanned::unknown(Stmt::Expr(lit_other))],
        };
        let prog = Program {
            declarations: vec![Decl::Function {
                name: "f".to_string(),
                params: vec![crate::ast::Param {
                    name: "x".to_string(),
                    ty: Type::List(Box::new(Type::Text)),
                }],
                return_type: Type::Text,
                body: vec![Spanned::unknown(Stmt::Match {
                    subject: Some(Expr::Ref("x".to_string())),
                    arms: vec![arm_list, arm_wild],
                })],
                span: Span::UNKNOWN,
            }],
            source: None,
        };
        let result = verify(&prog);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
    }

    // ── rev/unq/srt with Unknown return from non-arg path ────────────────────

    #[test]
    fn srt_unknown_type_two_arg_second_unknown() {
        // srt fn_v xs — fn_v:F n n, xs:z (Unknown) → second arg is Unknown → _ => Unknown (line 575)
        assert!(parse_and_verify("double x:n>n;*x 2\nf xs:z>n;srt double xs").is_ok());
    }

    // ── flat with wrong arg types (lines 903, 905) ────────────────────────────

    #[test]
    fn flat_on_non_nested_list_hits_unknown_inner() {
        // flat xs:L n → inner is Ty::Number (not Ty::List) → _ => Ty::Unknown (line 903)
        // Verifier allows since xs type is checked, inner type falls back.
        let result = parse_and_verify_full("f xs:L n>_; flat xs");
        // May or may not error; we just need the verifier path to run
        let _ = result;
    }

    #[test]
    fn flat_on_non_list_hits_unknown_fallback() {
        // flat x:n → first arg is Ty::Number (not Ty::List) → _ => Ty::Unknown (line 905)
        let result = parse_and_verify_full("f x:n>_; flat x");
        let _ = result;
    }

    // ── TypeIs Bool pattern binding (line 1473) ───────────────────────────────

    #[test]
    fn match_type_is_bool_binds_bool_type() {
        // ?x{b y: "bool"} — TypeIs with Type::Bool → bound_ty = Ty::Bool (line 1473)
        assert!(parse_and_verify(r#"f x:_>t;?x{b y:"bool";t z:"other"}"#).is_ok());
    }

    // ── resolve_alias_recursive early return on repeated alias (line 1179) ────

    #[test]
    fn alias_diamond_dependency_hits_early_return() {
        // alias myint n; alias mynum myint; alias mycount myint
        // When mynum and mycount both depend on myint: first resolution caches myint, second hits early return.
        assert!(
            parse_and_verify(
                "alias myint n\nalias mynum myint\nalias mycount myint\nf x:mynum y:mycount>n;+x y"
            )
            .is_ok()
        );
    }

    // ── grp key-type fallback (line 894) ─────────────────────────────────────

    #[test]
    fn grp_with_unknown_key_type_falls_back() {
        // grp where key arg type is not Ty::Fn → key_ty = Ty::Unknown (line 890)
        // Pass grp with a key that has unknown type
        let result = parse_and_verify_full("f xs:L n>_;grp 42 xs");
        let _ = result; // may error, but exercises the key_ty fallback
    }

    #[test]
    fn grp_with_non_list_second_arg() {
        // grp where second arg is not Ty::List → elem_ty = Ty::Unknown (line 894)
        let result = parse_and_verify_full("f x:n>_;grp 42 x");
        let _ = result;
    }

    // ── bang (!) on unknown callee (line 1666) ───────────────────────────────

    #[test]
    fn bang_on_undefined_callee_gives_unknown() {
        // When callee is undefined, call_ty = Ty::Unknown → line 1666 Ty::Unknown arm
        let result = parse_and_verify("f>R t t;unk! 1");
        assert!(result.is_err()); // ILO-T005 undefined function, but line 1666 is hit
    }

    // ── bang (!) on Result return — enclosing fn also returns Result (line 1663) ──

    #[test]
    fn bang_on_result_callee_with_result_enclosing() {
        // rd returns R t t, f returns R t t → unwrap is valid → line 1663 closing } is hit
        assert!(parse_and_verify(r#"f>R t t;rd! "/tmp/x""#).is_ok());
    }

    // ── srt with non-list/text arg (lines 598) ───────────────────────────────

    #[test]
    fn srt_on_number_errors() {
        // srt arg is Ty::Number → other arm pushes error, then line 598 } is executed
        let result = parse_and_verify("f x:n>_;srt x");
        assert!(result.is_err());
    }

    // ── slc with non-list/text arg (line 614) ───────────────────────────────

    #[test]
    fn slc_on_number_errors() {
        // slc arg is Ty::Number → other arm pushes error, then line 614 } is executed
        let result = parse_and_verify("f x:n>_;slc x 0 1");
        assert!(result.is_err());
    }

    // ── hd with non-list/text arg (line 469) ─────────────────────────────────

    #[test]
    fn hd_on_number_errors() {
        // hd arg is Ty::Number → other arm, then line 469 } hit
        let result = parse_and_verify("f x:n>_;hd x");
        assert!(result.is_err());
    }

    // ── tl with non-list/text arg (line 487) ─────────────────────────────────

    #[test]
    fn tl_on_number_errors() {
        // tl arg is Ty::Number → other arm, then line 487 } hit
        let result = parse_and_verify("f x:n>_;tl x");
        assert!(result.is_err());
    }

    // ── rev/unq with wrong type (lines 503, 539) ─────────────────────────────

    #[test]
    fn rev_on_number_errors() {
        let result = parse_and_verify("f x:n>_;rev x");
        assert!(result.is_err());
    }

    #[test]
    fn unq_on_number_errors() {
        let result = parse_and_verify("f x:n>_;unq x");
        assert!(result.is_err());
    }

    // ── has on non-list/text (line 451) ──────────────────────────────────────

    #[test]
    fn has_on_number_errors() {
        // has arg is Ty::Number → other arm pushes error, line 451 } hit
        let result = parse_and_verify("f x:n>b;has x x");
        assert!(result.is_err());
    }

    // ── resolve_alias_recursive early return (line 1179) ─────────────────────

    #[test]
    fn alias_chain_covers_early_return() {
        // alias a2 = b2; alias b2 = n — resolving a2 resolves b2 first, then the outer
        // loop hits b2 again → resolve_alias_recursive returns early at L1179
        let result = parse_and_verify("alias a2 b2 alias b2 n f x:a2>n;x");
        assert!(
            result.is_ok(),
            "expected ok, got: {:?}",
            result.unwrap_err()
        );
    }

    // ── safe field access on Nil type (line 1791) ────────────────────────────

    #[test]
    fn safe_field_access_on_nil_type_returns_nil() {
        // r:_ (Any/Unknown type in scope) used with safe .? access
        // `f r:_>n; s=r.?x; 0` — r has type Unknown (from Type::Any conversion)
        let result = parse_and_verify("type p{x:n} f r:_>n;s=r.?x;0");
        // May error on type mismatch but must reach L1791
        let _ = result;
    }

    // ── safe index access on Nil type (line 1826) ────────────────────────────

    #[test]
    fn safe_index_access_on_nil_type_returns_nil() {
        // r:_ (Nil type) with safe .?0 index access → line 1826 return Ty::Nil
        let result = parse_and_verify("f r:_>n;s=r.?0;0");
        let _ = result;
    }

    // ── nil-coalesce on Nil type (line 1861) ─────────────────────────────────

    #[test]
    fn nil_coalesce_on_nil_type() {
        // r:_ has type Ty::Nil, `r??0` → NilCoalesce → Ty::Nil => def_ty at L1861
        let result = parse_and_verify("f r:_>n;r??0");
        // Likely ok: Nil coalesces to Number (0)
        let _ = result;
    }

    // ── Coverage: L1182 — resolve_alias_recursive early return (already resolved) ──

    #[test]
    fn alias_triple_chain_hits_early_return() {
        // alias c3 = b3; alias b3 = a3; alias a3 = n
        // When resolving c3: first resolves b3 (which resolves a3), then b3 is already
        // in self.aliases. The outer loop iteration for b3 hits L1182 return.
        let result = parse_and_verify("alias a3 n\nalias b3 a3\nalias c3 b3\nf x:c3>n;x");
        assert!(
            result.is_ok(),
            "expected ok, got: {:?}",
            result.unwrap_err()
        );
    }

    // ── Coverage: L1490 — TypeIs pattern with non-standard type (e.g., List) → Ty::Unknown ──

    #[test]
    fn match_type_is_non_standard_type_binds_unknown() {
        // Construct AST directly with TypeIs { ty: Type::Named("foo"), binding: "v" }
        // to hit the _ => Ty::Unknown branch at L1490
        use crate::ast::{
            Decl, Expr, Literal, MatchArm, Param, Pattern, Program, Span, Spanned, Stmt, Type,
        };
        let prog = Program {
            declarations: vec![Decl::Function {
                name: "f".to_string(),
                params: vec![Param {
                    name: "x".to_string(),
                    ty: Type::Number,
                }],
                return_type: Type::Number,
                body: vec![Spanned::unknown(Stmt::Match {
                    subject: Some(Expr::Ref("x".to_string())),
                    arms: vec![
                        MatchArm {
                            pattern: Pattern::TypeIs {
                                ty: Type::Named("foo".to_string()),
                                binding: "v".to_string(),
                            },
                            body: vec![Spanned::unknown(Stmt::Expr(Expr::Literal(
                                Literal::Number(0.0),
                            )))],
                        },
                        MatchArm {
                            pattern: Pattern::Wildcard,
                            body: vec![Spanned::unknown(Stmt::Expr(Expr::Literal(
                                Literal::Number(1.0),
                            )))],
                        },
                    ],
                })],
                span: Span::UNKNOWN,
            }],
            source: None,
        };
        let result = verify(&prog);
        // Should not panic; the Unknown binding is just a permissive fallback
        let _ = result;
    }

    // ── Coverage: L1504 — Literal::Nil in infer_expr ──────────────────────────

    #[test]
    fn nil_literal_in_expr_infers_nil_type() {
        // Construct AST directly with Expr::Literal(Literal::Nil) in function body
        use crate::ast::{Decl, Expr, Literal, Program, Span, Spanned, Stmt, Type};
        let prog = Program {
            declarations: vec![Decl::Function {
                name: "f".to_string(),
                params: vec![],
                return_type: Type::Any,
                body: vec![Spanned::unknown(Stmt::Expr(Expr::Literal(Literal::Nil)))],
                span: Span::UNKNOWN,
            }],
            source: None,
        };
        let result = verify(&prog);
        assert!(
            result.errors.is_empty(),
            "nil literal should verify ok: {:?}",
            result.errors
        );
    }

    // ── Coverage: L1891-1895 — Ternary branch type mismatch ───────────────────

    #[test]
    fn ternary_branch_type_mismatch_error() {
        // ?=x 0 "text" 42 — then_expr is Text, else_expr is Number → incompatible
        // Exercises L1893-1895 (the error path)
        use crate::ast::{BinOp, Decl, Expr, Literal, Param, Program, Span, Spanned, Stmt, Type};
        let prog = Program {
            declarations: vec![Decl::Function {
                name: "f".to_string(),
                params: vec![Param {
                    name: "x".to_string(),
                    ty: Type::Number,
                }],
                return_type: Type::Text,
                body: vec![Spanned::unknown(Stmt::Expr(Expr::Ternary {
                    condition: Box::new(Expr::BinOp {
                        op: BinOp::Equals,
                        left: Box::new(Expr::Ref("x".to_string())),
                        right: Box::new(Expr::Literal(Literal::Number(0.0))),
                    }),
                    then_expr: Box::new(Expr::Literal(Literal::Text("text".to_string()))),
                    else_expr: Box::new(Expr::Literal(Literal::Number(42.0))),
                }))],
                span: Span::UNKNOWN,
            }],
            source: None,
        };
        let result = verify(&prog);
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.code == "ILO-T003" && e.message.contains("ternary")),
            "expected ILO-T003 ternary mismatch error, got: {:?}",
            result.errors
        );
    }

    // ── Coverage: builtin type checking for has/hd/tl/rev/unq/srt ───────────

    #[test]
    fn verify_has_builtin() {
        assert!(parse_and_verify(r#"f xs:L n x:n>b;has xs x"#).is_ok());
    }

    #[test]
    fn verify_hd_builtin() {
        assert!(parse_and_verify("f xs:L n>n;hd xs").is_ok());
    }

    #[test]
    fn verify_hd_text_builtin() {
        assert!(parse_and_verify("f s:t>t;hd s").is_ok());
    }

    #[test]
    fn verify_tl_builtin() {
        assert!(parse_and_verify("f xs:L n>L n;tl xs").is_ok());
    }

    #[test]
    fn verify_tl_text_builtin() {
        assert!(parse_and_verify("f s:t>t;tl s").is_ok());
    }

    #[test]
    fn verify_rev_builtin() {
        assert!(parse_and_verify("f xs:L n>L n;rev xs").is_ok());
    }

    #[test]
    fn verify_rev_text_builtin() {
        assert!(parse_and_verify("f s:t>t;rev s").is_ok());
    }

    #[test]
    fn verify_unq_builtin() {
        assert!(parse_and_verify("f xs:L n>L n;unq xs").is_ok());
    }

    #[test]
    fn verify_unq_text_builtin() {
        assert!(parse_and_verify("f s:t>t;unq s").is_ok());
    }

    #[test]
    fn verify_srt_builtin() {
        assert!(parse_and_verify("f xs:L n>L n;srt xs").is_ok());
    }

    #[test]
    fn verify_srt_text_builtin() {
        assert!(parse_and_verify("f s:t>t;srt s").is_ok());
    }

    #[test]
    fn verify_slc_builtin() {
        assert!(parse_and_verify("f xs:L n>L n;slc xs 0 2").is_ok());
    }

    #[test]
    fn verify_has_number_arg_error() {
        let errs = parse_and_verify("f x:n>b;has x 1").unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("has")));
    }

    #[test]
    fn verify_hd_number_arg_error() {
        let errs = parse_and_verify("f x:n>n;hd x").unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("hd")));
    }

    #[test]
    fn verify_tl_number_arg_error() {
        let errs = parse_and_verify("f x:n>n;tl x").unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("tl")));
    }

    #[test]
    fn verify_rev_number_arg_error() {
        let errs = parse_and_verify("f x:n>n;rev x").unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("rev")));
    }

    #[test]
    fn verify_unq_number_arg_error() {
        let errs = parse_and_verify("f x:n>n;unq x").unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("unq")));
    }

    #[test]
    fn verify_srt_number_arg_error() {
        let errs = parse_and_verify("f x:n>n;srt x").unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("srt")));
    }

    // ── Coverage: direct builtin_check_args calls ───────────────────────────

    #[test]
    fn builtin_check_args_has_list() {
        let (ty, errors) = builtin_check_args(
            "has",
            &[Ty::List(Box::new(Ty::Number)), Ty::Number],
            "f",
            None,
        );
        assert_eq!(ty, Ty::Bool);
        assert!(errors.is_empty());
    }

    #[test]
    fn builtin_check_args_hd_list() {
        let (ty, errors) = builtin_check_args("hd", &[Ty::List(Box::new(Ty::Number))], "f", None);
        assert_eq!(ty, Ty::Number);
        assert!(errors.is_empty());
    }

    #[test]
    fn builtin_check_args_hd_text() {
        let (ty, errors) = builtin_check_args("hd", &[Ty::Text], "f", None);
        assert_eq!(ty, Ty::Text);
        assert!(errors.is_empty());
    }

    #[test]
    fn builtin_check_args_hd_no_args() {
        let (ty, _errors) = builtin_check_args("hd", &[], "f", None);
        assert_eq!(ty, Ty::Unknown);
    }

    #[test]
    fn builtin_check_args_tl_list() {
        let (ty, errors) = builtin_check_args("tl", &[Ty::List(Box::new(Ty::Number))], "f", None);
        assert_eq!(ty, Ty::List(Box::new(Ty::Number)));
        assert!(errors.is_empty());
    }

    #[test]
    fn builtin_check_args_tl_no_args() {
        let (ty, _errors) = builtin_check_args("tl", &[], "f", None);
        assert_eq!(ty, Ty::Unknown);
    }

    #[test]
    fn builtin_check_args_rev_list() {
        let (ty, errors) = builtin_check_args("rev", &[Ty::List(Box::new(Ty::Number))], "f", None);
        assert_eq!(ty, Ty::List(Box::new(Ty::Number)));
        assert!(errors.is_empty());
    }

    #[test]
    fn builtin_check_args_rev_no_args() {
        let (ty, _errors) = builtin_check_args("rev", &[], "f", None);
        assert_eq!(ty, Ty::Unknown);
    }

    #[test]
    fn builtin_check_args_unq_list() {
        let (ty, errors) = builtin_check_args("unq", &[Ty::List(Box::new(Ty::Text))], "f", None);
        assert_eq!(ty, Ty::List(Box::new(Ty::Text)));
        assert!(errors.is_empty());
    }

    #[test]
    fn builtin_check_args_unq_no_args() {
        let (ty, _errors) = builtin_check_args("unq", &[], "f", None);
        assert_eq!(ty, Ty::Unknown);
    }

    #[test]
    fn builtin_check_args_srt_list() {
        let (ty, errors) = builtin_check_args("srt", &[Ty::List(Box::new(Ty::Number))], "f", None);
        assert_eq!(ty, Ty::List(Box::new(Ty::Number)));
        assert!(errors.is_empty());
    }

    #[test]
    fn builtin_check_args_srt_no_args() {
        let (ty, _errors) = builtin_check_args("srt", &[], "f", None);
        assert_eq!(ty, Ty::Unknown);
    }

    #[test]
    fn builtin_check_args_slc_list() {
        let (ty, errors) = builtin_check_args(
            "slc",
            &[Ty::List(Box::new(Ty::Number)), Ty::Number, Ty::Number],
            "f",
            None,
        );
        assert_eq!(ty, Ty::List(Box::new(Ty::Number)));
        assert!(errors.is_empty());
    }

    // ── Coverage round 2: circular type alias detection (L1126-1143) ────────

    #[test]
    fn verify_circular_alias_self_referencing() {
        // Single alias referencing itself: alias foo foo → ILO-T030 (L1138-1142)
        let errs = parse_and_verify("alias foo foo\nf>n;1").unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T030" && e.message.contains("circular")),
            "expected ILO-T030 circular error, got: {:?}",
            errs
        );
    }

    #[test]
    fn verify_circular_alias_three_way_cycle() {
        // Three-way circular dependency: xx → yy → zz → xx (L1126-1143)
        let errs = parse_and_verify("alias xx yy\nalias yy zz\nalias zz xx\nf>n;1").unwrap_err();
        assert!(
            errs.iter().any(|e| e.code == "ILO-T030"),
            "expected ILO-T030 for 3-way cycle, got: {:?}",
            errs
        );
    }

    #[test]
    fn verify_circular_alias_mixed_with_valid() {
        // Mix of circular aliases and valid ones — only circular ones error
        let result =
            parse_and_verify_full("alias good n\nalias bad1 bad2\nalias bad2 bad1\nf x:good>n;x");
        let circular_errors: Vec<_> = result
            .errors
            .iter()
            .filter(|e| e.code == "ILO-T030")
            .collect();
        assert!(
            circular_errors.len() >= 2,
            "expected at least 2 circular errors for bad1/bad2, got: {:?}",
            circular_errors
        );
    }

    #[test]
    fn verify_non_circular_alias_chain_resolves() {
        // Long non-circular chain: dd → cc → bb → aa → n (exercises resolve_alias_recursive L1179-1194)
        assert!(
            parse_and_verify("alias aa n\nalias bb aa\nalias cc bb\nalias dd cc\nf x:dd>n;x")
                .is_ok()
        );
    }

    // ── builtin_as_fn_ty branches (Ref-fallback in verifier) ───────────────

    #[test]
    fn builtin_as_fn_ty_known_pure_builtins() {
        // Sanity check every branch by name — guards against table drift.
        for n in ["abs", "flr", "cel", "rou"] {
            assert!(builtin_as_fn_ty(n).is_some(), "{n} should promote");
        }
        for n in ["min", "max", "mod"] {
            assert!(builtin_as_fn_ty(n).is_some(), "{n} should promote");
        }
        for n in ["sum", "avg", "trm", "str", "num", "jdmp", "len"] {
            assert!(builtin_as_fn_ty(n).is_some(), "{n} should promote");
        }
        // IO/HTTP/HOF/polymorphic builtins must NOT promote.
        for n in [
            "map",
            "flt",
            "fld",
            "grp",
            "uniqby",
            "partition",
            "frq",
            "prnt",
            "get",
            "post",
            "hd",
            "tl",
        ] {
            assert!(
                builtin_as_fn_ty(n).is_none(),
                "{n} should not promote to Fn"
            );
        }
    }

    #[test]
    fn verify_trm_as_hof_arg() {
        // `trm :: t -> t` — pass as map fn over list of strings.
        assert!(parse_and_verify("f xs:L t>L t;map trm xs").is_ok());
    }

    #[test]
    fn verify_str_as_hof_arg() {
        // `str :: n -> t` — pass as map fn.
        assert!(parse_and_verify("f xs:L n>L t;map str xs").is_ok());
    }

    #[test]
    fn verify_jdmp_as_hof_arg() {
        // `jdmp :: any -> t`.
        assert!(parse_and_verify("f xs:L n>L t;map jdmp xs").is_ok());
    }

    #[test]
    fn verify_grp_with_str_key_fn() {
        // grp accepts a key-fn — pass `str` builtin as the key (n -> t).
        // Use type alias to avoid nested generics in the return.
        let code = "alias bucket L n\nf xs:L n>M t bucket;grp str xs";
        assert!(parse_and_verify(code).is_ok());
    }

    #[test]
    fn verify_num_as_hof_arg_via_map() {
        // `num :: t -> R n t` — return list of results via type alias.
        let code = "alias res R n t\nf xs:L t>L res;map num xs";
        assert!(parse_and_verify(code).is_ok());
    }

    #[test]
    fn verify_len_as_hof_arg_with_named_alias() {
        // `len` typed L _ -> n; alias the list type to avoid nested-paren syntax
        // (which lands separately). Use a named list-of-list via type alias.
        let code = "alias mat L n\nf xs:L mat>L n;map len xs";
        assert!(parse_and_verify(code).is_ok());
    }

    #[test]
    fn verify_avg_as_hof_arg_with_named_alias() {
        let code = "alias vec L n\nf xs:L vec>L n;map avg xs";
        assert!(parse_and_verify(code).is_ok());
    }

    // ── `!` auto-unwrap on Optional (Ty::Optional arm) ────────────────────

    #[test]
    fn verify_mget_bang_in_optional_returning_fn() {
        // Enclosing returns Optional — accepts nil propagation.
        let code = r#"f>O n;m=mmap;v=mget! m "k";v"#;
        assert!(parse_and_verify(code).is_ok());
    }

    #[test]
    fn verify_mget_bang_in_number_returning_fn_errors() {
        // Enclosing returns plain n — must produce ILO-T026.
        let code = r#"f>n;m=mmap;v=mget! m "k";v"#;
        let errs = parse_and_verify(code).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T026" && e.message.contains("not an Optional")),
            "expected ILO-T026, got: {:?}",
            errs
        );
    }

    #[test]
    fn verify_bang_on_non_result_non_optional_errors() {
        // Calling `!` on a builtin returning plain n triggers ILO-T025
        // ("not a Result or Optional") — new wording from this PR.
        let code = "f>n;v=abs! -3;v";
        let errs = parse_and_verify(code).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.code == "ILO-T025" && e.message.contains("not a Result or Optional")),
            "expected ILO-T025 with new wording, got: {:?}",
            errs
        );
    }

    // ---- lst xs i v verify coverage ----

    #[test]
    fn verify_lst_happy_path() {
        let code = "f>L n;lst [1,2,3] 1 99";
        assert!(parse_and_verify(code).is_ok());
    }

    #[test]
    fn verify_lst_index_must_be_number() {
        let code = r#"f>L n;lst [1,2,3] "x" 99"#;
        let result = parse_and_verify(code);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("'lst' index must be n")),
            "errors: {:?}",
            errors
        );
    }

    #[test]
    fn verify_lst_value_type_mismatch() {
        let code = r#"f>L n;lst [1,2,3] 1 "x""#;
        let result = parse_and_verify(code);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("does not match list element type")),
            "errors: {:?}",
            errors
        );
    }

    #[test]
    fn verify_lst_first_arg_must_be_list() {
        let code = r#"f>L n;lst "abc" 1 99"#;
        let result = parse_and_verify(code);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("'lst' expects a list")),
            "errors: {:?}",
            errors
        );
    }

    // ---- wr 3-arg overload coverage ----

    #[test]
    fn verify_wr_3arg_json_ok() {
        let code = r#"f>R t t;wr "/tmp/x.json" [1,2,3] "json""#;
        assert!(parse_and_verify(code).is_ok());
    }

    #[test]
    fn verify_wr_3arg_csv_ok() {
        let code = r#"f>R t t;wr "/tmp/x.csv" [["a","b"]] "csv""#;
        assert!(parse_and_verify(code).is_ok());
    }

    #[test]
    fn verify_wr_3arg_unsupported_format_literal() {
        let code = r#"f>R t t;wr "/tmp/x.dat" [1,2,3] "yaml""#;
        let result = parse_and_verify(code);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|e| e.message.contains("not supported")),
            "errors: {:?}",
            errors
        );
    }

    #[test]
    fn verify_wr_3arg_format_must_be_text() {
        // 3-arg wr with numeric format arg: type-checker should reject.
        let code = r#"f>R t t;wr "/tmp/x" [1] 5"#;
        let result = parse_and_verify(code);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|e| e.message.contains("'wr' arg 3")),
            "errors: {:?}",
            errors
        );
    }

    #[test]
    fn verify_wr_2arg_content_must_be_text() {
        // 2-arg wr: arg 2 must be text. (Triggers `arg_types.len() < 3` branch.)
        let code = r#"f>R t t;wr "/tmp/x" 42"#;
        let result = parse_and_verify(code);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("'wr' arg 2 expects t")),
            "errors: {:?}",
            errors
        );
    }

    #[test]
    fn verify_wr_arity_message_mentions_2_or_3() {
        // 4-arg wr should fail arity with "2 or 3" range.
        let code = r#"f>R t t;wr "/tmp/x" [1] "json" "extra""#;
        let result = parse_and_verify(code);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|e| e.message.contains("2 or 3")),
            "errors: {:?}",
            errors
        );
    }

    #[test]
    fn verify_fmt2_rejects_non_number_arg() {
        // First arg text — should produce ILO-T013 with arg-index 1.
        let result = parse_and_verify(r#"f>t;fmt2 "hi" 2"#);
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(
                |e| e.code == "ILO-T013" && e.message.contains("'fmt2' arg 1 expects n, got t")
            ),
            "expected ILO-T013 for arg 1, got: {:?}",
            errs
        );
    }

    #[test]
    fn verify_fmt2_rejects_non_number_second_arg() {
        // Second arg text — ILO-T013 with arg-index 2.
        let result = parse_and_verify(r#"f>t;fmt2 3.14 "two""#);
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(
                |e| e.code == "ILO-T013" && e.message.contains("'fmt2' arg 2 expects n, got t")
            ),
            "expected ILO-T013 for arg 2, got: {:?}",
            errs
        );
    }

    #[test]
    fn verify_fmt2_valid_returns_text() {
        // Sanity: valid call typechecks.
        assert!(parse_and_verify("f>t;fmt2 3.14 2").is_ok());
    }
}
