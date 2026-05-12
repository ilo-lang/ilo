use serde::{Deserialize, Serialize};

pub mod source_map;
pub use source_map::SourceMap;

// ---- Span infrastructure ----

/// Byte range within source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const UNKNOWN: Span = Span { start: 0, end: 0 };

    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// Wraps a node with its source span. Transparent to serde (serializes as inner node only).
#[derive(Debug, Clone, PartialEq)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

#[allow(dead_code)] // used in tests and as codegen infrastructure
impl<T> Spanned<T> {
    pub fn new(node: T, span: Span) -> Self {
        Spanned { node, span }
    }

    pub fn unknown(node: T) -> Self {
        Spanned {
            node,
            span: Span::UNKNOWN,
        }
    }
}

impl<T> std::ops::Deref for Spanned<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.node
    }
}

impl<T: Serialize> Serialize for Spanned<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.node.serialize(serializer)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Spanned<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(|node| Spanned {
            node,
            span: Span::UNKNOWN,
        })
    }
}

// ---- Core AST types ----

/// Types in idea9 — single-char base types, composable
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Type {
    Number,                       // n
    Text,                         // t
    Bool,                         // b
    Any,                          // _  — "don't care" / unknown type
    Optional(Box<Type>),          // O type  — nullable (nil or the inner type)
    List(Box<Type>),              // L type
    Map(Box<Type>, Box<Type>),    // M key value  — dynamic key-value collection
    Result(Box<Type>, Box<Type>), // R ok err
    Sum(Vec<String>),             // S a b c  — closed set of named string variants
    Fn(Vec<Type>, Box<Type>),     // F param... return  (last type is return)
    Named(String),                // user-defined type name or type variable
}

/// A parameter: `name:type`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    pub ty: Type,
}

/// Top-level declarations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Decl {
    /// `name params>return;body`
    Function {
        name: String,
        params: Vec<Param>,
        return_type: Type,
        body: Vec<Spanned<Stmt>>,
        #[serde(skip)]
        span: Span,
    },

    /// `type name{field:type;...}`
    TypeDef {
        name: String,
        fields: Vec<Param>,
        #[serde(skip)]
        span: Span,
    },

    /// `tool name"desc" params>return timeout:n,retry:n`
    Tool {
        name: String,
        description: String,
        params: Vec<Param>,
        return_type: Type,
        timeout: Option<f64>,
        retry: Option<f64>,
        #[serde(skip)]
        span: Span,
    },

    /// `alias name type` — type alias (pure sugar, resolved at verify time)
    Alias {
        name: String,
        target: Type,
        #[serde(skip)]
        span: Span,
    },

    /// `use "path/to/file.ilo"` — import all declarations from another file.
    /// `use "path/to/file.ilo" [name1 name2]` — import only named declarations.
    /// Resolved before verification; replaced by the imported declarations in
    /// the merged program. Stripped by the verifier/codegen as a safety net.
    Use {
        path: String,
        /// `None` = import all; `Some(names)` = import only those names.
        only: Option<Vec<String>>,
        #[serde(skip)]
        span: Span,
    },

    /// Poison node inserted during parser error recovery.
    /// Suppressed by the verifier; omitted from JSON AST output
    /// (filtered by the custom serializer on Program.declarations).
    Error {
        #[serde(skip)]
        span: Span,
    },
}

