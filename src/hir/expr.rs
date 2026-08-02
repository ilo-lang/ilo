//! HIR expressions and statements.
//!
//! Thin layer over the verified AST. See `DESIGN.md` for the full list of
//! departures; the load-bearing ones are:
//!
//!   * Bodies split into prefix `stmts` + optional `tail` expression.
//!   * Guards split into `If` (braced) and `GuardReturn` (braceless).
//!   * Negated guards are folded into `UnaryOp(Not)` on the condition during
//!     lowering — backends only see one shape.
//!   * `Ternary` is gone; both `Ternary` and `?expr{arms}` go through `Match`
//!     or the lowered `If` expression below.

use crate::ast::{BinOp, Literal, Span, UnaryOp, UnwrapMode};
use crate::hir::types::Ty;

/// HIR statement.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// `name = expr`
    Let {
        name: String,
        value: Expr,
        span: Span,
    },

    /// Braced conditional: `cond { then } [else { else_ }]`.
    ///
    /// No early-return semantics — the body runs (or doesn't) and execution
    /// continues with the next statement. The negated form (`!cond { ... }`)
    /// is folded into a `UnaryOp(Not)` wrapper on `cond` by the lowering
    /// pass, so every `If` here has positive polarity.
    If {
        cond: Expr,
        then: Body,
        else_: Option<Body>,
        span: Span,
    },

    /// Braceless guard: `cond expr` — early-returns `value` from the
    /// enclosing function when `cond` is true (or the inverse if the source
    /// used the negated form; lowering folds polarity into the condition).
    GuardReturn { cond: Expr, value: Expr, span: Span },

    /// `?expr{arms}` used as a statement.
    Match {
        subject: Option<Expr>,
        arms: Vec<MatchArm>,
        span: Span,
    },

    /// `@binding collection { body }`
    ForEach {
        binding: String,
        collection: Expr,
        body: Body,
        span: Span,
    },

    /// `@binding start..end { body }`
    ForRange {
        binding: String,
        start: Expr,
        end: Expr,
        body: Body,
        span: Span,
    },

    /// `wh cond { body }`
    While { cond: Expr, body: Body, span: Span },

    /// `ret expr` — explicit early return from a function.
    Return { value: Expr, span: Span },

    /// `brk` or `brk expr`
    Break { value: Option<Expr>, span: Span },

    /// `cnt`
    Continue { span: Span },

    /// `{a;b;c} = expr` — destructure record fields into local bindings.
    Destructure {
        bindings: Vec<String>,
        value: Expr,
        span: Span,
    },

    /// Bare expression. Side-effects only — for tail values, use `Body::tail`.
    Expr { value: Expr, span: Span },
}

/// HIR expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal {
        value: Literal,
        ty: Ty,
        span: Span,
    },

    Ref {
        name: String,
        ty: Ty,
        span: Span,
    },

    Field {
        object: Box<Expr>,
        field: String,
        safe: bool,
        ty: Ty,
        span: Span,
    },

    Index {
        object: Box<Expr>,
        index: usize,
        safe: bool,
        ty: Ty,
        span: Span,
    },

    Call {
        function: String,
        args: Vec<Expr>,
        unwrap: UnwrapMode,
        ty: Ty,
        span: Span,
    },

    BinOp {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
        ty: Ty,
        span: Span,
    },

    UnaryOp {
        op: UnaryOp,
        operand: Box<Expr>,
        ty: Ty,
        span: Span,
    },

    /// `~expr` — Ok constructor.
    Ok {
        inner: Box<Expr>,
        ty: Ty,
        span: Span,
    },

    /// `^expr` — Err constructor.
    Err {
        inner: Box<Expr>,
        ty: Ty,
        span: Span,
    },

    List {
        items: Vec<Expr>,
        ty: Ty,
        span: Span,
    },

    Record {
        type_name: String,
        fields: Vec<(String, Expr)>,
        ty: Ty,
        span: Span,
    },

    /// `?expr{arms}` used as a value.
    Match {
        subject: Option<Box<Expr>>,
        arms: Vec<MatchArm>,
        ty: Ty,
        span: Span,
    },

    /// `a ?? b`
    NilCoalesce {
        value: Box<Expr>,
        default: Box<Expr>,
        ty: Ty,
        span: Span,
    },

    /// `obj with field:val ...`
    With {
        object: Box<Expr>,
        updates: Vec<(String, Expr)>,
        ty: Ty,
        span: Span,
    },

    /// Lowered from `Expr::Ternary` — a value-level if/else.
    /// `?=x 0 10 20` lowers to `If { cond: x==0, then: 10, else_: 20 }`.
    If {
        cond: Box<Expr>,
        then: Box<Expr>,
        else_: Box<Expr>,
        ty: Ty,
        span: Span,
    },

    /// Construct a closure: bind capture values onto a named (lifted)
    /// function. Carried through unchanged from the AST; see `DESIGN.md`.
    MakeClosure {
        fn_name: String,
        captures: Vec<Expr>,
        ty: Ty,
        span: Span,
    },
}

