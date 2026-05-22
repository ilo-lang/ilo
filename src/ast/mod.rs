use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

/// Bound on a generic type variable.
///
/// A small fixed set — enough for `sort`/`cmp`/`min`/`max` and numeric ops
/// without shipping a full typeclass system.
///
/// | Bound        | Permitted concrete types                  |
/// |--------------|-------------------------------------------|
/// | `Any`        | anything (default when no bound given)    |
/// | `Comparable` | `n`, `t`, `b`                             |
/// | `Numeric`    | `n`                                       |
/// | `Text`       | `t`                                       |
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Bound {
    Any,
    Comparable,
    Numeric,
    Text,
}

impl std::fmt::Display for Bound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Bound::Any => write!(f, "any"),
            Bound::Comparable => write!(f, "comparable"),
            Bound::Numeric => write!(f, "numeric"),
            Bound::Text => write!(f, "text"),
        }
    }
}

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

/// A variant in a sum-type declaration.
/// `Circle(n)` → Variant { name: "circle", payload: Some(Type::Number) }
/// `red`       → Variant { name: "red",    payload: None }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Variant {
    pub name: String,
    pub payload: Option<Type>,
}

/// Compile-time predicate for conditional `use` — `use ?wasm "a.ilo" : "b.ilo"`.
///
/// `wasm`   — true when building for wasm32 (`--target wasm`).
/// `native` — true when building for a native host (`--target native`, default).
/// `test`   — true when running under `ilo test` (`--target test`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UsePredicate {
    Wasm,
    Native,
    Test,
}

impl UsePredicate {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "wasm" => Some(Self::Wasm),
            "native" => Some(Self::Native),
            "test" => Some(Self::Test),
            _ => None,
        }
    }
}