/// Statements
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Stmt {
    /// `name=expr`
    Let { name: String, value: Expr },

    /// `cond{body}` or `!cond{body}` — conditional execution (no early return)
    /// `cond{then}{else}` — ternary (value, no early return)
    /// `cond expr` — braceless guard (early return)
    Guard {
        condition: Expr,
        negated: bool,
        body: Vec<Spanned<Stmt>>,
        else_body: Option<Vec<Spanned<Stmt>>>,
        /// true for braceless guards (`cond expr`), which still early-return.
        /// false for braced guards (`cond{body}`), which are conditional execution.
        #[serde(default)]
        braceless: bool,
    },

    /// `?expr{arms}` or `?{arms}`
    Match {
        subject: Option<Expr>,
        arms: Vec<MatchArm>,
    },

    /// `@binding collection{body}`
    ForEach {
        binding: String,
        collection: Expr,
        body: Vec<Spanned<Stmt>>,
    },

    /// `@binding start..end{body}` — range iteration
    ForRange {
        binding: String,
        start: Expr,
        end: Expr,
        body: Vec<Spanned<Stmt>>,
    },

    /// `wh cond{body}` — while loop
    While {
        condition: Expr,
        body: Vec<Spanned<Stmt>>,
    },

    /// `ret expr` — early return from function
    Return(Expr),

    /// `brk` or `brk expr` — exit enclosing loop
    Break(Option<Expr>),

    /// `cnt` — skip to next iteration of enclosing loop
    Continue,

    /// `{a;b;c}=expr` — destructure record fields into local bindings
    Destructure { bindings: Vec<String>, value: Expr },

    /// Expression as statement (last expr is return value)
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Vec<Spanned<Stmt>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Pattern {
    /// `^e:` — binds error value
    Err(String),
    /// `~v:` — binds ok value
    Ok(String),
    /// Literal pattern: `"gold":`, `1000:`
    Literal(Literal),
    /// `_:` — wildcard / catch-all
    Wildcard,
    /// `n v:`, `t v:`, `b v:`, `l v:` — branch on runtime type, bind value
    TypeIs { ty: Type, binding: String },
}

/// Expressions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    Literal(Literal),

    /// Variable reference
    Ref(String),

    /// Field access: `obj.field` or safe `obj.?field`
    Field {
        object: Box<Expr>,
        field: String,
        safe: bool,
    },

    /// Index access: `list.0`, `list.1` or safe `list.?0`
    Index {
        object: Box<Expr>,
        index: usize,
        safe: bool,
    },

    /// Function call with positional args: `func arg1 arg2`
    /// When `unwrap` is true, `func! args` auto-unwraps Result:
    /// Ok(v) → v, Err(e) → propagate Err to enclosing function.
    Call {
        function: String,
        args: Vec<Expr>,
        #[serde(default)]
        unwrap: bool,
    },

    /// Prefix binary op: `+a b`, `*a b`
    BinOp {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },

    /// Unary negation: `!expr` (logical) or `-expr` (numeric)
    UnaryOp {
        op: UnaryOp,
        operand: Box<Expr>,
    },

    /// Ok constructor: `~expr`
    Ok(Box<Expr>),

    /// Err constructor: `^expr`
    Err(Box<Expr>),

    /// List literal
    List(Vec<Expr>),

    /// Record construction: `typename field:val field:val`
    Record {
        type_name: String,
        fields: Vec<(String, Expr)>,
    },

    /// Match expression: `?expr{arms}` or `?{arms}` used as value
    Match {
        subject: Option<Box<Expr>>,
        arms: Vec<MatchArm>,
    },

    /// Nil-coalesce: `a ?? b` — if a is nil, evaluate b
    NilCoalesce {
        value: Box<Expr>,
        default: Box<Expr>,
    },

    /// With expression: `obj with field:val`
    With {
        object: Box<Expr>,
        updates: Vec<(String, Expr)>,
    },

    /// Prefix ternary: `?=x 0 10 20` → if x==0 then 10 else 20
    Ternary {
        condition: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    Number(f64),
    Text(String),
    Bool(bool),
    Nil,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BinOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Equals,
    NotEquals,
    GreaterThan,
    LessThan,
    GreaterOrEqual,
    LessOrEqual,
    And,
    Or,
    Append,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UnaryOp {
    Not,
    Negate,
}

fn serialize_decls<S: serde::Serializer>(decls: &[Decl], s: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeSeq;
    let mut seq = s.serialize_seq(None)?;
    for d in decls
        .iter()
        .filter(|d| !matches!(d, Decl::Error { .. } | Decl::Use { .. }))
    {
        seq.serialize_element(d)?;
    }
    seq.end()
}

/// A complete program is a list of declarations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Program {
    #[serde(serialize_with = "serialize_decls")]
    pub declarations: Vec<Decl>,
    #[serde(skip)]
    pub source: Option<String>,
}

// Long-form aliases for builtins. Each maps (long_name, canonical_short_name).
// Programs using long forms work identically but emit a hint toward the short form.
const BUILTIN_ALIASES: &[(&str, &str)] = &[
    // Math
    ("floor", "flr"),
    ("ceil", "cel"),
    ("round", "rou"),
    ("random", "rnd"),
    // Conversion
    ("string", "str"),
    ("number", "num"),
    // Collections
    ("length", "len"),
    ("head", "hd"),
    ("tail", "tl"),
    ("reverse", "rev"),
    ("sort", "srt"),
    ("slice", "slc"),
    ("unique", "unq"),
    ("filter", "flt"),
    ("fold", "fld"),
    ("flatten", "flat"),
    ("concat", "cat"),
    ("contains", "has"),
    ("group", "grp"),
    ("average", "avg"),
    ("print", "prnt"),
    ("trim", "trm"),
    ("split", "spl"),
    ("format", "fmt"),
    ("regex", "rgx"),
    ("regex_sub", "rgxsub"),
    ("read", "rd"),
    ("readlines", "rdl"),
    ("readbuf", "rdb"),
    ("write", "wr"),
    ("writelines", "wrl"),
];

/// If `name` is a long-form alias, return the canonical short form.
/// Otherwise return None.
pub fn resolve_alias(name: &str) -> Option<&'static str> {
    BUILTIN_ALIASES
        .iter()
        .find(|(long, _)| *long == name)
        .map(|(_, short)| *short)
}

