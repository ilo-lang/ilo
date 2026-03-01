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
        Spanned { node, span: Span::UNKNOWN }
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
        T::deserialize(deserializer).map(|node| Spanned { node, span: Span::UNKNOWN })
    }
}

// ---- Core AST types ----

/// Types in idea9 — single-char base types, composable
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Type {
    Number,  // n
    Text,    // t
    Bool,    // b
    Nil,     // _
    List(Box<Type>),             // L type
    Result(Box<Type>, Box<Type>), // R ok err
    Named(String),               // user-defined type name
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

    /// `cond{body}` or `!cond{body}` — guard (early return)
    /// `cond{then}{else}` — ternary (value, no early return)
    Guard {
        condition: Expr,
        negated: bool,
        body: Vec<Spanned<Stmt>>,
        else_body: Option<Vec<Spanned<Stmt>>>,
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
}

/// Expressions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    Literal(Literal),

    /// Variable reference
    Ref(String),

    /// Field access: `obj.field` or safe `obj.?field`
    Field { object: Box<Expr>, field: String, safe: bool },

    /// Index access: `list.0`, `list.1` or safe `list.?0`
    Index { object: Box<Expr>, index: usize, safe: bool },

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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    Number(f64),
    Text(String),
    Bool(bool),
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
    for d in decls.iter().filter(|d| !matches!(d, Decl::Error { .. })) {
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

#[cfg(test)]
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
            body: vec![Spanned::unknown(Stmt::Expr(Expr::Literal(Literal::Number(1.0))))],
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

    #[test]
    fn program_json_round_trip() {
        // Ensure existing JSON AST shape is preserved
        let prog = Program {
            declarations: vec![Decl::Function {
                name: "f".to_string(),
                params: vec![Param { name: "x".to_string(), ty: Type::Number }],
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
}