/// Top-level declarations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Decl {
    /// `name<a:Comparable b> params>return;body`
    /// `type_params` holds bounded generic type-variable declarations.
    /// Syntax: `<letter>` or `<letter:Bound>`, space-separated inside `<...>`.
    /// When absent the vec is empty and existing `a`/`b`/etc. type-variable
    /// behaviour (treat as Unknown / accept any type) is preserved.
    Function {
        name: String,
        /// Generic type-variable declarations: `<a:Comparable b:Numeric c>`.
        /// Empty means no explicit generic params (legacy behaviour).
        #[serde(skip)]
        type_params: Vec<(String, Bound)>,
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
    /// `use alias:"path/to/file.ilo"` — import all public declarations, prefixed
    ///   with `alias-` (e.g. `math-dbl`, `math-half`). Private (`_`-prefixed)
    ///   symbols are always excluded from named-module imports.
    /// `use ?wasm "wasm-mod.ilo" : "native-mod.ilo"` — conditional import:
    ///   import `path` when the predicate is true for the current build target,
    ///   otherwise import `alt_path`. Resolved before verification.
    /// `use re:"path/to/file.ilo" [name1 name2]` — import AND re-export the named
    ///   declarations: they become part of this module's public surface.
    /// Resolved before verification; replaced by the imported declarations in
    /// the merged program. Stripped by the verifier/codegen as a safety net.
    Use {
        path: String,
        /// `None` = import all; `Some(names)` = import only those names.
        only: Option<Vec<String>>,
        /// Named module alias: `use alias:"path"` sets this to `Some("alias")`.
        /// When set, imported public symbols are renamed `alias-<name>`.
        alias: Option<String>,
        /// Conditional form: `use ?<pred> "true-path" : "false-path"`.
        /// When `Some`, `path` is the true-branch and `alt_path` is the
        /// false-branch. `only` and `alias` are disallowed in this form.
        predicate: Option<UsePredicate>,
        /// The false-branch path for conditional imports. `None` for unconditional.
        alt_path: Option<String>,
        /// Re-export flag: `use re:"path" [names]` makes the listed names part
        /// of this module's public surface (visible to consumers of this module).
        /// Without this flag, imported names are internal to this module only.
        reexport: bool,
        #[serde(skip)]
        span: Span,
    },

    /// `type Name = Circle(n) | Square(n) | red` — named discriminated union
    SumType {
        name: String,
        variants: Vec<Variant>,
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

/// Whether a `defer` fires on all exits or only on error exits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeferKind {
    /// `defer expr` — runs on any function exit (normal or error).
    Always,
    /// `errdefer expr` — runs only when the function exits via an error
    /// (Result `Err` propagation, panic-unwrap, or runtime error).
    OnError,
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

    /// `@binding start..end{body}` or `@binding start..end by step{body}` — range iteration
    ForRange {
        binding: String,
        start: Expr,
        end: Expr,
        /// Optional step size (`by <expr>`). `None` means step of 1.
        step: Option<Expr>,
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

    /// `defer expr` / `errdefer expr` — register a cleanup expression to run
    /// at function-scope exit.  `Always` fires on both normal and error exit;
    /// `OnError` fires only when the function exits via an error path.
    /// Multiple defers in one function body execute in LIFO order.
    /// v1 scope: function-level only (not block-level).
    Defer { expr: Expr, kind: DeferKind },

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
    /// `Circle(r):` — match a named-sum variant, optionally bind payload
    Variant {
        tag: String,
        binding: Option<String>,
    },
    /// `pat1|pat2|...:` — matches if any alternative matches (OR pattern)
    Or(Vec<Pattern>),
}

/// Auto-unwrap mode on `Expr::Call`. See `Expr::Call` for full semantics.
///
/// Stored as a single field instead of paired booleans so the type system
/// enforces "propagate" and "panic" are mutually exclusive — every call site
/// holds exactly one mode, never two flags that could drift apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum UnwrapMode {
    /// No auto-unwrap.
    #[default]
    None,
    /// `func! args` — on Err/nil, early-return to the enclosing function.
    Propagate,
    /// `func!! args` — on Err/nil, abort with diagnostic + exit 1.
    Panic,
}

impl UnwrapMode {
    /// True for `!` (propagate via early-return).
    pub fn is_propagate(self) -> bool {
        matches!(self, UnwrapMode::Propagate)
    }

    /// True for `!!` (abort with diagnostic + exit 1).
    pub fn is_panic(self) -> bool {
        matches!(self, UnwrapMode::Panic)
    }

    /// True if either `!` or `!!` is in effect — i.e. the call result must be
    /// unwrapped (Result → inner, Optional → inner) one way or another.
    pub fn is_any(self) -> bool {
        !matches!(self, UnwrapMode::None)
    }
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
    ///
    /// The `unwrap` field controls auto-unwrap behaviour on Result / Optional returns:
    /// - `UnwrapMode::None`: no auto-unwrap. The call result passes through verbatim.
    /// - `UnwrapMode::Propagate` (written `func! args`): on `~v` / non-nil → inner value;
    ///   on `^e` / nil → early-return that value as the enclosing function's return.
    ///   Verifier enforces the enclosing function's return type can carry the
    ///   propagated value (R for Result-returning callee, O / Nil / Unknown for Optional).
    /// - `UnwrapMode::Panic` (written `func!! args`): on `~v` / non-nil → inner value;
    ///   on `^e` / nil → abort with diagnostic + exit 1 via the runtime-error channel.
    ///   No enclosing-return constraint.
    Call {
        function: String,
        args: Vec<Expr>,
        #[serde(default)]
        unwrap: UnwrapMode,
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

    /// Anonymous record literal: `{field:val field:val}` — no typename required.
    /// Type checker synthesises a structural type; runtime uses `"__anon"` as the
    /// Value::Record type_name since engines only care about field names.
    AnonRecord {
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

    /// Construct a closure: bind capture values onto a named (lifted) function.
    ///
    /// Emitted by the parser when an inline lambda `(params>ret;body)` has free
    /// variables in its body. The lambda is lifted to a synthetic top-level
    /// `__lit_N` decl whose parameter list is `[original_params..., capture_params...]`,
    /// and the call site becomes `Expr::MakeClosure { fn_name: "__lit_N", captures: [Ref(c1), ...] }`.
    ///
    /// At runtime this evaluates to `Value::Closure { fn_name, captures: [v1, ...] }`,
    /// which closure-aware HOFs (`srt`, `map`, `flt`, `fld`, `grp`, `uniqby`,
    /// `partition`, `flatmap`) treat as an N-arg-capturing fn-ref: each per-item
    /// call gets the captures appended after the item args. Captures are
    /// by-value snapshots, matching the existing single-ctx form (#186).
    MakeClosure {
        fn_name: String,
        captures: Vec<Expr>,
    },

    /// Gleam-style `todo "reason"` — satisfies any return type; panics at runtime
    /// with the given reason message. Use when a branch is not yet implemented.
    Todo(Box<Expr>),

    /// Gleam-style `panic "reason"` — satisfies any return type; panics at runtime
    /// with the given reason message. Use to mark branches that should never execute.
    Panic(Box<Expr>),
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
    /// Function names whose declaration was started but failed to parse to
    /// completion (header recognised, then header/body/return-type errored).
    /// Populated by `parser::parse` and consumed by `verify::verify` to skip
    /// cascading type errors against these functions: their bodies are
    /// suppressed from type-checking, and `undefined function` diagnostics
    /// at call sites collapse to one cross-reference rather than firing
    /// per call. Maps name -> (code, line, col) of the originating parse
    /// error so cross-reference hints can point back at the root cause.
    #[serde(skip)]
    pub parse_failed_fns: HashMap<String, ParseFailRef>,
}

/// Identity of the parse error that disabled type-checking for a function.
/// Carried on `Program.parse_failed_fns` so verify can render hints like
/// "definition failed to parse - see ILO-P009 at line 12" without re-reading
/// the parse-error list.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseFailRef {
    pub code: &'static str,
    pub span: Span,
}

// Builtin aliases. Each maps (alias, canonical_name). Programs using the
// alias work identically and emit a hint toward the canonical form.
//
// Most entries are long-form aliases (`length` → `len`) so an agent reaching
// for the verbose form from another language gets the canonical short name
// after one run. A small number of entries go the other direction
// (canonical name is the longer form) where the canonical is multi-char
// and a natural short-form has been carved out as a permanent ergonomic alias
// — see `rng` → `range` (nlp-engineer rerun8). Short-form aliases only land
// when the short doesn't shadow a plausible user binding, per the
// 3+ char-first convention published in AGENTS.md.
const BUILTIN_ALIASES: &[(&str, &str)] = &[
    // Math
    ("floor", "flr"),
    ("ceil", "cel"),
    ("round", "rou"),
    // `rand` is the universal short-form for random across C, Python (`random.random`
    // shortened in muscle memory), Rust (`rand` crate), Go (`math/rand`), JS
    // (`Math.random`), etc. Aliasing to canonical `rnd` cuts the round-vs-random
    // confusion event-trace-analyser rerun11 surfaced: agents read `rnd` as
    // "round" (drop-vowels of `round`) and reach for it when they want rounding.
    // With `rand` available, the unambiguous choice for random is `rand` and
    // for rounding is `rou`/`round`; `rnd` stays as the canonical for backward
    // compat. Costs 1 extra char vs `rnd`, accepted for trap-avoidance.
    ("rand", "rnd"),
    ("random", "rnd"),
    // `rng` is a short-form alias for the canonical `range` builtin. Personas
    // working in numeric / simulation / regression code reach for it first
    // because `range a b` is load-bearing and the 5-char hit is paid
    // repeatedly. Survey across examples and persona artifacts found zero
    // user bindings called `rng`, so this clears the PR #343 short-alias
    // guard (no plausible-user-binding shadow).
    ("rng", "range"),
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
    // `lset xs i v` is a discoverability alias for `lst xs i v`. Personas
    // reach for it from the `mset`/`lset` mental model (L↔M parallelism);
    // canonical name stays `lst`, so bytecode and `fmt` output are unchanged.
    ("lset", "lst"),
    ("unique", "unq"),
    ("filter", "flt"),
    ("fold", "fld"),
    // Note: no `count` → `ct` alias. `count` is a common user-fn name
    // (see examples/unq-numbers.ilo); aliasing it would trample on the
    // existing user code that defines `count` as a per-program helper.
    // Users wanting a long form can keep their own `count` function and
    // call `ct` directly when they want the builtin.
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
    ("regex_all", "rgxall"),
    ("regex_sub", "rgxsub"),
    ("read", "rd"),
    ("readlines", "rdl"),
    ("readbuf", "rdb"),
    ("write", "wr"),
    ("writelines", "wrl"),
    // Map ops — long-form hyphen aliases for the canonical short names.
    // ilo identifiers use hyphens, not underscores, so only hyphen forms
    // are valid surface syntax.
    ("map-get", "mget"),
    ("map-set", "mset"),
    ("map-has", "mhas"),
    ("map-del", "mdel"),
    // Alias-of-alias: map-keys / map-values resolve to the canonical
    // short forms `mkeys` / `mvals` directly (no two-hop needed since
    // resolve_alias is a single table lookup).
    ("map-keys", "mkeys"),
    ("map-values", "mvals"),
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
            start,
            end,
            step,
            body,
            ..
        } => {
            resolve_aliases_expr(start);
            resolve_aliases_expr(end);
            if let Some(s) = step {
                resolve_aliases_expr(s);
            }
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
        Stmt::Defer { expr, .. } => resolve_aliases_expr(expr),
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
        Expr::Ref(name) => {
            // Bare alias-as-value reference (e.g. `map rng xs` if a future
            // workload reaches for it). Rewrite to the canonical name so the
            // verifier sees the same fn-ref it would for a canonical builtin.
            // The dominant alias-as-call path is handled in the Call arm
            // above; this arm is the safety net for HOF arg positions and
            // any parser path that builds a `Ref` for an alias name.
            if let Some(canonical) = resolve_alias(name) {
                *name = canonical.to_string();
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
        Expr::Record { fields, .. } | Expr::AnonRecord { fields } => {
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
        Expr::MakeClosure { captures, .. } => {
            for cap in captures {
                resolve_aliases_expr(cap);
            }
        }
        Expr::Todo(inner) | Expr::Panic(inner) => resolve_aliases_expr(inner),
        Expr::Literal(_) | Expr::Field { .. } | Expr::Index { .. } => {}
    }
}

/// Desugar `xs.i` where `i` is a variable in scope into `at xs i`.
///
/// The parser builds `Expr::Field { object, field: "i" }` for `xs.i` because
/// at parse time we can't tell whether `xs` is a record (field access) or a
/// list (indexed access). If `i` is a bound variable in scope, the user almost
/// certainly meant indexed access, so we rewrite to a `Call` to the `at`
/// builtin. Record field access keeps working because record field names are
/// usually not also locals: we additionally guard against collisions by
/// refusing to rewrite when `field` matches a declared field on any record
/// type in the program.
///
/// Only rewrites the strict `.field` form, not the safe `.?field` form.
/// Multiple personas have flagged the resulting "field access on non-record
/// type L _" error as the single biggest token tax in list workloads.
pub fn desugar_dot_var_index(program: &mut Program) {
    // Collect every field name declared on any record type. These names act
    // as static field identifiers and must keep record-access semantics even
    // when shadowed by a local binding.
    let mut record_fields: std::collections::HashSet<String> = std::collections::HashSet::new();
    for decl in &program.declarations {
        if let Decl::TypeDef { fields, .. } = decl {
            for p in fields {
                record_fields.insert(p.name.clone());
            }
        }
        // Also collect field names from anonymous record literals so that
        // `r.name` where `name` happens to be a local variable is NOT
        // rewritten to `at r name` — anonymous records are still records.
        if let Decl::Function { body, .. } = decl {
            collect_anon_record_fields_stmts(body, &mut record_fields);
        }
    }

    for decl in &mut program.declarations {
        if let Decl::Function { params, body, .. } = decl {
            let mut scope: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
            for stmt in body {
                desugar_stmt(&mut stmt.node, &mut scope, &record_fields);
            }
        }
    }
}

fn desugar_stmt(stmt: &mut Stmt, scope: &mut Vec<String>, rf: &std::collections::HashSet<String>) {
    match stmt {
        Stmt::Let { name, value } => {
            desugar_expr(value, scope, rf);
            scope.push(name.clone());
        }
        Stmt::Expr(expr) => desugar_expr(expr, scope, rf),
        Stmt::Return(expr) => desugar_expr(expr, scope, rf),
        Stmt::Break(opt) => {
            if let Some(e) = opt {
                desugar_expr(e, scope, rf);
            }
        }
        Stmt::Continue => {}
        Stmt::Guard {
            condition,
            body,
            else_body,
            ..
        } => {
            desugar_expr(condition, scope, rf);
            let depth = scope.len();
            for s in body {
                desugar_stmt(&mut s.node, scope, rf);
            }
            scope.truncate(depth);
            if let Some(eb) = else_body {
                let depth = scope.len();
                for s in eb {
                    desugar_stmt(&mut s.node, scope, rf);
                }
                scope.truncate(depth);
            }
        }
        Stmt::Match { subject, arms } => {
            if let Some(e) = subject {
                desugar_expr(e, scope, rf);
            }
            for arm in arms {
                let depth = scope.len();
                match &arm.pattern {
                    Pattern::Err(b) | Pattern::Ok(b) => scope.push(b.clone()),
                    Pattern::TypeIs { binding, .. } => scope.push(binding.clone()),
                    _ => {}
                }
                for s in &mut arm.body {
                    desugar_stmt(&mut s.node, scope, rf);
                }
                scope.truncate(depth);
            }
        }
        Stmt::ForEach {
            binding,
            collection,
            body,
        } => {
            desugar_expr(collection, scope, rf);
            let depth = scope.len();
            scope.push(binding.clone());
            for s in body {
                desugar_stmt(&mut s.node, scope, rf);
            }
            scope.truncate(depth);
        }
        Stmt::ForRange {
            binding,
            start,
            end,
            step,
            body,
        } => {
            desugar_expr(start, scope, rf);
            desugar_expr(end, scope, rf);
            if let Some(st) = step {
                desugar_expr(st, scope, rf);
            }
            let depth = scope.len();
            scope.push(binding.clone());
            for s in body {
                desugar_stmt(&mut s.node, scope, rf);
            }
            scope.truncate(depth);
        }
        Stmt::While { condition, body } => {
            desugar_expr(condition, scope, rf);
            let depth = scope.len();
            for s in body {
                desugar_stmt(&mut s.node, scope, rf);
            }
            scope.truncate(depth);
        }
        Stmt::Destructure { bindings, value } => {
            desugar_expr(value, scope, rf);
            for b in bindings {
                scope.push(b.clone());
            }
        }
        Stmt::Defer { expr, .. } => desugar_expr(expr, scope, rf),
    }
}

fn desugar_expr(expr: &mut Expr, scope: &[String], rf: &std::collections::HashSet<String>) {
    // First, recurse into children. We do this before checking the current
    // node so nested `xs.i.j` chains get rewritten bottom-up.
    match expr {
        Expr::Field { object, .. } => desugar_expr(object, scope, rf),
        Expr::Index { object, .. } => desugar_expr(object, scope, rf),
        Expr::Call { args, .. } => {
            for a in args {
                desugar_expr(a, scope, rf);
            }
        }
        Expr::BinOp { left, right, .. } => {
            desugar_expr(left, scope, rf);
            desugar_expr(right, scope, rf);
        }
        Expr::UnaryOp { operand, .. } => desugar_expr(operand, scope, rf),
        Expr::Ok(inner) | Expr::Err(inner) => desugar_expr(inner, scope, rf),
        Expr::NilCoalesce { value, default } => {
            desugar_expr(value, scope, rf);
            desugar_expr(default, scope, rf);
        }
        Expr::List(items) => {
            for it in items {
                desugar_expr(it, scope, rf);
            }
        }
        Expr::Record { fields, .. } | Expr::AnonRecord { fields } => {
            for (_, v) in fields {
                desugar_expr(v, scope, rf);
            }
        }
        Expr::Match { subject, arms } => {
            if let Some(s) = subject {
                desugar_expr(s, scope, rf);
            }
            for arm in arms {
                // Arms get their own scope frame via desugar_stmt.
                let mut local_scope: Vec<String> = scope.to_vec();
                match &arm.pattern {
                    Pattern::Err(b) | Pattern::Ok(b) => local_scope.push(b.clone()),
                    Pattern::TypeIs { binding, .. } => local_scope.push(binding.clone()),
                    _ => {}
                }
                for s in &mut arm.body {
                    desugar_stmt(&mut s.node, &mut local_scope, rf);
                }
            }
        }
        Expr::With { object, updates } => {
            desugar_expr(object, scope, rf);
            for (_, v) in updates {
                desugar_expr(v, scope, rf);
            }
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            desugar_expr(condition, scope, rf);
            desugar_expr(then_expr, scope, rf);
            desugar_expr(else_expr, scope, rf);
        }
        Expr::MakeClosure { captures, .. } => {
            for c in captures {
                desugar_expr(c, scope, rf);
            }
        }
        Expr::Todo(inner) | Expr::Panic(inner) => desugar_expr(inner, scope, rf),
        Expr::Literal(_) | Expr::Ref(_) => {}
    }

    // Now check if this Field node is `obj.<var>` where `<var>` is in scope
    // and not also a record field name. If so, rewrite to `at obj var`.
    if let Expr::Field {
        object,
        field,
        safe,
    } = expr
    {
        if !*safe && scope.iter().any(|b| b == field) && !rf.contains(field) {
            let obj = std::mem::replace(object.as_mut(), Expr::Literal(Literal::Nil));
            let field_name = field.clone();
            *expr = Expr::Call {
                function: "at".to_string(),
                args: vec![obj, Expr::Ref(field_name)],
                unwrap: UnwrapMode::None,
            };
        }
    }
}

/// Collect field names from all AnonRecord literals in a statement list.
fn collect_anon_record_fields_stmts(
    stmts: &[Spanned<Stmt>],
    out: &mut std::collections::HashSet<String>,
) {
    for stmt in stmts {
        collect_anon_record_fields_stmt(&stmt.node, out);
    }
}

fn collect_anon_record_fields_stmt(stmt: &Stmt, out: &mut std::collections::HashSet<String>) {
    match stmt {
        Stmt::Let { value, .. } => collect_anon_record_fields_expr(value, out),
        Stmt::Expr(e) | Stmt::Return(e) => collect_anon_record_fields_expr(e, out),
        Stmt::Break(Some(e)) => collect_anon_record_fields_expr(e, out),
        Stmt::Guard {
            condition,
            body,
            else_body,
            ..
        } => {
            collect_anon_record_fields_expr(condition, out);
            collect_anon_record_fields_stmts(body, out);
            if let Some(eb) = else_body {
                collect_anon_record_fields_stmts(eb, out);
            }
        }
        Stmt::While { condition, body } => {
            collect_anon_record_fields_expr(condition, out);
            collect_anon_record_fields_stmts(body, out);
        }
        Stmt::ForEach {
            collection, body, ..
        } => {
            collect_anon_record_fields_expr(collection, out);
            collect_anon_record_fields_stmts(body, out);
        }
        Stmt::Destructure { value, .. } => collect_anon_record_fields_expr(value, out),
        _ => {}
    }
}

fn collect_anon_record_fields_expr(expr: &Expr, out: &mut std::collections::HashSet<String>) {
    match expr {
        Expr::AnonRecord { fields } => {
            for (name, val) in fields {
                out.insert(name.clone());
                collect_anon_record_fields_expr(val, out);
            }
        }
        Expr::Record { fields, .. } => {
            for (_, val) in fields {
                collect_anon_record_fields_expr(val, out);
            }
        }
        Expr::Call { args, .. } => {
            for arg in args {
                collect_anon_record_fields_expr(arg, out);
            }
        }
        Expr::BinOp { left, right, .. } => {
            collect_anon_record_fields_expr(left, out);
            collect_anon_record_fields_expr(right, out);
        }
        Expr::UnaryOp { operand, .. } => collect_anon_record_fields_expr(operand, out),
        Expr::Field { object, .. } => collect_anon_record_fields_expr(object, out),
        Expr::Index { object, .. } => collect_anon_record_fields_expr(object, out),
        Expr::With { object, updates } => {
            collect_anon_record_fields_expr(object, out);
            for (_, val) in updates {
                collect_anon_record_fields_expr(val, out);
            }
        }
        Expr::List(items) => {
            for item in items {
                collect_anon_record_fields_expr(item, out);
            }
        }
        Expr::Ok(e) | Expr::Err(e) => collect_anon_record_fields_expr(e, out),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_anon_record_fields_expr(condition, out);
            collect_anon_record_fields_expr(then_expr, out);
            collect_anon_record_fields_expr(else_expr, out);
        }
        Expr::NilCoalesce { value, default } => {
            collect_anon_record_fields_expr(value, out);
            collect_anon_record_fields_expr(default, out);
        }
        _ => {}
    }
}

/// Cycle-capability classifier for runtime values of a given static type.
///
/// Background: ilo's runtime is reference-counted (Arc in the tree
/// interpreter, custom RC on `HeapObj` in the VM). Pure RC cannot reclaim
/// reference cycles (A -> B -> A). Most RC languages pair RC with a cycle
/// collector to handle this. ilo deliberately does NOT — the surface
/// language is structurally cycle-free:
///
///   * Records are immutable after construction. `with` allocates a fresh
///     record; there is no field-assignment expression. Fields are bound
///     from already-evaluated values, so a field cannot refer forward to
///     the record being built.
///   * Lists and maps are persistent. Mutation goes through copy-on-share
///     (`Arc::make_mut`, fresh `HeapObj` allocation). You cannot install
///     a reference back to a holder you no longer have a write handle to.
///   * Closures capture by value.
///   * Numbers, booleans, text, sums-of-strings, nil are inline / immutable.
///
/// This classifier exists as a foundation. It defines the invariant
/// explicitly and gives us a regression surface if a future language
/// change quietly introduces a cycle-forming construct. It is also the
/// hook a future cycle collector would consult to prune immutable types.
///
/// Default policy: when in doubt, return `true` (cycle-capable). It is
/// always sound to mark a type cycle-capable; the cost is unnecessary
/// scanning. The unsound case is the reverse: marking a cycle-capable
/// type clean would let a real cycle leak forever.
impl Type {
    /// Returns true if a runtime value of this type could possibly
    /// participate in a reference cycle under ilo's current memory model.
    ///
    /// `resolve_record` resolves a record type name to its field types.
    /// Pass `&|_| None` to treat all `Named` references conservatively
    /// (cycle-capable).
    pub fn can_form_cycle<F>(&self, resolve_record: &F) -> bool
    where
        F: Fn(&str) -> Option<Vec<Type>>,
    {
        fn rec<F>(ty: &Type, seen: &mut Vec<String>, resolve_record: &F) -> bool
        where
            F: Fn(&str) -> Option<Vec<Type>>,
        {
            match ty {
                // Inline primitives.
                Type::Number | Type::Bool | Type::Sum(_) => false,
                // Immutable shared bytes; no embedded references.
                Type::Text => false,
                // No information at the type level.
                Type::Any => true,
                // Closures capture by value. Without a per-closure capture
                // type list at this layer we conservatively mark Fn as
                // cycle-capable. Param/return types here describe the
                // call-site arrow, not the captured environment.
                Type::Fn(_, _) => true,
                // Wrappers inherit cycle-capability from their inner type.
                Type::Optional(inner) | Type::List(inner) => rec(inner, seen, resolve_record),
                Type::Result(ok, err) => {
                    rec(ok, seen, resolve_record) || rec(err, seen, resolve_record)
                }
                Type::Map(k, v) => rec(k, seen, resolve_record) || rec(v, seen, resolve_record),
                Type::Named(name) => {
                    if seen.iter().any(|s| s == name) {
                        // Closed loop on the resolution path — by definition
                        // cycle-capable.
                        return true;
                    }
                    match resolve_record(name.as_str()) {
                        Some(fields) => {
                            seen.push(name.clone());
                            let result = fields.iter().any(|f| rec(f, seen, resolve_record));
                            seen.pop();
                            result
                        }
                        // Unknown name (type variable, missing record,
                        // unresolved alias): conservative default.
                        None => true,
                    }
                }
            }
        }

        let mut seen: Vec<String> = Vec::new();
        rec(self, &mut seen, resolve_record)
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
            type_params: vec![],
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
            parse_failed_fns: Default::default(),
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
                type_params: vec![],
                name: "f".to_string(),
                params: vec![],
                return_type: Type::Number,
                body: vec![Spanned::unknown(Stmt::While {
                    condition: Expr::Call {
                        function: "length".to_string(),
                        args: vec![Expr::Ref("x".to_string())],
                        unwrap: UnwrapMode::None,
                    },
                    body: vec![Spanned::unknown(Stmt::Expr(Expr::Call {
                        function: "length".to_string(),
                        args: vec![Expr::Ref("y".to_string())],
                        unwrap: UnwrapMode::None,
                    }))],
                })],
                span: Span::UNKNOWN,
            }],
            source: None,
            parse_failed_fns: Default::default(),
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
                type_params: vec![],
                name: "f".to_string(),
                params: vec![],
                return_type: Type::Number,
                body: vec![Spanned::unknown(Stmt::Return(Expr::Call {
                    function: "length".to_string(),
                    args: vec![Expr::Ref("x".to_string())],
                    unwrap: UnwrapMode::None,
                }))],
                span: Span::UNKNOWN,
            }],
            source: None,
            parse_failed_fns: Default::default(),
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
                type_params: vec![],
                name: "f".to_string(),
                params: vec![],
                return_type: Type::Number,
                body: vec![Spanned::unknown(Stmt::Destructure {
                    bindings: vec!["a".to_string(), "b".to_string()],
                    value: Expr::Call {
                        function: "length".to_string(),
                        args: vec![Expr::Ref("x".to_string())],
                        unwrap: UnwrapMode::None,
                    },
                })],
                span: Span::UNKNOWN,
            }],
            source: None,
            parse_failed_fns: Default::default(),
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
                type_params: vec![],
                name: "f".to_string(),
                params: vec![],
                return_type: Type::Number,
                body: vec![Spanned::unknown(Stmt::Break(Some(Expr::Call {
                    function: "length".to_string(),
                    args: vec![Expr::Ref("x".to_string())],
                    unwrap: UnwrapMode::None,
                })))],
                span: Span::UNKNOWN,
            }],
            source: None,
            parse_failed_fns: Default::default(),
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
                type_params: vec![],
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
            parse_failed_fns: Default::default(),
        };
        resolve_aliases(&mut prog);
        assert!(matches!(&prog.declarations[0], Decl::Function { body, .. } if body.len() == 2));
    }

    #[test]
    fn resolve_aliases_nil_coalesce_expr() {
        // L465-467: NilCoalesce variant in resolve_aliases_expr
        let mut prog = Program {
            declarations: vec![Decl::Function {
                type_params: vec![],
                name: "f".to_string(),
                params: vec![],
                return_type: Type::Number,
                body: vec![Spanned::unknown(Stmt::Expr(Expr::NilCoalesce {
                    value: Box::new(Expr::Call {
                        function: "length".to_string(),
                        args: vec![Expr::Ref("x".to_string())],
                        unwrap: UnwrapMode::None,
                    }),
                    default: Box::new(Expr::Call {
                        function: "reverse".to_string(),
                        args: vec![Expr::Ref("y".to_string())],
                        unwrap: UnwrapMode::None,
                    }),
                }))],
                span: Span::UNKNOWN,
            }],
            source: None,
            parse_failed_fns: Default::default(),
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
                type_params: vec![],
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
                            unwrap: UnwrapMode::None,
                        },
                    )],
                }))],
                span: Span::UNKNOWN,
            }],
            source: None,
            parse_failed_fns: Default::default(),
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
                type_params: vec![],
                name: "f".to_string(),
                params: vec![],
                return_type: Type::Number,
                body: vec![Spanned::unknown(Stmt::Expr(Expr::Match {
                    subject: Some(Box::new(Expr::Call {
                        function: "length".to_string(),
                        args: vec![Expr::Ref("x".to_string())],
                        unwrap: UnwrapMode::None,
                    })),
                    arms: vec![MatchArm {
                        pattern: Pattern::Wildcard,
                        body: vec![Spanned::unknown(Stmt::Expr(Expr::Call {
                            function: "reverse".to_string(),
                            args: vec![Expr::Ref("y".to_string())],
                            unwrap: UnwrapMode::None,
                        }))],
                    }],
                }))],
                span: Span::UNKNOWN,
            }],
            source: None,
            parse_failed_fns: Default::default(),
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
                type_params: vec![],
                name: "f".to_string(),
                params: vec![],
                return_type: Type::Number,
                body: vec![Spanned::unknown(Stmt::Expr(Expr::With {
                    object: Box::new(Expr::Call {
                        function: "length".to_string(),
                        args: vec![Expr::Ref("x".to_string())],
                        unwrap: UnwrapMode::None,
                    }),
                    updates: vec![(
                        "a".to_string(),
                        Expr::Call {
                            function: "reverse".to_string(),
                            args: vec![Expr::Ref("y".to_string())],
                            unwrap: UnwrapMode::None,
                        },
                    )],
                }))],
                span: Span::UNKNOWN,
            }],
            source: None,
            parse_failed_fns: Default::default(),
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
                type_params: vec![],
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
            parse_failed_fns: Default::default(),
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
                type_params: vec![],
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
                            unwrap: UnwrapMode::None,
                        }))],
                    }],
                })],
                span: Span::UNKNOWN,
            }],
            source: None,
            parse_failed_fns: Default::default(),
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
                type_params: vec![],
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
                            unwrap: UnwrapMode::None,
                        }))],
                    }],
                }))],
                span: Span::UNKNOWN,
            }],
            source: None,
            parse_failed_fns: Default::default(),
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

    fn parse_one(src: &str) -> Program {
        let tokens = crate::lexer::lex(src).unwrap();
        let token_spans: Vec<(crate::lexer::Token, Span)> = tokens
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
            .collect();
        let (mut prog, errors) = crate::parser::parse(token_spans);
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        resolve_aliases(&mut prog);
        prog
    }

    #[test]
    fn desugar_rewrites_xs_dot_i_when_i_is_param() {
        let mut prog = parse_one("pick xs:L n i:n>n;xs.i\n");
        desugar_dot_var_index(&mut prog);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        // `xs.i` should now be `at xs i`.
        let Stmt::Expr(Expr::Call { function, args, .. }) = &body[0].node else {
            panic!("expected Call after desugar, got {:?}", body[0].node)
        };
        assert_eq!(function, "at");
        assert_eq!(args.len(), 2);
        assert!(matches!(&args[0], Expr::Ref(n) if n == "xs"));
        assert!(matches!(&args[1], Expr::Ref(n) if n == "i"));
    }

    #[test]
    fn desugar_leaves_xs_dot_0_alone() {
        let mut prog = parse_one("first xs:L n>n;xs.0\n");
        desugar_dot_var_index(&mut prog);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        // Literal index stays as Expr::Index (parser already handles this).
        assert!(matches!(
            &body[0].node,
            Stmt::Expr(Expr::Index { index: 0, .. })
        ));
    }

    #[test]
    fn desugar_preserves_record_field_when_field_is_param() {
        // `name` is both a parameter and a declared field on `person`.
        // The collision guard must keep `p.name` as a Field access.
        let mut prog = parse_one(
            "type person{name:t;age:n}\n\ngreet name:t>t;p=person name:\"Alice\" age:30;p.name\n",
        );
        desugar_dot_var_index(&mut prog);
        let Decl::Function { body, .. } = &prog.declarations[1] else {
            panic!(
                "expected greet at index 1, declarations: {:?}",
                prog.declarations.len()
            )
        };
        // Last stmt is `p.name`, must still be Expr::Field.
        let last = &body[body.len() - 1].node;
        assert!(
            matches!(last, Stmt::Expr(Expr::Field { field, .. }) if field == "name"),
            "expected Field after desugar, got {last:?}"
        );
    }

    #[test]
    fn desugar_rewrites_inside_range_loop_body() {
        let mut prog = parse_one("mysum xs:L n>n;s=0;@i 0..(len xs){v=xs.i;s=+s v};+s 0\n");
        desugar_dot_var_index(&mut prog);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        // Find the @i loop and inspect its body for the rewritten Call.
        let mut found_at_call = false;
        for stmt in body {
            if let Stmt::ForRange {
                body: loop_body, ..
            } = &stmt.node
            {
                for ls in loop_body {
                    if let Stmt::Let {
                        value: Expr::Call { function, args, .. },
                        ..
                    } = &ls.node
                    {
                        if function == "at"
                            && args.len() == 2
                            && matches!(&args[0], Expr::Ref(n) if n == "xs")
                            && matches!(&args[1], Expr::Ref(n) if n == "i")
                        {
                            found_at_call = true;
                        }
                    }
                }
            }
        }
        assert!(found_at_call, "expected `at xs i` rewrite inside loop body");
    }

    #[test]
    fn desugar_leaves_field_when_field_not_in_scope() {
        // `name` is not a param, not a binding, not a declared field.
        // The parser leaves it as Field; desugar should also leave it
        // (verifier will then flag as expected for the user's program).
        let mut prog = parse_one("f x:n>n;p=x;p.name\n");
        desugar_dot_var_index(&mut prog);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let last = &body[body.len() - 1].node;
        assert!(
            matches!(last, Stmt::Expr(Expr::Field { field, .. }) if field == "name"),
            "expected Field unchanged when field name not in scope, got {last:?}"
        );
    }

    // ---- can_form_cycle tests ----
    //
    // The whole point of the classifier is to make our cycle-freedom
    // invariant testable. If any of these change, the language has grown
    // a new cycle-forming construct and the runtime needs a cycle
    // collector before that change ships.

    fn no_records(_: &str) -> Option<Vec<Type>> {
        None
    }

    #[test]
    fn primitives_cannot_cycle() {
        let resolver = |s: &str| no_records(s);
        assert!(!Type::Number.can_form_cycle(&resolver));
        assert!(!Type::Bool.can_form_cycle(&resolver));
        assert!(!Type::Text.can_form_cycle(&resolver));
        assert!(!Type::Sum(vec!["a".into(), "b".into()]).can_form_cycle(&resolver));
    }

    #[test]
    fn lists_and_maps_of_primitives_cannot_cycle() {
        let resolver = |s: &str| no_records(s);
        assert!(!Type::List(Box::new(Type::Number)).can_form_cycle(&resolver));
        assert!(!Type::List(Box::new(Type::Text)).can_form_cycle(&resolver));
        assert!(!Type::Map(Box::new(Type::Text), Box::new(Type::Number)).can_form_cycle(&resolver));
        assert!(!Type::Optional(Box::new(Type::Number)).can_form_cycle(&resolver));
        assert!(
            !Type::Result(Box::new(Type::Number), Box::new(Type::Text)).can_form_cycle(&resolver)
        );
    }

    #[test]
    fn any_is_conservative() {
        let resolver = |s: &str| no_records(s);
        assert!(Type::Any.can_form_cycle(&resolver));
        assert!(Type::List(Box::new(Type::Any)).can_form_cycle(&resolver));
    }

    #[test]
    fn fn_is_conservative() {
        // Closures capture by value. The capture types aren't exposed at the
        // Type level, so the classifier marks Fn cycle-capable.
        let resolver = |s: &str| no_records(s);
        assert!(Type::Fn(vec![Type::Number], Box::new(Type::Number)).can_form_cycle(&resolver));
    }

    #[test]
    fn unknown_named_is_conservative() {
        let resolver = |s: &str| no_records(s);
        assert!(Type::Named("Whatever".into()).can_form_cycle(&resolver));
    }

    #[test]
    fn record_of_primitives_cannot_cycle() {
        let resolver = |s: &str| match s {
            "Point" => Some(vec![Type::Number, Type::Number]),
            _ => None,
        };
        assert!(!Type::Named("Point".into()).can_form_cycle(&resolver));
    }

    #[test]
    fn record_with_primitive_list_cannot_cycle() {
        let resolver = |s: &str| match s {
            "Bag" => Some(vec![Type::Text, Type::List(Box::new(Type::Number))]),
            _ => None,
        };
        assert!(!Type::Named("Bag".into()).can_form_cycle(&resolver));
    }

    #[test]
    fn record_containing_any_field_can_cycle() {
        let resolver = |s: &str| match s {
            "Box" => Some(vec![Type::Any]),
            _ => None,
        };
        assert!(Type::Named("Box".into()).can_form_cycle(&resolver));
    }

    #[test]
    fn record_containing_function_field_can_cycle() {
        let resolver = |s: &str| match s {
            "Handler" => Some(vec![Type::Fn(vec![Type::Number], Box::new(Type::Number))]),
            _ => None,
        };
        assert!(Type::Named("Handler".into()).can_form_cycle(&resolver));
    }

    #[test]
    fn self_referential_record_marked_cycle_capable() {
        // type Node { next:Node } — not constructible today (records are
        // immutable, so you can't tie the knot), but the classifier still
        // marks the *type* cycle-capable. If we ever add a primitive that
        // would let you build one, the runtime needs to know.
        let resolver = |s: &str| match s {
            "Node" => Some(vec![Type::Named("Node".into())]),
            _ => None,
        };
        assert!(Type::Named("Node".into()).can_form_cycle(&resolver));
    }

    #[test]
    fn mutually_recursive_records_marked_cycle_capable() {
        let resolver = |s: &str| match s {
            "A" => Some(vec![Type::Named("B".into())]),
            "B" => Some(vec![Type::Named("A".into())]),
            _ => None,
        };
        assert!(Type::Named("A".into()).can_form_cycle(&resolver));
        assert!(Type::Named("B".into()).can_form_cycle(&resolver));
    }

    #[test]
    fn list_of_record_inherits_record_capability() {
        let primitive_record = |s: &str| match s {
            "Point" => Some(vec![Type::Number, Type::Number]),
            _ => None,
        };
        assert!(
            !Type::List(Box::new(Type::Named("Point".into()))).can_form_cycle(&primitive_record)
        );

        let cyclic_record = |s: &str| match s {
            "Node" => Some(vec![Type::Named("Node".into())]),
            _ => None,
        };
        assert!(Type::List(Box::new(Type::Named("Node".into()))).can_form_cycle(&cyclic_record));
    }
}