/// Iterate over all (long_name, short_name) builtin alias pairs.
/// Used by the parser to mirror arity/HOF metadata onto long-form names.
pub fn all_builtin_aliases() -> impl Iterator<Item = (&'static str, &'static str)> {
    BUILTIN_ALIASES.iter().copied()
}

/// Resolve aliases in all Call expressions throughout a program.
/// Mutates function names in-place so downstream passes see only canonical names.
pub fn resolve_aliases(program: &mut Program) {
    for decl in &mut program.declarations {
        if let Decl::Function { body, .. } = decl {
            for stmt in body {
                resolve_aliases_stmt(&mut stmt.node);
            }
        }
    }
}

fn resolve_aliases_stmt(stmt: &mut Stmt) {
    match stmt {
        Stmt::Expr(expr) | Stmt::Let { value: expr, .. } => resolve_aliases_expr(expr),
        Stmt::Guard {
            condition,
            body,
            else_body,
            ..
        } => {
            resolve_aliases_expr(condition);
            for s in body {
                resolve_aliases_stmt(&mut s.node);
            }
            if let Some(eb) = else_body {
                for s in eb {
                    resolve_aliases_stmt(&mut s.node);
                }
            }
        }
        Stmt::Match { subject, arms } => {
            if let Some(expr) = subject {
                resolve_aliases_expr(expr);
            }
            for arm in arms {
                for s in &mut arm.body {
                    resolve_aliases_stmt(&mut s.node);
                }
            }
        }
        Stmt::ForEach {
            collection, body, ..
        } => {
            resolve_aliases_expr(collection);
            for s in body {
                resolve_aliases_stmt(&mut s.node);
            }
        }
        Stmt::ForRange {
            start, end, body, ..
        } => {
            resolve_aliases_expr(start);
            resolve_aliases_expr(end);
            for s in body {
                resolve_aliases_stmt(&mut s.node);
            }
        }
        Stmt::While { condition, body } => {
            resolve_aliases_expr(condition);
            for s in body {
                resolve_aliases_stmt(&mut s.node);
            }
        }
        Stmt::Return(expr) => resolve_aliases_expr(expr),
        Stmt::Destructure { value, .. } => resolve_aliases_expr(value),
        Stmt::Break(Some(expr)) => resolve_aliases_expr(expr),
        Stmt::Break(None) | Stmt::Continue => {}
    }
}