impl Expr {
    /// Return this expression's static type slot, or `Ty::Unknown` when the
    /// node doesn't carry one (currently every HIR expression carries `ty`,
    /// but the helper exists so backends don't have to match every variant).
    pub fn ty(&self) -> &Ty {
        match self {
            Expr::Literal { ty, .. }
            | Expr::Ref { ty, .. }
            | Expr::Field { ty, .. }
            | Expr::Index { ty, .. }
            | Expr::Call { ty, .. }
            | Expr::BinOp { ty, .. }
            | Expr::UnaryOp { ty, .. }
            | Expr::Ok { ty, .. }
            | Expr::Err { ty, .. }
            | Expr::List { ty, .. }
            | Expr::Record { ty, .. }
            | Expr::Match { ty, .. }
            | Expr::NilCoalesce { ty, .. }
            | Expr::With { ty, .. }
            | Expr::If { ty, .. }
            | Expr::MakeClosure { ty, .. } => ty,
        }
    }

    pub fn span(&self) -> Span {
        match self {
            Expr::Literal { span, .. }
            | Expr::Ref { span, .. }
            | Expr::Field { span, .. }
            | Expr::Index { span, .. }
            | Expr::Call { span, .. }
            | Expr::BinOp { span, .. }
            | Expr::UnaryOp { span, .. }
            | Expr::Ok { span, .. }
            | Expr::Err { span, .. }
            | Expr::List { span, .. }
            | Expr::Record { span, .. }
            | Expr::Match { span, .. }
            | Expr::NilCoalesce { span, .. }
            | Expr::With { span, .. }
            | Expr::If { span, .. }
            | Expr::MakeClosure { span, .. } => *span,
        }
    }
}

/// Match arm carrying the (already typed) pattern and arm body.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Body,
    pub span: Span,
}

/// HIR patterns. Mirrors `ast::Pattern`; type slot is the verifier's view of
/// the bound value (best-effort) so backends can size pattern bindings
/// without re-deriving.
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Err { binding: String, ty: Ty },
    Ok { binding: String, ty: Ty },
    Literal(Literal),
    Wildcard,
    TypeIs { ty: Ty, binding: String },
}

/// A block body — prefix statements with side-effects, then an optional tail
/// expression that's the value of the block.
///
/// Function bodies, if/else arms, match arms, and loop bodies all share this
/// shape. The split lets backends ask "what's the value of this block?" in
/// O(1) instead of scanning the last statement and special-casing `Expr` vs
/// anything else.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Body {
    pub stmts: Vec<Stmt>,
    pub tail: Option<Expr>,
}

impl Body {
    pub fn new(stmts: Vec<Stmt>, tail: Option<Expr>) -> Self {
        Body { stmts, tail }
    }

    pub fn is_empty(&self) -> bool {
        self.stmts.is_empty() && self.tail.is_none()
    }
}