fn resolve_aliases_expr(expr: &mut Expr) {
    match expr {
        Expr::Call { function, args, .. } => {
            if let Some(canonical) = resolve_alias(function) {
                *function = canonical.to_string();
            }
            for arg in args {
                resolve_aliases_expr(arg);
            }
        }
        Expr::BinOp { left, right, .. } => {
            resolve_aliases_expr(left);
            resolve_aliases_expr(right);
        }
        Expr::UnaryOp { operand, .. } => resolve_aliases_expr(operand),
        Expr::Ok(inner) | Expr::Err(inner) => resolve_aliases_expr(inner),
        Expr::NilCoalesce { value, default } => {
            resolve_aliases_expr(value);
            resolve_aliases_expr(default);
        }
        Expr::List(items) => {
            for item in items {
                resolve_aliases_expr(item);
            }
        }
        Expr::Record { fields, .. } => {
            for (_, val) in fields {
                resolve_aliases_expr(val);
            }
        }
        Expr::Match { subject, arms } => {
            if let Some(s) = subject {
                resolve_aliases_expr(s);
            }
            for arm in arms {
                for s in &mut arm.body {
                    resolve_aliases_stmt(&mut s.node);
                }
            }
        }
        Expr::With { object, updates } => {
            resolve_aliases_expr(object);
            for (_, val) in updates {
                resolve_aliases_expr(val);
            }
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            resolve_aliases_expr(condition);
            resolve_aliases_expr(then_expr);
            resolve_aliases_expr(else_expr);
        }
        Expr::Literal(_) | Expr::Ref(_) | Expr::Field { .. } | Expr::Index { .. } => {}
    }
}

#[cfg(test)]
#[allow(clippy::approx_constant)]
mod tests {
    use super::*;

    #[test]
    fn span_unknown_is_zero() {
        assert_eq!(Span::UNKNOWN, Span { start: 0, end: 0 });
    }

    #[test]
    fn span_merge_takes_extremes() {
        let a = Span { start: 5, end: 10 };
        let b = Span { start: 2, end: 15 };
        let merged = a.merge(b);
        assert_eq!(merged, Span { start: 2, end: 15 });
    }

    #[test]
    fn span_merge_same() {
        let a = Span { start: 3, end: 7 };
        assert_eq!(a.merge(a), a);
    }

    #[test]
    fn span_merge_non_overlapping() {
        let a = Span { start: 0, end: 5 };
        let b = Span { start: 10, end: 20 };
        assert_eq!(a.merge(b), Span { start: 0, end: 20 });
    }

    #[test]
    fn span_default_is_zero() {
        let s = Span::default();
        assert_eq!(s, Span { start: 0, end: 0 });
    }

    #[test]
    fn spanned_deref() {
        let s = Spanned::new(42, Span { start: 0, end: 2 });
        assert_eq!(*s, 42);
    }

    #[test]
    fn spanned_unknown() {
        let s = Spanned::unknown("hello");
        assert_eq!(s.span, Span::UNKNOWN);
        assert_eq!(*s, "hello");
    }

    #[test]
    fn spanned_serialize_transparent() {
        let s = Spanned::new(42i32, Span { start: 5, end: 10 });
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "42");
    }

    #[test]
    fn spanned_deserialize_transparent() {
        let s: Spanned<i32> = serde_json::from_str("42").unwrap();
        assert_eq!(s.node, 42);
        assert_eq!(s.span, Span::UNKNOWN);
    }

    #[test]
    fn spanned_serialize_complex() {
        let expr = Spanned::new(
            Expr::Literal(Literal::Number(3.14)),
            Span { start: 0, end: 4 },
        );
        let json = serde_json::to_string(&expr).unwrap();
        // Should serialize as the inner Expr, not as a wrapper
        assert!(json.contains("Number"));
        assert!(!json.contains("span"));
    }

    #[test]
    fn decl_span_not_serialized() {
        let decl = Decl::Function {
            name: "f".to_string(),
            params: vec![],
            return_type: Type::Number,
            body: vec![Spanned::unknown(Stmt::Expr(Expr::Literal(
                Literal::Number(1.0),
            )))],
            span: Span { start: 0, end: 10 },
        };
        let json = serde_json::to_string(&decl).unwrap();
        assert!(!json.contains("span"));
    }

    #[test]
    fn program_source_not_serialized() {
        let prog = Program {
            declarations: vec![],
            source: Some("f x:n>n;x".to_string()),
        };
        let json = serde_json::to_string(&prog).unwrap();
        assert!(!json.contains("source"));
        assert!(!json.contains("f x:n>n;x"));
    }

    // ── Coverage: resolve_aliases_stmt / resolve_aliases_expr paths ──────────

    #[test]
    fn resolve_aliases_while_stmt() {
        // L440-442: While variant in resolve_aliases_stmt
        let mut prog = Program {
            declarations: vec![Decl::Function {
                name: "f".to_string(),
                params: vec![],
                return_type: Type::Number,
                body: vec![Spanned::unknown(Stmt::While {
                    condition: Expr::Call {
                        function: "length".to_string(),
                        args: vec![Expr::Ref("x".to_string())],
                        unwrap: false,
                    },
                    body: vec![Spanned::unknown(Stmt::Expr(Expr::Call {
                        function: "length".to_string(),
                        args: vec![Expr::Ref("y".to_string())],
                        unwrap: false,
                    }))],
                })],
                span: Span::UNKNOWN,
            }],
            source: None,
        };
        resolve_aliases(&mut prog);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::While {
            condition,
            body: wbody,
        } = &body[0].node
        else {
            panic!("expected While")
        };
        let Expr::Call { function, .. } = condition else {
            panic!("expected call")
        };
        assert_eq!(function, "len");
        let Stmt::Expr(Expr::Call { function: f2, .. }) = &wbody[0].node else {
            panic!("expected call")
        };
        assert_eq!(f2, "len");
    }

    #[test]
    fn resolve_aliases_return_stmt() {
        // L444: Return variant in resolve_aliases_stmt
        let mut prog = Program {
            declarations: vec![Decl::Function {
                name: "f".to_string(),
                params: vec![],
                return_type: Type::Number,
                body: vec![Spanned::unknown(Stmt::Return(Expr::Call {
                    function: "length".to_string(),
                    args: vec![Expr::Ref("x".to_string())],
                    unwrap: false,
                }))],
                span: Span::UNKNOWN,
            }],
            source: None,
        };
        resolve_aliases(&mut prog);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Return(Expr::Call { function, .. }) = &body[0].node else {
            panic!("expected Return(Call)")
        };
        assert_eq!(function, "len");
    }

    #[test]
    fn resolve_aliases_destructure_stmt() {
        // L445: Destructure variant in resolve_aliases_stmt
        let mut prog = Program {
            declarations: vec![Decl::Function {
                name: "f".to_string(),
                params: vec![],
                return_type: Type::Number,
                body: vec![Spanned::unknown(Stmt::Destructure {
                    bindings: vec!["a".to_string(), "b".to_string()],
                    value: Expr::Call {
                        function: "length".to_string(),
                        args: vec![Expr::Ref("x".to_string())],
                        unwrap: false,
                    },
                })],
                span: Span::UNKNOWN,
            }],
            source: None,
        };
        resolve_aliases(&mut prog);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Destructure {
            value: Expr::Call { function, .. },
            ..
        } = &body[0].node
        else {
            panic!("expected Destructure")
        };
        assert_eq!(function, "len");
    }

    #[test]
    fn resolve_aliases_break_with_value() {
        // L446: Break(Some(expr)) variant in resolve_aliases_stmt
        let mut prog = Program {
            declarations: vec![Decl::Function {
                name: "f".to_string(),
                params: vec![],
                return_type: Type::Number,
                body: vec![Spanned::unknown(Stmt::Break(Some(Expr::Call {
                    function: "length".to_string(),
                    args: vec![Expr::Ref("x".to_string())],
                    unwrap: false,
                })))],
                span: Span::UNKNOWN,
            }],
            source: None,
        };
        resolve_aliases(&mut prog);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Break(Some(Expr::Call { function, .. })) = &body[0].node else {
            panic!("expected Break(Some(Call))")
        };
        assert_eq!(function, "len");
    }

    #[test]
    fn resolve_aliases_break_none_and_continue() {
        // L447: Break(None) | Continue — no-op, just ensure no panic
        let mut prog = Program {
            declarations: vec![Decl::Function {
                name: "f".to_string(),
                params: vec![],
                return_type: Type::Number,
                body: vec![
                    Spanned::unknown(Stmt::Break(None)),
                    Spanned::unknown(Stmt::Continue),
                ],
                span: Span::UNKNOWN,
            }],
            source: None,
        };
        resolve_aliases(&mut prog);
        assert!(matches!(&prog.declarations[0], Decl::Function { body, .. } if body.len() == 2));
    }

    #[test]
    fn resolve_aliases_nil_coalesce_expr() {
        // L465-467: NilCoalesce variant in resolve_aliases_expr
        let mut prog = Program {
            declarations: vec![Decl::Function {
                name: "f".to_string(),
                params: vec![],
                return_type: Type::Number,
                body: vec![Spanned::unknown(Stmt::Expr(Expr::NilCoalesce {
                    value: Box::new(Expr::Call {
                        function: "length".to_string(),
                        args: vec![Expr::Ref("x".to_string())],
                        unwrap: false,
                    }),
                    default: Box::new(Expr::Call {
                        function: "reverse".to_string(),
                        args: vec![Expr::Ref("y".to_string())],
                        unwrap: false,
                    }),
                }))],
                span: Span::UNKNOWN,
            }],
            source: None,
        };
        resolve_aliases(&mut prog);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Expr(Expr::NilCoalesce { value, default }) = &body[0].node else {
            panic!("expected NilCoalesce")
        };
        let Expr::Call { function, .. } = value.as_ref() else {
            panic!("expected call")
        };
        assert_eq!(function, "len");
        let Expr::Call { function: f2, .. } = default.as_ref() else {
            panic!("expected call")
        };
        assert_eq!(f2, "rev");
    }

    #[test]
    fn resolve_aliases_record_expr() {
        // L472-473: Record variant in resolve_aliases_expr
        let mut prog = Program {
            declarations: vec![Decl::Function {
                name: "f".to_string(),
                params: vec![],
                return_type: Type::Number,
                body: vec![Spanned::unknown(Stmt::Expr(Expr::Record {
                    type_name: "point".to_string(),
                    fields: vec![(
                        "x".to_string(),
                        Expr::Call {
                            function: "length".to_string(),
                            args: vec![Expr::Ref("a".to_string())],
                            unwrap: false,
                        },
                    )],
                }))],
                span: Span::UNKNOWN,
            }],
            source: None,
        };
        resolve_aliases(&mut prog);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Expr(Expr::Record { fields, .. }) = &body[0].node else {
            panic!("expected Record")
        };
        let Expr::Call { function, .. } = &fields[0].1 else {
            panic!("expected call")
        };
        assert_eq!(function, "len");
    }

    #[test]
    fn resolve_aliases_match_expr() {
        // L475-478: Match variant (as expression) in resolve_aliases_expr
        let mut prog = Program {
            declarations: vec![Decl::Function {
                name: "f".to_string(),
                params: vec![],
                return_type: Type::Number,
                body: vec![Spanned::unknown(Stmt::Expr(Expr::Match {
                    subject: Some(Box::new(Expr::Call {
                        function: "length".to_string(),
                        args: vec![Expr::Ref("x".to_string())],
                        unwrap: false,
                    })),
                    arms: vec![MatchArm {
                        pattern: Pattern::Wildcard,
                        body: vec![Spanned::unknown(Stmt::Expr(Expr::Call {
                            function: "reverse".to_string(),
                            args: vec![Expr::Ref("y".to_string())],
                            unwrap: false,
                        }))],
                    }],
                }))],
                span: Span::UNKNOWN,
            }],
            source: None,
        };
        resolve_aliases(&mut prog);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Expr(Expr::Match { subject, arms }) = &body[0].node else {
            panic!("expected Match")
        };
        let Some(s) = subject else {
            panic!("expected subject")
        };
        let Expr::Call { function, .. } = s.as_ref() else {
            panic!("expected call")
        };
        assert_eq!(function, "len");
        let Stmt::Expr(Expr::Call { function: f2, .. }) = &arms[0].body[0].node else {
            panic!("expected call")
        };
        assert_eq!(f2, "rev");
    }

    #[test]
    fn resolve_aliases_with_expr() {
        // L481-483: With variant in resolve_aliases_expr
        let mut prog = Program {
            declarations: vec![Decl::Function {
                name: "f".to_string(),
                params: vec![],
                return_type: Type::Number,
                body: vec![Spanned::unknown(Stmt::Expr(Expr::With {
                    object: Box::new(Expr::Call {
                        function: "length".to_string(),
                        args: vec![Expr::Ref("x".to_string())],
                        unwrap: false,
                    }),
                    updates: vec![(
                        "a".to_string(),
                        Expr::Call {
                            function: "reverse".to_string(),
                            args: vec![Expr::Ref("y".to_string())],
                            unwrap: false,
                        },
                    )],
                }))],
                span: Span::UNKNOWN,
            }],
            source: None,
        };
        resolve_aliases(&mut prog);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Expr(Expr::With { object, updates }) = &body[0].node else {
            panic!("expected With")
        };
        let Expr::Call { function, .. } = object.as_ref() else {
            panic!("expected call")
        };
        assert_eq!(function, "len");
        let Expr::Call { function: f2, .. } = &updates[0].1 else {
            panic!("expected call")
        };
        assert_eq!(f2, "rev");
    }

    #[test]
    fn program_json_round_trip() {
        // Ensure existing JSON AST shape is preserved
        let prog = Program {
            declarations: vec![Decl::Function {
                name: "f".to_string(),
                params: vec![Param {
                    name: "x".to_string(),
                    ty: Type::Number,
                }],
                return_type: Type::Number,
                body: vec![Spanned::unknown(Stmt::Expr(Expr::Ref("x".to_string())))],
                span: Span { start: 0, end: 13 },
            }],
            source: Some("f x:n>n;x".to_string()),
        };
        let json = serde_json::to_string_pretty(&prog).unwrap();
        let deserialized: Program = serde_json::from_str(&json).unwrap();
        // Source and spans are lost on deserialization (skipped), but structure matches
        assert_eq!(deserialized.declarations.len(), 1);
        assert!(deserialized.source.is_none());
    }

    // resolve_aliases_stmt: Stmt::Match with subject = None
    // Covers the `^0` else-branch at line 457 where `if let Some(expr) = subject` is false
    #[test]
    fn resolve_aliases_stmt_match_no_subject() {
        let mut prog = Program {
            declarations: vec![Decl::Function {
                name: "f".to_string(),
                params: vec![],
                return_type: Type::Number,
                body: vec![Spanned::unknown(Stmt::Match {
                    subject: None,
                    arms: vec![MatchArm {
                        pattern: Pattern::Wildcard,
                        body: vec![Spanned::unknown(Stmt::Expr(Expr::Call {
                            function: "len".to_string(),
                            args: vec![Expr::Ref("x".to_string())],
                            unwrap: false,
                        }))],
                    }],
                })],
                span: Span::UNKNOWN,
            }],
            source: None,
        };
        // resolve_aliases replaces known aliases; "len" → "length" (if aliased) or stays
        resolve_aliases(&mut prog);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        // After resolve_aliases, the Match node should still be present
        assert!(
            matches!(&body[0].node, Stmt::Match { subject: None, arms } if arms.len() == 1),
            "expected Match{{None}} after resolve_aliases"
        );
    }

    // resolve_aliases_expr: Expr::Match with subject = None
    // Covers the `^0` else-branch at line 527 where `if let Some(s) = subject` is false
    #[test]
    fn resolve_aliases_expr_match_no_subject() {
        let mut prog = Program {
            declarations: vec![Decl::Function {
                name: "f".to_string(),
                params: vec![],
                return_type: Type::Number,
                body: vec![Spanned::unknown(Stmt::Expr(Expr::Match {
                    subject: None,
                    arms: vec![MatchArm {
                        pattern: Pattern::Wildcard,
                        body: vec![Spanned::unknown(Stmt::Expr(Expr::Call {
                            function: "len".to_string(),
                            args: vec![Expr::Ref("y".to_string())],
                            unwrap: false,
                        }))],
                    }],
                }))],
                span: Span::UNKNOWN,
            }],
            source: None,
        };
        resolve_aliases(&mut prog);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        // After resolve_aliases, the Expr::Match node should still be present
        assert!(
            matches!(&body[0].node, Stmt::Expr(Expr::Match { subject: None, arms }) if arms.len() == 1),
            "expected Expr::Match{{None}} after resolve_aliases"
        );
    }
}
