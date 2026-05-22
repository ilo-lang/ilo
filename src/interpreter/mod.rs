use crate::ast::*;
use crate::builtins::{Builtin, CharAtResult, char_at_signed};
use crate::caps::Caps;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub mod http_wasm;
pub mod json;

// ── Trace hook ────────────────────────────────────────────────────────────────

/// One trace event emitted after each statement executes.
/// Schema matches the ILO-72 proposal:
/// `{"schemaVersion":1,"line":N,"stmt":"...","bindings":{...},"result":...}`
#[derive(Debug)]
pub struct TraceEvent {
    /// 1-based source line of the statement start, or 0 if unknown.
    pub line: usize,
    /// Source text of the statement (trimmed), or empty if unavailable.
    pub stmt: String,
    /// All variable bindings visible in the current scope after the statement.
    pub bindings: Vec<(String, Value)>,
    /// The value produced by the statement (Nil for side-effect statements).
    pub result: Value,
}

/// One trace event emitted after each sub-expression evaluates (depth=expr).
#[derive(Debug)]
pub struct ExprTraceEvent {
    /// 1-based source line, or 0 if unknown.
    pub line: usize,
    /// Source text of the expression (trimmed), or empty if unavailable.
    pub expr: String,
    /// Names referenced by this expression (Ref nodes touched).
    pub refs: Vec<String>,
    /// The value produced by this expression.
    pub result: Value,
}

// Thread-local trace sink. When `Some`, `eval_body` fires it after each
// statement. Set to `Some` by `run_with_trace` and cleared on return.
std::thread_local! {
    #[allow(clippy::type_complexity)]
    pub(crate) static TRACE_HOOK: std::cell::RefCell<Option<Box<dyn FnMut(TraceEvent)>>> =
        const { std::cell::RefCell::new(None) };

    // Source text used to look up statement spans; set alongside TRACE_HOOK.
    pub(crate) static TRACE_SOURCE: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };

    // Expression-level hook (depth=expr).
    #[allow(clippy::type_complexity)]
    static EXPR_TRACE_HOOK: std::cell::RefCell<Option<Box<dyn FnMut(ExprTraceEvent)>>> =
        const { std::cell::RefCell::new(None) };

    // Current statement span — updated by eval_body so eval_expr can use it.
    static CURRENT_STMT_SPAN: std::cell::RefCell<Span> =
        const { std::cell::RefCell::new(Span { start: 0, end: 0 }) };
}

/// Returns true if a trace hook is currently installed.
/// Used by the VM's OP_STMT handler to skip event collection on the hot path.
#[inline]
pub(crate) fn trace_hook_active() -> bool {
    TRACE_HOOK.with(|h| h.borrow().is_some())
}

/// Fire the installed trace hook with a pre-built event.
/// No-op if no hook is installed (guard is inside TRACE_HOOK.with).
#[inline]
pub(crate) fn fire_trace_hook(ev: TraceEvent) {
    TRACE_HOOK.with(|h| {
        if let Some(ref mut hook) = *h.borrow_mut() {
            hook(ev);
        }
    });
}

/// Run `program` with a per-statement trace callback.
/// `on_event` is called after each statement in the entry function body.
pub fn run_with_trace<F>(
    program: &Program,
    func_name: Option<&str>,
    args: Vec<Value>,
    on_event: F,
) -> Result<Value>
where
    F: FnMut(TraceEvent) + 'static,
{
    run_with_trace_opts(
        program,
        func_name,
        args,
        on_event,
        None::<fn(ExprTraceEvent)>,
    )
}

/// Run `program` with per-statement and optional per-expression trace callbacks.
pub fn run_with_trace_opts<F, G>(
    program: &Program,
    func_name: Option<&str>,
    args: Vec<Value>,
    on_stmt: F,
    on_expr: Option<G>,
) -> Result<Value>
where
    F: FnMut(TraceEvent) + 'static,
    G: FnMut(ExprTraceEvent) + 'static,
{
    // Install the hooks.
    TRACE_HOOK.with(|h| {
        *h.borrow_mut() = Some(Box::new(on_stmt));
    });
    TRACE_SOURCE.with(|s| {
        *s.borrow_mut() = program.source.clone();
    });
    if let Some(expr_hook) = on_expr {
        EXPR_TRACE_HOOK.with(|h| {
            *h.borrow_mut() = Some(Box::new(expr_hook));
        });
    }

    let result = run_with_env(program, func_name, args, Env::new());

    // Always clear the hooks, even on error.
    TRACE_HOOK.with(|h| {
        *h.borrow_mut() = None;
    });
    TRACE_SOURCE.with(|s| {
        *s.borrow_mut() = None;
    });
    EXPR_TRACE_HOOK.with(|h| {
        *h.borrow_mut() = None;
    });

    result
}

/// A typed key for `Value::Map` and `HeapObj::Map`.
///
/// Two variants — `Text` for string keys and `Int` for integer keys.
/// Floats are normalised to `Int` at the builtin boundary (`floor` + `as i64`),
/// matching the indexing convention of `at xs i`. NaN/Infinity keys are
/// rejected with a runtime error there, so they never reach `MapKey`.
///
/// `Bool` is intentionally not represented: token-cost wise, bool maps are
/// always shorter to express as a two-arm `?` than as a two-entry map, and the
/// surface syntax for bool literals as map keys would add lexer ambiguity.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum MapKey {
    Text(String),
    Int(i64),
}

impl MapKey {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            MapKey::Text(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            MapKey::Int(n) => Some(*n),
            _ => None,
        }
    }

    /// Stringified form used by serialisations that need a textual key
    /// (e.g. JSON object keys, CSV headers, the `Display` impl).
    pub fn to_display_string(&self) -> String {
        match self {
            MapKey::Text(s) => s.clone(),
            MapKey::Int(n) => n.to_string(),
        }
    }

    /// Normalise an ilo `Value` into a `MapKey`. Used at the builtin boundary
    /// for `mget`, `mset`, `mhas`, `mdel`. Numbers floor to i64 to match
    /// `at xs i`; non-finite numbers are rejected.
    pub fn from_value(v: &Value, op_name: &str) -> std::result::Result<Self, RuntimeError> {
        match v {
            Value::Text(s) => Ok(MapKey::Text((**s).clone())),
            Value::Number(n) => {
                if !n.is_finite() {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("{op_name}: numeric key must be finite, got {n}"),
                    ));
                }
                Ok(MapKey::Int(n.floor() as i64))
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("{op_name}: key must be text or number, got {other:?}"),
            )),
        }
    }
}

impl std::fmt::Display for MapKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MapKey::Text(s) => write!(f, "{s}"),
            MapKey::Int(n) => write!(f, "{n}"),
        }
    }
}

/// Total ordering for deterministic iteration order in `mkeys`/`mvals`.
/// In well-typed code maps are homogeneous, so the cross-variant ordering
/// (Int < Text) is academic — it just keeps tests deterministic if a poorly
/// typed map mixes both.
impl Ord for MapKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (MapKey::Int(a), MapKey::Int(b)) => a.cmp(b),
            (MapKey::Text(a), MapKey::Text(b)) => a.cmp(b),
            (MapKey::Int(_), MapKey::Text(_)) => std::cmp::Ordering::Less,
            (MapKey::Text(_), MapKey::Int(_)) => std::cmp::Ordering::Greater,
        }
    }
}

impl PartialOrd for MapKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Convert a `MapKey` back to a `Value` for use in builtin returns such as
/// `mkeys`. Text → `Value::Text`, Int → `Value::Number(f64)`.
pub fn map_key_to_value(k: &MapKey) -> Value {
    match k {
        MapKey::Text(s) => Value::Text(Arc::new(s.clone())),
        MapKey::Int(n) => Value::Number(*n as f64),
    }
}

type StdinLinesInner =
    Arc<Mutex<Box<dyn Iterator<Item = std::result::Result<String, std::io::Error>> + Send>>>;

/// A lazy handle to stdin's line iterator.
///
/// Wraps a `BufRead::lines()` iterator behind `Arc<Mutex<>>` so that
/// `Value::LazyStdinLines` can be `Clone` (cheaply: only the Arc refcount
/// is bumped) and `PartialEq` (identity: two handles are equal iff they
/// share the same underlying stdin). Produced by `for-line stdin` and
/// consumed by the tree-walker's `Stmt::ForEach` arm, which calls
/// `next()` on each iteration rather than collecting all lines upfront.
///
/// On WASM the variant is never constructed (the builtin returns Err early).
/// The `Debug` impl shows `<stdin-lines>` to keep output readable.
#[allow(clippy::type_complexity)]
pub struct StdinLinesHandle {
    inner: StdinLinesInner,
}

impl std::fmt::Debug for StdinLinesHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<stdin-lines>")
    }
}

impl Clone for StdinLinesHandle {
    fn clone(&self) -> Self {
        StdinLinesHandle {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl PartialEq for StdinLinesHandle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

#[cfg(not(target_family = "wasm"))]
impl Default for StdinLinesHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl StdinLinesHandle {
    /// Create a new handle owning a locked stdin lines iterator.
    #[cfg(not(target_family = "wasm"))]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        use std::io::{BufRead, BufReader};
        // Wrap stdin in a BufReader (which is Send) rather than holding a
        // StdinLock (which is not Send). A single ilo program is
        // single-threaded on the hot path, so the per-read locking that
        // Stdin does internally is fine.
        let reader = BufReader::new(std::io::stdin());
        let stdin_box: Box<
            dyn Iterator<Item = std::result::Result<String, std::io::Error>> + Send,
        > = Box::new(reader.lines());
        StdinLinesHandle {
            inner: Arc::new(Mutex::new(stdin_box)),
        }
    }

    /// Pull the next line from the underlying iterator.
    pub fn next_line(&self) -> Option<std::result::Result<String, std::io::Error>> {
        self.inner
            .lock()
            .expect("StdinLinesHandle lock poisoned")
            .next()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    Text(Arc<String>),
    Bool(bool),
    Nil,
    List(Arc<Vec<Value>>),
    Map(Arc<HashMap<MapKey, Value>>),
    Record {
        type_name: String,
        fields: HashMap<String, Value>,
    },
    Ok(Box<Value>),
    Err(Box<Value>),
    /// Capability world token — produced by the `world` builtin.
    /// Carries the four boolean capability flags derived from CLI `--allow-*`
    /// flags at startup. Passed to I/O-performing functions as an explicit
    /// proof-of-authority parameter (Zig/Zero pattern).
    World {
        net: bool,
        read: bool,
        write: bool,
        run: bool,
    },
    /// A reference to a named function — produced when a function name is used as a value.
    FnRef(String),
    /// A closure: a named (lifted) function plus by-value capture snapshots.
    ///
    /// Produced by `Expr::MakeClosure` when an inline lambda `(params>ret;body)`
    /// closes over enclosing-scope variables. Closure-aware HOFs (srt, map,
    /// flt, fld, grp, uniqby, partition, flatmap) detect this and append the
    /// captures after the per-item args at call time. The lifted function's
    /// param list is `[original_params..., capture_params...]`, so the
    /// captures naturally line up as trailing args.
    Closure {
        fn_name: String,
        captures: Vec<Value>,
    },
    /// A tagged variant value from a named sum type declaration.
    /// `Circle 5.0` → `Variant { type_name: "shape", tag: "circle", payload: Some(Number(5.0)) }`
    Variant {
        type_name: String,
        tag: String,
        payload: Option<Box<Value>>,
    },
    /// Lazy stdin line iterator.  Produced by `for-line stdin`.
    /// Consumed by `Stmt::ForEach`: each iteration calls `next_line()`,
    /// so lines are read one at a time as the loop runs — stdin is never
    /// fully buffered.  On WASM the builtin returns `Err` before this
    /// variant is constructed.
    LazyStdinLines(StdinLinesHandle),
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Number(n) => {
                if *n == (*n as i64) as f64 {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{}", n)
                }
            }
            Value::Text(s) => write!(f, "{}", s),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Nil => write!(f, "nil"),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            Value::Record { type_name, fields } => {
                // Sort keys lexicographically so Display is deterministic
                // across engines (tree/VM/Cranelift). The underlying field
                // storage is a HashMap, whose iteration order varies; agents
                // diffing `prnt`/`fmt` output across engines need a stable
                // canonical order. See ilo_assessment_feedback #5bg.
                write!(f, "{} {{", type_name)?;
                let mut keys: Vec<&String> = fields.keys().collect();
                keys.sort();
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", k, fields[*k])?;
                }
                write!(f, "}}")
            }
            Value::Map(m) => {
                write!(f, "{{")?;
                let mut keys: Vec<&MapKey> = m.keys().collect();
                keys.sort();
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        write!(f, "; ")?;
                    }
                    write!(f, "{}: {}", k, m[*k])?;
                }
                write!(f, "}}")
            }
            Value::Ok(v) => write!(f, "~{}", v),
            Value::Err(v) => write!(f, "^{}", v),
            Value::World {
                net,
                read,
                write,
                run,
            } => {
                write!(
                    f,
                    "World {{net: {net}, read: {read}, write: {write}, run: {run}}}"
                )
            }
            Value::FnRef(name) => write!(f, "<fn:{}>", name),
            Value::Closure { fn_name, captures } => {
                write!(f, "<closure:{}[", fn_name)?;
                for (i, c) in captures.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", c)?;
                }
                write!(f, "]>")
            }
            Value::Variant { tag, payload, .. } => match payload {
                Some(p) => write!(f, "{tag}({p})"),
                None => write!(f, "{tag}"),
            },
            Value::LazyStdinLines(_) => write!(f, "<stdin-lines>"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Runtime error: {message}")]
pub struct RuntimeError {
    pub code: &'static str,
    pub message: String,
    pub span: Option<crate::ast::Span>,
    pub call_stack: Vec<String>,
    /// When set, the `!` operator is propagating an Err value — not a real error.
    pub propagate_value: Option<Box<Value>>,
}

impl RuntimeError {
    fn new(code: &'static str, msg: impl Into<String>) -> Self {
        RuntimeError {
            code,
            message: msg.into(),
            span: None,
            call_stack: Vec::new(),
            propagate_value: None,
        }
    }
}

type Result<T> = std::result::Result<T, RuntimeError>;

struct Env {
    /// Flat variable store — all scopes in one Vec. Each entry is (name, value).
    vars: Vec<(String, Value)>,
    /// Stack of indices into `vars` marking where each scope starts.
    scope_marks: Vec<usize>,
    functions: HashMap<String, Decl>,
    /// Variant constructors from `Decl::SumType`. Maps variant name → (type_name, has_payload).
    /// Used by `call_function` to construct `Value::Variant`.
    sum_variants: HashMap<String, (String, bool)>,
    call_stack: Vec<String>,
    tool_provider: Option<std::sync::Arc<dyn crate::tools::ToolProvider>>,
    #[cfg(feature = "tools")]
    tokio_runtime: Option<std::sync::Arc<tokio::runtime::Runtime>>,
    /// CLI capability policy — checked at IO builtin call sites.
    caps: Arc<Caps>,
    /// Per-call-frame defer stack.  Each entry is `(expr_clone, kind)`.
    /// Pushed when a `Stmt::Defer` is executed; drained LIFO at function exit.
    /// Outer frames are saved/restored by `call_function`.
    defer_stack: Vec<(crate::ast::Expr, crate::ast::DeferKind)>,
}

impl Env {
    fn new() -> Self {
        Env {
            vars: Vec::new(),
            scope_marks: vec![0],
            functions: HashMap::new(),
            sum_variants: HashMap::new(),
            call_stack: Vec::new(),
            tool_provider: None,
            #[cfg(feature = "tools")]
            tokio_runtime: None,
            caps: Arc::new(Caps::default()),
            defer_stack: Vec::new(),
        }
    }

    fn with_caps(caps: Arc<Caps>) -> Self {
        Env {
            vars: Vec::new(),
            scope_marks: vec![0],
            functions: HashMap::new(),
            sum_variants: HashMap::new(),
            call_stack: Vec::new(),
            tool_provider: None,
            #[cfg(feature = "tools")]
            tokio_runtime: None,
            caps,
            defer_stack: Vec::new(),
        }
    }

    fn with_tools(
        provider: std::sync::Arc<dyn crate::tools::ToolProvider>,
        #[cfg(feature = "tools")] runtime: std::sync::Arc<tokio::runtime::Runtime>,
    ) -> Self {
        Env {
            vars: Vec::new(),
            scope_marks: vec![0],
            functions: HashMap::new(),
            sum_variants: HashMap::new(),
            call_stack: Vec::new(),
            tool_provider: Some(provider),
            #[cfg(feature = "tools")]
            tokio_runtime: Some(runtime),
            caps: Arc::new(Caps::default()),
            defer_stack: Vec::new(),
        }
    }

    fn with_tools_and_caps(
        provider: std::sync::Arc<dyn crate::tools::ToolProvider>,
        #[cfg(feature = "tools")] runtime: std::sync::Arc<tokio::runtime::Runtime>,
        caps: Arc<Caps>,
    ) -> Self {
        Env {
            vars: Vec::new(),
            scope_marks: vec![0],
            functions: HashMap::new(),
            sum_variants: HashMap::new(),
            call_stack: Vec::new(),
            tool_provider: Some(provider),
            #[cfg(feature = "tools")]
            tokio_runtime: Some(runtime),
            caps,
            defer_stack: Vec::new(),
        }
    }

    fn push_scope(&mut self) {
        self.scope_marks.push(self.vars.len());
    }

    fn pop_scope(&mut self) {
        let mark = self
            .scope_marks
            .pop()
            .expect("unbalanced push_scope/pop_scope");
        self.vars.truncate(mark);
    }

    fn set(&mut self, name: &str, value: Value) {
        // Update existing binding in any enclosing scope (innermost first)
        for entry in self.vars.iter_mut().rev() {
            if entry.0 == name {
                entry.1 = value;
                return;
            }
        }
        // No existing binding — create in innermost scope
        self.vars.push((name.to_string(), value));
    }

    /// Move out the current value bound to `name`, leaving the slot intact with
    /// `Value::Nil`. Returns `None` if no binding exists. Used by the
    /// self-rebind accumulator peephole to drop the env's Arc reference so
    /// `Arc::make_mut` can mutate the heap object in place.
    fn take(&mut self, name: &str) -> Option<Value> {
        for entry in self.vars.iter_mut().rev() {
            if entry.0 == name {
                return Some(std::mem::replace(&mut entry.1, Value::Nil));
            }
        }
        None
    }

    /// Always create a fresh binding in the innermost scope (used for function parameters).
    fn define(&mut self, name: &str, value: Value) {
        self.vars.push((name.to_string(), value));
    }

    fn get(&self, name: &str) -> Result<Value> {
        for (k, v) in self.vars.iter().rev() {
            if k == name {
                return Ok(v.clone());
            }
        }
        // Function names resolve to FnRef when used as values
        if self.functions.contains_key(name) {
            return Ok(Value::FnRef(name.to_string()));
        }
        // Variant constructor names: 0-arg variants resolve directly to Value::Variant;
        // payload variants resolve to FnRef so they can be called with an argument.
        if let Some((type_name, has_payload)) = self.sum_variants.get(name) {
            if *has_payload {
                return Ok(Value::FnRef(name.to_string()));
            } else {
                return Ok(Value::Variant {
                    type_name: type_name.clone(),
                    tag: name.to_string(),
                    payload: None,
                });
            }
        }
        // Builtin names also resolve to FnRef so they can be passed to
        // higher-order builtins (e.g. `fld max xs 0`).
        if Builtin::is_builtin(name) {
            return Ok(Value::FnRef(name.to_string()));
        }
        // `nil` is emitted as Expr::Ref("nil") by the parser (so that sum-type
        // variants named `nil` resolve correctly).  When no `nil` variant is
        // registered, fall back to the built-in nil value.
        if name == "nil" {
            return Ok(Value::Nil);
        }
        Err(RuntimeError::new(
            "ILO-R001",
            format!("undefined variable: {}", name),
        ))
    }

    fn function(&self, name: &str) -> Result<Decl> {
        self.functions
            .get(name)
            .cloned()
            .ok_or_else(|| RuntimeError::new("ILO-R002", format!("undefined function: {}", name)))
    }
}

/// Signal that a body produced an early return
enum BodyResult {
    /// Normal completion, last value
    Value(Value),
    /// Early return from guard
    Return(Value),
    /// Break from loop, with optional value
    Break(Value),
    /// Continue to next loop iteration
    Continue,
    /// Tail call: the body's final value would be the result of calling
    /// `callee` with `args`. The trampoline in `call_function` picks this up
    /// and rebinds parameters instead of recursing into Rust, so deep tail
    /// recursion runs in constant host-stack space.
    ///
    /// Only synthesised in tail position (last stmt of a body that is itself
    /// in tail position of its enclosing call). Only synthesised when the
    /// callee resolves to a user-defined function with no auto-unwrap (`!` /
    /// `!!`) on the call site, because both unwrap forms need to inspect the
    /// callee's return value before deciding whether to propagate.
    TailCall { callee: String, args: Vec<Value> },
}

pub fn run(program: &Program, func_name: Option<&str>, args: Vec<Value>) -> Result<Value> {
    run_with_env(program, func_name, args, Env::new())
}

/// Run with a capability policy. Operations that violate the policy return
/// `Value::Err(...)` rather than executing; they do NOT panic or abort.
pub fn run_with_caps(
    program: &Program,
    func_name: Option<&str>,
    args: Vec<Value>,
    caps: Arc<Caps>,
) -> Result<Value> {
    run_with_env(program, func_name, args, Env::with_caps(caps))
}

/// Dispatch a builtin call from the VM/Cranelift tree-bridge (`OP_CALL_BUILTIN_TREE`).
///
/// Used by `--run-vm` and `--jit` to delegate tree-only builtins
/// (`rgx`, `rgxall`, `fmt` variadic, 2-arg `rd`, `rdb`) to the same code path
/// the tree interpreter uses. Caller has already converted NanVal arg
/// registers to owned `Value`s; we return an owned `Value` for the caller
/// to NaN-box back into a result register.
///
/// Scope is deliberately limited to builtins that need no `Env`: no FnRef
/// args, no user-function callbacks, no tool dispatch. The call_function
/// dispatcher tolerates an empty Env for this subset because none of these
/// builtins look up user bindings or invoke other functions.
pub fn call_builtin_for_bridge(name: &str, args: Vec<Value>) -> Result<Value> {
    let mut env = Env::new();
    call_function(&mut env, name, args)
}

/// Program-aware variant of `call_builtin_for_bridge`.
///
/// HOF builtins (`grp`, `uniqby`, `partition`, 2-arg `srt`) invoke
/// user-defined callbacks via `call_function(env, ...)`, which requires
/// `env.functions` to be populated. The VM/Cranelift tree-bridge passes
/// the active AST `Program` here so we can register every `Decl::Function`
/// and `Decl::Tool` into the Env before dispatch, mirroring the prefix of
/// `run_with_env`. Builtins that don't need an Env still work, so it's
/// safe to route every bridge call through this entry point once the AST
/// is available.
pub fn call_builtin_for_bridge_with_program(
    name: &str,
    args: Vec<Value>,
    program: &Program,
) -> Result<Value> {
    let mut env = Env::new();
    for decl in &program.declarations {
        match decl {
            Decl::Function { name, .. } | Decl::Tool { name, .. } => {
                env.functions.insert(name.clone(), decl.clone());
            }
            Decl::SumType { name, variants, .. } => {
                for v in variants {
                    env.sum_variants
                        .insert(v.name.clone(), (name.clone(), v.payload.is_some()));
                }
            }
            Decl::TypeDef { .. } | Decl::Alias { .. } | Decl::Use { .. } | Decl::Error { .. } => {}
        }
    }
    call_function(&mut env, name, args)
}

pub fn run_with_tools(
    program: &Program,
    func_name: Option<&str>,
    args: Vec<Value>,
    provider: std::sync::Arc<dyn crate::tools::ToolProvider>,
    #[cfg(feature = "tools")] runtime: std::sync::Arc<tokio::runtime::Runtime>,
) -> Result<Value> {
    let env = Env::with_tools(
        provider,
        #[cfg(feature = "tools")]
        runtime,
    );
    run_with_env(program, func_name, args, env)
}

/// Run with tools AND a capability policy.
pub fn run_with_tools_and_caps(
    program: &Program,
    func_name: Option<&str>,
    args: Vec<Value>,
    provider: std::sync::Arc<dyn crate::tools::ToolProvider>,
    #[cfg(feature = "tools")] runtime: std::sync::Arc<tokio::runtime::Runtime>,
    caps: Arc<Caps>,
) -> Result<Value> {
    let env = Env::with_tools_and_caps(
        provider,
        #[cfg(feature = "tools")]
        runtime,
        caps,
    );
    run_with_env(program, func_name, args, env)
}

fn run_with_env(
    program: &Program,
    func_name: Option<&str>,
    args: Vec<Value>,
    mut env: Env,
) -> Result<Value> {
    // Register all functions, tools, and sum type variant constructors
    for decl in &program.declarations {
        match decl {
            Decl::Function { name, .. } | Decl::Tool { name, .. } => {
                env.functions.insert(name.clone(), decl.clone());
            }
            Decl::SumType { name, variants, .. } => {
                for v in variants {
                    env.sum_variants
                        .insert(v.name.clone(), (name.clone(), v.payload.is_some()));
                }
            }
            Decl::TypeDef { .. } | Decl::Alias { .. } | Decl::Use { .. } | Decl::Error { .. } => {}
        }
    }

    // Find function to call
    let target = match func_name {
        Some(name) => name.to_string(),
        None => {
            // Find first function
            program
                .declarations
                .iter()
                .find_map(|d| match d {
                    Decl::Function { name, .. } => Some(name.clone()),
                    _ => None,
                })
                .ok_or_else(|| RuntimeError::new("ILO-R012", "no functions defined"))?
        }
    };

    call_function(&mut env, &target, args)
}

/// Parse a string into a structured Value given a format name.
/// Grid formats ("csv", "tsv") → Ok(List of rows).
/// Graph formats ("json")      → Ok(parsed JSON) or Err(parse error message).
/// Raw/unknown                 → Ok(plain Text).
/// Box-Muller transform: sample from N(mu, sigma) using two uniform [0,1) samples.
/// Delegates to the shared `crate::rng` module so all engines produce the same sequence.
/// Kept as a thin wrapper because it is exercised directly in unit tests.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn box_muller_normal(mu: f64, sigma: f64) -> f64 {
    crate::rng::normal(mu, sigma)
}

/// Base64url-no-pad encoder. Alphabet per RFC 4648 §5 (URL-safe: `-` / `_`),
/// no `=` padding. Total — never fails — and allocation-free apart from the
/// returned `String`.
///
/// Kept as a small in-file helper rather than pulling the `base64` crate so
/// the cryptographic-random path stays additive against current main, which
/// does not yet have the crypto primitives family (those land in a separate
/// branch). When the crypto branch merges, this helper can be folded into the
/// shared base64url encoder without changing `rand-bytes` semantics.
#[inline]
fn b64url_no_pad_encode(bytes: &[u8]) -> String {
    const ALPHA: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    // Output length: ceil(n * 4 / 3) with the trailing `=` chars stripped.
    let n = bytes.len();
    let cap = n.div_ceil(3) * 4;
    let mut out = Vec::with_capacity(cap);
    let mut chunks = bytes.chunks_exact(3);
    for chunk in chunks.by_ref() {
        let b0 = chunk[0];
        let b1 = chunk[1];
        let b2 = chunk[2];
        out.push(ALPHA[(b0 >> 2) as usize]);
        out.push(ALPHA[(((b0 & 0b11) << 4) | (b1 >> 4)) as usize]);
        out.push(ALPHA[(((b1 & 0b1111) << 2) | (b2 >> 6)) as usize]);
        out.push(ALPHA[(b2 & 0b111111) as usize]);
    }
    let rem = chunks.remainder();
    match rem.len() {
        1 => {
            let b0 = rem[0];
            out.push(ALPHA[(b0 >> 2) as usize]);
            out.push(ALPHA[((b0 & 0b11) << 4) as usize]);
        }
        2 => {
            let b0 = rem[0];
            let b1 = rem[1];
            out.push(ALPHA[(b0 >> 2) as usize]);
            out.push(ALPHA[(((b0 & 0b11) << 4) | (b1 >> 4)) as usize]);
            out.push(ALPHA[((b1 & 0b1111) << 2) as usize]);
        }
        _ => {}
    }
    // SAFETY: every byte pushed is from ALPHA, which is ASCII-only.
    debug_assert!(out.iter().all(|b| b.is_ascii()));
    String::from_utf8(out).expect("base64url alphabet is ASCII-only")
}

/// `rand-bytes n > t` — generate `n` cryptographically random bytes from the
/// platform CSPRNG (via the `getrandom` crate), return as a base64url-no-pad
/// text. Distinct from `rnd` (seedable uniform float for simulations) and
/// `rndn` (seedable Normal float): this is the path agents need for JWT `jti`,
/// CSRF tokens, session IDs, and nonces.
///
/// Output is base64url with no padding so the result drops straight into
/// HTTP headers, cookies, and query strings without further encoding. Callers
/// that need raw bytes can decode with `b64u-dec` once that lands; in the
/// meantime the encoded form is what every realistic use case wants.
///
/// `#[inline(never)]` matches the recurring stack-overflow guard pattern
/// used by other tree-bridge implementations — keeps the dispatch site small
/// even in release builds, where this is invoked from the hot path.
#[inline(never)]
pub(crate) fn eval_rand_bytes(arg: &Value) -> Result<Value> {
    let n_f = match arg {
        Value::Number(n) => *n,
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("rand-bytes requires a number, got {other:?}"),
            ));
        }
    };
    if !n_f.is_finite() {
        return Err(RuntimeError::new(
            "ILO-R009",
            format!("rand-bytes: n is not finite ({n_f})"),
        ));
    }
    if n_f < 0.0 {
        return Err(RuntimeError::new(
            "ILO-R009",
            format!("rand-bytes: n must be non-negative, got {n_f}"),
        ));
    }
    // Cap at 1 MiB. Larger CSPRNG draws are almost certainly a bug (typo in
    // the byte count, mismatched units); cheaper to surface as ILO-R009 than
    // to allocate gigabytes of base64 output. 1 MiB raw → ~1.4 MB encoded.
    const MAX_BYTES: f64 = 1024.0 * 1024.0;
    if n_f > MAX_BYTES {
        return Err(RuntimeError::new(
            "ILO-R009",
            format!(
                "rand-bytes: n={n_f} exceeds 1 MiB cap; if you really need this much CSPRNG output, call rand-bytes in a loop"
            ),
        ));
    }
    let n = n_f as usize;
    if n == 0 {
        return Ok(Value::Text(Arc::new(String::new())));
    }
    let mut buf = vec![0u8; n];
    if let Err(e) = getrandom::getrandom(&mut buf) {
        return Err(RuntimeError::new(
            "ILO-R009",
            format!("rand-bytes: CSPRNG read failed: {e}"),
        ));
    }
    Ok(Value::Text(Arc::new(b64url_no_pad_encode(&buf))))
}

/// Exponential moving average over `xs` with smoothing factor `a` in [0, 1].
///
/// Recurrence: `ewm[0] = xs[0]`, `ewm[i] = a*xs[i] + (1-a)*ewm[i-1]`.
/// Boundary cases: `a = 0` freezes at `xs[0]`; `a = 1` reproduces `xs`.
/// Caller validates `a` and element types; this fn assumes `xs` is a list
/// of `f64` and `0 <= a <= 1`.
///
/// Marked `#[inline(never)]` from the start: the tree-walker dispatch hot
/// path inlines aggressively and stack-overflows surface there first for
/// recursive number-list reducers under deep persona workloads. Keeping
/// the loop in its own frame insulates the dispatcher.
#[inline(never)]
pub(crate) fn ewm_compute(xs: &[f64], a: f64) -> Vec<f64> {
    if xs.is_empty() {
        return Vec::new();
    }
    let one_minus_a = 1.0 - a;
    let mut out: Vec<f64> = Vec::with_capacity(xs.len());
    let mut prev = xs[0];
    out.push(prev);
    for &x in &xs[1..] {
        prev = a * x + one_minus_a * prev;
        out.push(prev);
    }
    out
}

/// Rolling-sum over a window of size `n`. Output length = `xs.len() - n + 1`;
/// empty when `n > xs.len()`. O(n) total via running-sum: one add and one
/// subtract per step, not the O(n*w) `sum (slc xs i (i+n))` recipe.
///
/// `#[inline(never)]` for the same reason as `ewm_compute` — keep the loop
/// out of the dispatcher's stack frame under deep persona workloads.
#[inline(never)]
pub(crate) fn rsum_compute(n: usize, xs: &[f64]) -> Vec<f64> {
    let len = xs.len();
    if n == 0 || n > len {
        return Vec::new();
    }
    let out_len = len - n + 1;
    let mut out: Vec<f64> = Vec::with_capacity(out_len);
    // Seed the running sum from the first window.
    let mut s: f64 = xs[..n].iter().sum();
    out.push(s);
    for i in n..len {
        s += xs[i];
        s -= xs[i - n];
        out.push(s);
    }
    out
}

/// Rolling-average over a window of size `n`. Same shape as `rsum_compute`;
/// each output is the running sum divided by `n`. O(n) total.
#[inline(never)]
pub(crate) fn ravg_compute(n: usize, xs: &[f64]) -> Vec<f64> {
    let sums = rsum_compute(n, xs);
    if sums.is_empty() {
        return sums;
    }
    let denom = n as f64;
    sums.into_iter().map(|s| s / denom).collect()
}

/// Rolling-min over a window of size `n` via the monotonic-deque idiom.
/// Output length = `xs.len() - n + 1`; empty when `n > xs.len()`. Each
/// element is pushed and popped at most once across the whole pass, so
/// total work is O(xs.len()), not O(xs.len() * n) like a naive per-window
/// `min` scan. NaN inputs sort as ">" everything else (consistent with
/// `min xs`/`max xs`) so a single NaN in a window does not poison the
/// output the way it does for `rsum`/`ravg`.
#[inline(never)]
pub(crate) fn rmin_compute(n: usize, xs: &[f64]) -> Vec<f64> {
    let len = xs.len();
    if n == 0 || n > len {
        return Vec::new();
    }
    let out_len = len - n + 1;
    let mut out: Vec<f64> = Vec::with_capacity(out_len);
    // Deque stores indices into `xs`; values are strictly increasing
    // along the deque (front is the current window's min). `partial_cmp`
    // treats NaN as incomparable; we fall back to `Greater` so NaNs sink
    // toward the back of the deque rather than masquerading as the min.
    let mut dq: std::collections::VecDeque<usize> = std::collections::VecDeque::with_capacity(n);
    for i in 0..len {
        // Drop indices that have fallen out of the window.
        while let Some(&front) = dq.front() {
            if front + n <= i {
                dq.pop_front();
            } else {
                break;
            }
        }
        // Maintain monotonicity: pop any tail whose value is >= the new value.
        while let Some(&back) = dq.back() {
            let cmp = xs[back]
                .partial_cmp(&xs[i])
                .unwrap_or(std::cmp::Ordering::Greater);
            if cmp != std::cmp::Ordering::Less {
                dq.pop_back();
            } else {
                break;
            }
        }
        dq.push_back(i);
        if i + 1 >= n {
            // Front of the deque is the index of the min in the current window.
            let &front = dq.front().expect("deque non-empty after push");
            out.push(xs[front]);
        }
    }
    out
}

/// POSIX `dirname` on a forward-slash path string. See `Builtin::Dirname`
/// in the builtin dispatch above for the full semantics + edge-case table.
///
/// Implemented over the raw string (not `std::path::Path`) so output is
/// stable across Unix and Windows builds — ilo's path builtins are
/// forward-slash-only in 0.12.1; Windows separator handling lands in 0.13.0.
pub(crate) fn dirname_posix(p: &str) -> String {
    if p.is_empty() {
        return String::new();
    }
    // Special case: the root is its own dirname (POSIX `dirname /` -> "/").
    if p == "/" {
        return "/".to_string();
    }
    // Strip a trailing `/` so `"foo/"` is treated as "the dir entry `foo`"
    // and `dirname "foo/"` -> "foo". Don't strip the only-slash case (handled
    // above) — that would turn "/" into "".
    let trimmed = if p.ends_with('/') && p.len() > 1 {
        &p[..p.len() - 1]
    } else {
        p
    };
    match trimmed.rfind('/') {
        // No `/` -> no directory component. POSIX returns "." here; we return
        // "" so `cat [dirname p, basename p] "/"` round-trips a plain filename
        // to itself without injecting a phantom `./` prefix. Documented in SPEC.
        None => String::new(),
        // The slash is the leading root marker: parent is `/`.
        Some(0) => "/".to_string(),
        // Otherwise parent is everything up to (but not including) the slash.
        Some(i) => trimmed[..i].to_string(),
    }
}

/// POSIX `basename` on a forward-slash path string. See `Builtin::Basename`
/// in the builtin dispatch above for the full semantics + edge-case table.
pub(crate) fn basename_posix(p: &str) -> String {
    if p.is_empty() {
        return String::new();
    }
    // `basename /` -> "/" (POSIX edge: root is its own basename).
    if p == "/" {
        return "/".to_string();
    }
    // Strip trailing `/` so `basename "foo/"` -> "foo".
    let trimmed = if p.ends_with('/') && p.len() > 1 {
        &p[..p.len() - 1]
    } else {
        p
    };
    match trimmed.rfind('/') {
        None => trimmed.to_string(),
        Some(i) => trimmed[i + 1..].to_string(),
    }
}

/// Join path segments with `/`, collapsing duplicate separators at joints
/// and dropping empty segments. See `Builtin::Pathjoin` for the full table.
///
/// The implementation strips trailing `/` from every segment except the very
/// first (so a leading `["/", ...]` keeps its absolute root), and strips
/// leading `/` from every segment except the first. Empty segments after
/// trimming are dropped.
pub(crate) fn pathjoin_posix(parts: &[&str]) -> String {
    let mut out = String::new();
    let mut first = true;
    for (idx, seg) in parts.iter().enumerate() {
        // First segment: preserve leading `/` (the absolute-root marker), but
        // still trim its trailing `/` to dedupe at the joint.
        let s = if idx == 0 {
            seg.trim_end_matches('/')
        } else {
            seg.trim_start_matches('/').trim_end_matches('/')
        };
        // Special-case the first segment being exactly "/" (or all slashes):
        // trim_end_matches eats everything, but we want the root preserved.
        let s = if idx == 0 && !seg.is_empty() && s.is_empty() && seg.starts_with('/') {
            "/"
        } else {
            s
        };
        if s.is_empty() {
            continue;
        }
        if first {
            out.push_str(s);
            first = false;
        } else {
            // Avoid emitting `//` when the previous accumulator already ended
            // in `/` (which only happens when the first segment was `/`).
            if !out.ends_with('/') {
                out.push('/');
            }
            out.push_str(s);
        }
    }
    out
}

/// Parse a human-readable duration string into total seconds (f64).
///
/// Accepts mixed sequences of `<number><unit>` pairs, optionally separated by
/// spaces. Numbers may be integers or decimals. Unit names accepted:
///
/// | abbreviation | full names                    | multiplier (seconds) |
/// |---|---|---|
/// | `w`          | week, weeks                   | 604800               |
/// | `d`          | day, days                     | 86400                |
/// | `h`          | hour, hours, hr, hrs          | 3600                 |
/// | `m`          | min, mins, minute, minutes    | 60                   |
/// | `s`          | sec, secs, second, seconds    | 1                    |
///
/// Parsed `fmt` placeholder spec. Lean by design — agents compose `fmt2`
/// / `padl` / `padr` for anything off the spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FmtSpec {
    /// `{}` — default `Display`.
    Bare,
    /// `{.Nf}` or `{:.Nf}` — N decimal places. Number arg required.
    Precision(usize),
    /// `{:N}` — right-align Display to width N (space-pad). Any arg type.
    WidthRight(usize),
    /// `{:Nd}` — integer right-align to width N. Number arg required.
    IntWidth(usize),
    /// `{:<N}` — left-align Display to width N (space-pad). Any arg type.
    WidthLeft(usize),
}

/// Parse a `{...}` spec body. Returns None for syntactically valid braces
/// that aren't one of the four supported shapes — callers raise ILO-R009
/// / ILO-T013 with the offending literal so agents see what they wrote.
///
/// Accepts the literal placeholder text including the outer braces:
///   "{}", "{.2f}", "{:.3f}", "{:5}", "{:5d}", "{:<5}".
pub(crate) fn parse_fmt_spec(spec: &str) -> Option<FmtSpec> {
    let inner = spec.strip_prefix('{')?.strip_suffix('}')?;
    if inner.is_empty() {
        return Some(FmtSpec::Bare);
    }
    // `{.Nf}` — no colon, precision shorthand.
    if let Some(rest) = inner.strip_prefix('.')
        && let Some(digits) = rest.strip_suffix('f')
        && !digits.is_empty()
        && digits.chars().all(|c| c.is_ascii_digit())
    {
        return digits.parse::<usize>().ok().map(FmtSpec::Precision);
    }
    // The remaining specs all start with `:`.
    let body = inner.strip_prefix(':')?;
    // `:.Nf`
    if let Some(rest) = body.strip_prefix('.')
        && let Some(digits) = rest.strip_suffix('f')
        && !digits.is_empty()
        && digits.chars().all(|c| c.is_ascii_digit())
    {
        return digits.parse::<usize>().ok().map(FmtSpec::Precision);
    }
    // Width digits are space-pad only — zero-padded widths (`{:06d}`) are
    // deliberately out of scope. Reject any leading `0` on a multi-digit
    // width so agents see a clear error instead of silently getting a
    // space-padded value.
    let is_plain_width = |s: &str| {
        !s.is_empty()
            && s.chars().all(|c| c.is_ascii_digit())
            && !(s.len() > 1 && s.starts_with('0'))
    };
    // `:<N` — left-align width.
    if let Some(digits) = body.strip_prefix('<')
        && is_plain_width(digits)
    {
        return digits.parse::<usize>().ok().map(FmtSpec::WidthLeft);
    }
    // `:Nd` — integer width.
    if let Some(digits) = body.strip_suffix('d')
        && is_plain_width(digits)
    {
        return digits.parse::<usize>().ok().map(FmtSpec::IntWidth);
    }
    // `:N` — width (string or stringified value).
    if is_plain_width(body) {
        return body.parse::<usize>().ok().map(FmtSpec::WidthRight);
    }
    None
}

/// Apply a parsed spec to a single arg. Returns Err with a short hint when
/// the arg type doesn't fit the spec (e.g. `{:Nd}` with a non-number).
pub(crate) fn apply_fmt_spec(spec: &FmtSpec, arg: &Value) -> std::result::Result<String, String> {
    match spec {
        FmtSpec::Bare => Ok(format!("{}", arg)),
        FmtSpec::Precision(n) => match arg {
            Value::Number(x) => Ok(format!("{:.*}", *n, x)),
            other => Err(format!(
                "decimal-precision spec requires a number, got {:?}",
                other
            )),
        },
        FmtSpec::IntWidth(w) => match arg {
            Value::Number(x) => {
                // Truncate toward zero — matches `str` for integer-valued
                // doubles and keeps `{:5d}` predictable for floats like
                // 42.9 (renders `42`, not `43`). Round explicitly via `rou`
                // if you want rounding.
                let n = *x as i64;
                let s = n.to_string();
                Ok(pad_left(&s, *w))
            }
            other => Err(format!("`d` width spec requires a number, got {:?}", other)),
        },
        FmtSpec::WidthRight(w) => {
            let s = format!("{}", arg);
            Ok(pad_left(&s, *w))
        }
        FmtSpec::WidthLeft(w) => {
            let s = format!("{}", arg);
            Ok(pad_right(&s, *w))
        }
    }
}

fn pad_left(s: &str, width: usize) -> String {
    let n = s.chars().count();
    if n >= width {
        s.to_string()
    } else {
        let pad = " ".repeat(width - n);
        format!("{pad}{s}")
    }
}

fn pad_right(s: &str, width: usize) -> String {
    let n = s.chars().count();
    if n >= width {
        s.to_string()
    } else {
        let pad = " ".repeat(width - n);
        format!("{s}{pad}")
    }
}

/// Examples: `"3 weeks 2 days 5 hours"`, `"4h 32m"`, `"1d"`, `"1.5 hours"`,
/// `"90s"`, `"2w3d"`.
///
/// Returns `Err` if the input is blank or no valid unit pair is found.
pub(crate) fn dur_parse(s: &str) -> std::result::Result<f64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("dur-parse: empty input".to_string());
    }
    let mut total = 0.0_f64;
    let mut found_any = false;
    // Sign is "sticky": a leading `-` applies to every following token until
    // an explicit `+` (or another `-`) resets it. This makes the round-trip
    // `dur-fmt -> dur-parse` symmetric for negative multi-part durations
    // such as "-1m 30s" (= -90), where the formatter emits a single leading
    // minus rather than signing every token.
    let mut sign = 1.0_f64;

    // Walk through the string matching <number> <ws?> <unit> sequences.
    let mut rest = s;
    while !rest.is_empty() {
        // Skip leading whitespace and separators.
        let trimmed = rest.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == ',');
        if trimmed.is_empty() {
            break;
        }
        rest = trimmed;

        // Consume an optional sign — updates the sticky running sign.
        if let Some(r) = rest.strip_prefix('-') {
            sign = -1.0_f64;
            rest = r;
        } else if let Some(r) = rest.strip_prefix('+') {
            sign = 1.0_f64;
            rest = r;
        }

        // Consume the number (integer or decimal).
        let num_end = rest
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(rest.len());
        if num_end == 0 {
            // No digit at current position — skip one char (handles garbage).
            let skip = rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            rest = &rest[skip..];
            continue;
        }
        let num_str = &rest[..num_end];
        let num: f64 = match num_str.parse() {
            Ok(n) => n,
            Err(_) => {
                // Malformed number; skip past it.
                rest = &rest[num_end..];
                continue;
            }
        };
        rest = &rest[num_end..];

        // Skip optional whitespace between number and unit.
        rest = rest.trim_start_matches(|c: char| c.is_ascii_whitespace());

        // Consume the unit (letters only).
        let unit_end = rest
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(rest.len());
        if unit_end == 0 {
            // No unit — skip (bare number without unit is not a duration token).
            continue;
        }
        let unit = &rest[..unit_end];
        rest = &rest[unit_end..];

        let multiplier: f64 = match unit.to_ascii_lowercase().as_str() {
            "w" | "week" | "weeks" => 604_800.0,
            "d" | "day" | "days" => 86_400.0,
            "h" | "hr" | "hrs" | "hour" | "hours" => 3_600.0,
            "m" | "min" | "mins" | "minute" | "minutes" => 60.0,
            "s" | "sec" | "secs" | "second" | "seconds" => 1.0,
            _ => {
                // Unknown unit — skip this token pair.
                continue;
            }
        };
        total += sign * num * multiplier;
        found_any = true;
    }

    if !found_any {
        return Err(format!("dur-parse: no recognised unit in {:?}", s));
    }
    Ok(total)
}

/// Format a duration given in seconds into a human-readable string.
///
/// Uses the largest applicable unit; drops zero parts; always includes at
/// least one part. Fractional seconds are preserved on the seconds
/// component with up to 3 decimal places, trailing zeros stripped, so the
/// `dur-fmt -> dur-parse` round-trip is information-preserving for any
/// value representable to ~3dp on the seconds digit.
///
/// Negative values format with a single leading minus rather than signing
/// each token. The matching `dur-parse` treats a leading `-` as sticky
/// (applies to every following token until an explicit `+` resets it), so
/// `dur-fmt(-90)` → `"-1m 30s"` parses back to `-90`.
///
/// | input (s)    | output         |
/// |---|---|
/// | 0            | "0s"           |
/// | 0.5          | "0.5s"         |
/// | 45           | "45s"          |
/// | 90           | "1m 30s"       |
/// | 90.5         | "1m 30.5s"     |
/// | 3600         | "1h"           |
/// | 9720         | "2h 42m"       |
/// | 86400        | "1 day"        |
/// | 604800       | "1 week"       |
/// | -90          | "-1m 30s"      |
pub(crate) fn dur_fmt(secs: f64) -> String {
    if !secs.is_finite() {
        return format!("{secs}");
    }
    let negative = secs < 0.0;
    let total_secs = secs.abs();

    // Work in integer seconds + fractional part.
    let whole = total_secs.trunc() as u64;
    let frac = total_secs - whole as f64;

    let weeks = whole / 604_800;
    let rem = whole % 604_800;
    let days = rem / 86_400;
    let rem = rem % 86_400;
    let hours = rem / 3_600;
    let rem = rem % 3_600;
    let minutes = rem / 60;
    let seconds = rem % 60;

    let mut parts: Vec<String> = Vec::with_capacity(5);
    if weeks > 0 {
        parts.push(if weeks == 1 {
            "1 week".to_string()
        } else {
            format!("{weeks} weeks")
        });
    }
    if days > 0 {
        parts.push(if days == 1 {
            "1 day".to_string()
        } else {
            format!("{days} days")
        });
    }
    if hours > 0 {
        parts.push(if hours == 1 {
            "1h".to_string()
        } else {
            format!("{hours}h")
        });
    }
    if minutes > 0 {
        parts.push(format!("{minutes}m"));
    }
    // Seconds: show if nonzero, or if we still have a fractional part to
    // carry. If everything else is 0, show "0s". Fractional seconds are
    // always emitted when present (with up to 3 decimal places, trailing
    // zeros stripped), so `dur-fmt -> dur-parse` round-trips without losing
    // sub-second precision.
    let has_frac = frac > 1e-9;
    if seconds > 0 || has_frac {
        if has_frac {
            let total_s = seconds as f64 + frac;
            let formatted = format!("{:.3}", total_s);
            let formatted = formatted.trim_end_matches('0').trim_end_matches('.');
            parts.push(format!("{formatted}s"));
        } else {
            parts.push(format!("{seconds}s"));
        }
    }

    if parts.is_empty() {
        // Input was exactly zero — render explicitly so callers always get
        // at least one component back.
        parts.push("0s".to_string());
    }

    let joined = parts.join(" ");
    if negative {
        format!("-{joined}")
    } else {
        joined
    }
}

/// Recursive depth-first walk over `root`, collecting paths relative to it,
/// sorted lexicographically. Symlinks are not followed (uses `file_type`,
/// not `metadata`, on each entry).
///
/// Errors reading `root` itself surface as `Err(String)` so the caller can
/// wrap them as `Value::Err` — a walk that can't open its starting point is
/// a real failure the agent needs to see. Errors reading a *subdirectory*
/// encountered during traversal (most commonly `PermissionDenied` on
/// sandbox roots, sibling-user dirs, or system paths like `/var/db` on
/// macOS) are silently skipped: the subdir's own entry is still included
/// in the output, but its contents are not enumerated. Without this,
/// `walk /` or `walk ~` would abort on the first unreadable child and
/// lose every other path it had already collected — the opposite of what
/// an agent doing a "find me all the X files" pass wants.
///
/// Shared by `walk` and `glob`: `glob` is `walk_collect` plus a matcher pass,
/// keeping pattern semantics and traversal semantics in lockstep (so a
/// pattern that fails to match still pays the same cost / sees the same set
/// of paths that `walk` would have produced — predictable for the agent).
fn walk_collect(root: &std::path::Path) -> std::result::Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    // Open the root up-front so a missing or unreadable starting point is
    // a hard error, distinguishable from "some descendant was unreadable".
    let root_rd = std::fs::read_dir(root).map_err(|e| e.to_string())?;
    let mut stack: Vec<(std::path::PathBuf, std::fs::ReadDir)> = Vec::new();
    stack.push((root.to_path_buf(), root_rd));
    while let Some((_cur, rd)) = stack.pop() {
        for entry in rd {
            // A bad individual entry (rare: filename decoding etc.) is skipped
            // for the same reason an unreadable subdir is — losing the whole
            // walk over one bad inode hurts more than a missing path.
            let ent = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = ent.path();
            let rel = path.strip_prefix(root).unwrap_or(&path);
            // Forward slashes on every OS — paths are an ilo string, not an
            // OS path, and downstream code (cat, fmt, rd) does not care which
            // separator was used. Picking `/` makes cross-platform tests deterministic.
            let rel_str = rel
                .to_string_lossy()
                .into_owned()
                .replace(std::path::MAIN_SEPARATOR, "/");
            out.push(rel_str);
            // `file_type` over `metadata` to avoid following symlinks (cycle
            // trap). If we can't even read the file_type, treat the entry as
            // a leaf — same skip rationale as above.
            let is_dir = match ent.file_type() {
                Ok(ft) => ft.is_dir(),
                Err(_) => false,
            };
            if is_dir {
                // Attempt to descend. PermissionDenied (and any other read_dir
                // failure on a subdir) is skipped silently: the directory's
                // own path is already in `out`, we just don't enumerate its
                // contents. This is the fix for the "chmod 000 poisons the
                // whole walk" regression.
                if let Ok(child_rd) = std::fs::read_dir(&path) {
                    stack.push((path, child_rd));
                }
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Shell-style glob match against the relative path `text`.
///
/// Pattern operators:
/// - `?` matches one character within a single path segment (no `/`).
/// - `*` matches a run of characters within a single path segment (no `/`).
/// - `[abc]` / `[a-z]` matches a character class; leading `!` or `^` negates.
/// - `**` matches any number of nested segments, including zero. Must occupy
///   a full segment (i.e. preceded and followed by `/` or end of string).
///
/// All other characters match literally. The matcher is recursive but bounded
/// by pattern length, so worst-case is the usual O(n*m) glob cost. No
/// `walkdir` / `glob` crate dependency — keeps the build lean.
fn glob_match(pat: &str, text: &str) -> bool {
    glob_match_bytes(pat.as_bytes(), text.as_bytes())
}

fn glob_match_bytes(pat: &[u8], text: &[u8]) -> bool {
    // Recursive backtracker. Anchored at both ends.
    let (mut pi, mut ti) = (0usize, 0usize);
    while pi < pat.len() {
        match pat[pi] {
            b'*' => {
                // `**` segment: matches any number of nested segments.
                // Must be bounded by `/` or string-end on both sides to count
                // as the recursive form; otherwise treated as plain `*`.
                let is_double = pi + 1 < pat.len() && pat[pi + 1] == b'*';
                let left_boundary = pi == 0 || pat[pi - 1] == b'/';
                let right_boundary_at = pi + 2;
                let right_boundary =
                    is_double && (right_boundary_at == pat.len() || pat[right_boundary_at] == b'/');
                if is_double && left_boundary && right_boundary {
                    // `**` followed by `/<rest>` or end. Try every match of
                    // the rest at every position from `ti` to `text.len()`.
                    let rest_start = if right_boundary_at < pat.len() {
                        right_boundary_at + 1
                    } else {
                        right_boundary_at
                    };
                    let rest = &pat[rest_start..];
                    // Empty rest means `**` is the whole tail — matches anything left.
                    if rest.is_empty() {
                        return true;
                    }
                    let mut probe = ti;
                    loop {
                        if glob_match_bytes(rest, &text[probe..]) {
                            return true;
                        }
                        if probe == text.len() {
                            return false;
                        }
                        probe += 1;
                    }
                }
                // Single `*`: matches any run within the current segment.
                // Greedy with backtrack via recursion on the remainder.
                let rest = &pat[pi + 1..];
                // Empty remainder: match to next `/` or end.
                let mut probe = ti;
                loop {
                    if glob_match_bytes(rest, &text[probe..]) {
                        return true;
                    }
                    if probe == text.len() || text[probe] == b'/' {
                        return false;
                    }
                    probe += 1;
                }
            }
            b'?' => {
                if ti >= text.len() || text[ti] == b'/' {
                    return false;
                }
                pi += 1;
                ti += 1;
            }
            b'[' => {
                if ti >= text.len() || text[ti] == b'/' {
                    return false;
                }
                // Parse [class]: optional leading `!`/`^` for negation,
                // then a sequence of chars or `a-z` ranges, terminated by `]`.
                let mut j = pi + 1;
                let mut negate = false;
                if j < pat.len() && (pat[j] == b'!' || pat[j] == b'^') {
                    negate = true;
                    j += 1;
                }
                let mut matched = false;
                let mut found_close = false;
                while j < pat.len() {
                    if pat[j] == b']' && j > pi + 1 + (negate as usize) {
                        found_close = true;
                        break;
                    }
                    // Range form: a-b
                    if j + 2 < pat.len() && pat[j + 1] == b'-' && pat[j + 2] != b']' {
                        if text[ti] >= pat[j] && text[ti] <= pat[j + 2] {
                            matched = true;
                        }
                        j += 3;
                    } else {
                        if text[ti] == pat[j] {
                            matched = true;
                        }
                        j += 1;
                    }
                }
                if !found_close {
                    // Unterminated `[`: treat as literal char (no panic).
                    if text[ti] != b'[' {
                        return false;
                    }
                    pi += 1;
                    ti += 1;
                    continue;
                }
                if matched == negate {
                    return false;
                }
                pi = j + 1;
                ti += 1;
            }
            c => {
                if ti >= text.len() || text[ti] != c {
                    return false;
                }
                pi += 1;
                ti += 1;
            }
        }
    }
    ti == text.len()
}

fn parse_format(fmt: &str, content: &str) -> std::result::Result<Value, String> {
    match fmt {
        "csv" | "tsv" => {
            let sep = if fmt == "tsv" { '\t' } else { ',' };
            let rows: Vec<Value> = parse_csv_content(content, sep)
                .into_iter()
                .map(|row| {
                    let fields: Vec<Value> =
                        row.into_iter().map(|s| Value::Text(Arc::new(s))).collect();
                    Value::List(Arc::new(fields))
                })
                .collect();
            Ok(Value::List(Arc::new(rows)))
        }
        "json" => serde_json::from_str::<serde_json::Value>(content)
            .map(serde_json_to_value)
            .map_err(|e| e.to_string()),
        _ => Ok(Value::Text(Arc::new(content.to_string()))),
    }
}

/// Parse a full CSV/TSV document into rows of fields, RFC 4180 compliant.
///
/// Unlike a line-based split, this scanner tracks quote state across record
/// boundaries so that a quoted field containing an embedded newline is
/// preserved as a single cell. Both `\n` and `\r\n` are accepted as record
/// separators outside quotes; inside quotes they are kept verbatim. A final
/// trailing newline does not produce an extra empty row.
///
/// History: the previous implementation called `content.lines()` then handed
/// each line to a per-line quote-aware parser. That meant any cell containing
/// `\n` (which `write_csv_tsv` correctly emits as a quoted multi-line field
/// per RFC 4180) was re-parsed as two rows on the way back in, so ilo silently
/// mis-parsed CSV it had just written. csv-pipeline rerun10 flagged this as
/// the one blocker on round-trip integrity.
fn parse_csv_content(content: &str, sep: char) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = content.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                // Inside quotes, newlines (including \r\n) are part of the
                // field. Keep them verbatim.
                field.push(c);
            }
        } else if c == '"' {
            in_quotes = true;
        } else if c == sep {
            row.push(std::mem::take(&mut field));
        } else if c == '\n' {
            row.push(std::mem::take(&mut field));
            rows.push(std::mem::take(&mut row));
        } else if c == '\r' {
            // Accept \r\n as a record terminator; bare \r outside quotes is
            // treated the same way (matches `content.lines()` previously and
            // keeps platform-CR-only files readable).
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            row.push(std::mem::take(&mut field));
            rows.push(std::mem::take(&mut row));
        } else {
            field.push(c);
        }
    }
    // Flush the trailing record. A file that ends with `\n` already emitted
    // its last row in the loop and field/row are empty here — skip pushing
    // a spurious empty row in that case. A file with no trailing newline
    // still has one record left to flush.
    if !field.is_empty() || !row.is_empty() || in_quotes {
        row.push(field);
        rows.push(row);
    }
    rows
}

// Note: a separate per-line `parse_csv_row` previously existed but its only
// caller (`parse_format`) was the source of the multi-line round-trip bug
// fixed in csv-pipeline rerun10. The full-document `parse_csv_content` above
// is now the single entry point for csv/tsv parsing.

// ── Linear algebra helpers ──────────────────────────────────────────

/// Coerce a `Value` into a row-major matrix `Vec<Vec<f64>>`.
/// Native matrix-vector multiply: row-by-row dot product.
///
/// `matvec xm ys` returns the flat vector `r` where
/// `r[i] = sum_j xm[i][j] * ys[j]`. Skips the wrap-as-column-matrix +
/// `flatten` ceremony required to use `matmul` for this case — the
/// common shape `flatten matmul xm (map (y:n>L n;[y]) ys)` collapses
/// to a single `matvec xm ys` call.
///
/// `#[inline(never)]` keeps this body out of `call_function`'s already-
/// huge frame, same pattern as the recurring stack-overflow band-aid in
/// #494 / #506 / lstsq (#515 / #5am). `cargo nextest` runs with a tighter
/// stack budget than `cargo test`, so an inlined arm trips the deep
/// braceless-guard fibonacci recursion test in CI even when local
/// tests pass.
#[inline(never)]
fn matvec_run(xm_val: &Value, ys_val: &Value) -> Result<Value> {
    let xm = matrix_from_value(xm_val, "matvec")?;
    let ys = vec_from_value(ys_val, "matvec")?;
    let n_rows = xm.len();
    if n_rows == 0 {
        return Err(RuntimeError::new(
            "ILO-R009",
            "matvec: empty matrix".to_string(),
        ));
    }
    let n_cols = xm[0].len();
    if n_cols == 0 {
        return Err(RuntimeError::new(
            "ILO-R009",
            "matvec: matrix has zero columns".to_string(),
        ));
    }
    for row in &xm {
        if row.len() != n_cols {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!(
                    "matvec: ragged rows (expected {n_cols} cols, got {})",
                    row.len()
                ),
            ));
        }
    }
    if ys.len() != n_cols {
        return Err(RuntimeError::new(
            "ILO-R009",
            format!(
                "matvec: dim mismatch (matrix has {n_cols} cols, ys has {})",
                ys.len()
            ),
        ));
    }
    let mut out: Vec<Value> = Vec::with_capacity(n_rows);
    for row in &xm {
        let mut s = 0.0_f64;
        for (k, &v) in row.iter().enumerate() {
            s += v * ys[k];
        }
        out.push(Value::Number(s));
    }
    Ok(Value::List(Arc::new(out)))
}

fn matrix_from_value(v: &Value, name: &str) -> Result<Vec<Vec<f64>>> {
    let rows = match v {
        Value::List(rs) => rs,
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("{}: expected a list of lists, got {:?}", name, other),
            ));
        }
    };
    let mut mat: Vec<Vec<f64>> = Vec::with_capacity(rows.len());
    for row in rows.iter() {
        let cells = match row {
            Value::List(cs) => cs,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("{}: each row must be a list, got {:?}", name, other),
                ));
            }
        };
        let mut r: Vec<f64> = Vec::with_capacity(cells.len());
        for c in cells.iter() {
            match c {
                Value::Number(n) => r.push(*n),
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("{}: matrix cells must be numbers, got {:?}", name, other),
                    ));
                }
            }
        }
        mat.push(r);
    }
    Ok(mat)
}

/// Coerce a `Value` into a vector `Vec<f64>`.
fn vec_from_value(v: &Value, name: &str) -> Result<Vec<f64>> {
    let items = match v {
        Value::List(xs) => xs,
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("{}: expected a list of numbers, got {:?}", name, other),
            ));
        }
    };
    let mut out: Vec<f64> = Vec::with_capacity(items.len());
    for item in items.iter() {
        match item {
            Value::Number(n) => out.push(*n),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("{}: vector items must be numbers, got {:?}", name, other),
                ));
            }
        }
    }
    Ok(out)
}

/// Format one field for csv/tsv output following RFC 4180 quoting.
/// Quote the field if it contains the separator, `"`, or newline.
/// Inner `"` are escaped as `""`.
fn fmt_csv_field(v: &Value, sep: char) -> String {
    let raw = match v {
        Value::Text(s) => (**s).clone(),
        Value::Number(n) => {
            if *n == (*n as i64) as f64 {
                format!("{}", *n as i64)
            } else {
                format!("{n}")
            }
        }
        Value::Bool(b) => format!("{b}"),
        Value::Nil => String::new(),
        other => format!("{other}"),
    };
    if raw.contains(sep) || raw.contains('"') || raw.contains('\n') {
        format!("\"{}\"", raw.replace('"', "\"\""))
    } else {
        raw
    }
}

/// Serialise a list of rows as csv or tsv.
///
/// Row shapes:
///   * `L (L _)` — list of lists, no header row.
///   * `L record` — header from the first record's fields (keys sorted for
///     stable output across runs since fields are stored in a HashMap).
///   * `L (M k v)` — header from the first map's keys (sorted likewise).
pub(crate) fn write_csv_tsv(rows: &[Value], sep: char) -> Result<String> {
    let mut out = String::new();
    let first = match rows.first() {
        Some(r) => r,
        None => return Ok(out),
    };
    let (header, use_keys): (Option<Vec<String>>, bool) = match first {
        Value::List(_) => (None, false),
        Value::Record { fields, .. } => {
            let mut keys: Vec<String> = fields.keys().cloned().collect();
            keys.sort();
            (Some(keys), true)
        }
        Value::Map(m) => {
            let mut keys: Vec<MapKey> = m.keys().cloned().collect();
            keys.sort();
            (
                Some(keys.iter().map(|k| k.to_display_string()).collect()),
                true,
            )
        }
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!(
                    "wr: each row must be a list, record, or map, got {:?}",
                    other
                ),
            ));
        }
    };
    if let Some(ref keys) = header {
        for (i, k) in keys.iter().enumerate() {
            if i > 0 {
                out.push(sep);
            }
            out.push_str(&fmt_csv_field(&Value::Text(Arc::new(k.clone())), sep));
        }
        out.push('\n');
    }
    for row in rows {
        match (row, use_keys, header.as_ref()) {
            (Value::List(fields), false, _) => {
                for (i, f) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push(sep);
                    }
                    out.push_str(&fmt_csv_field(f, sep));
                }
                out.push('\n');
            }
            (Value::Record { fields, .. }, true, Some(keys)) => {
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        out.push(sep);
                    }
                    let v = fields.get(k).cloned().unwrap_or(Value::Nil);
                    out.push_str(&fmt_csv_field(&v, sep));
                }
                out.push('\n');
            }
            (Value::Map(m), true, Some(keys)) => {
                // Build a stringified-key view of the map to match the
                // header order. Header keys are stringified per
                // `MapKey::to_display_string` so a numeric key `1` matches
                // the header column "1".
                let str_view: HashMap<String, &Value> =
                    m.iter().map(|(k, v)| (k.to_display_string(), v)).collect();
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        out.push(sep);
                    }
                    let v = str_view
                        .get(k.as_str())
                        .copied()
                        .cloned()
                        .unwrap_or(Value::Nil);
                    out.push_str(&fmt_csv_field(&v, sep));
                }
                out.push('\n');
            }
            (other, _, _) => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "wr: row shape mismatch (expected {} rows), got {:?}",
                        if use_keys { "record/map" } else { "list" },
                        other
                    ),
                ));
            }
        }
    }
    Ok(out)
}

/// LU decomposition with partial pivoting, in-place on an owned matrix.
/// Returns (LU, pivot indices, determinant, singular flag). When `singular`
/// is true, the LU and det values are still well-formed (det = 0) but the
/// system has no unique solution.
#[allow(clippy::needless_range_loop)]
pub(crate) fn lu_decompose(mut a: Vec<Vec<f64>>) -> (Vec<Vec<f64>>, Vec<usize>, f64, bool) {
    let n = a.len();
    let mut piv: Vec<usize> = (0..n).collect();
    let mut det_sign = 1.0_f64;
    let mut singular = false;
    for k in 0..n {
        // Pivot: find row with max |a[i][k]| for i in k..n
        let mut max_val = a[k][k].abs();
        let mut max_row = k;
        for i in (k + 1)..n {
            let v = a[i][k].abs();
            if v > max_val {
                max_val = v;
                max_row = i;
            }
        }
        if max_val < 1e-12 {
            singular = true;
            continue;
        }
        if max_row != k {
            a.swap(k, max_row);
            piv.swap(k, max_row);
            det_sign = -det_sign;
        }
        let pivot = a[k][k];
        for i in (k + 1)..n {
            a[i][k] /= pivot;
            let factor = a[i][k];
            for j in (k + 1)..n {
                a[i][j] -= factor * a[k][j];
            }
        }
    }
    let mut det = det_sign;
    for (i, row) in a.iter().enumerate().take(n) {
        det *= row[i];
    }
    if singular {
        det = 0.0;
    }
    (a, piv, det, singular)
}

/// Solve LUx = Pb using the result of `lu_decompose`.
pub(crate) fn lu_solve(lu: &[Vec<f64>], piv: &[usize], b: &[f64]) -> Vec<f64> {
    let n = lu.len();
    // Apply permutation: y = Pb
    let mut x: Vec<f64> = (0..n).map(|i| b[piv[i]]).collect();
    // Forward solve Ly = Pb (L has unit diagonal)
    for i in 0..n {
        for j in 0..i {
            let lij = lu[i][j];
            x[i] -= lij * x[j];
        }
    }
    // Back solve Ux = y
    for i in (0..n).rev() {
        for j in (i + 1)..n {
            let uij = lu[i][j];
            x[i] -= uij * x[j];
        }
        x[i] /= lu[i][i];
    }
    x
}

/// Ordinary least squares via the normal equations.
///
/// Returns coefficients `b` minimising `||xm·b - ys||²`. Composes the
/// `solve (Xᵀ X) (Xᵀ y)` recipe inline (no intermediate `Value` allocs)
/// rather than dispatching back through the builtin table. Same precision
/// tier as `solve` (LU with partial pivoting); numerically inferior to
/// QR/SVD for ill-conditioned designs.
///
/// `#[inline(never)]` keeps this body out of `call_function`'s already-huge
/// frame — same pattern as #506/#494 for sha2/hmac and caps fields. Failure
/// to do so causes a `cargo nextest` stack-overflow regression on the
/// braceless-guard fibonacci test in CI (deeper recursion budget than `cargo
/// test`).
#[inline(never)]
fn lstsq_run(xm_val: &Value, ys_val: &Value) -> Result<Value> {
    let xm = matrix_from_value(xm_val, "lstsq")?;
    let ys = vec_from_value(ys_val, "lstsq")?;
    let n_rows = xm.len();
    if n_rows == 0 {
        return Err(RuntimeError::new(
            "ILO-R009",
            "lstsq: empty design matrix".to_string(),
        ));
    }
    let n_cols = xm[0].len();
    if n_cols == 0 {
        return Err(RuntimeError::new(
            "ILO-R009",
            "lstsq: design matrix has zero columns".to_string(),
        ));
    }
    for row in &xm {
        if row.len() != n_cols {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!(
                    "lstsq: ragged design matrix (expected {n_cols} cols, got {})",
                    row.len()
                ),
            ));
        }
    }
    if ys.len() != n_rows {
        return Err(RuntimeError::new(
            "ILO-R009",
            format!(
                "lstsq: ys length {} must match design matrix row count {n_rows}",
                ys.len()
            ),
        ));
    }
    if n_cols > n_rows {
        return Err(RuntimeError::new(
            "ILO-R009",
            format!(
                "lstsq: underdetermined system ({n_cols} columns > {n_rows} rows); normal-equation OLS requires rows >= columns"
            ),
        ));
    }
    // Xᵀ — n_cols × n_rows
    let mut xt: Vec<Vec<f64>> = vec![vec![0.0; n_rows]; n_cols];
    for (i, row) in xm.iter().enumerate() {
        for (j, &v) in row.iter().enumerate() {
            xt[j][i] = v;
        }
    }
    // XᵀX — n_cols × n_cols
    let mut xtx: Vec<Vec<f64>> = vec![vec![0.0; n_cols]; n_cols];
    for i in 0..n_cols {
        for j in 0..n_cols {
            let mut s = 0.0;
            for k in 0..n_rows {
                s += xt[i][k] * xm[k][j];
            }
            xtx[i][j] = s;
        }
    }
    // Xᵀy — length n_cols
    let mut xty: Vec<f64> = vec![0.0; n_cols];
    for i in 0..n_cols {
        let mut s = 0.0;
        for k in 0..n_rows {
            s += xt[i][k] * ys[k];
        }
        xty[i] = s;
    }
    let (lu, piv, _det, singular) = lu_decompose(xtx);
    if singular {
        return Err(RuntimeError::new(
            "ILO-R009",
            "lstsq: normal-equation matrix XᵀX is singular (rank-deficient design)".to_string(),
        ));
    }
    let x = lu_solve(&lu, &piv, &xty);
    Ok(Value::List(Arc::new(
        x.into_iter().map(Value::Number).collect(),
    )))
}

// ── URL + base64url encoding cluster ────────────────────────────────────────
//
// Each builtin lives in its own #[inline(never)] helper so the call_function
// dispatch frame stays small. With four large arms inlined into the dispatch
// switch the debug-build stack frame of call_function grew past the default
// 2 MiB thread stack and tripped the fib(10) recursion test on Linux CI.

#[inline(never)]
fn urlenc_impl(arg: &Value) -> Result<Value> {
    // urlenc s > t — RFC 3986 percent-encode every byte that isn't in the
    // unreserved set ALPHA / DIGIT / `-` / `.` / `_` / `~`. Total.
    use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
    const UNRESERVED_PUNCT: &AsciiSet = &NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'.')
        .remove(b'_')
        .remove(b'~');
    let s = match arg {
        Value::Text(s) => s.clone(),
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("urlenc requires text, got {:?}", other),
            ));
        }
    };
    let encoded: String = utf8_percent_encode(s.as_str(), UNRESERVED_PUNCT).collect();
    Ok(Value::Text(Arc::new(encoded)))
}

#[inline(never)]
fn urldec_impl(arg: &Value) -> Result<Value> {
    // urldec s > R t t — inverse of urlenc. Err on stray `%` not followed by
    // two hex digits or on decoded bytes that aren't valid UTF-8.
    let s = match arg {
        Value::Text(s) => s.clone(),
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("urldec requires text, got {:?}", other),
            ));
        }
    };
    // Validate percent escapes up-front so silent passthrough doesn't mask
    // malformed input. The crate's decode_utf8 returns Ok for "abc%" / "abc%2",
    // which would defeat the R t t contract.
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit()
            {
                return Ok(Value::Err(Box::new(Value::Text(Arc::new(format!(
                    "urldec: invalid percent escape at byte {}",
                    i
                ))))));
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    match percent_encoding::percent_decode_str(s.as_str()).decode_utf8() {
        Ok(cow) => Ok(Value::Ok(Box::new(Value::Text(Arc::new(cow.into_owned()))))),
        Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(format!(
            "urldec: invalid UTF-8 in decoded bytes: {}",
            e
        )))))),
    }
}

/// `idxof s sub > O n` — Unicode code-point index of the first occurrence of
/// `sub` in `s`. Returns `Value::Nil` when not found. Index is in code-point
/// units (same convention as `at`), not raw byte offsets.
///
/// `#[inline(never)]` keeps this body out of `call_function`'s already-huge
/// frame; the helper is small enough that the call overhead is in the noise.
#[inline(never)]
fn idxof_impl(s_arg: &Value, sub_arg: &Value) -> Result<Value> {
    let s = match s_arg {
        Value::Text(s) => s.as_str(),
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("idxof: first arg must be text, got {:?}", other),
            ));
        }
    };
    let sub = match sub_arg {
        Value::Text(s) => s.as_str(),
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("idxof: second arg must be text, got {:?}", other),
            ));
        }
    };
    // Empty needle: matches at position 0 (Python / JS semantics).
    if sub.is_empty() {
        return Ok(Value::Number(0.0));
    }
    // Find the byte offset first (cheap), then count code points up to that
    // byte to get the char-index.  O(n) but allocation-free.
    match s.find(sub) {
        None => Ok(Value::Nil),
        Some(byte_offset) => {
            let char_idx = s[..byte_offset].chars().count();
            Ok(Value::Number(char_idx as f64))
        }
    }
}

#[inline(never)]
fn b64u_impl(arg: &Value) -> Result<Value> {
    // b64u s > t — base64url-encode the UTF-8 bytes of s using the URL-safe
    // alphabet (RFC 4648 §5: `-`/`_` instead of `+`/`/`) with padding stripped.
    // Total.
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let s = match arg {
        Value::Text(s) => s.clone(),
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("b64u requires text, got {:?}", other),
            ));
        }
    };
    Ok(Value::Text(Arc::new(URL_SAFE_NO_PAD.encode(s.as_bytes()))))
}

#[inline(never)]
fn b64u_dec_impl(arg: &Value) -> Result<Value> {
    // b64u-dec s > R t t — inverse of b64u. Err on input outside the
    // base64url alphabet, on `=` padding (strict no-pad round-trip), or on
    // decoded bytes that aren't valid UTF-8.
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let s = match arg {
        Value::Text(s) => s.clone(),
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("b64u-dec requires text, got {:?}", other),
            ));
        }
    };
    match URL_SAFE_NO_PAD.decode(s.as_bytes()) {
        Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(format!(
            "b64u-dec: invalid base64url input: {}",
            e
        )))))),
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => Ok(Value::Ok(Box::new(Value::Text(Arc::new(text))))),
            Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(format!(
                "b64u-dec: decoded bytes are not valid UTF-8: {}",
                e
            )))))),
        },
    }
}

// ── Crypto primitives cluster ───────────────────────────────────────────────
//
// `sha256`, `hmac-sha256`, `b64`, `b64-dec`, `hex`, `ct-eq`. All tree-bridge
// eligible: pure text-in / text-or-bool-out, no FnRef args, no I/O. VM and
// Cranelift inherit cross-engine parity through the existing bridge at zero
// opcode cost.
//
// Each helper is `#[inline(never)]` so the call_function dispatch frame stays
// small (same pattern as the URL + base64url cluster and the calendar
// arithmetic helpers — inlining all of these into the dispatch switch tips
// debug-build stack frames past the default 2 MiB pthread stack on Linux CI).

#[inline(never)]
fn sha256_impl(arg: &Value) -> Result<Value> {
    // sha256 s > t — SHA-256 of the UTF-8 bytes of s, returned as a lowercase
    // hex string. Total: no error path. 32 raw bytes → 64 hex chars.
    use sha2::{Digest, Sha256};
    let s = match arg {
        Value::Text(s) => s.clone(),
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("sha256 requires text, got {:?}", other),
            ));
        }
    };
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    let digest = h.finalize();
    Ok(Value::Text(Arc::new(hex::encode(digest))))
}

/// Shared helper: validate and hex-decode a text value for sha256-hex / sha256d.
/// Returns ILO-T013 on odd-length or non-hex input.
fn hex_decode_arg(arg: &Value, caller: &str) -> Result<Vec<u8>> {
    let s = match arg {
        Value::Text(s) => s.clone(),
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("{caller} requires text, got {:?}", other),
            ));
        }
    };
    if s.len() % 2 != 0 {
        return Err(RuntimeError::new(
            "ILO-R009",
            format!(
                "{caller}: hex input must have even length, got {} chars",
                s.len()
            ),
        ));
    }
    hex::decode(s.as_ref())
        .map_err(|e| RuntimeError::new("ILO-R009", format!("{caller}: invalid hex input: {e}")))
}

#[inline(never)]
fn sha256_hex_impl(arg: &Value) -> Result<Value> {
    // sha256-hex hex:t > t — SHA-256 of hex-decoded bytes, returned as a
    // lowercase hex string. Errors (ILO-T013) on odd-length or non-hex input.
    use sha2::{Digest, Sha256};
    let bytes = hex_decode_arg(arg, "sha256-hex")?;
    let mut h = Sha256::new();
    h.update(&bytes);
    Ok(Value::Text(Arc::new(hex::encode(h.finalize()))))
}

#[inline(never)]
fn sha256d_impl(arg: &Value) -> Result<Value> {
    // sha256d hex:t > t — double-SHA256 of hex-decoded bytes (Bitcoin Merkle
    // protocol: sha256(sha256(x))). Returns lowercase hex of the outer digest.
    // Errors (ILO-T013) on odd-length or non-hex input.
    use sha2::{Digest, Sha256};
    let bytes = hex_decode_arg(arg, "sha256d")?;
    let inner = Sha256::digest(&bytes);
    let outer = Sha256::digest(inner);
    Ok(Value::Text(Arc::new(hex::encode(outer))))
}

#[inline(never)]
fn hmac_sha256_impl(key_arg: &Value, msg_arg: &Value) -> Result<Value> {
    // hmac-sha256 key:t msg:t > t — HMAC-SHA256 of msg under key. Returns the
    // 32-byte MAC as a lowercase hex string. Use with `ct-eq` to verify
    // signatures without leaking timing info.
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let key = match key_arg {
        Value::Text(s) => s.clone(),
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("hmac-sha256: key must be text, got {:?}", other),
            ));
        }
    };
    let msg = match msg_arg {
        Value::Text(s) => s.clone(),
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("hmac-sha256: msg must be text, got {:?}", other),
            ));
        }
    };
    // `Hmac::<Sha256>::new_from_slice` only errors on disallowed key length,
    // which for HMAC-SHA256 is never (any byte length is allowed). The
    // `expect` documents that invariant.
    let mut mac =
        <Hmac<Sha256>>::new_from_slice(key.as_bytes()).expect("HMAC-SHA256 accepts any key length");
    mac.update(msg.as_bytes());
    let tag = mac.finalize().into_bytes();
    Ok(Value::Text(Arc::new(hex::encode(tag))))
}

#[inline(never)]
fn tokcount_impl(arg: &Value) -> Result<Value> {
    // tokcount s > n — approximate cl100k_base token count of string s.
    //
    // STUB: uses a bytes/3.4 approximation (empirical mean bytes-per-token for
    // English prose under cl100k_base). Correct within ~5% for natural-language
    // skill files. A follow-up (ILO-47) will replace this with a real BPE
    // tokeniser (tiktoken-rs or similar) once crate WASM and licence questions
    // are resolved.
    //
    // f64::ceil ensures we round up, matching Python tiktoken's exact count
    // on short strings where the approximation could otherwise round down
    // and produce a false-passing token budget check.
    match arg {
        Value::Text(s) => {
            let bytes = s.len() as f64;
            let count = (bytes / 3.4_f64).ceil();
            Ok(Value::Number(count))
        }
        other => Err(RuntimeError::new(
            "ILO-R009",
            format!("tokcount requires text, got {:?}", other),
        )),
    }
}

#[inline(never)]
fn b64_impl(arg: &Value) -> Result<Value> {
    // b64 s > t — standard base64 (RFC 4648 §4) encode of the UTF-8 bytes of
    // s, with `=` padding. Distinct from `b64u` which uses the URL-safe
    // alphabet and strips padding. Total.
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    let s = match arg {
        Value::Text(s) => s.clone(),
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("b64 requires text, got {:?}", other),
            ));
        }
    };
    Ok(Value::Text(Arc::new(STANDARD.encode(s.as_bytes()))))
}

#[inline(never)]
fn b64_dec_impl(arg: &Value) -> Result<Value> {
    // b64-dec s > R t t — inverse of b64. Err on input outside the standard
    // base64 alphabet, or on decoded bytes that aren't valid UTF-8. Uses the
    // strict standard engine (requires `=` padding to match the encoder).
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    let s = match arg {
        Value::Text(s) => s.clone(),
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("b64-dec requires text, got {:?}", other),
            ));
        }
    };
    match STANDARD.decode(s.as_bytes()) {
        Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(format!(
            "b64-dec: invalid base64 input: {}",
            e
        )))))),
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => Ok(Value::Ok(Box::new(Value::Text(Arc::new(text))))),
            Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(format!(
                "b64-dec: decoded bytes are not valid UTF-8: {}",
                e
            )))))),
        },
    }
}

#[inline(never)]
fn hex_impl(arg: &Value) -> Result<Value> {
    // hex s > t — lowercase hex encode of the UTF-8 bytes of s. Total:
    // every byte maps to exactly 2 hex chars. Companion to sha256 / hmac-sha256
    // which already emit hex; use `hex` when you need to encode arbitrary
    // text bytes for transport (e.g. binary marshalling, logging escapes).
    let s = match arg {
        Value::Text(s) => s.clone(),
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("hex requires text, got {:?}", other),
            ));
        }
    };
    Ok(Value::Text(Arc::new(hex::encode(s.as_bytes()))))
}

#[inline(never)]
fn ct_eq_impl(a_arg: &Value, b_arg: &Value) -> Result<Value> {
    // ct-eq a:t b:t > b — constant-time text equality. Returns true iff the
    // UTF-8 byte sequences of a and b are identical, comparing in constant
    // time (no short-circuit on the first differing byte). Use when comparing
    // secrets (HMAC digests, session tokens, API keys) so a timing attacker
    // can't binary-search the secret one byte at a time.
    //
    // For different-length inputs we return `false` without invoking the
    // constant-time path; length is not secret in practice (HMAC digests are
    // fixed-length, tokens are emitted with a known size).
    use subtle::ConstantTimeEq;
    let a = match a_arg {
        Value::Text(s) => s.clone(),
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("ct-eq: first arg must be text, got {:?}", other),
            ));
        }
    };
    let b = match b_arg {
        Value::Text(s) => s.clone(),
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("ct-eq: second arg must be text, got {:?}", other),
            ));
        }
    };
    if a.len() != b.len() {
        return Ok(Value::Bool(false));
    }
    let eq: bool = a.as_bytes().ct_eq(b.as_bytes()).into();
    Ok(Value::Bool(eq))
}

#[inline(never)]
fn hex_rev_impl(arg: &Value) -> Result<Value> {
    // hex-rev s > t — reverse byte order of a hex-encoded string.
    // Input is a hex string (any case); length must be even (2 chars per
    // byte). Odd-length input errors ILO-T013 with a padding hint. Case
    // is preserved: `abCD` reversed is `CDab`. Total for even-length hex.
    // Use for little-endian ↔ big-endian conversions (e.g. Bitcoin txid).
    let s = match arg {
        Value::Text(s) => s.clone(),
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("hex-rev requires text, got {:?}", other),
            ));
        }
    };
    if s.len() % 2 != 0 {
        return Err(RuntimeError::new(
            "ILO-T013",
            format!(
                "hex-rev: input length {} is odd — hex strings must encode whole bytes (2 chars \
                 per byte); hint: pad to even length first (e.g. prepend \"0\")",
                s.len()
            ),
        ));
    }
    // Reverse byte pairs in-place. No heap allocation beyond the output String.
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = s.len();
    while i >= 2 {
        i -= 2;
        // SAFETY: `s` is a valid &str; slicing at even byte boundaries keeps
        // UTF-8 validity since ASCII hex chars are all single-byte code points.
        out.push(bytes[i] as char);
        out.push(bytes[i + 1] as char);
    }
    Ok(Value::Text(Arc::new(out)))
}

// ── Calendar arithmetic cluster ─────────────────────────────────────────────
//
// Each builtin lives in its own #[inline(never)] helper so the call_function
// dispatch frame stays small. Four chrono-heavy arms inlined into the
// dispatch switch pushed call_function's debug-build stack frame past the
// default 2 MiB pthread stack on Linux CI, tripping the fib(10) recursion
// in `interpret_braceless_guard_fibonacci`. Same pattern as the URL +
// base64url cluster above and lstsq (#515).

#[inline(never)]
fn add_mo_impl(epoch_arg: &Value, months_arg: &Value) -> Result<Value> {
    // add-mo dt:n n:n > n — add N calendar months to epoch, snapping
    // end-of-month. N may be negative. Jan 31 + 1 mo = Feb 28/29.
    // Returns the resulting epoch at 00:00 UTC.
    use chrono::{Datelike, NaiveDate, TimeZone, Utc};
    let epoch = match epoch_arg {
        Value::Number(n) => *n,
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!(
                    "add-mo: first arg must be a number (epoch), got {:?}",
                    other
                ),
            ));
        }
    };
    let months = match months_arg {
        Value::Number(n) => *n as i32,
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!(
                    "add-mo: second arg must be a number (months), got {:?}",
                    other
                ),
            ));
        }
    };
    let secs = epoch as i64;
    let dt = match Utc.timestamp_opt(secs, 0).single() {
        Some(d) => d,
        None => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("add-mo: epoch out of range: {epoch}"),
            ));
        }
    };
    let date = dt.date_naive();
    fn add_months_snap(date: NaiveDate, months: i32) -> Option<NaiveDate> {
        // Compute total months since year-0 in i64 so adding i32::MAX (or
        // i32::MIN) months to any chrono-representable year cannot wrap.
        // Pre-fix this was i32 arithmetic: `date.year() * 12 + months`
        // overflowed at i32::MAX months, panicking in debug and silently
        // wrapping to a valid date in release. The widened path now either
        // produces a valid NaiveDate or returns None, which the caller
        // surfaces as a clean ILO-R009 "result out of calendar range".
        let total: i64 = (date.year() as i64) * 12 + (date.month() as i64 - 1) + (months as i64);
        let y_i64 = total.div_euclid(12);
        let m = (total.rem_euclid(12) + 1) as u32;
        let y: i32 = i32::try_from(y_i64).ok()?;
        let max_day = {
            let next = if m == 12 {
                NaiveDate::from_ymd_opt(y.checked_add(1)?, 1, 1)
            } else {
                NaiveDate::from_ymd_opt(y, m + 1, 1)
            };
            (next? - NaiveDate::from_ymd_opt(y, m, 1)?).num_days() as u32
        };
        NaiveDate::from_ymd_opt(y, m, date.day().min(max_day))
    }
    match add_months_snap(date, months) {
        Some(d) => {
            let ts = d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();
            Ok(Value::Number(ts as f64))
        }
        None => Err(RuntimeError::new(
            "ILO-R009",
            "add-mo: result out of calendar range".to_string(),
        )),
    }
}

#[inline(never)]
fn last_dom_impl(arg: &Value) -> Result<Value> {
    // last-dom dt:n > n — epoch of the last day of the month containing dt
    // at 00:00 UTC. E.g. any Feb 2024 epoch -> 2024-02-29 00:00 UTC.
    use chrono::{Datelike, NaiveDate, TimeZone, Utc};
    let epoch = match arg {
        Value::Number(n) => *n,
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("last-dom: arg must be a number (epoch), got {:?}", other),
            ));
        }
    };
    let secs = epoch as i64;
    let dt = match Utc.timestamp_opt(secs, 0).single() {
        Some(d) => d,
        None => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("last-dom: epoch out of range: {epoch}"),
            ));
        }
    };
    let date = dt.date_naive();
    let y = date.year();
    let m = date.month();
    // First day of next month minus one day = last day of this month.
    let first_next = if m == 12 {
        NaiveDate::from_ymd_opt(y + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(y, m + 1, 1)
    };
    match first_next {
        Some(next) => {
            let last = next.pred_opt().unwrap();
            let ts = last.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();
            Ok(Value::Number(ts as f64))
        }
        None => Err(RuntimeError::new(
            "ILO-R009",
            "last-dom: month arithmetic out of range".to_string(),
        )),
    }
}

#[inline(never)]
fn next_business_day_impl(arg: &Value) -> Result<Value> {
    // next-business-day dt:n > n — next weekday after dt (skip Sat/Sun).
    // If dt is Mon-Thu the result is the next day.
    // If dt is Fri the result is the following Mon.
    // If dt is Sat the result is Mon (+2). If dt is Sun the result is Mon (+1).
    // Returns the resulting epoch at 00:00 UTC.
    use chrono::{Datelike, Duration, NaiveDate, TimeZone, Utc, Weekday};
    let epoch = match arg {
        Value::Number(n) => *n,
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!(
                    "next-business-day: arg must be a number (epoch), got {:?}",
                    other
                ),
            ));
        }
    };
    let secs = epoch as i64;
    let dt = match Utc.timestamp_opt(secs, 0).single() {
        Some(d) => d,
        None => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("next-business-day: epoch out of range: {epoch}"),
            ));
        }
    };
    let date: NaiveDate = dt.date_naive();
    let days_ahead: i64 = match date.weekday() {
        Weekday::Fri => 3,
        Weekday::Sat => 2,
        _ => 1,
    };
    let next = date + Duration::days(days_ahead);
    let ts = next.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();
    Ok(Value::Number(ts as f64))
}

#[inline(never)]
fn day_of_week_impl(arg: &Value) -> Result<Value> {
    // day-of-week dt:n > n — 0=Sun, 1=Mon, 2=Tue, 3=Wed, 4=Thu, 5=Fri, 6=Sat.
    // Follows the JS/ISO convention where Sunday=0 (not 7), giving agents a
    // zero-based index usable directly with range-based dispatch.
    use chrono::{Datelike, TimeZone, Utc, Weekday};
    let epoch = match arg {
        Value::Number(n) => *n,
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("day-of-week: arg must be a number (epoch), got {:?}", other),
            ));
        }
    };
    let secs = epoch as i64;
    let dt = match Utc.timestamp_opt(secs, 0).single() {
        Some(d) => d,
        None => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("day-of-week: epoch out of range: {epoch}"),
            ));
        }
    };
    let dow: u32 = match dt.date_naive().weekday() {
        Weekday::Sun => 0,
        Weekday::Mon => 1,
        Weekday::Tue => 2,
        Weekday::Wed => 3,
        Weekday::Thu => 4,
        Weekday::Fri => 5,
        Weekday::Sat => 6,
    };
    Ok(Value::Number(dow as f64))
}

/// `bisect xs:L n target:n > n` — Python `bisect_left` insertion point in
/// a sorted numeric list. Returns the leftmost index `i` such that
/// `xs[0..i] < target <= xs[i..]`. Empty list returns `0`; target greater
/// than every element returns `len(xs)`; on ties the leftmost matching
/// index wins. NaN target propagates as NaN (matches `argmax`/`argmin`).
///
/// Caller is responsible for the sortedness precondition; we do NOT
/// validate it. The contract is that on a sorted input the result
/// satisfies the inequality above; on an unsorted input the result is
/// well-defined-but-meaningless rather than an error. Matches Python's
/// `bisect` module which also documents but does not enforce sortedness.
///
/// Bitwise op helpers (ILO-58 MVP). Each operand is converted to `u32` by
/// truncating to `u64` and masking to 32 bits (mod 2^32). The result is
/// cast back to `f64`. Shift/rotate amounts are taken mod 32 so out-of-
/// range values don't panic or saturate — consistent with Lua, Java, and
/// JavaScript's unsigned right-shift semantics.
///
/// `#[inline(never)]` keeps the call_function dispatch frame compact,
/// matching the established per-builtin helper pattern.
#[inline(never)]
fn run_bitwise(b: crate::builtins::Builtin, args: &[Value]) -> Result<Value> {
    use crate::builtins::Builtin;

    /// Extract a u32 from a Value::Number (mod 2^32).
    fn to_u32(v: &Value, pos: &str, name: &str) -> Result<u32> {
        match v {
            Value::Number(n) => {
                if !n.is_finite() {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("{name}: {pos} must be a finite number, got {n}"),
                    ));
                }
                Ok((*n as i64) as u32)
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("{name}: {pos} must be a number, got {other:?}"),
            )),
        }
    }

    let name = b.name();
    let result: u32 = match b {
        Builtin::Band => {
            to_u32(&args[0], "first arg", name)? & to_u32(&args[1], "second arg", name)?
        }
        Builtin::Bor => {
            to_u32(&args[0], "first arg", name)? | to_u32(&args[1], "second arg", name)?
        }
        Builtin::Bxor => {
            to_u32(&args[0], "first arg", name)? ^ to_u32(&args[1], "second arg", name)?
        }
        Builtin::Bnot => !to_u32(&args[0], "arg", name)?,
        Builtin::Bshl => {
            let x = to_u32(&args[0], "first arg", name)?;
            let n = to_u32(&args[1], "second arg", name)? % 32;
            x << n
        }
        Builtin::Bshr => {
            let x = to_u32(&args[0], "first arg", name)?;
            let n = to_u32(&args[1], "second arg", name)? % 32;
            x >> n
        }
        Builtin::Brot => {
            let x = to_u32(&args[0], "first arg", name)?;
            let n = to_u32(&args[1], "second arg", name)? % 32;
            x.rotate_left(n)
        }
        _ => unreachable!(),
    };
    Ok(Value::Number(result as f64))
}

/// 64-bit bitwise ops (ILO-395). Same shape as `run_bitwise` but masks to u64.
///
/// f64 can exactly represent integers up to 2^53; values >= 2^53 may lose
/// precision on the f64↔u64 round-trip. Inputs should stay within safe range.
///
/// `#[inline(never)]` keeps the call_function dispatch frame compact,
/// matching the established per-builtin helper pattern.
#[inline(never)]
fn run_bitwise_64(b: crate::builtins::Builtin, args: &[Value]) -> Result<Value> {
    use crate::builtins::Builtin;

    /// Extract a u64 from a Value::Number (mod 2^64 via cast).
    ///
    /// Negative f64 values are treated as signed and wrapped (matching the
    /// 32-bit `to_u32` behaviour). For f64 >= 0, we cast directly to u64 to
    /// avoid the i64 saturation that would occur for values in [2^63, 2^64).
    fn to_u64(v: &Value, pos: &str, name: &str) -> Result<u64> {
        match v {
            Value::Number(n) => {
                if !n.is_finite() {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("{name}: {pos} must be a finite number, got {n}"),
                    ));
                }
                let result = if *n < 0.0 {
                    // Negative: treat as signed, wrap into u64 via i64.
                    (*n as i64) as u64
                } else {
                    // Non-negative: cast directly to avoid i64 saturation for
                    // values in [2^63, 2^64).
                    *n as u64
                };
                Ok(result)
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("{name}: {pos} must be a number, got {other:?}"),
            )),
        }
    }

    let name = b.name();
    let result: u64 = match b {
        Builtin::Band64 => {
            to_u64(&args[0], "first arg", name)? & to_u64(&args[1], "second arg", name)?
        }
        Builtin::Bor64 => {
            to_u64(&args[0], "first arg", name)? | to_u64(&args[1], "second arg", name)?
        }
        Builtin::Bxor64 => {
            to_u64(&args[0], "first arg", name)? ^ to_u64(&args[1], "second arg", name)?
        }
        Builtin::Bnot64 => !to_u64(&args[0], "arg", name)?,
        Builtin::Bshl64 => {
            let x = to_u64(&args[0], "first arg", name)?;
            let n = to_u64(&args[1], "second arg", name)? % 64;
            x << n
        }
        Builtin::Bshr64 => {
            let x = to_u64(&args[0], "first arg", name)?;
            let n = to_u64(&args[1], "second arg", name)? % 64;
            x >> n
        }
        Builtin::Brot64 => {
            let x = to_u64(&args[0], "first arg", name)?;
            let n = (to_u64(&args[1], "second arg", name)? % 64) as u32;
            x.rotate_left(n)
        }
        _ => unreachable!(),
    };
    Ok(Value::Number(result as f64))
}

/// `#[inline(never)]` matches the established per-builtin helper pattern
/// (see `day_of_week_impl` above and the `vm_*` family) so the
/// call_function dispatch frame stays compact.
#[inline(never)]
fn run_bisect(list_arg: &Value, target_arg: &Value) -> Result<Value> {
    let items = match list_arg {
        Value::List(l) => l,
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("bisect: first arg must be a list, got {:?}", other),
            ));
        }
    };
    let target = match target_arg {
        Value::Number(n) => *n,
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("bisect: target must be a number, got {:?}", other),
            ));
        }
    };
    // NaN target: propagate. No total order against NaN means every branch
    // of the comparison is false; returning NaN matches the policy used by
    // `argmax`/`argmin` and avoids an arbitrary lo/hi result.
    if target.is_nan() {
        return Ok(Value::Number(f64::NAN));
    }
    // Empty list: insertion point is always 0.
    if items.is_empty() {
        return Ok(Value::Number(0.0));
    }
    // Validate element types up-front so type errors surface before the
    // search loop touches them — same shape as `argsort` above.
    for item in items.iter() {
        if !matches!(item, Value::Number(_)) {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("bisect: list elements must be numbers, got {:?}", item),
            ));
        }
    }
    // Classic bisect_left: half-open `[lo, hi)` window narrowed by strict
    // `<` so equal elements land to the right of the inserted target.
    let mut lo: usize = 0;
    let mut hi: usize = items.len();
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let Value::Number(m) = items[mid] else {
            unreachable!("validated above")
        };
        if m < target {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    Ok(Value::Number(lo as f64))
}

// Each builtin lives in its own #[inline(never)] helper so the call_function
// dispatch frame stays off the Rust call stack for deep-recursion tests
// (see #494 / #506 / #515 / ILO-341).

#[inline(never)]
fn median_run(items: &[Value]) -> Result<Value> {
    if items.is_empty() {
        return Err(RuntimeError::new(
            "ILO-R009",
            "median: cannot take median of an empty list".to_string(),
        ));
    }
    let mut nums: Vec<f64> = Vec::with_capacity(items.len());
    for item in items.iter() {
        match item {
            Value::Number(n) => nums.push(*n),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("median: list elements must be numbers, got {:?}", other),
                ));
            }
        }
    }
    if nums.iter().any(|x| x.is_nan()) {
        return Ok(Value::Number(f64::NAN));
    }
    nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = nums.len();
    let m = if n % 2 == 1 {
        nums[n / 2]
    } else {
        (nums[n / 2 - 1] + nums[n / 2]) / 2.0
    };
    Ok(Value::Number(m))
}

#[inline(never)]
fn quantile_run(items: &[Value], p: f64) -> Result<Value> {
    if items.is_empty() {
        return Err(RuntimeError::new(
            "ILO-R009",
            "quantile: cannot take quantile of an empty list".to_string(),
        ));
    }
    let mut nums: Vec<f64> = Vec::with_capacity(items.len());
    for item in items.iter() {
        match item {
            Value::Number(n) => nums.push(*n),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("quantile: list elements must be numbers, got {:?}", other),
                ));
            }
        }
    }
    if nums.iter().any(|x| x.is_nan()) {
        return Ok(Value::Number(f64::NAN));
    }
    nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p = p.clamp(0.0, 1.0);
    let n = nums.len();
    if n == 1 {
        return Ok(Value::Number(nums[0]));
    }
    let pos = p * (n - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    let frac = pos - lo as f64;
    let q = nums[lo] + frac * (nums[hi] - nums[lo]);
    Ok(Value::Number(q))
}

#[inline(never)]
fn variance_run(items: &[Value]) -> Result<Value> {
    if items.is_empty() {
        return Err(RuntimeError::new(
            "ILO-R009",
            "variance: cannot take variance of an empty list".to_string(),
        ));
    }
    let mut nums: Vec<f64> = Vec::with_capacity(items.len());
    for item in items.iter() {
        match item {
            Value::Number(n) => nums.push(*n),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("variance: list elements must be numbers, got {:?}", other),
                ));
            }
        }
    }
    let n = nums.len();
    if n == 1 {
        return Err(RuntimeError::new(
            "ILO-R009",
            "variance: at least 2 samples required".to_string(),
        ));
    }
    if nums.iter().any(|x| x.is_nan()) {
        return Ok(Value::Number(f64::NAN));
    }
    let mean = nums.iter().sum::<f64>() / n as f64;
    let sse: f64 = nums.iter().map(|x| (x - mean).powi(2)).sum();
    Ok(Value::Number(sse / (n - 1) as f64))
}

#[inline(never)]
fn stdev_run(items: &[Value]) -> Result<Value> {
    if items.is_empty() {
        return Err(RuntimeError::new(
            "ILO-R009",
            "stdev: cannot take stdev of an empty list".to_string(),
        ));
    }
    let mut nums: Vec<f64> = Vec::with_capacity(items.len());
    for item in items.iter() {
        match item {
            Value::Number(n) => nums.push(*n),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("stdev: list elements must be numbers, got {:?}", other),
                ));
            }
        }
    }
    let n = nums.len();
    if n == 1 {
        return Err(RuntimeError::new(
            "ILO-R009",
            "stdev: at least 2 samples required".to_string(),
        ));
    }
    if nums.iter().any(|x| x.is_nan()) {
        return Ok(Value::Number(f64::NAN));
    }
    let mean = nums.iter().sum::<f64>() / n as f64;
    let sse: f64 = nums.iter().map(|x| (x - mean).powi(2)).sum();
    Ok(Value::Number((sse / (n - 1) as f64).sqrt()))
}

#[inline(never)]
fn rgx_run(pattern: &str, input: &str) -> Result<Value> {
    let re = regex::Regex::new(pattern)
        .map_err(|e| RuntimeError::new("ILO-R009", format!("rgx: invalid regex pattern: {e}")))?;
    let result: Vec<Value> = if re.captures_len() > 1 {
        re.captures(input)
            .map(|caps| {
                (1..caps.len())
                    .filter_map(|i| {
                        caps.get(i)
                            .map(|m| Value::Text(Arc::new(m.as_str().to_string())))
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        re.find_iter(input)
            .map(|m| Value::Text(Arc::new(m.as_str().to_string())))
            .collect()
    };
    Ok(Value::List(Arc::new(result)))
}

#[inline(never)]
fn rgxall_run(pattern: &str, input: &str) -> Result<Value> {
    let re = regex::Regex::new(pattern).map_err(|e| {
        RuntimeError::new("ILO-R009", format!("rgxall: invalid regex pattern: {e}"))
    })?;
    let result: Vec<Value> = if re.captures_len() > 1 {
        re.captures_iter(input)
            .map(|caps| {
                let groups: Vec<Value> = (1..caps.len())
                    .filter_map(|i| {
                        caps.get(i)
                            .map(|m| Value::Text(Arc::new(m.as_str().to_string())))
                    })
                    .collect();
                Value::List(Arc::new(groups))
            })
            .collect()
    } else {
        re.find_iter(input)
            .map(|m| {
                Value::List(Arc::new(vec![Value::Text(Arc::new(
                    m.as_str().to_string(),
                ))]))
            })
            .collect()
    };
    Ok(Value::List(Arc::new(result)))
}

#[inline(never)]
fn rgxall1_run(pattern: &str, input: &str) -> Result<Value> {
    let re = regex::Regex::new(pattern).map_err(|e| {
        RuntimeError::new("ILO-R009", format!("rgxall1: invalid regex pattern: {e}"))
    })?;
    let group_count = re.captures_len().saturating_sub(1);
    if group_count >= 2 {
        return Err(RuntimeError::new(
            "ILO-R009",
            format!(
                "rgxall1: pattern has {group_count} capture groups; rgxall1 only supports 0 or 1. Use rgxall for L (L t) with every group preserved."
            ),
        ));
    }
    let result: Vec<Value> = if group_count == 1 {
        re.captures_iter(input)
            .filter_map(|caps| {
                caps.get(1)
                    .map(|m| Value::Text(Arc::new(m.as_str().to_string())))
            })
            .collect()
    } else {
        re.find_iter(input)
            .map(|m| Value::Text(Arc::new(m.as_str().to_string())))
            .collect()
    };
    Ok(Value::List(Arc::new(result)))
}

#[inline(never)]
fn rgxall_multi_run(pats: &Arc<Vec<Value>>, input: &Arc<String>) -> Result<Value> {
    let mut result: Vec<Value> = Vec::new();
    for (i, pat_val) in pats.iter().enumerate() {
        let pattern = match pat_val {
            Value::Text(s) => s.as_str(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "rgxall-multi: pats[{i}] must be a string pattern, got {:?}",
                        other
                    ),
                ));
            }
        };
        let re = regex::Regex::new(pattern).map_err(|e| {
            RuntimeError::new(
                "ILO-R009",
                format!("rgxall-multi: invalid regex pattern at index {i}: {e}"),
            )
        })?;
        let group_count = re.captures_len().saturating_sub(1);
        if group_count >= 2 {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!(
                    "rgxall-multi: pattern at index {i} has {group_count} capture groups; rgxall-multi only supports 0 or 1 per pattern. Use rgxall for L (L t) with every group preserved."
                ),
            ));
        }
        if group_count == 1 {
            re.captures_iter(input.as_str())
                .filter_map(|caps| {
                    caps.get(1)
                        .map(|m| Value::Text(Arc::new(m.as_str().to_string())))
                })
                .for_each(|v| result.push(v));
        } else {
            re.find_iter(input.as_str())
                .map(|m| Value::Text(Arc::new(m.as_str().to_string())))
                .for_each(|v| result.push(v));
        }
    }
    Ok(Value::List(Arc::new(result)))
}

#[inline(never)]
fn rgxsub_run(pattern: &str, replacement: &str, subject: &str) -> Result<Value> {
    let re = regex::Regex::new(pattern).map_err(|e| {
        RuntimeError::new("ILO-R009", format!("rgxsub: invalid regex pattern: {e}"))
    })?;
    Ok(Value::Text(Arc::new(
        re.replace_all(subject, replacement).into_owned(),
    )))
}

#[inline(never)]
fn matmul_run(a_rows: &[Value], b_rows: &[Value]) -> Result<Value> {
    let mut a: Vec<Vec<f64>> = Vec::with_capacity(a_rows.len());
    let mut a_cols: Option<usize> = None;
    for row in a_rows.iter() {
        match row {
            Value::List(r) => {
                match a_cols {
                    None => a_cols = Some(r.len()),
                    Some(n) if n != r.len() => {
                        return Err(RuntimeError::new(
                            "ILO-R009",
                            format!(
                                "matmul: ragged rows in first arg (expected {n} cols, got {})",
                                r.len()
                            ),
                        ));
                    }
                    _ => {}
                }
                let mut nums = Vec::with_capacity(r.len());
                for v in r.iter() {
                    match v {
                        Value::Number(n) => nums.push(*n),
                        other => {
                            return Err(RuntimeError::new(
                                "ILO-R009",
                                format!("matmul: elements must be numbers, got {:?}", other),
                            ));
                        }
                    }
                }
                a.push(nums);
            }
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("matmul: rows must be lists, got {:?}", other),
                ));
            }
        }
    }
    let mut b: Vec<Vec<f64>> = Vec::with_capacity(b_rows.len());
    let mut b_cols: Option<usize> = None;
    for row in b_rows.iter() {
        match row {
            Value::List(r) => {
                match b_cols {
                    None => b_cols = Some(r.len()),
                    Some(n) if n != r.len() => {
                        return Err(RuntimeError::new(
                            "ILO-R009",
                            format!(
                                "matmul: ragged rows in second arg (expected {n} cols, got {})",
                                r.len()
                            ),
                        ));
                    }
                    _ => {}
                }
                let mut nums = Vec::with_capacity(r.len());
                for v in r.iter() {
                    match v {
                        Value::Number(n) => nums.push(*n),
                        other => {
                            return Err(RuntimeError::new(
                                "ILO-R009",
                                format!("matmul: elements must be numbers, got {:?}", other),
                            ));
                        }
                    }
                }
                b.push(nums);
            }
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("matmul: rows must be lists, got {:?}", other),
                ));
            }
        }
    }
    let a_rows_n = a.len();
    let a_cols_n = a_cols.unwrap_or(0);
    let b_rows_n = b.len();
    let b_cols_n = b_cols.unwrap_or(0);
    if a_cols_n != b_rows_n {
        return Err(RuntimeError::new(
            "ILO-R009",
            format!(
                "matmul: shape mismatch (a is {a_rows_n}x{a_cols_n}, b is {b_rows_n}x{b_cols_n})"
            ),
        ));
    }
    let mut out: Vec<Value> = Vec::with_capacity(a_rows_n);
    #[allow(clippy::needless_range_loop)]
    for i in 0..a_rows_n {
        let mut row: Vec<Value> = Vec::with_capacity(b_cols_n);
        for j in 0..b_cols_n {
            let mut s = 0.0_f64;
            for k in 0..a_cols_n {
                s += a[i][k] * b[k][j];
            }
            row.push(Value::Number(s));
        }
        out.push(Value::List(Arc::new(row)));
    }
    Ok(Value::List(Arc::new(out)))
}

#[inline(never)]
fn ifft_run(items: &[Value]) -> Result<Value> {
    if items.is_empty() {
        return Err(RuntimeError::new(
            "ILO-R009",
            "ifft: input list must not be empty".to_string(),
        ));
    }
    let mut re: Vec<f64> = Vec::with_capacity(items.len());
    let mut im: Vec<f64> = Vec::with_capacity(items.len());
    for item in items.iter() {
        match item {
            Value::List(pair) if pair.len() == 2 => {
                let r = match &pair[0] {
                    Value::Number(n) => *n,
                    _ => {
                        return Err(RuntimeError::new(
                            "ILO-R009",
                            "ifft: pair elements must be numbers".to_string(),
                        ));
                    }
                };
                let i = match &pair[1] {
                    Value::Number(n) => *n,
                    _ => {
                        return Err(RuntimeError::new(
                            "ILO-R009",
                            "ifft: pair elements must be numbers".to_string(),
                        ));
                    }
                };
                re.push(r);
                im.push(i);
            }
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "ifft: each element must be a [real, imag] pair, got {:?}",
                        other
                    ),
                ));
            }
        }
    }
    let n = next_pow2(re.len());
    re.resize(n, 0.0);
    im.resize(n, 0.0);
    cooley_tukey(&mut re, &mut im, true);
    let result: Vec<Value> = re.into_iter().map(Value::Number).collect();
    Ok(Value::List(Arc::new(result)))
}

#[inline(never)]
fn where_run(cond: &[Value], xs: &[Value], ys: &[Value]) -> Result<Value> {
    if cond.len() != xs.len() || cond.len() != ys.len() {
        return Err(RuntimeError::new(
            "ILO-R009",
            format!(
                "where: length mismatch — cond={}, xs={}, ys={}; all three lists must be the same length",
                cond.len(),
                xs.len(),
                ys.len()
            ),
        ));
    }
    let mut out = Vec::with_capacity(cond.len());
    for (i, c) in cond.iter().enumerate() {
        match c {
            Value::Bool(true) => out.push(xs[i].clone()),
            Value::Bool(false) => out.push(ys[i].clone()),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "where: cond element at index {} must be a bool, got {:?}",
                        i, other
                    ),
                ));
            }
        }
    }
    Ok(Value::List(Arc::new(out)))
}

// --- per-builtin #[inline(never)] helpers (ILO-408 batch 2) ---
// Each helper keeps its body out of call_function's already-huge stack frame
// so debug-build stack overflows are avoided on deep-recursion tests.

/// `#[inline(never)]` keeps Clamp (+ inline Min/Max 2-arg + Argmax/Argmin)
/// out of call_function's frame.
#[inline(never)]
fn clamp_run(x: f64, lo: f64, hi: f64) -> Value {
    Value::Number(x.min(hi).max(lo))
}

/// `#[inline(never)]` — chunks n xs > L (L a)
#[inline(never)]
fn chunks_run(n_raw: f64, list_arg: &Value) -> Result<Value> {
    if n_raw.fract() != 0.0 || n_raw <= 0.0 {
        return Err(RuntimeError::new(
            "ILO-R009",
            format!("chunks: size must be a positive integer, got {n_raw}"),
        ));
    }
    let n = n_raw as usize;
    let xs = match list_arg {
        Value::List(items) => items,
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("chunks: requires a list, got {:?}", other),
            ));
        }
    };
    let mut out: Vec<Value> = Vec::with_capacity(xs.len().div_ceil(n));
    for chunk in xs.chunks(n) {
        out.push(Value::List(Arc::new(chunk.to_vec())));
    }
    Ok(Value::List(Arc::new(out)))
}

/// `#[inline(never)]` — ewm xs a > L n
#[inline(never)]
fn ewm_run(list_arg: &Value, a: f64) -> Result<Value> {
    let items = match list_arg {
        Value::List(l) => l,
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("ewm: first arg must be a list, got {:?}", other),
            ));
        }
    };
    if !(0.0..=1.0).contains(&a) {
        return Err(RuntimeError::new(
            "ILO-R009",
            format!("ewm: smoothing factor a must be in [0, 1], got {}", a),
        ));
    }
    let mut nums: Vec<f64> = Vec::with_capacity(items.len());
    for item in items.iter() {
        match item {
            Value::Number(n) => nums.push(*n),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("ewm: list elements must be numbers, got {:?}", other),
                ));
            }
        }
    }
    let out: Vec<Value> = ewm_compute(&nums, a)
        .into_iter()
        .map(Value::Number)
        .collect();
    Ok(Value::List(Arc::new(out)))
}

/// `#[inline(never)]` — cap s > t (capitalise first Unicode scalar)
/// Also handles the padl/padr arm that immediately follows in source order
/// (they form one logical "arm" for the script counter since no `if builtin ==`
/// separates them).
#[inline(never)]
fn cap_run(arg: &Value) -> Result<Value> {
    match arg {
        Value::Text(s) => {
            let mut chars = s.chars();
            let out = match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            };
            Ok(Value::Text(Arc::new(out)))
        }
        other => Err(RuntimeError::new(
            "ILO-R009",
            format!("cap requires text, got {:?}", other),
        )),
    }
}

/// `#[inline(never)]` — padl/padr s width [pad_char] > t
#[inline(never)]
fn padl_padr_run(is_left: bool, args: &[Value]) -> Result<Value> {
    let name = if is_left { "padl" } else { "padr" };
    let s = match &args[0] {
        Value::Text(t) => t.clone(),
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("{name} arg 1 requires text, got {:?}", other),
            ));
        }
    };
    let w = match &args[1] {
        Value::Number(n) => {
            if !n.is_finite() || n.fract() != 0.0 {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("{name} width must be a non-negative integer, got {n}"),
                ));
            }
            if *n < 0.0 {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("{name} width must be non-negative, got {n}"),
                ));
            }
            *n as usize
        }
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("{name} arg 2 requires number, got {:?}", other),
            ));
        }
    };
    let pad_char: char = if args.len() == 3 {
        match &args[2] {
            Value::Text(t) => {
                let mut iter = t.chars();
                match (iter.next(), iter.next()) {
                    (Some(c), None) => c,
                    _ => {
                        return Err(RuntimeError::new(
                            "ILO-R009",
                            format!(
                                "{name} pad char must be a 1-character string, got {:?}",
                                t.as_str()
                            ),
                        ));
                    }
                }
            }
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "{name} pad char must be a 1-character string, got {:?}",
                        other
                    ),
                ));
            }
        }
    } else {
        ' '
    };
    let char_count = s.chars().count();
    if char_count >= w {
        return Ok(Value::Text(s));
    }
    let pad: String = std::iter::repeat_n(pad_char, w - char_count).collect();
    let out = if is_left {
        format!("{pad}{s}")
    } else {
        format!("{s}{pad}")
    };
    Ok(Value::Text(Arc::new(out)))
}

/// `#[inline(never)]` — wr path content [fmt] > R t t
#[inline(never)]
fn wr_run(env: &mut Env, args: Vec<Value>) -> Result<Value> {
    let path = match &args[0] {
        Value::Text(s) => s.clone(),
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("wr: first arg must be a text path, got {:?}", other),
            ));
        }
    };
    if let Err(msg) = env.caps.check_write(path.as_str()) {
        return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
    }
    let content = if args.len() == 3 {
        let fmt = match &args[2] {
            Value::Text(s) => s.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("wr: format arg must be text, got {:?}", other),
                ));
            }
        };
        match fmt.as_str() {
            "csv" | "tsv" => {
                let sep = if fmt.as_str() == "csv" { ',' } else { '\t' };
                let rows = match &args[1] {
                    Value::List(l) => l,
                    other => {
                        return Err(RuntimeError::new(
                            "ILO-R009",
                            format!("wr: data for {fmt} must be a list of rows, got {:?}", other),
                        ));
                    }
                };
                write_csv_tsv(rows, sep)?
            }
            "json" => {
                fn value_to_json_local(v: &Value) -> serde_json::Value {
                    match v {
                        Value::Number(n) => serde_json::Value::from(*n),
                        Value::Text(s) => serde_json::Value::from(s.as_str()),
                        Value::Bool(b) => serde_json::Value::from(*b),
                        Value::List(l) => {
                            serde_json::Value::Array(l.iter().map(value_to_json_local).collect())
                        }
                        Value::Map(m) => {
                            let obj: serde_json::Map<String, serde_json::Value> = m
                                .iter()
                                .map(|(k, v)| (k.to_display_string(), value_to_json_local(v)))
                                .collect();
                            serde_json::Value::Object(obj)
                        }
                        Value::Nil => serde_json::Value::Null,
                        other => serde_json::Value::from(format!("{other}")),
                    }
                }
                serde_json::to_string_pretty(&value_to_json_local(&args[1]))
                    .unwrap_or_else(|e| format!("json error: {e}"))
            }
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("wr: unknown format '{other}', expected csv, tsv, or json"),
                ));
            }
        }
    } else {
        match &args[1] {
            Value::Text(s) => (**s).clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("wr: second arg must be text content, got {:?}", other),
                ));
            }
        }
    };
    match std::fs::write(path.as_str(), &content) {
        Ok(()) => Ok(Value::Ok(Box::new(Value::Text(path)))),
        Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
    }
}

/// `#[inline(never)]` — fmt template args... > t
#[inline(never)]
fn fmt_run(args: &[Value]) -> Result<Value> {
    let template = match &args[0] {
        Value::Text(s) => s.clone(),
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("fmt first arg must be text template, got {:?}", other),
            ));
        }
    };
    let mut result = String::new();
    let mut arg_idx = 1;
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{'
            && (chars.peek() == Some(&'}')
                || chars.peek() == Some(&':')
                || chars.peek() == Some(&'.'))
        {
            let mut spec = String::from("{");
            let mut terminated = false;
            for sc in chars.by_ref() {
                spec.push(sc);
                if sc == '}' {
                    terminated = true;
                    break;
                }
            }
            if !terminated {
                result.push_str(&spec);
                continue;
            }
            match parse_fmt_spec(&spec) {
                Some(FmtSpec::Bare) => {
                    if arg_idx < args.len() {
                        result.push_str(&format!("{}", args[arg_idx]));
                        arg_idx += 1;
                    } else {
                        result.push_str("{}");
                    }
                }
                Some(spec_kind) => {
                    if arg_idx >= args.len() {
                        return Err(RuntimeError::new(
                            "ILO-R009",
                            format!("fmt template spec `{spec}` has no matching value arg"),
                        ));
                    }
                    let rendered = apply_fmt_spec(&spec_kind, &args[arg_idx]).map_err(|e| {
                        RuntimeError::new("ILO-R009", format!("fmt spec `{spec}`: {e}"))
                    })?;
                    result.push_str(&rendered);
                    arg_idx += 1;
                }
                None => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!(
                            "fmt: unsupported placeholder spec `{spec}`. \
                             Supported: `{{}}`, `{{.Nf}}` / `{{:.Nf}}` (decimal places), \
                             `{{:N}}` (right-align width), `{{:Nd}}` (integer width), \
                             `{{:<N}}` (left-align width). Zero-padded widths and hex/sign \
                             are out of scope; compose via `fmt2` / `padl` / `padr`."
                        ),
                    ));
                }
            }
        } else {
            result.push(c);
        }
    }
    Ok(Value::Text(Arc::new(result)))
}

/// `#[inline(never)]` — flt fn [ctx] xs > L a
#[inline(never)]
fn flt_run(
    env: &mut Env,
    fn_name: &str,
    captures: Vec<Value>,
    ctx: Option<Value>,
    items: Arc<Vec<Value>>,
) -> Result<Value> {
    let mut keep: Vec<bool> = Vec::with_capacity(items.len());
    for item in items.iter() {
        let mut call_args = match &ctx {
            Some(c) => vec![item.clone(), c.clone()],
            None => vec![item.clone()],
        };
        call_args.extend(captures.iter().cloned());
        match call_function(env, fn_name, call_args)? {
            Value::Bool(true) => keep.push(true),
            Value::Bool(false) => keep.push(false),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("flt: predicate must return bool, got {:?}", other),
                ));
            }
        }
    }
    let mut items = items;
    if Arc::strong_count(&items) == 1 {
        let inner = Arc::make_mut(&mut items);
        let mut idx = 0usize;
        inner.retain(|_| {
            let k = keep[idx];
            idx += 1;
            k
        });
        Ok(Value::List(items))
    } else {
        let mut result = Vec::with_capacity(keep.iter().filter(|k| **k).count());
        for (item, &k) in items.iter().zip(keep.iter()) {
            if k {
                result.push(item.clone());
            }
        }
        Ok(Value::List(Arc::new(result)))
    }
}

/// `#[inline(never)]` — grp fn xs > M t (L a)
#[inline(never)]
fn grp_run(
    env: &mut Env,
    fn_name: &str,
    captures: Vec<Value>,
    items: Arc<Vec<Value>>,
) -> Result<Value> {
    let mut groups: std::collections::HashMap<MapKey, Vec<Value>> =
        std::collections::HashMap::new();
    for item in items.iter() {
        let mut call_args = vec![item.clone()];
        call_args.extend(captures.iter().cloned());
        let key = call_function(env, fn_name, call_args)?;
        let map_key = match &key {
            Value::Text(s) => MapKey::Text((**s).clone()),
            Value::Number(n) => {
                if !n.is_finite() {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("grp: numeric key must be finite, got {n}"),
                    ));
                }
                MapKey::Int(n.floor() as i64)
            }
            Value::Bool(b) => MapKey::Text(format!("{b}")),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "grp: key function must return a string, number, or bool, got {:?}",
                        other
                    ),
                ));
            }
        };
        groups.entry(map_key).or_default().push(item.clone());
    }
    let map: HashMap<MapKey, Value> = groups
        .into_iter()
        .map(|(k, v)| (k, Value::List(Arc::new(v))))
        .collect();
    Ok(Value::Map(Arc::new(map)))
}

/// `#[inline(never)]` — uniqby fn xs > L a
#[inline(never)]
fn uniqby_run(
    env: &mut Env,
    fn_name: &str,
    captures: Vec<Value>,
    items: Arc<Vec<Value>>,
) -> Result<Value> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<Value> = Vec::new();
    for item in items.iter() {
        let mut call_args = vec![item.clone()];
        call_args.extend(captures.iter().cloned());
        let key = call_function(env, fn_name, call_args)?;
        let key_str = match &key {
            Value::Text(s) => format!("t:{s}"),
            Value::Number(n) => {
                if *n == (*n as i64) as f64 {
                    format!("n:{}", *n as i64)
                } else {
                    format!("n:{n}")
                }
            }
            Value::Bool(b) => format!("b:{b}"),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "uniqby: key function must return a string, number, or bool, got {:?}",
                        other
                    ),
                ));
            }
        };
        if seen.insert(key_str) {
            out.push(item.clone());
        }
    }
    Ok(Value::List(Arc::new(out)))
}

/// `#[inline(never)]` — post url body [headers] > R t t
#[inline(never)]
#[allow(dead_code)]
fn post_run(env: &mut Env, args: Vec<Value>) -> Result<Value> {
    let (url, body) = match (&args[0], &args[1]) {
        (Value::Text(u), Value::Text(b)) => (u.clone(), b.clone()),
        _ => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("pst requires (t, t), got ({:?}, {:?})", args[0], args[1]),
            ));
        }
    };
    if let Err(msg) = env.caps.check_net(url.as_str()) {
        return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
    }
    let headers = if args.len() == 3 {
        match &args[2] {
            Value::Map(m) => m
                .iter()
                .map(|(k, v)| {
                    let vs: String = match v {
                        Value::Text(s) => (**s).clone(),
                        other => format!("{other:?}"),
                    };
                    (k.to_display_string(), vs)
                })
                .collect::<Vec<_>>(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("pst headers must be M t t, got {:?}", other),
                ));
            }
        }
    } else {
        vec![]
    };
    #[cfg(feature = "http")]
    {
        let mut req = minreq::post(url.as_str()).with_body(body.as_str());
        for (k, v) in &headers {
            req = req.with_header(k.as_str(), v.as_str());
        }
        match req.send() {
            Ok(resp) => match resp.as_str() {
                Ok(b) => Ok(Value::Ok(Box::new(Value::Text(Arc::new(b.to_string()))))),
                Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(format!(
                    "response is not valid UTF-8: {e}"
                )))))),
            },
            Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
        }
    }
    #[cfg(not(feature = "http"))]
    {
        let _ = (url, body, headers);
        Ok(Value::Err(Box::new(Value::Text(
            "http feature not enabled".to_string().into(),
        ))))
    }
}

/// `#[inline(never)]` — put/pat url body [headers] > R t t
#[inline(never)]
fn put_pat_run(env: &mut Env, builtin: Option<Builtin>, args: Vec<Value>) -> Result<Value> {
    let name = builtin.unwrap().name();
    let (url, body) = match (&args[0], &args[1]) {
        (Value::Text(u), Value::Text(b)) => (u.clone(), b.clone()),
        _ => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("{name} requires (t, t), got ({:?}, {:?})", args[0], args[1]),
            ));
        }
    };
    if let Err(msg) = env.caps.check_net(url.as_str()) {
        return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
    }
    let headers = if args.len() == 3 {
        match &args[2] {
            Value::Map(m) => m
                .iter()
                .map(|(k, v)| {
                    let vs: String = match v {
                        Value::Text(s) => (**s).clone(),
                        other => format!("{other:?}"),
                    };
                    (k.to_display_string(), vs)
                })
                .collect::<Vec<_>>(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("{name} headers must be M t t, got {:?}", other),
                ));
            }
        }
    } else {
        vec![]
    };
    #[cfg(feature = "http")]
    {
        let mut req = match builtin {
            Some(Builtin::Put) => minreq::put(url.as_str()),
            Some(Builtin::Pat) => minreq::patch(url.as_str()),
            _ => unreachable!(),
        }
        .with_body(body.as_str());
        for (k, v) in &headers {
            req = req.with_header(k.as_str(), v.as_str());
        }
        match req.send() {
            Ok(resp) => match resp.as_str() {
                Ok(b) => Ok(Value::Ok(Box::new(Value::Text(Arc::new(b.to_string()))))),
                Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(format!(
                    "response is not valid UTF-8: {e}"
                )))))),
            },
            Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
        }
    }
    #[cfg(not(feature = "http"))]
    {
        let _ = (url, body, headers);
        Ok(Value::Err(Box::new(Value::Text(
            "http feature not enabled".to_string().into(),
        ))))
    }
}

/// `#[inline(never)]` — del/hed/opt url [headers] > R t t
#[inline(never)]
fn del_hed_opt_run(env: &mut Env, builtin: Option<Builtin>, args: Vec<Value>) -> Result<Value> {
    let name = builtin.unwrap().name();
    let url = match &args[0] {
        Value::Text(u) => u.clone(),
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("{name} requires text (url), got {:?}", other),
            ));
        }
    };
    if let Err(msg) = env.caps.check_net(url.as_str()) {
        return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
    }
    let headers = if args.len() == 2 {
        match &args[1] {
            Value::Map(m) => m
                .iter()
                .map(|(k, v)| {
                    let vs: String = match v {
                        Value::Text(s) => (**s).clone(),
                        other => format!("{other:?}"),
                    };
                    (k.to_display_string(), vs)
                })
                .collect::<Vec<_>>(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("{name} headers must be M t t, got {:?}", other),
                ));
            }
        }
    } else {
        vec![]
    };
    #[cfg(feature = "http")]
    {
        let mut req = match builtin {
            Some(Builtin::Del) => minreq::delete(url.as_str()),
            Some(Builtin::Hed) => minreq::head(url.as_str()),
            Some(Builtin::Opt) => minreq::options(url.as_str()),
            _ => unreachable!(),
        };
        for (k, v) in &headers {
            req = req.with_header(k.as_str(), v.as_str());
        }
        match req.send() {
            Ok(resp) => match resp.as_str() {
                Ok(body) => Ok(Value::Ok(Box::new(Value::Text(Arc::new(body.to_string()))))),
                Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(format!(
                    "response is not valid UTF-8: {e}"
                )))))),
            },
            Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
        }
    }
    #[cfg(not(feature = "http"))]
    {
        let _ = (url, headers);
        Ok(Value::Err(Box::new(Value::Text(
            "http feature not enabled".to_string().into(),
        ))))
    }
}

/// `#[inline(never)]` — rolling-window reducers: rsum/ravg/rmin n xs > L n
#[inline(never)]
fn rolling_window_run(env_name: &str, b: Builtin, n_f: f64, list_arg: &Value) -> Result<Value> {
    let name = env_name;
    if !n_f.is_finite() || n_f.fract() != 0.0 {
        return Err(RuntimeError::new(
            "ILO-R009",
            format!(
                "{name}: window size n must be a non-negative integer, got {}",
                n_f
            ),
        ));
    }
    if n_f <= 0.0 {
        return Err(RuntimeError::new(
            "ILO-R009",
            format!("{name}: window size n must be >= 1, got {}", n_f),
        ));
    }
    let n = n_f as usize;
    let items = match list_arg {
        Value::List(l) => l,
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("{name}: second arg must be a list, got {:?}", other),
            ));
        }
    };
    let mut nums: Vec<f64> = Vec::with_capacity(items.len());
    for item in items.iter() {
        match item {
            Value::Number(v) => nums.push(*v),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("{name}: list elements must be numbers, got {:?}", other),
                ));
            }
        }
    }
    let computed = match b {
        Builtin::Rsum => rsum_compute(n, &nums),
        Builtin::Ravg => ravg_compute(n, &nums),
        Builtin::Rmin => rmin_compute(n, &nums),
        _ => unreachable!(),
    };
    let out: Vec<Value> = computed.into_iter().map(Value::Number).collect();
    Ok(Value::List(Arc::new(out)))
}

/// `#[inline(never)]` — argmax/argmin xs > n
#[inline(never)]
fn argmax_argmin_run(is_min: bool, name: &str, arg: &Value) -> Result<Value> {
    let items = match arg {
        Value::List(l) => l,
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("{name}: arg must be a list, got {:?}", other),
            ));
        }
    };
    if items.is_empty() {
        return Err(RuntimeError::new(
            "ILO-R009",
            format!("{name}: cannot take {name} of an empty list"),
        ));
    }
    let mut best_idx: usize = 0;
    let mut best_val: Option<f64> = None;
    for (i, item) in items.iter().enumerate() {
        match item {
            Value::Number(n) => {
                if n.is_nan() {
                    return Ok(Value::Number(f64::NAN));
                }
                match best_val {
                    None => {
                        best_val = Some(*n);
                        best_idx = i;
                    }
                    Some(cur) => {
                        let better = if is_min { *n < cur } else { *n > cur };
                        if better {
                            best_val = Some(*n);
                            best_idx = i;
                        }
                    }
                }
            }
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("{name}: list elements must be numbers, got {:?}", other),
                ));
            }
        }
    }
    Ok(Value::Number(best_idx as f64))
}

/// `#[inline(never)]` — setunion/setinter/setdiff xs ys > L a
#[inline(never)]
fn setops_run(builtin: Option<Builtin>, xs_arg: &Value, ys_arg: &Value) -> Result<Value> {
    let op_name = match builtin {
        Some(Builtin::Setunion) => "setunion",
        Some(Builtin::Setinter) => "setinter",
        Some(Builtin::Setdiff) => "setdiff",
        _ => unreachable!(),
    };
    let xs = match xs_arg {
        Value::List(items) => items,
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("{op_name} arg 1 requires a list, got {:?}", other),
            ));
        }
    };
    let ys = match ys_arg {
        Value::List(items) => items,
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("{op_name} arg 2 requires a list, got {:?}", other),
            ));
        }
    };
    fn key_for(v: &Value, op_name: &str) -> std::result::Result<String, RuntimeError> {
        match v {
            Value::Text(s) => Ok(format!("t:{s}")),
            Value::Number(n) => {
                if *n == (*n as i64) as f64 {
                    Ok(format!("n:{}", *n as i64))
                } else {
                    Ok(format!("n:{n}"))
                }
            }
            Value::Bool(b) => Ok(format!("b:{b}")),
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!(
                    "{op_name}: elements must be text, number, or bool, got {:?}",
                    other
                ),
            )),
        }
    }
    use std::collections::{HashMap, HashSet};
    let mut set_a: HashSet<String> = HashSet::new();
    let mut a_first: HashMap<String, Value> = HashMap::new();
    for v in xs.iter() {
        let k = key_for(v, op_name)?;
        if set_a.insert(k.clone()) {
            a_first.insert(k, v.clone());
        }
    }
    let mut set_b: HashSet<String> = HashSet::new();
    let mut b_first: HashMap<String, Value> = HashMap::new();
    for v in ys.iter() {
        let k = key_for(v, op_name)?;
        if set_b.insert(k.clone()) {
            b_first.insert(k, v.clone());
        }
    }
    let (result_keys, value_lookup): (Vec<String>, &HashMap<String, Value>) = match builtin {
        Some(Builtin::Setunion) => {
            let mut keys: Vec<String> = set_a.union(&set_b).cloned().collect();
            let mut merged = a_first;
            for (k, v) in &b_first {
                merged.entry(k.clone()).or_insert_with(|| v.clone());
            }
            keys.sort();
            let mut out: Vec<Value> = Vec::with_capacity(keys.len());
            for k in &keys {
                if let Some(v) = merged.get(k) {
                    out.push(v.clone());
                }
            }
            return Ok(Value::List(Arc::new(out)));
        }
        Some(Builtin::Setinter) => (
            set_a.intersection(&set_b).cloned().collect::<Vec<_>>(),
            &a_first,
        ),
        Some(Builtin::Setdiff) => (
            set_a.difference(&set_b).cloned().collect::<Vec<_>>(),
            &a_first,
        ),
        _ => unreachable!(),
    };
    let mut keys = result_keys;
    keys.sort();
    let mut out: Vec<Value> = Vec::with_capacity(keys.len());
    for k in &keys {
        if let Some(v) = value_lookup.get(k) {
            out.push(v.clone());
        }
    }
    Ok(Value::List(Arc::new(out)))
}

// ── Signal/math cluster helpers (0.13.0) ────────────────────────────────────

/// `convolve xs ys > L n` — discrete linear convolution of two real-valued
/// sequences. Output length = `len xs + len ys - 1`. O(n*m) direct-sum
/// implementation; suitable for short-to-medium kernels (signal filtering,
/// polynomial multiplication). Mirrors NumPy `np.convolve(a, b, mode="full")`.
///
/// `#[inline(never)]` keeps this body out of `call_function`'s already-large
/// dispatch frame and matches the established per-builtin helper pattern.
#[inline(never)]
fn run_convolve(xs_val: &Value, ys_val: &Value) -> Result<Value> {
    let xs = match xs_val {
        Value::List(l) => l,
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("convolve: first arg must be a list, got {:?}", other),
            ));
        }
    };
    let ys = match ys_val {
        Value::List(l) => l,
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("convolve: second arg must be a list, got {:?}", other),
            ));
        }
    };
    if xs.is_empty() || ys.is_empty() {
        return Err(RuntimeError::new(
            "ILO-R009",
            "convolve: both input lists must be non-empty".to_string(),
        ));
    }
    let mut a = Vec::with_capacity(xs.len());
    for item in xs.iter() {
        match item {
            Value::Number(n) => a.push(*n),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "convolve: first list elements must be numbers, got {:?}",
                        other
                    ),
                ));
            }
        }
    }
    let mut b = Vec::with_capacity(ys.len());
    for item in ys.iter() {
        match item {
            Value::Number(n) => b.push(*n),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "convolve: second list elements must be numbers, got {:?}",
                        other
                    ),
                ));
            }
        }
    }
    convolve_compute(&a, &b)
        .map(|v| Value::List(Arc::new(v.into_iter().map(Value::Number).collect())))
}

/// Direct-sum discrete linear convolution. Output length = n + m - 1.
/// `#[inline(never)]` per the established per-builtin helper pattern.
#[inline(never)]
fn convolve_compute(a: &[f64], b: &[f64]) -> Result<Vec<f64>> {
    let n = a.len() + b.len() - 1;
    let mut out = vec![0.0_f64; n];
    for (i, &ai) in a.iter().enumerate() {
        for (j, &bj) in b.iter().enumerate() {
            out[i + j] += ai * bj;
        }
    }
    Ok(out)
}

/// `searchsorted xs targets > L n` — batch sorted-list insertion points.
/// Applies `bisect_left` semantics to each target, equivalent to
/// `map (t:n>n; bisect xs t) targets` but avoids the lambda overhead per
/// target. Callers are responsible for the sortedness precondition (same
/// contract as `bisect`).
///
/// `#[inline(never)]` per the established per-builtin helper pattern.
#[inline(never)]
fn run_searchsorted(xs_val: &Value, targets_val: &Value) -> Result<Value> {
    let xs = match xs_val {
        Value::List(l) => l,
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("searchsorted: first arg must be a list, got {:?}", other),
            ));
        }
    };
    let targets = match targets_val {
        Value::List(l) => l,
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!(
                    "searchsorted: second arg must be a list of targets, got {:?}",
                    other
                ),
            ));
        }
    };
    // Validate element types in xs up-front (same contract as bisect).
    let mut nums: Vec<f64> = Vec::with_capacity(xs.len());
    for item in xs.iter() {
        match item {
            Value::Number(n) => nums.push(*n),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "searchsorted: sorted-list elements must be numbers, got {:?}",
                        other
                    ),
                ));
            }
        }
    }
    let mut out = Vec::with_capacity(targets.len());
    for t_val in targets.iter() {
        let t = match t_val {
            Value::Number(n) => *n,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "searchsorted: target elements must be numbers, got {:?}",
                        other
                    ),
                ));
            }
        };
        if t.is_nan() {
            out.push(Value::Number(f64::NAN));
            continue;
        }
        let mut lo = 0usize;
        let mut hi = nums.len();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if nums[mid] < t {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        out.push(Value::Number(lo as f64));
    }
    Ok(Value::List(Arc::new(out)))
}

/// `cabs pair > n` — complex magnitude sqrt(re² + im²).
/// Accepts a 2-element `[re, im]` list (the same shape produced by `fft`).
///
/// `#[inline(never)]` per the established per-builtin helper pattern.
#[inline(never)]
fn run_cabs(pair_val: &Value) -> Result<Value> {
    let pair = match pair_val {
        Value::List(l) if l.len() == 2 => l,
        Value::List(l) => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!(
                    "cabs: expected [re, im] pair (length 2), got length {}",
                    l.len()
                ),
            ));
        }
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("cabs: arg must be a [re, im] list, got {:?}", other),
            ));
        }
    };
    let re = match &pair[0] {
        Value::Number(n) => *n,
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("cabs: re must be a number, got {:?}", other),
            ));
        }
    };
    let im = match &pair[1] {
        Value::Number(n) => *n,
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("cabs: im must be a number, got {:?}", other),
            ));
        }
    };
    Ok(Value::Number((re * re + im * im).sqrt()))
}

/// `cmul a b > L n` — complex multiply of two `[re, im]` pairs.
/// Returns a new `[re, im]` 2-element list. Implements
/// `(re_a*re_b - im_a*im_b, re_a*im_b + im_a*re_b)`.
///
/// `#[inline(never)]` per the established per-builtin helper pattern.
#[inline(never)]
fn run_cmul(a_val: &Value, b_val: &Value) -> Result<Value> {
    fn extract_pair(v: &Value, label: &str) -> Result<(f64, f64)> {
        let pair = match v {
            Value::List(l) if l.len() == 2 => l,
            Value::List(l) => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "cmul: {label} must be a [re, im] pair (length 2), got length {}",
                        l.len()
                    ),
                ));
            }
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("cmul: {label} must be a [re, im] list, got {:?}", other),
                ));
            }
        };
        let re = match &pair[0] {
            Value::Number(n) => *n,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("cmul: {label} re must be a number, got {:?}", other),
                ));
            }
        };
        let im = match &pair[1] {
            Value::Number(n) => *n,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("cmul: {label} im must be a number, got {:?}", other),
                ));
            }
        };
        Ok((re, im))
    }
    let (re_a, im_a) = extract_pair(a_val, "a")?;
    let (re_b, im_b) = extract_pair(b_val, "b")?;
    let re_out = re_a * re_b - im_a * im_b;
    let im_out = re_a * im_b + im_a * re_b;
    Ok(Value::List(Arc::new(vec![
        Value::Number(re_out),
        Value::Number(im_out),
    ])))
}

/// `pdist2 xs ys > L n` — element-wise squared Euclidean distance.
/// Equivalent to `map (i:n>n; pow (- (at xs i) (at ys i)) 2) (range 0 (len xs))`.
/// Lists must have the same length; mismatch raises ILO-R009.
///
/// `#[inline(never)]` per the established per-builtin helper pattern.
#[inline(never)]
fn run_pdist2(xs_val: &Value, ys_val: &Value) -> Result<Value> {
    let xs = match xs_val {
        Value::List(l) => l,
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("pdist2: first arg must be a list, got {:?}", other),
            ));
        }
    };
    let ys = match ys_val {
        Value::List(l) => l,
        other => {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("pdist2: second arg must be a list, got {:?}", other),
            ));
        }
    };
    if xs.len() != ys.len() {
        return Err(RuntimeError::new(
            "ILO-R009",
            format!(
                "pdist2: lists must have the same length, got {} and {}",
                xs.len(),
                ys.len()
            ),
        ));
    }
    let mut out = Vec::with_capacity(xs.len());
    for (x_val, y_val) in xs.iter().zip(ys.iter()) {
        let x = match x_val {
            Value::Number(n) => *n,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "pdist2: first list elements must be numbers, got {:?}",
                        other
                    ),
                ));
            }
        };
        let y = match y_val {
            Value::Number(n) => *n,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "pdist2: second list elements must be numbers, got {:?}",
                        other
                    ),
                ));
            }
        };
        let d = x - y;
        out.push(Value::Number(d * d));
    }
    Ok(Value::List(Arc::new(out)))
}

fn call_function(env: &mut Env, name: &str, args: Vec<Value>) -> Result<Value> {
    // Builtins — resolve name to enum once, then dispatch via match
    let builtin = Builtin::from_name(name);
    if builtin == Some(Builtin::Len) {
        if args.len() != 1 {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("len: expected 1 arg, got {}", args.len()),
            ));
        }
        return match &args[0] {
            Value::Text(s) => Ok(Value::Number(s.len() as f64)),
            Value::List(l) => Ok(Value::Number(l.len() as f64)),
            Value::Map(m) => Ok(Value::Number(m.len() as f64)),
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("len requires string, list, or map, got {:?}", other),
            )),
        };
    }
    // Map builtins
    if builtin == Some(Builtin::Mmap) && args.is_empty() {
        return Ok(Value::Map(Arc::new(HashMap::new())));
    }
    if builtin == Some(Builtin::Mget) && args.len() == 2 {
        return match &args[0] {
            Value::Map(m) => {
                let key = MapKey::from_value(&args[1], "mget")?;
                Ok(m.get(&key).cloned().unwrap_or(Value::Nil))
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "mget: expects map and key".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Mset) && args.len() == 3 {
        // Consume args so we can move the Arc<HashMap> and try Arc::make_mut for
        // the RC=1 in-place mutation fast path (mirrors VM PR #249).
        let mut it = args.into_iter();
        let map_val = it.next().unwrap();
        let key_val = it.next().unwrap();
        let value = it.next().unwrap();
        return match map_val {
            Value::Map(mut m) => {
                let key = MapKey::from_value(&key_val, "mset")?;
                let inner = Arc::make_mut(&mut m);
                inner.insert(key, value);
                Ok(Value::Map(m))
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "mset: expects map, key, and value".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::MgetOr) && args.len() == 3 {
        // mget-or m k default — value at key, else default. Same lookup as
        // mget; never returns nil. Verifier enforces `default:v` matches the
        // map value type.
        return match &args[0] {
            Value::Map(m) => {
                let key = MapKey::from_value(&args[1], "mget-or")?;
                Ok(m.get(&key).cloned().unwrap_or_else(|| args[2].clone()))
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "mget-or: expects map, key, and default".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Mhas) && args.len() == 2 {
        return match &args[0] {
            Value::Map(m) => {
                let key = MapKey::from_value(&args[1], "mhas")?;
                Ok(Value::Bool(m.contains_key(&key)))
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "mhas: expects map and key".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Mkeys) && args.len() == 1 {
        return match &args[0] {
            Value::Map(m) => {
                let mut keys: Vec<&MapKey> = m.keys().collect();
                keys.sort();
                Ok(Value::List(Arc::new(
                    keys.into_iter().map(map_key_to_value).collect(),
                )))
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "mkeys: expects a map".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Mvals) && args.len() == 1 {
        return match &args[0] {
            Value::Map(m) => {
                let mut pairs: Vec<(&MapKey, &Value)> = m.iter().collect();
                pairs.sort_by_key(|(k, _)| (*k).clone());
                Ok(Value::List(Arc::new(
                    pairs.into_iter().map(|(_, v)| v.clone()).collect(),
                )))
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "mvals: expects a map".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Mpairs) && args.len() == 1 {
        return match &args[0] {
            Value::Map(m) => {
                let mut pairs: Vec<(&MapKey, &Value)> = m.iter().collect();
                pairs.sort_by_key(|(k, _)| (*k).clone());
                let out: Vec<Value> = pairs
                    .into_iter()
                    .map(|(k, v)| Value::List(Arc::new(vec![map_key_to_value(k), v.clone()])))
                    .collect();
                Ok(Value::List(Arc::new(out)))
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "mpairs: expects a map".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Mdel) && args.len() == 2 {
        let mut it = args.into_iter();
        let map_val = it.next().unwrap();
        let key_val = it.next().unwrap();
        return match map_val {
            Value::Map(mut m) => {
                let key = MapKey::from_value(&key_val, "mdel")?;
                let inner = Arc::make_mut(&mut m);
                inner.remove(&key);
                Ok(Value::Map(m))
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "mdel: expects map and key".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Det) && args.len() == 1 {
        let mat = matrix_from_value(&args[0], "det")?;
        let n = mat.len();
        if n == 0 {
            return Err(RuntimeError::new(
                "ILO-R009",
                "det: empty matrix".to_string(),
            ));
        }
        for row in &mat {
            if row.len() != n {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    "det: matrix must be square".to_string(),
                ));
            }
        }
        let (_lu, _piv, det, _) = lu_decompose(mat);
        return Ok(Value::Number(det));
    }
    if builtin == Some(Builtin::Inv) && args.len() == 1 {
        let mat = matrix_from_value(&args[0], "inv")?;
        let n = mat.len();
        if n == 0 {
            return Err(RuntimeError::new(
                "ILO-R009",
                "inv: empty matrix".to_string(),
            ));
        }
        for row in &mat {
            if row.len() != n {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    "inv: matrix must be square".to_string(),
                ));
            }
        }
        let (lu, piv, _det, singular) = lu_decompose(mat);
        if singular {
            return Err(RuntimeError::new(
                "ILO-R009",
                "inv: matrix is singular".to_string(),
            ));
        }
        let mut cols: Vec<Vec<f64>> = Vec::with_capacity(n);
        for j in 0..n {
            let mut e = vec![0.0; n];
            e[j] = 1.0;
            cols.push(lu_solve(&lu, &piv, &e));
        }
        // Assemble row-major: result[i][j] = cols[j][i]
        let rows: Vec<Value> = (0..n)
            .map(|i| {
                Value::List(Arc::new(
                    (0..n)
                        .map(|j| Value::Number(cols[j][i]))
                        .collect::<Vec<_>>(),
                ))
            })
            .collect();
        return Ok(Value::List(Arc::new(rows)));
    }
    if builtin == Some(Builtin::Solve) && args.len() == 2 {
        let mat = matrix_from_value(&args[0], "solve")?;
        let b = vec_from_value(&args[1], "solve")?;
        let n = mat.len();
        if n == 0 {
            return Err(RuntimeError::new(
                "ILO-R009",
                "solve: empty matrix".to_string(),
            ));
        }
        for row in &mat {
            if row.len() != n {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    "solve: matrix must be square".to_string(),
                ));
            }
        }
        if b.len() != n {
            return Err(RuntimeError::new(
                "ILO-R009",
                "solve: vector length must match matrix size".to_string(),
            ));
        }
        let (lu, piv, _det, singular) = lu_decompose(mat);
        if singular {
            return Err(RuntimeError::new(
                "ILO-R009",
                "solve: matrix is singular".to_string(),
            ));
        }
        let x = lu_solve(&lu, &piv, &b);
        return Ok(Value::List(Arc::new(
            x.into_iter().map(Value::Number).collect(),
        )));
    }
    if builtin == Some(Builtin::Lstsq) && args.len() == 2 {
        // Out-of-line helper to keep this arm's frame off the giant
        // `call_function` stack frame. Same pattern documented in #506
        // for sha2/hmac and #494 for caps fields: each builtin arm adds
        // frame bloat that compounds with deep recursion through
        // tree-walking tests like `interpret_braceless_guard_fibonacci`.
        return lstsq_run(&args[0], &args[1]);
    }
    if builtin == Some(Builtin::Str) {
        if args.len() != 1 {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("str: expected 1 arg, got {}", args.len()),
            ));
        }
        return match &args[0] {
            Value::Number(n) => {
                let s = if n.fract() == 0.0 && n.abs() < 1e15 {
                    format!("{}", *n as i64)
                } else {
                    format!("{}", n)
                };
                Ok(Value::Text(Arc::new(s)))
            }
            Value::Text(_) => Ok(args[0].clone()),
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("str requires a number or text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Num) {
        if args.len() != 1 {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("num: expected 1 arg, got {}", args.len()),
            ));
        }
        return match &args[0] {
            Value::Text(s) => {
                // Trim leading/trailing ASCII whitespace before parsing so CSV
                // cells like " 77516" (space after the comma) round-trip
                // through `num` without silently becoming Err. Internal
                // whitespace ("1 2") still fails the parse.
                let trimmed = s.trim_matches(|c: char| c.is_ascii_whitespace());
                match trimmed.parse::<f64>() {
                    Ok(n) => Ok(Value::Ok(Box::new(Value::Number(n)))),
                    Err(_) => Ok(Value::Err(Box::new(Value::Text(s.clone())))),
                }
            }
            // num is polymorphic: numeric input is identity-wrapped Ok(n).
            // Closes the `num (jpar! body)` pattern where JSON bodies that
            // are bare numbers used to need the str→num roundtrip.
            Value::Number(n) => Ok(Value::Ok(Box::new(Value::Number(*n)))),
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("num requires text or number, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Abs) {
        if args.len() != 1 {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("abs: expected 1 arg, got {}", args.len()),
            ));
        }
        return match &args[0] {
            Value::Number(n) => Ok(Value::Number(n.abs())),
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("abs requires a number, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Mod) && args.len() == 2 {
        return match (&args[0], &args[1]) {
            (Value::Number(a), Value::Number(b)) => {
                if *b == 0.0 {
                    Err(RuntimeError::new("ILO-R003", "modulo by zero".to_string()))
                } else {
                    Ok(Value::Number(a % b))
                }
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "mod requires two numbers".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Fmod) && args.len() == 2 {
        return match (&args[0], &args[1]) {
            (Value::Number(a), Value::Number(b)) => {
                if *b == 0.0 {
                    Err(RuntimeError::new(
                        "ILO-R003",
                        "fmod: modulo by zero".to_string(),
                    ))
                } else {
                    // floor-mod: always non-negative when b > 0.
                    // Equivalent to Python's % and JS Math.floor((a % b + b) % b).
                    // NaN/Inf inputs propagate via f64 % semantics, matching
                    // every other math builtin (`abs`, `sqrt`, `pow`, `/`).
                    let r = a % b;
                    Ok(Value::Number(if r != 0.0 && r.signum() != b.signum() {
                        r + b
                    } else {
                        r
                    }))
                }
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "fmod requires two numbers".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Clamp) && args.len() == 3 {
        return match (&args[0], &args[1], &args[2]) {
            (Value::Number(x), Value::Number(lo), Value::Number(hi)) => Ok(clamp_run(*x, *lo, *hi)),
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "clamp requires three numbers".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Min) && args.len() == 2 {
        return match (&args[0], &args[1]) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a.min(*b))),
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "min requires two numbers".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Max) && args.len() == 2 {
        return match (&args[0], &args[1]) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a.max(*b))),
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "max requires two numbers".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Argmax) && args.len() == 1 {
        return argmax_argmin_run(false, "argmax", &args[0]);
    }
    if builtin == Some(Builtin::Argmin) && args.len() == 1 {
        return argmax_argmin_run(true, "argmin", &args[0]);
    }
    if builtin == Some(Builtin::Argsort) && args.len() == 1 {
        // argsort xs:L n > L n — sorted-index permutation (ascending).
        // Empty list returns empty list. Stable sort (preserves original
        // index order on ties), matching numpy's stable kind default for
        // small inputs.
        let items = match &args[0] {
            Value::List(l) => l,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("argsort: arg must be a list, got {:?}", other),
                ));
            }
        };
        if items.is_empty() {
            return Ok(Value::List(Arc::new(Vec::new())));
        }
        // Validate element types up-front so type errors surface before sort.
        for item in items.iter() {
            if !matches!(item, Value::Number(_)) {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("argsort: list elements must be numbers, got {:?}", item),
                ));
            }
        }
        let mut idxs: Vec<usize> = (0..items.len()).collect();
        idxs.sort_by(|&a, &b| {
            let (Value::Number(x), Value::Number(y)) = (&items[a], &items[b]) else {
                unreachable!()
            };
            x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
        });
        let out: Vec<Value> = idxs.into_iter().map(|i| Value::Number(i as f64)).collect();
        return Ok(Value::List(Arc::new(out)));
    }
    if builtin == Some(Builtin::Bisect) && args.len() == 2 {
        return run_bisect(&args[0], &args[1]);
    }
    // Bitwise ops (ILO-58 MVP). All operate on f64 → u32 (mod 2^32) → f64.
    if matches!(
        builtin,
        Some(
            Builtin::Band
                | Builtin::Bor
                | Builtin::Bxor
                | Builtin::Bnot
                | Builtin::Bshl
                | Builtin::Bshr
                | Builtin::Brot
        )
    ) {
        let b = builtin.unwrap();
        let expected_argc = if b == Builtin::Bnot { 1 } else { 2 };
        if args.len() == expected_argc {
            return run_bitwise(b, &args);
        }
    }
    // 64-bit bitwise ops (ILO-395). Operate on f64 → u64 (mod 2^64) → f64.
    // Values >= 2^53 may lose precision on the f64↔u64 round-trip.
    if matches!(
        builtin,
        Some(
            Builtin::Band64
                | Builtin::Bor64
                | Builtin::Bxor64
                | Builtin::Bnot64
                | Builtin::Bshl64
                | Builtin::Bshr64
                | Builtin::Brot64
        )
    ) {
        let b = builtin.unwrap();
        let expected_argc = if b == Builtin::Bnot64 { 1 } else { 2 };
        if args.len() == expected_argc {
            return run_bitwise_64(b, &args);
        }
    }
    if matches!(builtin, Some(Builtin::Min | Builtin::Max)) && args.len() == 1 {
        // 1-arg list form: returns the min/max element of a list of numbers.
        // Mirrors `avg`/`median` ergonomics so `max [1 2 3]` == 3.
        let items = match &args[0] {
            Value::List(l) => l,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("{name}: arg must be a list, got {:?}", other),
                ));
            }
        };
        if items.is_empty() {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("{name}: cannot take {name} of an empty list"),
            ));
        }
        let is_min = builtin == Some(Builtin::Min);
        // NaN-propagation contract: any NaN element → NaN result. Matches
        // median/stdev/variance and the VM helper `vm_min_max_lst`. Avoids
        // silently mis-comparing NaNs via `f64::min`/`max`, which return the
        // non-NaN argument and would mask NaN inputs entirely.
        let mut best: Option<f64> = None;
        for item in items.iter() {
            match item {
                Value::Number(n) => {
                    if n.is_nan() {
                        return Ok(Value::Number(f64::NAN));
                    }
                    best = Some(match best {
                        None => *n,
                        Some(cur) => {
                            if is_min {
                                cur.min(*n)
                            } else {
                                cur.max(*n)
                            }
                        }
                    });
                }
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("{name}: list elements must be numbers, got {:?}", other),
                    ));
                }
            }
        }
        return Ok(Value::Number(best.unwrap()));
    }
    if matches!(builtin, Some(Builtin::Flr | Builtin::Cel | Builtin::Rou)) && args.len() == 1 {
        return match &args[0] {
            Value::Number(n) => {
                let result = match builtin {
                    Some(Builtin::Flr) => n.floor(),
                    Some(Builtin::Cel) => n.ceil(),
                    _ => n.round(),
                };
                Ok(Value::Number(result))
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("{} requires a number, got {:?}", name, other),
            )),
        };
    }
    if matches!(
        builtin,
        Some(
            Builtin::Sqrt
                | Builtin::Log
                | Builtin::Exp
                | Builtin::Sin
                | Builtin::Cos
                | Builtin::Tan
                | Builtin::Log10
                | Builtin::Log2
                | Builtin::Asin
                | Builtin::Acos
                | Builtin::Atan
        )
    ) && args.len() == 1
    {
        return match &args[0] {
            Value::Number(n) => {
                let result = match builtin {
                    Some(Builtin::Sqrt) => n.sqrt(),
                    Some(Builtin::Log) => n.ln(),
                    Some(Builtin::Exp) => n.exp(),
                    Some(Builtin::Sin) => n.sin(),
                    Some(Builtin::Cos) => n.cos(),
                    Some(Builtin::Tan) => n.tan(),
                    Some(Builtin::Log10) => n.log10(),
                    Some(Builtin::Log2) => n.log2(),
                    Some(Builtin::Asin) => n.asin(),
                    Some(Builtin::Acos) => n.acos(),
                    _ => n.atan(),
                };
                Ok(Value::Number(result))
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("{} requires a number, got {:?}", name, other),
            )),
        };
    }
    if builtin == Some(Builtin::Pow) && args.len() == 2 {
        return match (&args[0], &args[1]) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a.powf(*b))),
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "pow requires two numbers".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Atan2) && args.len() == 2 {
        return match (&args[0], &args[1]) {
            (Value::Number(y), Value::Number(x)) => Ok(Value::Number(y.atan2(*x))),
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "atan2 requires two numbers".to_string(),
            )),
        };
    }
    // Math constants (0.12.1). Zero-arg, no allocation, no error path.
    // Returning the canonical Rust f64 consts keeps cross-engine values
    // bit-identical with the VM / Cranelift bridge (which dispatches here)
    // and with `math.pi` / `math.tau` / `math.e` emitted by the Python
    // backend, all of which agree on the IEEE-754 representation.
    if builtin == Some(Builtin::Pi) && args.is_empty() {
        return Ok(Value::Number(std::f64::consts::PI));
    }
    if builtin == Some(Builtin::Tau) && args.is_empty() {
        return Ok(Value::Number(std::f64::consts::TAU));
    }
    if builtin == Some(Builtin::Eu) && args.is_empty() {
        return Ok(Value::Number(std::f64::consts::E));
    }
    if builtin == Some(Builtin::Now) && args.is_empty() {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        return Ok(Value::Number(ts));
    }
    if builtin == Some(Builtin::NowMs) && args.is_empty() {
        // Unix epoch in milliseconds as f64. Paired with `now` (seconds)
        // for perf-bisection workloads where seconds is too coarse to
        // see a sub-second phase delta. f64 keeps integer precision
        // for ms timestamps well past year 10000.
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as f64;
        return Ok(Value::Number(ms));
    }
    if builtin == Some(Builtin::Sleep) && args.len() == 1 {
        // sleep ms — blocks the current thread for `ms` milliseconds.
        // Returns Nil so it composes as a statement inside loop bodies
        // without polluting the printed result. Negative / NaN / Inf are
        // treated as zero so a stray `sleep -1` cannot hang the engine.
        return match &args[0] {
            Value::Number(ms) => {
                let clamped = if ms.is_finite() && *ms > 0.0 {
                    // Cap at u64::MAX milliseconds; longer than any sane
                    // program will ever need, and avoids the f64→u64 cast
                    // overflowing into undefined behaviour for huge inputs.
                    ms.min(u64::MAX as f64) as u64
                } else {
                    0
                };
                if clamped > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(clamped));
                }
                Ok(Value::Nil)
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("sleep requires a number of milliseconds, got {other:?}"),
            )),
        };
    }
    if builtin == Some(Builtin::Rndn) && args.len() == 2 {
        return match (&args[0], &args[1]) {
            (Value::Number(mu), Value::Number(sigma)) => {
                Ok(Value::Number(crate::rng::normal(*mu, *sigma)))
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "rndn requires two numbers".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Dtfmt) && args.len() == 2 {
        return match (&args[0], &args[1]) {
            (Value::Number(epoch), Value::Text(fmt_str)) => {
                if !epoch.is_finite() {
                    return Ok(Value::Err(Box::new(Value::Text(Arc::new(format!(
                        "dtfmt: epoch is not finite ({epoch})"
                    ))))));
                }
                if *epoch < i64::MIN as f64 || *epoch > i64::MAX as f64 {
                    return Ok(Value::Err(Box::new(Value::Text(Arc::new(format!(
                        "dtfmt: epoch out of range ({epoch})"
                    ))))));
                }
                let secs = *epoch as i64;
                match chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0) {
                    Some(dt) => {
                        let formatted = dt.format(fmt_str.as_str()).to_string();
                        Ok(Value::Ok(Box::new(Value::Text(Arc::new(formatted)))))
                    }
                    None => Ok(Value::Err(Box::new(Value::Text(Arc::new(format!(
                        "dtfmt: timestamp out of range ({secs})"
                    )))))),
                }
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "dtfmt requires a number (epoch) and text (format)".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Dtparse) && args.len() == 2 {
        return match (&args[0], &args[1]) {
            (Value::Text(text), Value::Text(fmt_str)) => {
                let parsed = chrono::NaiveDateTime::parse_from_str(text, fmt_str)
                    .map(|ndt| ndt.and_utc().timestamp() as f64)
                    .or_else(|_| {
                        chrono::NaiveDate::parse_from_str(text, fmt_str)
                            .map(|nd| nd.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp() as f64)
                    });
                match parsed {
                    Ok(n) => Ok(Value::Ok(Box::new(Value::Number(n)))),
                    Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(format!(
                        "dtparse: {e}"
                    )))))),
                }
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "dtparse requires two text args".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::DtparseRel) && args.len() == 2 {
        return match (&args[0], &args[1]) {
            (Value::Text(phrase), Value::Number(now_epoch)) => {
                Ok(dtparse_rel(phrase.as_str(), *now_epoch))
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "dtparse-rel requires text phrase and number epoch".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Rnd) {
        if args.is_empty() {
            return Ok(Value::Number(crate::rng::f64()));
        }
        if args.len() == 2 {
            return match (&args[0], &args[1]) {
                (Value::Number(a), Value::Number(b)) => {
                    let lo = *a as i64;
                    let hi = *b as i64;
                    if lo > hi {
                        return Err(RuntimeError::new(
                            "ILO-R009",
                            format!("rnd: lower bound {} > upper bound {}", lo, hi),
                        ));
                    }
                    Ok(Value::Number(crate::rng::i64_range(lo, hi) as f64))
                }
                _ => Err(RuntimeError::new(
                    "ILO-R009",
                    "rnd requires two numbers".to_string(),
                )),
            };
        }
    }
    if builtin == Some(Builtin::RandBytes) && args.len() == 1 {
        return eval_rand_bytes(&args[0]);
    }
    if builtin == Some(Builtin::Seed) && args.len() == 1 {
        return match &args[0] {
            Value::Number(n) => {
                crate::rng::seed(*n as u64);
                Ok(Value::Nil)
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("seed requires a number, got {other:?}"),
            )),
        };
    }
    if builtin == Some(Builtin::Spl) && args.len() == 2 {
        return match (&args[0], &args[1]) {
            (Value::Text(s), Value::Text(sep)) => {
                let parts: Vec<Value> = s
                    .split(sep.as_str())
                    .map(|p| Value::Text(Arc::new(p.to_string())))
                    .collect();
                Ok(Value::List(Arc::new(parts)))
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "spl requires two text args".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Cat) && args.len() == 2 {
        return match (&args[0], &args[1]) {
            (Value::List(items), Value::Text(sep)) => {
                let mut parts: Vec<String> = Vec::new();
                for item in items.iter() {
                    match item {
                        Value::Text(s) => parts.push((**s).clone()),
                        other => {
                            return Err(RuntimeError::new(
                                "ILO-R009",
                                format!("cat: list items must be text, got {:?}", other),
                            ));
                        }
                    }
                }
                Ok(Value::Text(Arc::new(parts.join(sep.as_str()))))
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "cat requires a list and text separator".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Has) && args.len() == 2 {
        return match &args[0] {
            Value::List(items) => Ok(Value::Bool(items.contains(&args[1]))),
            Value::Text(s) => match &args[1] {
                Value::Text(needle) => Ok(Value::Bool(s.contains(needle.as_str()))),
                other => Err(RuntimeError::new(
                    "ILO-R009",
                    format!("has: text search requires text needle, got {:?}", other),
                )),
            },
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("has requires a list or text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Hd) && args.len() == 1 {
        return match &args[0] {
            Value::List(items) => {
                if items.is_empty() {
                    Err(RuntimeError::new("ILO-R009", "hd: empty list".to_string()))
                } else {
                    Ok(items[0].clone())
                }
            }
            Value::Text(s) => {
                if s.is_empty() {
                    Err(RuntimeError::new("ILO-R009", "hd: empty text".to_string()))
                } else {
                    Ok(Value::Text(Arc::new(s.chars().next().unwrap().to_string())))
                }
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("hd requires a list or text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::At) && args.len() == 2 {
        // Index auto-floors numeric values: `at xs 1.7` is `at xs 1`,
        // `at xs -1.5` floors to `-2` (then resolved against length).
        // Removes the `flr (/ ln 2)` ceremony when indexing with computed
        // floats. Non-numeric still errors.
        let i = match &args[1] {
            Value::Number(n) => n.floor() as i64,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("at: index must be a number, got {:?}", other),
                ));
            }
        };
        return match &args[0] {
            Value::List(items) => {
                let len = items.len() as i64;
                let adjusted = if i < 0 { i + len } else { i };
                if adjusted < 0 || adjusted >= len {
                    Err(RuntimeError::new(
                        "ILO-R009",
                        format!(
                            "at: index {i} out of range for list of length {}",
                            items.len()
                        ),
                    ))
                } else {
                    Ok(items[adjusted as usize].clone())
                }
            }
            Value::Text(s) => match char_at_signed(s, i) {
                CharAtResult::Found(c) => Ok(Value::Text(Arc::new(c.to_string()))),
                CharAtResult::OutOfRange { len } => Err(RuntimeError::new(
                    "ILO-R009",
                    format!("at: index {i} out of range for text of length {len}"),
                )),
            },
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("at requires a list or text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::LgetOr) && args.len() == 3 {
        // lget-or xs i default — element at index, else default. Floors the
        // index like `at`, applies the same negative-index resolution, but
        // OOB returns default instead of erroring. Verifier enforces
        // `default:a` matches the list element type.
        let i = match &args[1] {
            Value::Number(n) => n.floor() as i64,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("lget-or: index must be a number, got {:?}", other),
                ));
            }
        };
        return match &args[0] {
            Value::List(items) => {
                let len = items.len() as i64;
                let adjusted = if i < 0 { i + len } else { i };
                if adjusted < 0 || adjusted >= len {
                    Ok(args[2].clone())
                } else {
                    Ok(items[adjusted as usize].clone())
                }
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "lget-or: expects a list".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::DefaultOnErr) && args.len() == 2 {
        // default-on-err r d — unwrap R T E to T, returning d on Err.
        // Mirror of `??` for Result. Pure: no I/O, no FnRef.
        return match &args[0] {
            Value::Ok(inner) => Ok(*inner.clone()),
            Value::Err(_) => Ok(args[1].clone()),
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!(
                    "default-on-err: first argument must be R T E (Ok or Err), got {:?}",
                    other
                ),
            )),
        };
    }
    if builtin == Some(Builtin::Urlenc) && args.len() == 1 {
        return urlenc_impl(&args[0]);
    }
    if builtin == Some(Builtin::Urldec) && args.len() == 1 {
        return urldec_impl(&args[0]);
    }
    if builtin == Some(Builtin::Idxof) && args.len() == 2 {
        return idxof_impl(&args[0], &args[1]);
    }
    if builtin == Some(Builtin::B64u) && args.len() == 1 {
        return b64u_impl(&args[0]);
    }
    if builtin == Some(Builtin::B64uDec) && args.len() == 1 {
        return b64u_dec_impl(&args[0]);
    }
    if builtin == Some(Builtin::Sha256) && args.len() == 1 {
        return sha256_impl(&args[0]);
    }
    if builtin == Some(Builtin::HmacSha256) && args.len() == 2 {
        return hmac_sha256_impl(&args[0], &args[1]);
    }
    if builtin == Some(Builtin::B64) && args.len() == 1 {
        return b64_impl(&args[0]);
    }
    if builtin == Some(Builtin::B64Dec) && args.len() == 1 {
        return b64_dec_impl(&args[0]);
    }
    if builtin == Some(Builtin::HexEnc) && args.len() == 1 {
        return hex_impl(&args[0]);
    }
    if builtin == Some(Builtin::CtEq) && args.len() == 2 {
        return ct_eq_impl(&args[0], &args[1]);
    }
    if builtin == Some(Builtin::Sha256Hex) && args.len() == 1 {
        return sha256_hex_impl(&args[0]);
    }
    if builtin == Some(Builtin::Sha256d) && args.len() == 1 {
        return sha256d_impl(&args[0]);
    }
    if builtin == Some(Builtin::HexRev) && args.len() == 1 {
        return hex_rev_impl(&args[0]);
    }
    if builtin == Some(Builtin::Tokcount) && args.len() == 1 {
        return tokcount_impl(&args[0]);
    }
    if builtin == Some(Builtin::Lst) && args.len() == 3 {
        let idx = match &args[1] {
            Value::Number(n) => {
                if *n < 0.0 || n.fract() != 0.0 {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        "lst: index must be a non-negative integer".to_string(),
                    ));
                }
                *n as usize
            }
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("lst: index must be a number, got {:?}", other),
                ));
            }
        };
        return match &args[0] {
            Value::List(items) => {
                if idx >= items.len() {
                    Err(RuntimeError::new(
                        "ILO-R009",
                        format!(
                            "lst: index {idx} out of range for list of length {}",
                            items.len()
                        ),
                    ))
                } else {
                    let mut new_items = (**items).clone();
                    new_items[idx] = args[2].clone();
                    Ok(Value::List(Arc::new(new_items)))
                }
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("lst requires a list, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Window) && args.len() == 2 {
        let n = match &args[0] {
            Value::Number(v) => {
                if !v.is_finite() || *v <= 0.0 || v.fract() != 0.0 {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("window: size must be a positive integer, got {}", v),
                    ));
                }
                *v as usize
            }
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("window: size must be a number, got {:?}", other),
                ));
            }
        };
        let xs = match &args[1] {
            Value::List(items) => items,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("window arg 2 requires a list, got {:?}", other),
                ));
            }
        };
        if n > xs.len() {
            return Ok(Value::List(Arc::new(vec![])));
        }
        let mut out = Vec::with_capacity(xs.len() - n + 1);
        for w in xs.windows(n) {
            out.push(Value::List(Arc::new(w.to_vec())));
        }
        return Ok(Value::List(Arc::new(out)));
    }
    if builtin == Some(Builtin::Zip) && args.len() == 2 {
        let xs = match &args[0] {
            Value::List(items) => items,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("zip arg 1 requires a list, got {:?}", other),
                ));
            }
        };
        let ys = match &args[1] {
            Value::List(items) => items,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("zip arg 2 requires a list, got {:?}", other),
                ));
            }
        };
        let n = xs.len().min(ys.len());
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            out.push(Value::List(Arc::new(vec![xs[i].clone(), ys[i].clone()])));
        }
        return Ok(Value::List(Arc::new(out)));
    }
    if builtin == Some(Builtin::Enumerate) && args.len() == 1 {
        let xs = match &args[0] {
            Value::List(items) => items,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("enumerate requires a list, got {:?}", other),
                ));
            }
        };
        let mut out = Vec::with_capacity(xs.len());
        for (i, v) in xs.iter().enumerate() {
            out.push(Value::List(Arc::new(vec![
                Value::Number(i as f64),
                v.clone(),
            ])));
        }
        return Ok(Value::List(Arc::new(out)));
    }
    if builtin == Some(Builtin::Range) && args.len() == 2 {
        return match (&args[0], &args[1]) {
            (Value::Number(a), Value::Number(b)) => {
                // Reject non-integer bounds rather than silently truncating
                // (e.g. `range 1.9 4.9` previously yielded `[1,2,3]`).
                if a.fract() != 0.0 || b.fract() != 0.0 {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        "range: bounds must be integers".to_string(),
                    ));
                }
                let start = *a as i64;
                let end = *b as i64;
                if start >= end {
                    return Ok(Value::List(Arc::new(Vec::new())));
                }
                let len = (end - start) as u64;
                if len > 1_000_000 {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("range too large: {len} elements (max 1000000)"),
                    ));
                }
                let mut out = Vec::with_capacity(len as usize);
                for i in start..end {
                    out.push(Value::Number(i as f64));
                }
                Ok(Value::List(Arc::new(out)))
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "range requires two numbers".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Linspace) && args.len() == 3 {
        // linspace a b n — n evenly-spaced floats from a to b inclusive
        // (numpy endpoint=True). n=0 returns []; n=1 returns [a]; n>=2 includes
        // both endpoints; equal endpoints repeat the value.
        let (a, b, n_raw) = match (&args[0], &args[1], &args[2]) {
            (Value::Number(a), Value::Number(b), Value::Number(n)) => (*a, *b, *n),
            _ => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    "linspace requires three numbers (a b n)".to_string(),
                ));
            }
        };
        if n_raw.fract() != 0.0 || n_raw < 0.0 {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("linspace: n must be a non-negative integer, got {n_raw}"),
            ));
        }
        let n = n_raw as u64;
        if n > 1_000_000 {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("linspace too large: {n} elements (max 1000000)"),
            ));
        }
        if n == 0 {
            return Ok(Value::List(Arc::new(Vec::new())));
        }
        if n == 1 {
            return Ok(Value::List(Arc::new(vec![Value::Number(a)])));
        }
        let mut out = Vec::with_capacity(n as usize);
        let step = (b - a) / ((n - 1) as f64);
        for i in 0..n {
            out.push(Value::Number(a + step * (i as f64)));
        }
        // Pin the final element exactly to b to avoid float-accumulated drift.
        if let Some(last) = out.last_mut() {
            *last = Value::Number(b);
        }
        return Ok(Value::List(Arc::new(out)));
    }
    if builtin == Some(Builtin::Ones) && args.len() == 1 {
        let n_raw = match &args[0] {
            Value::Number(n) => *n,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("ones: count must be a number, got {:?}", other),
                ));
            }
        };
        if n_raw.fract() != 0.0 || n_raw < 0.0 {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("ones: count must be a non-negative integer, got {n_raw}"),
            ));
        }
        let n = n_raw as u64;
        if n > 1_000_000 {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("ones too large: {n} elements (max 1000000)"),
            ));
        }
        let out = vec![Value::Number(1.0); n as usize];
        return Ok(Value::List(Arc::new(out)));
    }
    if builtin == Some(Builtin::Rep) && args.len() == 2 {
        let n_raw = match &args[0] {
            Value::Number(n) => *n,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("rep: count must be a number, got {:?}", other),
                ));
            }
        };
        if n_raw.fract() != 0.0 || n_raw < 0.0 {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("rep: count must be a non-negative integer, got {n_raw}"),
            ));
        }
        let n = n_raw as u64;
        if n > 1_000_000 {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("rep too large: {n} elements (max 1000000)"),
            ));
        }
        let v = &args[1];
        let out = vec![v.clone(); n as usize];
        return Ok(Value::List(Arc::new(out)));
    }
    // ----- zeros -----
    if builtin == Some(Builtin::Zeros) && args.len() == 1 {
        let n_raw = match &args[0] {
            Value::Number(n) => *n,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("zeros: count must be a number, got {:?}", other),
                ));
            }
        };
        if n_raw.fract() != 0.0 || n_raw < 0.0 {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("zeros: count must be a non-negative integer, got {n_raw}"),
            ));
        }
        let n = n_raw as u64;
        if n > 1_000_000 {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("zeros too large: {n} elements (max 1000000)"),
            ));
        }
        let out = vec![Value::Number(0.0); n as usize];
        return Ok(Value::List(Arc::new(out)));
    }
    // ----- arange -----
    if builtin == Some(Builtin::Arange) && args.len() == 3 {
        let (start, stop, step) = match (&args[0], &args[1], &args[2]) {
            (Value::Number(a), Value::Number(b), Value::Number(s)) => (*a, *b, *s),
            _ => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    "arange requires three numbers (start stop step)".to_string(),
                ));
            }
        };
        if step <= 0.0 {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("arange: step must be positive, got {step}"),
            ));
        }
        if start >= stop {
            return Ok(Value::List(Arc::new(Vec::new())));
        }
        let n = ((stop - start) / step).ceil() as u64;
        if n > 1_000_000 {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("arange too large: {n} elements (max 1000000)"),
            ));
        }
        let mut out = Vec::with_capacity(n as usize);
        let mut i = 0u64;
        loop {
            let v = start + step * (i as f64);
            if v >= stop {
                break;
            }
            out.push(Value::Number(v));
            i += 1;
            if i > 1_000_000 {
                break;
            }
        }
        return Ok(Value::List(Arc::new(out)));
    }
    // ----- vstack -----
    // vstack matrices:L > L — vertical concatenation of a list of row-lists.
    // Equivalent to flat on a list of matrices: [[r0,r1],[r2,r3]] → [r0,r1,r2,r3].
    #[inline(never)]
    fn vstack_run(matrices_val: &Value) -> Result<Value> {
        let matrices = match matrices_val {
            Value::List(xs) => xs.clone(),
            _ => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    "vstack: argument must be a list of matrices".to_string(),
                ));
            }
        };
        let mut out: Vec<Value> = Vec::new();
        for item in matrices.iter() {
            match item {
                Value::List(rows) => {
                    for row in rows.iter() {
                        out.push(row.clone());
                    }
                }
                _ => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        "vstack: each element must be a list (matrix or vector)".to_string(),
                    ));
                }
            }
        }
        Ok(Value::List(Arc::new(out)))
    }
    if builtin == Some(Builtin::Vstack) && args.len() == 1 {
        return vstack_run(&args[0]);
    }
    // ----- hstack -----
    // hstack matrices:L > L — horizontal concatenation: cat corresponding rows.
    // All matrices must have the same number of rows; each output row is the
    // concatenation of the corresponding input rows.
    #[inline(never)]
    fn hstack_run(matrices_val: &Value) -> Result<Value> {
        let matrices = match matrices_val {
            Value::List(xs) => xs.clone(),
            _ => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    "hstack: argument must be a list of matrices".to_string(),
                ));
            }
        };
        if matrices.is_empty() {
            return Ok(Value::List(Arc::new(Vec::new())));
        }
        // Collect as Vec<Vec<&Value>> — each element is a list of rows.
        let mut mats: Vec<Arc<Vec<Value>>> = Vec::with_capacity(matrices.len());
        for item in matrices.iter() {
            match item {
                Value::List(rows) => mats.push(rows.clone()),
                _ => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        "hstack: each element must be a list (matrix)".to_string(),
                    ));
                }
            }
        }
        let n_rows = mats[0].len();
        for m in &mats {
            if m.len() != n_rows {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "hstack: all matrices must have the same number of rows; expected {n_rows}, got {}",
                        m.len()
                    ),
                ));
            }
        }
        let mut out = Vec::with_capacity(n_rows);
        for r in 0..n_rows {
            let mut row: Vec<Value> = Vec::new();
            for m in &mats {
                match &m[r] {
                    Value::List(cols) => {
                        for col in cols.iter() {
                            row.push(col.clone());
                        }
                    }
                    other => row.push(other.clone()),
                }
            }
            out.push(Value::List(Arc::new(row)));
        }
        Ok(Value::List(Arc::new(out)))
    }
    if builtin == Some(Builtin::Hstack) && args.len() == 1 {
        return hstack_run(&args[0]);
    }
    // ----- column-stack -----
    // column-stack vecs:L > L — treat each vector (1-d list) as a column,
    // return a 2-d matrix (list of rows). All vectors must have the same length.
    #[inline(never)]
    fn column_stack_run(vecs_val: &Value) -> Result<Value> {
        let vecs = match vecs_val {
            Value::List(xs) => xs.clone(),
            _ => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    "column-stack: argument must be a list of vectors".to_string(),
                ));
            }
        };
        if vecs.is_empty() {
            return Ok(Value::List(Arc::new(Vec::new())));
        }
        let mut cols: Vec<Arc<Vec<Value>>> = Vec::with_capacity(vecs.len());
        for item in vecs.iter() {
            match item {
                Value::List(col) => cols.push(col.clone()),
                _ => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        "column-stack: each element must be a list (vector)".to_string(),
                    ));
                }
            }
        }
        let n_rows = cols[0].len();
        for col in &cols {
            if col.len() != n_rows {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "column-stack: all vectors must have the same length; expected {n_rows}, got {}",
                        col.len()
                    ),
                ));
            }
        }
        let mut out = Vec::with_capacity(n_rows);
        for r in 0..n_rows {
            let row: Vec<Value> = cols.iter().map(|col| col[r].clone()).collect();
            out.push(Value::List(Arc::new(row)));
        }
        Ok(Value::List(Arc::new(out)))
    }
    if builtin == Some(Builtin::ColumnStack) && args.len() == 1 {
        return column_stack_run(&args[0]);
    }
    // ----- hist -----
    // hist xs:L n_bins:n > L n — fixed-width histogram.
    // Returns a list of n_bins integer counts for equal-width bins over [min,max].
    #[inline(never)]
    fn hist_run(xs_val: &Value, n_bins_val: &Value) -> Result<Value> {
        let xs = match xs_val {
            Value::List(xs) => xs.clone(),
            _ => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    "hist: first argument must be a numeric list".to_string(),
                ));
            }
        };
        let n_bins_raw = match n_bins_val {
            Value::Number(n) => *n,
            _ => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    "hist: n_bins must be a number".to_string(),
                ));
            }
        };
        if n_bins_raw.fract() != 0.0 || n_bins_raw <= 0.0 {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("hist: n_bins must be a positive integer, got {n_bins_raw}"),
            ));
        }
        let n_bins = n_bins_raw as usize;
        if n_bins > 1_000_000 {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("hist: n_bins too large: {n_bins} (max 1000000)"),
            ));
        }
        let mut counts = vec![Value::Number(0.0); n_bins];
        if xs.is_empty() {
            return Ok(Value::List(Arc::new(counts)));
        }
        let mut vals: Vec<f64> = Vec::with_capacity(xs.len());
        for v in xs.iter() {
            match v {
                Value::Number(n) => vals.push(*n),
                _ => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        "hist: list elements must all be numbers".to_string(),
                    ));
                }
            }
        }
        let mn = vals.iter().cloned().fold(f64::INFINITY, f64::min);
        let mx = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let range = mx - mn;
        for &v in &vals {
            let bin = if range == 0.0 {
                0
            } else {
                let b = ((v - mn) / range * (n_bins as f64)).floor() as usize;
                b.min(n_bins - 1)
            };
            if let Value::Number(ref mut c) = counts[bin] {
                *c += 1.0;
            }
        }
        Ok(Value::List(Arc::new(counts)))
    }
    if builtin == Some(Builtin::Hist) && args.len() == 2 {
        return hist_run(&args[0], &args[1]);
    }
    if builtin == Some(Builtin::Chunks) && args.len() == 2 {
        let n_raw = match &args[0] {
            Value::Number(n) => *n,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("chunks: size must be a number, got {:?}", other),
                ));
            }
        };
        return chunks_run(n_raw, &args[1]);
    }
    if builtin == Some(Builtin::Setunion) && args.len() == 2 {
        return setops_run(builtin, &args[0], &args[1]);
    }
    if builtin == Some(Builtin::Setinter) && args.len() == 2 {
        return setops_run(builtin, &args[0], &args[1]);
    }
    if builtin == Some(Builtin::Setdiff) && args.len() == 2 {
        return setops_run(builtin, &args[0], &args[1]);
    }
    if builtin == Some(Builtin::Tl) && args.len() == 1 {
        return match &args[0] {
            Value::List(items) => {
                if items.is_empty() {
                    Err(RuntimeError::new("ILO-R009", "tl: empty list".to_string()))
                } else {
                    Ok(Value::List(Arc::new(items[1..].to_vec())))
                }
            }
            Value::Text(s) => {
                if s.is_empty() {
                    Err(RuntimeError::new("ILO-R009", "tl: empty text".to_string()))
                } else {
                    let mut chars = s.chars();
                    chars.next();
                    Ok(Value::Text(Arc::new(chars.collect())))
                }
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("tl requires a list or text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Rev) && args.len() == 1 {
        // RC=1 fast path: consume args by value so we own the Arc; Arc::make_mut
        // mutates the underlying Vec/String in place when the Arc is the sole
        // reference, else clones once. Mirrors the mset/mdel pattern landed in
        // PR #249. The clone-on-shared branch keeps the same observable
        // behaviour (callers still see a fresh List/Text), while the sole-owner
        // branch skips the buffer allocation entirely.
        let mut it = args.into_iter();
        let v = it.next().unwrap();
        return match v {
            Value::List(mut items) => {
                let inner = Arc::make_mut(&mut items);
                inner.reverse();
                Ok(Value::List(items))
            }
            Value::Text(mut s) => {
                let inner = Arc::make_mut(&mut s);
                let reversed: String = inner.chars().rev().collect();
                *inner = reversed;
                Ok(Value::Text(s))
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("rev requires a list or text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Srt) && args.len() == 1 {
        // RC=1 fast path: see Rev for rationale. When the input Arc is uniquely
        // owned, Arc::make_mut hands back &mut Vec<Value> and we sort in place;
        // when shared, it clones once and we sort the clone. Same observable
        // behaviour either way.
        let mut it = args.into_iter();
        let v = it.next().unwrap();
        return match v {
            Value::List(mut items) => {
                if items.is_empty() {
                    return Ok(Value::List(items));
                }
                let all_numbers = items.iter().all(|v| matches!(v, Value::Number(_)));
                let all_text = items.iter().all(|v| matches!(v, Value::Text(_)));
                if all_numbers {
                    let inner = Arc::make_mut(&mut items);
                    inner.sort_by(|a, b| {
                        if let (Value::Number(x), Value::Number(y)) = (a, b) {
                            x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
                        } else {
                            unreachable!()
                        }
                    });
                    Ok(Value::List(items))
                } else if all_text {
                    let inner = Arc::make_mut(&mut items);
                    inner.sort_by(|a, b| {
                        if let (Value::Text(x), Value::Text(y)) = (a, b) {
                            x.cmp(y)
                        } else {
                            unreachable!()
                        }
                    });
                    Ok(Value::List(items))
                } else {
                    Err(RuntimeError::new(
                        "ILO-R009",
                        "srt: list must contain all numbers or all text".to_string(),
                    ))
                }
            }
            Value::Text(mut s) => {
                let inner = Arc::make_mut(&mut s);
                let mut chars: Vec<char> = inner.chars().collect();
                chars.sort();
                *inner = chars.into_iter().collect();
                Ok(Value::Text(s))
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("srt requires a list or text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Srt) && (args.len() == 2 || args.len() == 3) {
        let fn_name = resolve_fn_ref(&args[0]).ok_or_else(|| {
            RuntimeError::new(
                "ILO-R009",
                format!(
                    "srt: key arg must be a function reference, got {:?}",
                    args[0]
                ),
            )
        })?;
        let captures = closure_captures(&args[0]);
        // closure-bind: srt fn ctx xs
        let (ctx, list_arg) = if args.len() == 3 {
            (Some(args[1].clone()), &args[2])
        } else {
            (None, &args[1])
        };
        let items = match list_arg {
            Value::List(l) => l.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("srt: list arg must be a list, got {:?}", other),
                ));
            }
        };
        // Compute keys for each item, then sort by key.
        // `items: Arc<Vec<Value>>` — unwrap if refcount=1, else clone the Vec.
        let owned_items: Vec<Value> = Arc::try_unwrap(items).unwrap_or_else(|arc| (*arc).clone());
        let mut keyed: Vec<(Value, Value)> = owned_items
            .into_iter()
            .map(|item| {
                let mut call_args = match &ctx {
                    Some(c) => vec![item.clone(), c.clone()],
                    None => vec![item.clone()],
                };
                call_args.extend(captures.iter().cloned());
                let key = call_function(env, &fn_name, call_args)?;
                Ok((key, item))
            })
            .collect::<Result<_>>()?;
        keyed.sort_by(|(ka, _), (kb, _)| match (ka, kb) {
            (Value::Number(a), Value::Number(b)) => {
                a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
            }
            (Value::Text(a), Value::Text(b)) => a.cmp(b),
            _ => std::cmp::Ordering::Equal,
        });
        return Ok(Value::List(Arc::new(
            keyed.into_iter().map(|(_, v)| v).collect(),
        )));
    }
    if builtin == Some(Builtin::Rsrt) && args.len() == 1 {
        return match &args[0] {
            Value::List(items) => {
                if items.is_empty() {
                    return Ok(Value::List(Arc::new(vec![])));
                }
                let all_numbers = items.iter().all(|v| matches!(v, Value::Number(_)));
                let all_text = items.iter().all(|v| matches!(v, Value::Text(_)));
                if all_numbers {
                    let mut sorted: Vec<Value> = (**items).clone();
                    sorted.sort_by(|a, b| {
                        if let (Value::Number(x), Value::Number(y)) = (a, b) {
                            y.partial_cmp(x).unwrap_or(std::cmp::Ordering::Equal)
                        } else {
                            unreachable!()
                        }
                    });
                    Ok(Value::List(Arc::new(sorted)))
                } else if all_text {
                    let mut sorted: Vec<Value> = (**items).clone();
                    sorted.sort_by(|a, b| {
                        if let (Value::Text(x), Value::Text(y)) = (a, b) {
                            y.cmp(x)
                        } else {
                            unreachable!()
                        }
                    });
                    Ok(Value::List(Arc::new(sorted)))
                } else {
                    Err(RuntimeError::new(
                        "ILO-R009",
                        "rsrt: list must contain all numbers or all text".to_string(),
                    ))
                }
            }
            Value::Text(s) => {
                let mut chars: Vec<char> = s.chars().collect();
                chars.sort_by(|a, b| b.cmp(a));
                Ok(Value::Text(Arc::new(chars.into_iter().collect())))
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("rsrt requires a list or text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Rsrt) && (args.len() == 2 || args.len() == 3) {
        // rsrt fn xs / rsrt fn ctx xs — descending sort by key function.
        // Mirrors the srt 2/3-arg path: compute keys for each element, then
        // sort by key using the REVERSED comparator (b.cmp(a) for text,
        // b.partial_cmp(a) for numbers). Element type is preserved.
        let fn_name = resolve_fn_ref(&args[0]).ok_or_else(|| {
            RuntimeError::new(
                "ILO-R009",
                format!(
                    "rsrt: key arg must be a function reference, got {:?}",
                    args[0]
                ),
            )
        })?;
        let captures = closure_captures(&args[0]);
        let (ctx, list_arg) = if args.len() == 3 {
            (Some(args[1].clone()), &args[2])
        } else {
            (None, &args[1])
        };
        let items = match list_arg {
            Value::List(l) => l.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("rsrt: list arg must be a list, got {:?}", other),
                ));
            }
        };
        let owned_items: Vec<Value> = Arc::try_unwrap(items).unwrap_or_else(|arc| (*arc).clone());
        let mut keyed: Vec<(Value, Value)> = owned_items
            .into_iter()
            .map(|item| {
                let mut call_args = match &ctx {
                    Some(c) => vec![item.clone(), c.clone()],
                    None => vec![item.clone()],
                };
                call_args.extend(captures.iter().cloned());
                let key = call_function(env, &fn_name, call_args)?;
                Ok((key, item))
            })
            .collect::<Result<_>>()?;
        keyed.sort_by(|(ka, _), (kb, _)| match (ka, kb) {
            (Value::Number(a), Value::Number(b)) => {
                b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal)
            }
            (Value::Text(a), Value::Text(b)) => b.cmp(a),
            _ => std::cmp::Ordering::Equal,
        });
        return Ok(Value::List(Arc::new(
            keyed.into_iter().map(|(_, v)| v).collect(),
        )));
    }
    if builtin == Some(Builtin::Slc) && args.len() == 3 {
        // Bounds accept negative integers Python-style, with one ergonomic
        // exception on the end bound:
        //   `slc xs -1 (len xs)` returns the last element
        //   `slc xs -2 (len xs)` returns the last two elements
        //   `slc xs -3 -1`       returns the penultimate window (Python-style)
        //   `slc xs 0 -1`        returns the WHOLE list (-1 = "to end" sugar
        //                        when start is non-negative; see
        //                        `resolve_slc_end` in builtins.rs)
        let start_raw = match &args[1] {
            Value::Number(n) => {
                if n.fract() != 0.0 {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        "slc: start index must be an integer".to_string(),
                    ));
                }
                *n as i64
            }
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("slc: start index must be a number, got {:?}", other),
                ));
            }
        };
        let end_raw = match &args[2] {
            Value::Number(n) => {
                if n.fract() != 0.0 {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        "slc: end index must be an integer".to_string(),
                    ));
                }
                *n as i64
            }
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("slc: end index must be a number, got {:?}", other),
                ));
            }
        };
        return match &args[0] {
            Value::List(items) => {
                let len = items.len();
                let end = crate::builtins::resolve_slc_end(start_raw, end_raw, len);
                let start = crate::builtins::resolve_slice_bound(start_raw, len).min(end);
                Ok(Value::List(Arc::new(items[start..end].to_vec())))
            }
            Value::Text(s) => {
                let chars: Vec<char> = s.chars().collect();
                let len = chars.len();
                let end = crate::builtins::resolve_slc_end(start_raw, end_raw, len);
                let start = crate::builtins::resolve_slice_bound(start_raw, len).min(end);
                Ok(Value::Text(Arc::new(chars[start..end].iter().collect())))
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("slc requires a list or text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Take) && args.len() == 2 {
        // Negative `n` means "all but the last |n|" (Python `xs[:n]`):
        // `take -1 [1,2,3]` returns `[1,2]`.
        let n = match &args[0] {
            Value::Number(n) => {
                if n.fract() != 0.0 {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        "take: count must be an integer".to_string(),
                    ));
                }
                *n as i64
            }
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("take: count must be a number, got {:?}", other),
                ));
            }
        };
        return match &args[1] {
            Value::List(items) => {
                let end = crate::builtins::resolve_take_count(n, items.len());
                Ok(Value::List(Arc::new(items[..end].to_vec())))
            }
            Value::Text(s) => {
                let chars: Vec<char> = s.chars().collect();
                let end = crate::builtins::resolve_take_count(n, chars.len());
                Ok(Value::Text(Arc::new(chars[..end].iter().collect())))
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("take requires a list or text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Drop) && args.len() == 2 {
        // Negative `n` means "keep only the last |n|" (Python `xs[n:]`):
        // `drop -1 [1,2,3]` returns `[3]`.
        let n = match &args[0] {
            Value::Number(n) => {
                if n.fract() != 0.0 {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        "drop: count must be an integer".to_string(),
                    ));
                }
                *n as i64
            }
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("drop: count must be a number, got {:?}", other),
                ));
            }
        };
        return match &args[1] {
            Value::List(items) => {
                let start = crate::builtins::resolve_drop_count(n, items.len());
                Ok(Value::List(Arc::new(items[start..].to_vec())))
            }
            Value::Text(s) => {
                let chars: Vec<char> = s.chars().collect();
                let start = crate::builtins::resolve_drop_count(n, chars.len());
                Ok(Value::Text(Arc::new(chars[start..].iter().collect())))
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("drop requires a list or text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Get) && (args.len() == 1 || args.len() == 2) {
        let url = match &args[0] {
            Value::Text(u) => u.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("get requires text (url), got {:?}", other),
                ));
            }
        };
        if let Err(msg) = env.caps.check_net(url.as_str()) {
            return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
        }
        let headers = if args.len() == 2 {
            match &args[1] {
                Value::Map(m) => m
                    .iter()
                    .map(|(k, v)| {
                        let vs: String = match v {
                            Value::Text(s) => (**s).clone(),
                            other => format!("{other:?}"),
                        };
                        (k.to_display_string(), vs)
                    })
                    .collect::<Vec<_>>(),
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("get headers must be M t t, got {:?}", other),
                    ));
                }
            }
        } else {
            vec![]
        };
        return {
            // On WASM, always route through the fetch host import regardless of
            // the `http` feature flag (minreq does not compile for wasm32).
            // On native builds, use minreq when `http` is enabled.
            let backend = http_wasm::default_backend();
            let result = backend.get(url.as_str(), &headers);
            Ok(http_wasm::result_to_value(result))
        };
    }
    if builtin == Some(Builtin::GetMany) && args.len() == 1 {
        let urls: Vec<String> = match &args[0] {
            Value::List(items) => {
                let mut out = Vec::with_capacity(items.len());
                for (i, v) in items.iter().enumerate() {
                    match v {
                        Value::Text(s) => out.push((**s).clone()),
                        other => {
                            return Err(RuntimeError::new(
                                "ILO-R009",
                                format!(
                                    "get-many requires L t (list of urls); element {i} is {:?}",
                                    other
                                ),
                            ));
                        }
                    }
                }
                out
            }
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("get-many requires L t (list of urls), got {:?}", other),
                ));
            }
        };
        // Cap check: verify each URL before issuing any requests.
        for url in &urls {
            if let Err(msg) = env.caps.check_net(url) {
                // Return the first blocked URL as a single Err in the list's envelope.
                return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
            }
        }
        return Ok(Value::List(Arc::new(get_many_fetch(&urls))));
    }
    if builtin == Some(Builtin::Post) && (args.len() == 2 || args.len() == 3) {
        let (url, body) = match (&args[0], &args[1]) {
            (Value::Text(u), Value::Text(b)) => (u.clone(), b.clone()),
            _ => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("pst requires (t, t), got ({:?}, {:?})", args[0], args[1]),
                ));
            }
        };
        if let Err(msg) = env.caps.check_net(url.as_str()) {
            return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
        }
        let headers = if args.len() == 3 {
            match &args[2] {
                Value::Map(m) => m
                    .iter()
                    .map(|(k, v)| {
                        let vs: String = match v {
                            Value::Text(s) => (**s).clone(),
                            other => format!("{other:?}"),
                        };
                        (k.to_display_string(), vs)
                    })
                    .collect::<Vec<_>>(),
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("pst headers must be M t t, got {:?}", other),
                    ));
                }
            }
        } else {
            vec![]
        };
        return {
            // Same backend selection as `get`: WASM → WasmFetchBackend,
            // native + `http` feature → NativeHttpBackend, otherwise stub.
            let backend = http_wasm::default_backend();
            let result = backend.post(url.as_str(), body.as_str(), &headers);
            Ok(http_wasm::result_to_value(result))
        };
    }
    // HTTP verb cluster (#5z). Delegated to #[inline(never)] helpers so the
    // call_function frame stays small. Each verb gets an explicit `if builtin ==`
    // guard so the check-dispatch-arms script can measure them individually.
    if builtin == Some(Builtin::Put) && (args.len() == 2 || args.len() == 3) {
        return put_pat_run(env, builtin, args);
    }
    if builtin == Some(Builtin::Pat) && (args.len() == 2 || args.len() == 3) {
        return put_pat_run(env, builtin, args);
    }
    if builtin == Some(Builtin::Del) && (args.len() == 1 || args.len() == 2) {
        return del_hed_opt_run(env, builtin, args);
    }
    if builtin == Some(Builtin::Hed) && (args.len() == 1 || args.len() == 2) {
        return del_hed_opt_run(env, builtin, args);
    }
    if builtin == Some(Builtin::Opt) && (args.len() == 1 || args.len() == 2) {
        return del_hed_opt_run(env, builtin, args);
    }
    if builtin == Some(Builtin::GetTo) && args.len() == 2 {
        let url = match &args[0] {
            Value::Text(u) => u.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("get-to requires text (url), got {:?}", other),
                ));
            }
        };
        let timeout_ms = match &args[1] {
            Value::Number(n) => *n,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("get-to requires n (timeout-ms), got {:?}", other),
                ));
            }
        };
        // minreq takes whole seconds; round up from milliseconds
        let timeout_secs = ((timeout_ms / 1000.0).ceil() as u64).max(1);
        return {
            #[cfg(feature = "http")]
            {
                let req = minreq::get(url.as_str()).with_timeout(timeout_secs);
                match req.send() {
                    Ok(resp) => match resp.as_str() {
                        Ok(body) => {
                            Ok(Value::Ok(Box::new(Value::Text(Arc::new(body.to_string())))))
                        }
                        Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(format!(
                            "response is not valid UTF-8: {e}"
                        )))))),
                    },
                    Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
                }
            }
            #[cfg(not(feature = "http"))]
            {
                let _ = (url, timeout_secs);
                Ok(Value::Err(Box::new(Value::Text(
                    "http feature not enabled".to_string().into(),
                ))))
            }
        };
    }
    if builtin == Some(Builtin::PstTo) && args.len() == 3 {
        let (url, body) = match (&args[0], &args[1]) {
            (Value::Text(u), Value::Text(b)) => (u.clone(), b.clone()),
            _ => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "pst-to requires (t, t, n), got ({:?}, {:?}, {:?})",
                        args[0], args[1], args[2]
                    ),
                ));
            }
        };
        let timeout_ms = match &args[2] {
            Value::Number(n) => *n,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("pst-to requires n (timeout-ms), got {:?}", other),
                ));
            }
        };
        let timeout_secs = ((timeout_ms / 1000.0).ceil() as u64).max(1);
        return {
            #[cfg(feature = "http")]
            {
                let req = minreq::post(url.as_str())
                    .with_body(body.as_str())
                    .with_timeout(timeout_secs);
                match req.send() {
                    Ok(resp) => match resp.as_str() {
                        Ok(b) => Ok(Value::Ok(Box::new(Value::Text(Arc::new(b.to_string()))))),
                        Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(format!(
                            "response is not valid UTF-8: {e}"
                        )))))),
                    },
                    Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
                }
            }
            #[cfg(not(feature = "http"))]
            {
                let _ = (url, body, timeout_secs);
                Ok(Value::Err(Box::new(Value::Text(
                    "http feature not enabled".to_string().into(),
                ))))
            }
        };
    }
    if builtin == Some(Builtin::Getx) && (args.len() == 1 || args.len() == 2) {
        // getx url           — 1-arg, returns R (M t _) t
        // getx url headers   — 2-arg, headers is M t t
        // Ok-map keys: status (n), headers (M t t), body (t).
        let url = match &args[0] {
            Value::Text(u) => u.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("getx requires text (url), got {:?}", other),
                ));
            }
        };
        if let Err(msg) = env.caps.check_net(url.as_str()) {
            return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
        }
        let headers = if args.len() == 2 {
            match &args[1] {
                Value::Map(m) => m
                    .iter()
                    .map(|(k, v)| {
                        let vs: String = match v {
                            Value::Text(s) => (**s).clone(),
                            other => format!("{other:?}"),
                        };
                        (k.to_display_string(), vs)
                    })
                    .collect::<Vec<_>>(),
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("getx headers must be M t t, got {:?}", other),
                    ));
                }
            }
        } else {
            vec![]
        };
        return {
            #[cfg(feature = "http")]
            {
                let mut req = minreq::get(url.as_str());
                for (k, v) in &headers {
                    req = req.with_header(k.as_str(), v.as_str());
                }
                match req.send() {
                    Ok(resp) => Ok(http_response_to_ok_map(&resp)),
                    Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
                }
            }
            #[cfg(not(feature = "http"))]
            {
                let _ = (url, headers);
                Ok(Value::Err(Box::new(Value::Text(
                    "http feature not enabled".to_string().into(),
                ))))
            }
        };
    }
    if builtin == Some(Builtin::Pstx) && (args.len() == 2 || args.len() == 3) {
        // pstx url body            — 2-arg, returns R (M t _) t
        // pstx url body headers    — 3-arg, headers is M t t
        let (url, body) = match (&args[0], &args[1]) {
            (Value::Text(u), Value::Text(b)) => (u.clone(), b.clone()),
            _ => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("pstx requires (t, t), got ({:?}, {:?})", args[0], args[1]),
                ));
            }
        };
        if let Err(msg) = env.caps.check_net(url.as_str()) {
            return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
        }
        let headers = if args.len() == 3 {
            match &args[2] {
                Value::Map(m) => m
                    .iter()
                    .map(|(k, v)| {
                        let vs: String = match v {
                            Value::Text(s) => (**s).clone(),
                            other => format!("{other:?}"),
                        };
                        (k.to_display_string(), vs)
                    })
                    .collect::<Vec<_>>(),
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("pstx headers must be M t t, got {:?}", other),
                    ));
                }
            }
        } else {
            vec![]
        };
        return {
            #[cfg(feature = "http")]
            {
                let mut req = minreq::post(url.as_str()).with_body(body.as_str());
                for (k, v) in &headers {
                    req = req.with_header(k.as_str(), v.as_str());
                }
                match req.send() {
                    Ok(resp) => Ok(http_response_to_ok_map(&resp)),
                    Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
                }
            }
            #[cfg(not(feature = "http"))]
            {
                let _ = (url, body, headers);
                Ok(Value::Err(Box::new(Value::Text(
                    "http feature not enabled".to_string().into(),
                ))))
            }
        };
    }
    if builtin == Some(Builtin::Run) && args.len() == 2 {
        // run cmd:t args:L t  >  R (M t t) t
        //
        // Argv-list process spawn. No shell, no string interpolation, no glob.
        // Returns a 3-key Map[Text, Text] on success: stdout / stderr / code.
        // Result Err only on spawn failure (cmd not found, permission denied,
        // and similar) — a non-zero exit code is NOT an error; the caller
        // inspects `code` in the map. Matches Python subprocess.run semantics.
        //
        // Captures stdout AND stderr separately. Output > RUN_OUTPUT_CAP bytes
        // (10 MiB per stream) is rejected with an Err to avoid runaway
        // buffering on misbehaving children. v1 has no env/cwd override and
        // no stdin piping — the child inherits the parent env+cwd and reads
        // /dev/null on stdin. Follow-ups: 4-arity form for env, optional
        // stdin Text arg, configurable output cap.
        let cmd = match &args[0] {
            Value::Text(s) => s.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("run requires text (cmd), got {:?}", other),
                ));
            }
        };
        let argv: Vec<String> = match &args[1] {
            Value::List(items) => {
                let mut out = Vec::with_capacity(items.len());
                for (i, v) in items.iter().enumerate() {
                    match v {
                        Value::Text(s) => out.push((**s).clone()),
                        other => {
                            return Err(RuntimeError::new(
                                "ILO-R009",
                                format!(
                                    "run argv must be L t (text list); element {i} is {:?}",
                                    other
                                ),
                            ));
                        }
                    }
                }
                out
            }
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("run argv must be L t (text list), got {:?}", other),
                ));
            }
        };
        if let Err(msg) = env.caps.check_run(cmd.as_str()) {
            return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
        }
        return Ok(run_spawn(cmd.as_str(), &argv));
    }
    if builtin == Some(Builtin::Run2) && args.len() == 2 {
        // run2 cmd:t args:L t  >  R RunResult t
        //
        // Like `run` but returns a typed Record{stdout:t; stderr:t; exit:n}
        // instead of a loose Map. Non-zero exit is NOT an error; Err only on
        // spawn failure (cmd not found, permission denied, etc.).
        let cmd = match &args[0] {
            Value::Text(s) => s.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("run2 requires text (cmd), got {:?}", other),
                ));
            }
        };
        let argv: Vec<String> = match &args[1] {
            Value::List(items) => {
                let mut out = Vec::with_capacity(items.len());
                for (i, v) in items.iter().enumerate() {
                    match v {
                        Value::Text(s) => out.push((**s).clone()),
                        other => {
                            return Err(RuntimeError::new(
                                "ILO-R009",
                                format!(
                                    "run2 argv must be L t (text list); element {i} is {:?}",
                                    other
                                ),
                            ));
                        }
                    }
                }
                out
            }
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("run2 argv must be L t (text list), got {:?}", other),
                ));
            }
        };
        return Ok(run_spawn_structured(cmd.as_str(), &argv));
    }
    if builtin == Some(Builtin::Run) && args.len() == 3 {
        // run cmd:t args:L t stdin:t  >  R (M t t) t
        //
        // Arity-3 extension: pipe `stdin` text into the child's stdin.
        // Identical semantics to the 2-arg form except stdin is piped
        // instead of /dev/null. Non-zero exit is NOT an error.
        let cmd = match &args[0] {
            Value::Text(s) => s.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("run requires text (cmd), got {:?}", other),
                ));
            }
        };
        let argv: Vec<String> = match &args[1] {
            Value::List(items) => {
                let mut out = Vec::with_capacity(items.len());
                for (i, v) in items.iter().enumerate() {
                    match v {
                        Value::Text(s) => out.push((**s).clone()),
                        other => {
                            return Err(RuntimeError::new(
                                "ILO-R009",
                                format!(
                                    "run argv must be L t (text list); element {i} is {:?}",
                                    other
                                ),
                            ));
                        }
                    }
                }
                out
            }
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("run argv must be L t (text list), got {:?}", other),
                ));
            }
        };
        let stdin_text = match &args[2] {
            Value::Text(s) => (**s).clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("run stdin arg must be t (text), got {:?}", other),
                ));
            }
        };
        if let Err(msg) = env.caps.check_run(cmd.as_str()) {
            return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
        }
        return Ok(run_spawn_with_stdin(cmd.as_str(), &argv, &stdin_text));
    }
    if builtin == Some(Builtin::Run2) && args.len() == 3 {
        // run2 cmd:t args:L t stdin:t  >  R RunResult t
        //
        // Arity-3 extension of run2: pipe `stdin` text into the child's
        // stdin. Returns the same typed RunResult record.
        let cmd = match &args[0] {
            Value::Text(s) => s.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("run2 requires text (cmd), got {:?}", other),
                ));
            }
        };
        let argv: Vec<String> = match &args[1] {
            Value::List(items) => {
                let mut out = Vec::with_capacity(items.len());
                for (i, v) in items.iter().enumerate() {
                    match v {
                        Value::Text(s) => out.push((**s).clone()),
                        other => {
                            return Err(RuntimeError::new(
                                "ILO-R009",
                                format!(
                                    "run2 argv must be L t (text list); element {i} is {:?}",
                                    other
                                ),
                            ));
                        }
                    }
                }
                out
            }
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("run2 argv must be L t (text list), got {:?}", other),
                ));
            }
        };
        let stdin_text = match &args[2] {
            Value::Text(s) => (**s).clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("run2 stdin arg must be t (text), got {:?}", other),
                ));
            }
        };
        if let Err(msg) = env.caps.check_run(cmd.as_str()) {
            return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
        }
        return Ok(run_spawn_structured_with_stdin(
            cmd.as_str(),
            &argv,
            &stdin_text,
        ));
    }
    if builtin == Some(Builtin::RunBg) && args.len() == 2 {
        // run-bg cmd:t args:L t  >  R n t
        //
        // Fire-and-forget background spawn. Returns Ok(pid:n) immediately
        // without waiting for the child. Child inherits parent stdout/stderr.
        // Err only on spawn failure (cmd not found, permission denied, etc.).
        let cmd = match &args[0] {
            Value::Text(s) => s.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("run-bg requires text (cmd), got {:?}", other),
                ));
            }
        };
        let argv: Vec<String> = match &args[1] {
            Value::List(items) => {
                let mut out = Vec::with_capacity(items.len());
                for (i, v) in items.iter().enumerate() {
                    match v {
                        Value::Text(s) => out.push((**s).clone()),
                        other => {
                            return Err(RuntimeError::new(
                                "ILO-R009",
                                format!(
                                    "run-bg argv must be L t (text list); element {i} is {:?}",
                                    other
                                ),
                            ));
                        }
                    }
                }
                out
            }
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("run-bg argv must be L t (text list), got {:?}", other),
                ));
            }
        };
        if let Err(msg) = env.caps.check_run(cmd.as_str()) {
            return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
        }
        return Ok(run_spawn_bg(cmd.as_str(), &argv));
    }
    if builtin == Some(Builtin::Trm) && args.len() == 1 {
        return match &args[0] {
            Value::Text(s) => Ok(Value::Text(Arc::new(s.trim().to_string()))),
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("trm requires text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Upr) && args.len() == 1 {
        return match &args[0] {
            Value::Text(s) => Ok(Value::Text(Arc::new(s.to_uppercase()))),
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("upr requires text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Lwr) && args.len() == 1 {
        return match &args[0] {
            Value::Text(s) => Ok(Value::Text(Arc::new(s.to_lowercase()))),
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("lwr requires text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Cap) && args.len() == 1 {
        return cap_run(&args[0]);
    }
    if builtin == Some(Builtin::Padl) && (args.len() == 2 || args.len() == 3) {
        return padl_padr_run(true, &args);
    }
    if builtin == Some(Builtin::Padr) && (args.len() == 2 || args.len() == 3) {
        return padl_padr_run(false, &args);
    }
    if builtin == Some(Builtin::Ord) && args.len() == 1 {
        return match &args[0] {
            Value::Text(s) => match s.chars().next() {
                Some(c) => Ok(Value::Number(c as u32 as f64)),
                None => Err(RuntimeError::new(
                    "ILO-R009",
                    "ord requires a non-empty string".to_string(),
                )),
            },
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("ord requires text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Chr) && args.len() == 1 {
        return match &args[0] {
            Value::Number(n) => {
                if !n.is_finite() || n.fract() != 0.0 || *n < 0.0 || *n > u32::MAX as f64 {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("chr requires a non-negative integer codepoint, got {n}"),
                    ));
                }
                let cp = *n as u32;
                match char::from_u32(cp) {
                    Some(c) => Ok(Value::Text(Arc::new(c.to_string()))),
                    None => Err(RuntimeError::new(
                        "ILO-R009",
                        format!("chr: {cp} is not a valid Unicode codepoint"),
                    )),
                }
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("chr requires number, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Chars) && args.len() == 1 {
        return match &args[0] {
            Value::Text(s) => Ok(Value::List(Arc::new(
                s.chars()
                    .map(|c| Value::Text(Arc::new(c.to_string())))
                    .collect(),
            ))),
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("chars requires text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Unq) && args.len() == 1 {
        return match &args[0] {
            Value::List(xs) => {
                let mut seen = std::collections::HashSet::new();
                let mut out = Vec::new();
                for v in xs.iter() {
                    let key = format!("{v:?}");
                    if seen.insert(key) {
                        out.push(v.clone());
                    }
                }
                Ok(Value::List(Arc::new(out)))
            }
            Value::Text(s) => {
                let mut seen = std::collections::HashSet::new();
                let deduped: String = s.chars().filter(|c| seen.insert(*c)).collect();
                Ok(Value::Text(Arc::new(deduped)))
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("unq requires a list or text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Fmt2) && args.len() == 2 {
        return match (&args[0], &args[1]) {
            (Value::Number(x), Value::Number(d)) => {
                let digits = if !d.is_finite() || *d <= 0.0 {
                    0usize
                } else {
                    (*d as usize).min(20)
                };
                Ok(Value::Text(Arc::new(format!("{:.*}", digits, x))))
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "fmt2 requires two numbers (x, digits)".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Fmt) && !args.is_empty() {
        return fmt_run(&args);
    }
    if builtin == Some(Builtin::Ls) && args.len() == 1 {
        // lsd dir > R (L t) t — list non-recursive directory entries (filenames
        // only, not full paths). Sorted lexicographically for determinism so
        // agent diffs stay stable across runs / filesystems. Missing dir or
        // permission denied surface as Err so the caller can branch with `!`
        // or pattern-match on the Result. Mirrors `rd`'s typed-error shape.
        let dir = match &args[0] {
            Value::Text(s) => s.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("ls requires text path, got {:?}", other),
                ));
            }
        };
        return match std::fs::read_dir(dir.as_str()) {
            Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
            Ok(rd) => {
                let mut names: Vec<String> = Vec::new();
                for entry in rd {
                    match entry {
                        Ok(ent) => {
                            // Lossy is intentional: ilo strings are UTF-8 and a
                            // non-UTF-8 path is vanishingly rare in agent
                            // workloads; failing the whole listing on one bad
                            // name would be more surprising than the U+FFFD.
                            names.push(ent.file_name().to_string_lossy().into_owned());
                        }
                        Err(e) => {
                            return Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string())))));
                        }
                    }
                }
                names.sort();
                let items: Vec<Value> = names
                    .into_iter()
                    .map(|n| Value::Text(Arc::new(n)))
                    .collect();
                Ok(Value::Ok(Box::new(Value::List(Arc::new(items)))))
            }
        };
    }
    if builtin == Some(Builtin::Walk) && args.len() == 1 {
        // walk dir > R (L t) t — recursive directory traversal. Returns paths
        // relative to `dir` (not absolute) so output is stable across cwd / OS
        // and composable with `cat`/`fmt` for downstream reads. Sorted for
        // deterministic output. Symlinks are not followed (avoids the cycle
        // trap that `find -L` and shell rglob hit on typical project trees).
        let dir = match &args[0] {
            Value::Text(s) => s.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("walk requires text path, got {:?}", other),
                ));
            }
        };
        if let Err(msg) = env.caps.check_read(dir.as_str()) {
            return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
        }
        let root = std::path::PathBuf::from(dir.as_str());
        match walk_collect(&root) {
            Ok(out) => {
                let items: Vec<Value> = out.into_iter().map(|n| Value::Text(Arc::new(n))).collect();
                return Ok(Value::Ok(Box::new(Value::List(Arc::new(items)))));
            }
            Err(e) => {
                return Ok(Value::Err(Box::new(Value::Text(Arc::new(e)))));
            }
        }
    }
    if builtin == Some(Builtin::Glob) && args.len() == 2 {
        // glob dir pat > R (L t) t — shell-style pattern filter under dir.
        // Pattern syntax: `*` matches any run within a path segment, `?` one
        // char within a segment, `[abc]` / `[a-z]` a char class, `**` matches
        // any number of nested segments (recursive). Sorted output. Paths
        // returned relative to `dir`, same as `walk`. Implemented as
        // `walk_collect` + matcher so we don't pull a transitive glob crate.
        let dir = match &args[0] {
            Value::Text(s) => s.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("glob requires text path, got {:?}", other),
                ));
            }
        };
        let pat = match &args[1] {
            Value::Text(s) => s.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("glob pattern must be text, got {:?}", other),
                ));
            }
        };
        if let Err(msg) = env.caps.check_read(dir.as_str()) {
            return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
        }
        let root = std::path::PathBuf::from(dir.as_str());
        match walk_collect(&root) {
            Ok(all) => {
                let items: Vec<Value> = all
                    .into_iter()
                    .filter(|p| glob_match(pat.as_str(), p))
                    .map(|n| Value::Text(Arc::new(n)))
                    .collect();
                return Ok(Value::Ok(Box::new(Value::List(Arc::new(items)))));
            }
            Err(e) => {
                return Ok(Value::Err(Box::new(Value::Text(Arc::new(e)))));
            }
        }
    }
    if builtin == Some(Builtin::Dirname) && args.len() == 1 {
        // dirname path:t > t — POSIX-style parent directory of `path`.
        //   "/a/b/c.txt" -> "/a/b"
        //   "a/b/c.txt"  -> "a/b"
        //   "/"          -> "/"   (root has no parent; POSIX)
        //   "foo.txt"    -> ""    (no dir component; matches POSIX, NOT ".")
        //   "foo/"       -> "foo" (trailing slash treated as empty final segment)
        //   "/a"         -> "/"   (single-component absolute path)
        //   ""           -> ""    (total function)
        // Pure-text implementation deliberately avoids std::path::Path because
        // its semantics shift between Unix/Windows builds; ilo's path builtins
        // are Unix forward-slash only in 0.12.1 (see SPEC.md).
        let p = match &args[0] {
            Value::Text(s) => s.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("dirname requires text path, got {:?}", other),
                ));
            }
        };
        return Ok(Value::Text(Arc::new(dirname_posix(p.as_str()))));
    }
    if builtin == Some(Builtin::Basename) && args.len() == 1 {
        // basename path:t > t — POSIX-style final path segment.
        //   "/a/b/c.txt" -> "c.txt"
        //   "a/b/c.txt"  -> "c.txt"
        //   "/"          -> "/"     (POSIX edge: basename of root is root)
        //   "foo.txt"    -> "foo.txt"
        //   "foo/"       -> "foo"   (trailing slash stripped first)
        //   ""           -> ""
        let p = match &args[0] {
            Value::Text(s) => s.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("basename requires text path, got {:?}", other),
                ));
            }
        };
        return Ok(Value::Text(Arc::new(basename_posix(p.as_str()))));
    }
    if builtin == Some(Builtin::Pathjoin) && args.len() == 1 {
        // pathjoin parts:L t > t — join list of segments with `/`, collapsing
        // duplicate separators at joints and dropping empty segments. List-form
        // (not variadic) so arity inference stays predictable.
        //   ["a" "b" "c.txt"]   -> "a/b/c.txt"
        //   ["/a/" "/b/" "c.txt"] -> "/a/b/c.txt"
        //   []                    -> ""
        //   ["foo"]               -> "foo"
        //   ["" "a" ""]           -> "a"
        //   ["/" "a"]             -> "/a"   (leading absolute root preserved)
        let parts = match &args[0] {
            Value::List(xs) => xs.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("pathjoin requires list of text, got {:?}", other),
                ));
            }
        };
        let mut segs: Vec<&str> = Vec::with_capacity(parts.len());
        for p in parts.iter() {
            match p {
                Value::Text(s) => segs.push(s.as_str()),
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("pathjoin requires list of text, got element {:?}", other),
                    ));
                }
            }
        }
        return Ok(Value::Text(Arc::new(pathjoin_posix(&segs))));
    }
    if builtin == Some(Builtin::DurParse) && args.len() == 1 {
        // dur-parse s:t > R n t — parse a human duration string into seconds.
        // Accepts mixed unit sequences: "3 weeks 2 days 5 hours", "4h 32m",
        // "1d", "1.5 hours". Lenient: case-insensitive, optional space between
        // number and unit, singular/plural, standard abbreviations s/m/h/d/w.
        // Returns Err on empty input or if no recognisable unit is found.
        let s = match &args[0] {
            Value::Text(s) => s.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("dur-parse requires text, got {:?}", other),
                ));
            }
        };
        match dur_parse(s.as_str()) {
            Ok(secs) => return Ok(Value::Ok(Box::new(Value::Number(secs)))),
            Err(msg) => return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg.to_string()))))),
        }
    }
    if builtin == Some(Builtin::DurFmt) && args.len() == 1 {
        // dur-fmt n:n > t — format seconds as human-readable duration.
        // Output uses the largest applicable unit; zero parts are dropped.
        // E.g. 9720 -> "2h 42m", 86400 -> "1d", 90 -> "1m 30s", 45 -> "45s".
        // Negative seconds formatted with a leading "-". Zero returns "0s".
        let secs = match &args[0] {
            Value::Number(n) => *n,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("dur-fmt requires a number (seconds), got {:?}", other),
                ));
            }
        };
        return Ok(Value::Text(Arc::new(dur_fmt(secs))));
    }
    if builtin == Some(Builtin::AddMo) && args.len() == 2 {
        return add_mo_impl(&args[0], &args[1]);
    }
    if builtin == Some(Builtin::LastDom) && args.len() == 1 {
        return last_dom_impl(&args[0]);
    }
    if builtin == Some(Builtin::NextBusinessDay) && args.len() == 1 {
        return next_business_day_impl(&args[0]);
    }
    if builtin == Some(Builtin::DayOfWeek) && args.len() == 1 {
        return day_of_week_impl(&args[0]);
    }
    if builtin == Some(Builtin::Fsize) && args.len() == 1 {
        // fsize path > R n t — file size in bytes. Err on missing,
        // permission-denied, or path-is-directory. Symlinks are followed
        // (matches POSIX `stat`, not `lstat`). Predicate counterpart is
        // `isfile` which collapses these errors into `false`.
        let path = match &args[0] {
            Value::Text(s) => s.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("fsize requires text path, got {:?}", other),
                ));
            }
        };
        if let Err(msg) = env.caps.check_read(path.as_str()) {
            return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
        }
        return match std::fs::metadata(path.as_str()) {
            Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
            Ok(md) if md.is_dir() => Ok(Value::Err(Box::new(Value::Text(Arc::new(format!(
                "{}: is a directory",
                path
            )))))),
            Ok(md) => Ok(Value::Ok(Box::new(Value::Number(md.len() as f64)))),
        };
    }
    if builtin == Some(Builtin::Mtime) && args.len() == 1 {
        // mtime path > R n t — last modification time as Unix epoch seconds
        // (f64). Err on missing or permission-denied. Symlinks followed.
        // Returns seconds (not ms) to match `now` — `now-ms` exists for
        // sub-second precision; mtime is a wall-clock timestamp and the
        // fractional second is preserved as f64.
        let path = match &args[0] {
            Value::Text(s) => s.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("mtime requires text path, got {:?}", other),
                ));
            }
        };
        if let Err(msg) = env.caps.check_read(path.as_str()) {
            return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
        }
        return match std::fs::metadata(path.as_str()) {
            Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
            Ok(md) => match md.modified() {
                Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
                Ok(t) => match t.duration_since(std::time::UNIX_EPOCH) {
                    Ok(d) => Ok(Value::Ok(Box::new(Value::Number(d.as_secs_f64())))),
                    Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
                },
            },
        };
    }
    if builtin == Some(Builtin::Isfile) && args.len() == 1 {
        // isfile path > b — true iff path resolves to a regular file
        // (following symlinks). Missing path, permission-denied, or
        // directory all return `false` — Python convention. The natural
        // branch shape is `?isfile p{...}`, so collapsing the error tier
        // into `false` keeps the call site one token wide.
        let path = match &args[0] {
            Value::Text(s) => s.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("isfile requires text path, got {:?}", other),
                ));
            }
        };
        if env.caps.check_read(path.as_str()).is_err() {
            return Ok(Value::Bool(false));
        }
        let is = std::fs::metadata(path.as_str())
            .map(|m| m.is_file())
            .unwrap_or(false);
        return Ok(Value::Bool(is));
    }
    if builtin == Some(Builtin::Isdir) && args.len() == 1 {
        // isdir path > b — true iff path resolves to a directory (following
        // symlinks). Missing / perm-denied / not-a-dir all return `false`.
        let path = match &args[0] {
            Value::Text(s) => s.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("isdir requires text path, got {:?}", other),
                ));
            }
        };
        if env.caps.check_read(path.as_str()).is_err() {
            return Ok(Value::Bool(false));
        }
        let is = std::fs::metadata(path.as_str())
            .map(|m| m.is_dir())
            .unwrap_or(false);
        return Ok(Value::Bool(is));
    }
    if builtin == Some(Builtin::TzOffset) && args.len() == 2 {
        // tz-offset tz:t epoch:n > R n t
        // Returns the UTC offset in seconds for the named IANA timezone at
        // the given Unix epoch. DST transitions are handled by chrono-tz:
        // the offset reflects the actual local time rule at that instant.
        // Returns Err on unknown timezone name. Positive = east of UTC.
        let tz_name = match &args[0] {
            Value::Text(s) => s.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("tz-offset: first arg must be text tz name, got {:?}", other),
                ));
            }
        };
        let epoch = match &args[1] {
            Value::Number(n) => *n,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "tz-offset: second arg must be number epoch, got {:?}",
                        other
                    ),
                ));
            }
        };
        let tz: chrono_tz::Tz = match tz_name.parse() {
            Ok(t) => t,
            Err(_) => {
                return Ok(Value::Err(Box::new(Value::Text(Arc::new(format!(
                    "tz-offset: unknown timezone {:?}",
                    tz_name.as_str()
                ))))));
            }
        };
        // Convert epoch seconds to a chrono::DateTime in the target tz.
        // from_timestamp gives a UTC DateTime; with_timezone applies the tz rules.
        // fix() on TzOffset yields a FixedOffset which carries local_minus_utc().
        let secs = epoch as i64;
        let utc_dt = chrono::DateTime::from_timestamp(secs, 0).unwrap_or_default();
        let local_dt = utc_dt.with_timezone(&tz);
        use chrono::offset::Offset as _;
        let offset_secs = local_dt.offset().fix().local_minus_utc() as f64;
        return Ok(Value::Ok(Box::new(Value::Number(offset_secs))));
    }
    if builtin == Some(Builtin::Rd) && (args.len() == 1 || args.len() == 2) {
        let path = match &args[0] {
            Value::Text(s) => s.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("rd requires text path, got {:?}", other),
                ));
            }
        };
        if let Err(msg) = env.caps.check_read(path.as_str()) {
            return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
        }
        // rd always returns raw text — no extension-based auto-parse.
        // 2-arg form (rd path fmt) is kept for explicit csv/tsv override but
        // callers wanting JSON must use `rd-json` or `rd path + jpar`.
        let fmt = if args.len() == 2 {
            match &args[1] {
                Value::Text(s) => s.as_str().to_owned(),
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("rd format must be text, got {:?}", other),
                    ));
                }
            }
        } else {
            "raw".to_owned()
        };
        return match std::fs::read_to_string(path.as_str()) {
            Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
            Ok(content) => match parse_format(&fmt, &content) {
                Ok(v) => Ok(Value::Ok(Box::new(v))),
                Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e))))),
            },
        };
    }
    if builtin == Some(Builtin::RdJson) && args.len() == 1 {
        let path = match &args[0] {
            Value::Text(s) => s.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("rd-json requires text path, got {:?}", other),
                ));
            }
        };
        if let Err(msg) = env.caps.check_read(path.as_str()) {
            return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
        }
        return match std::fs::read_to_string(path.as_str()) {
            Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
            Ok(content) => match parse_format("json", &content) {
                Ok(v) => Ok(Value::Ok(Box::new(v))),
                Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e))))),
            },
        };
    }
    if builtin == Some(Builtin::Rdb) && args.len() == 2 {
        let s = match &args[0] {
            Value::Text(s) => s.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("rdb requires text string, got {:?}", other),
                ));
            }
        };
        let fmt = match &args[1] {
            Value::Text(f) => f.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("rdb format must be text, got {:?}", other),
                ));
            }
        };
        return match parse_format(&fmt, &s) {
            Ok(v) => Ok(Value::Ok(Box::new(v))),
            Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e))))),
        };
    }
    if builtin == Some(Builtin::Rdl) && args.len() == 1 {
        return match &args[0] {
            Value::Text(path) => {
                if let Err(msg) = env.caps.check_read(path.as_str()) {
                    return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
                }
                match std::fs::read_to_string(path.as_str()) {
                    Ok(content) => {
                        let lines: Vec<Value> = content
                            .lines()
                            .map(|l| Value::Text(Arc::new(l.to_string())))
                            .collect();
                        Ok(Value::Ok(Box::new(Value::List(Arc::new(lines)))))
                    }
                    Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
                }
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("rdl requires text path, got {:?}", other),
            )),
        };
    }
    // rdin > R t t — read all of stdin as text.
    // rdinl > R (L t) t — read stdin line by line.
    // Both are 0-arg and return Err on I/O failure. On WASM targets stdin is
    // not available; we return Err immediately so callers can branch safely.
    if builtin == Some(Builtin::Rdin) && args.is_empty() {
        return rdin_impl();
    }
    if builtin == Some(Builtin::Rdinl) && args.is_empty() {
        return rdinl_impl();
    }
    // for-line stdin > LazyStdinLines — lazy line iterator over stdin.
    // Takes exactly one argument: the text literal "stdin".
    // Returns Value::LazyStdinLines (not wrapped in Result) so it can be
    // passed directly to `@binding (for-line stdin) {...}` foreach.
    // On WASM stdin is unavailable; returns Err immediately.
    if builtin == Some(Builtin::ForLine) && args.len() == 1 {
        return for_line_impl(&args[0]);
    }
    if builtin == Some(Builtin::Wr) && (args.len() == 2 || args.len() == 3) {
        return wr_run(env, args);
    }
    if builtin == Some(Builtin::Wra) && args.len() == 2 {
        let path = match &args[0] {
            Value::Text(s) => s.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("wra: first arg must be a text path, got {:?}", other),
                ));
            }
        };
        if let Err(msg) = env.caps.check_write(path.as_str()) {
            return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
        }
        let content = match &args[1] {
            Value::Text(s) => (**s).clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("wra: second arg must be text content, got {:?}", other),
                ));
            }
        };
        use std::io::Write as _;
        return match std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path.as_str())
        {
            Ok(mut f) => match f.write_all(content.as_bytes()) {
                Ok(()) => Ok(Value::Ok(Box::new(Value::Text(path)))),
                Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
            },
            Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
        };
    }
    if builtin == Some(Builtin::Wro) && args.len() == 2 {
        let path = match &args[0] {
            Value::Text(s) => s.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("wro: first arg must be a text path, got {:?}", other),
                ));
            }
        };
        if let Err(msg) = env.caps.check_write(path.as_str()) {
            return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
        }
        let content = match &args[1] {
            Value::Text(s) => (**s).clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("wro: second arg must be text content, got {:?}", other),
                ));
            }
        };
        return match std::fs::write(path.as_str(), content.as_bytes()) {
            Ok(()) => Ok(Value::Ok(Box::new(Value::Text(path)))),
            Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
        };
    }
    if builtin == Some(Builtin::Wrl) && args.len() == 2 {
        if let Value::Text(path) = &args[0] {
            if let Err(msg) = env.caps.check_write(path.as_str()) {
                return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
            }
        }
        return match (&args[0], &args[1]) {
            (Value::Text(path), Value::List(lines)) => {
                let mut content = String::new();
                for line in lines.iter() {
                    match line {
                        Value::Text(s) => {
                            content.push_str(s);
                            content.push('\n');
                        }
                        other => {
                            return Err(RuntimeError::new(
                                "ILO-R009",
                                format!("wrl list must contain text, got {:?}", other),
                            ));
                        }
                    }
                }
                match std::fs::write(path.as_str(), &content) {
                    Ok(()) => Ok(Value::Ok(Box::new(Value::Text(path.clone())))),
                    Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
                }
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("wrl requires text path and list of text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Jpth) && args.len() == 2 {
        return match (&args[0], &args[1]) {
            (Value::Text(json_str), Value::Text(path)) => {
                // Diagnose JSONPath-shaped input up-front so the agent gets a
                // clear pointer at the dot-path form instead of a misleading
                // "key not found: $".
                if let Some(msg) = crate::builtins::jpth_jsonpath_diagnostic(path) {
                    return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
                }
                match serde_json::from_str::<serde_json::Value>(json_str) {
                    Ok(parsed) => {
                        let mut current = &parsed;
                        for key in path.split('.') {
                            if let Ok(idx) = key.parse::<usize>() {
                                if let Some(v) = current.as_array().and_then(|a| a.get(idx)) {
                                    current = v;
                                } else {
                                    return Ok(Value::Err(Box::new(Value::Text(Arc::new(
                                        format!("key not found: {key}"),
                                    )))));
                                }
                            } else if let Some(v) = current.get(key) {
                                current = v;
                            } else {
                                return Ok(Value::Err(Box::new(Value::Text(Arc::new(format!(
                                    "key not found: {key}"
                                ))))));
                            }
                        }
                        // Return the value at the path as a typed ilo Value:
                        // arrays → L, objects → Record, primitives → matching
                        // scalar. This lets `mkeys`/`map`/`flt`/`@` and the
                        // `jkeys` builtin accept the result directly instead
                        // of seeing a stringified blob.
                        let typed = serde_json_to_value(current.clone());
                        Ok(Value::Ok(Box::new(typed)))
                    }
                    Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
                }
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "jpth requires two text args".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Jkeys) && args.len() == 2 {
        return match (&args[0], &args[1]) {
            (Value::Text(json_str), Value::Text(path)) => {
                if let Some(msg) = crate::builtins::jpth_jsonpath_diagnostic(path) {
                    return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
                }
                match serde_json::from_str::<serde_json::Value>(json_str) {
                    Ok(parsed) => {
                        let mut current = &parsed;
                        // Empty path = top-level. Skip walk so callers can do
                        // `jkeys! txt ""` for the root object.
                        if !path.is_empty() {
                            for key in path.split('.') {
                                if let Ok(idx) = key.parse::<usize>() {
                                    if let Some(v) = current.as_array().and_then(|a| a.get(idx)) {
                                        current = v;
                                    } else {
                                        return Ok(Value::Err(Box::new(Value::Text(Arc::new(
                                            format!("key not found: {key}"),
                                        )))));
                                    }
                                } else if let Some(v) = current.get(key) {
                                    current = v;
                                } else {
                                    return Ok(Value::Err(Box::new(Value::Text(Arc::new(
                                        format!("key not found: {key}"),
                                    )))));
                                }
                            }
                        }
                        match current.as_object() {
                            Some(obj) => {
                                let mut keys: Vec<String> = obj.keys().cloned().collect();
                                keys.sort();
                                let items: Vec<Value> =
                                    keys.into_iter().map(|k| Value::Text(Arc::new(k))).collect();
                                Ok(Value::Ok(Box::new(Value::List(Arc::new(items)))))
                            }
                            None => Ok(Value::Err(Box::new(Value::Text(Arc::new(
                                "jkeys: value at path is not a JSON object".to_string(),
                            ))))),
                        }
                    }
                    Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
                }
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "jkeys requires two text args".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Prnt) && args.len() == 1 {
        let v = args
            .into_iter()
            .next()
            .expect("prnt: arity=1 guaranteed by caller");
        let s = format!("{v}");
        // +1 for the trailing newline `println!` adds. Charging it keeps the
        // byte budget honest against a `wh true{prnt 0}` runaway.
        crate::runtime_guard::record_output(s.len() + 1);
        println!("{s}");
        return Ok(v);
    }
    if builtin == Some(Builtin::Jdmp) && args.len() == 1 {
        let json_val = value_to_json(&args[0]);
        return Ok(Value::Text(Arc::new(json_val.to_string())));
    }
    if builtin == Some(Builtin::Jpar) && args.len() == 1 {
        return match &args[0] {
            Value::Text(s) => match serde_json::from_str::<serde_json::Value>(s) {
                Ok(v) => Ok(Value::Ok(Box::new(serde_json_to_value(v)))),
                Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
            },
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("jpar requires text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::JparList) && args.len() == 1 {
        return match &args[0] {
            Value::Text(s) => match serde_json::from_str::<serde_json::Value>(s) {
                Ok(serde_json::Value::Array(arr)) => {
                    let items: Vec<Value> = arr.into_iter().map(serde_json_to_value).collect();
                    Ok(Value::Ok(Box::new(Value::List(Arc::new(items)))))
                }
                Ok(other) => {
                    let kind = match &other {
                        serde_json::Value::Object(_) => "object",
                        serde_json::Value::Null => "null",
                        serde_json::Value::Bool(_) => "bool",
                        serde_json::Value::Number(_) => "number",
                        serde_json::Value::String(_) => "string",
                        serde_json::Value::Array(_) => unreachable!(),
                    };
                    Ok(Value::Err(Box::new(Value::Text(Arc::new(format!(
                        "jpar-list: expected JSON array, got {kind}"
                    ))))))
                }
                Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
            },
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("jpar-list requires text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Rdjl) && args.len() == 1 {
        return match &args[0] {
            Value::Text(path) => match std::fs::read_to_string(path.as_str()) {
                Ok(content) => {
                    let mut items: Vec<Value> = Vec::new();
                    for line in content.split('\n') {
                        if line.is_empty() {
                            continue;
                        }
                        let parsed = match serde_json::from_str::<serde_json::Value>(line) {
                            Ok(v) => Value::Ok(Box::new(serde_json_to_value(v))),
                            Err(e) => Value::Err(Box::new(Value::Text(Arc::new(e.to_string())))),
                        };
                        items.push(parsed);
                    }
                    Ok(Value::List(Arc::new(items)))
                }
                Err(e) => Err(RuntimeError::new(
                    "ILO-R009",
                    format!("rdjl failed to read '{}': {}", path, e),
                )),
            },
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("rdjl requires text path, got {:?}", other),
            )),
        };
    }

    if builtin == Some(Builtin::Env) && args.len() == 1 {
        return match &args[0] {
            Value::Text(key) => {
                if let Err(msg) = env.caps.check_env(key.as_str()) {
                    return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
                }
                match std::env::var(key.as_str()) {
                    Ok(val) => Ok(Value::Ok(Box::new(Value::Text(Arc::new(val))))),
                    Err(_) => Ok(Value::Err(Box::new(Value::Text(Arc::new(format!(
                        "env var '{}' not set",
                        key
                    )))))),
                }
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("env requires text, got {:?}", other),
            )),
        };
    }

    // world > World — construct a World token from the current Caps policy.
    // Zero args; returns Value::World with boolean flags indicating which
    // capability dimensions are granted. Under Caps::Permissive all four are
    // true (legacy mode, full backwards compat). Under Caps::Restricted each
    // flag is true iff the corresponding Policy is not an empty list.
    if builtin == Some(Builtin::WorldCap) && args.is_empty() {
        let (net, read, write, run) = match env.caps.as_ref() {
            crate::caps::Caps::Permissive => (true, true, true, true),
            crate::caps::Caps::Restricted {
                net,
                read,
                write,
                run,
                ..
            } => {
                let cap_allowed = |p: &crate::caps::Policy| {
                    matches!(p, crate::caps::Policy::All)
                        || matches!(p, crate::caps::Policy::List(v) if !v.is_empty())
                };
                (
                    cap_allowed(net),
                    cap_allowed(read),
                    cap_allowed(write),
                    cap_allowed(run),
                )
            }
        };
        return Ok(Value::World {
            net,
            read,
            write,
            run,
        });
    }

    // world-no-net > World — construct a World with net=false.
    // All other caps (read, write, run) are inherited from the active Caps so
    // that an outer --allow-read / --allow-run policy is preserved.
    if builtin == Some(Builtin::WorldNoNet) && args.is_empty() {
        let (_net, read, write, run) = match env.caps.as_ref() {
            crate::caps::Caps::Permissive => (true, true, true, true),
            crate::caps::Caps::Restricted {
                net,
                read,
                write,
                run,
                ..
            } => {
                let cap_allowed = |p: &crate::caps::Policy| {
                    matches!(p, crate::caps::Policy::All)
                        || matches!(p, crate::caps::Policy::List(v) if !v.is_empty())
                };
                (
                    cap_allowed(net),
                    cap_allowed(read),
                    cap_allowed(write),
                    cap_allowed(run),
                )
            }
        };
        return Ok(Value::World {
            net: false, // statically denied
            read,
            write,
            run,
        });
    }

    // env-all -> R M t t: snapshot the full process environment as a
    // Map[Text, Text] wrapped in Ok. The Result wrapper mirrors `env key`
    // so callers can use `env-all!` to auto-unwrap; the Err arm is reserved
    // for future failure modes (non-UTF-8 vars, sandboxed envs). std::env::vars()
    // silently skips non-UTF-8 entries today, so the snapshot is always Ok.
    if builtin == Some(Builtin::EnvAll) && args.is_empty() {
        // env-all reads the entire environment; check capability using "*" sentinel.
        if let Err(msg) = env.caps.check_env("*") {
            return Ok(Value::Err(Box::new(Value::Text(Arc::new(msg)))));
        }
        let map: std::collections::HashMap<MapKey, Value> = std::env::vars()
            .map(|(k, v)| (MapKey::Text(k), Value::Text(Arc::new(v))))
            .collect();
        return Ok(Value::Ok(Box::new(Value::Map(Arc::new(map)))));
    }

    // Higher-order builtins: map, flt, fld
    // A function reference can be Value::FnRef(name) or Value::Text(Arc::new(name)) when the
    // function name was passed as a CLI string argument. Inline lambdas with
    // free-var capture produce Value::Closure, which carries the lifted fn
    // name plus by-value capture snapshots to append after the per-item args.
    fn resolve_fn_ref(val: &Value) -> Option<String> {
        match val {
            Value::FnRef(n) => Some(n.clone()),
            Value::Text(n) => Some((**n).clone()),
            Value::Closure { fn_name, .. } => Some(fn_name.clone()),
            _ => None,
        }
    }
    // Extract trailing captures from a closure value, if any. Closures carry
    // by-value snapshots of free variables that get appended after the
    // per-item args at each HOF call. Returns empty for plain FnRef/Text.
    fn closure_captures(val: &Value) -> Vec<Value> {
        match val {
            Value::Closure { captures, .. } => captures.clone(),
            _ => Vec::new(),
        }
    }
    if builtin == Some(Builtin::Map) && (args.len() == 2 || args.len() == 3) {
        let fn_name = resolve_fn_ref(&args[0]).ok_or_else(|| {
            RuntimeError::new(
                "ILO-R009",
                format!(
                    "map: first arg must be a function reference, got {:?}",
                    args[0]
                ),
            )
        })?;
        let captures = closure_captures(&args[0]);
        // closure-bind: map fn ctx xs
        let (ctx, list_arg) = if args.len() == 3 {
            (Some(args[1].clone()), &args[2])
        } else {
            (None, &args[1])
        };
        let items = match list_arg {
            Value::List(l) => l.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("map: list arg must be a list, got {:?}", other),
                ));
            }
        };
        let mut result = Vec::with_capacity(items.len());
        for item in items.iter().cloned() {
            let mut call_args = match &ctx {
                Some(c) => vec![item, c.clone()],
                None => vec![item],
            };
            call_args.extend(captures.iter().cloned());
            result.push(call_function(env, &fn_name, call_args)?);
        }
        return Ok(Value::List(Arc::new(result)));
    }
    // par-map fn xs [n] — general parallel fan-out.
    //
    // Applies `fn` to each element of `xs` up to `n` items in parallel
    // (default: num_cpus). Returns `L (R b t)` — per-item Ok/Err so a single
    // worker failure does not abort the rest. Order-preserving.
    //
    // The inner function may use any builtin (including I/O builtins that
    // check caps); capability checks run inside the worker threads as usual.
    //
    // The large body is extracted into `par_map_run` (marked `#[inline(never)]`)
    // following the dispatch-arm-size convention from #5ze / ILO-289.
    if builtin == Some(Builtin::ParMap) && (args.len() == 2 || args.len() == 3) {
        let fn_name = resolve_fn_ref(&args[0]).ok_or_else(|| {
            RuntimeError::new(
                "ILO-R009",
                format!(
                    "par-map: first arg must be a function reference, got {:?}",
                    args[0]
                ),
            )
        })?;
        let captures = closure_captures(&args[0]);
        let items = match &args[1] {
            Value::List(l) => l.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("par-map: second arg must be a list, got {:?}", other),
                ));
            }
        };
        let concurrency: usize = if args.len() == 3 {
            match &args[2] {
                Value::Number(n) => {
                    let n = *n as usize;
                    if n == 0 {
                        par_map_default_concurrency()
                    } else {
                        n
                    }
                }
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!(
                            "par-map: third arg must be a number (concurrency), got {:?}",
                            other
                        ),
                    ));
                }
            }
        } else {
            par_map_default_concurrency()
        };
        // Snapshot the function table and caps so worker threads can build
        // their own Env without holding a reference to the caller's Env.
        let fns_snapshot = env.functions.clone();
        let caps_snapshot = env.caps.clone();
        return Ok(Value::List(Arc::new(par_map_run(
            &fn_name,
            captures,
            &items,
            concurrency,
            fns_snapshot,
            caps_snapshot,
        ))));
    }
    // mapr fn xs: short-circuiting Result-aware map.
    //
    // The callee must return R b e. On each item:
    //   ~v  → unwrap to v and accumulate
    //   ^e  → return ^e immediately (whole call short-circuits)
    //   any → runtime error (callee broke its contract)
    //
    // Final return on the all-Ok path is ~(L b). Pair with `!` to thread
    // the err up into a Result-returning caller. Retires the
    // `ton s:t>n;r=num s;?r{~v:v;^_:0}` helper that html-scraper and
    // CSV-parsing personas kept writing. See ilo_assessment_feedback.md
    // line 2541 for the originating entry.
    //
    // Deliberately 2-arity only: no closure-bind ctx variant yet. If a
    // workload turns up that needs it, add it the same way `Map`/`Flt`
    // have it. Keeping the surface tight until a real need surfaces.
    if builtin == Some(Builtin::Mapr) && args.len() == 2 {
        let fn_name = resolve_fn_ref(&args[0]).ok_or_else(|| {
            RuntimeError::new(
                "ILO-R009",
                format!(
                    "mapr: first arg must be a function reference, got {:?}",
                    args[0]
                ),
            )
        })?;
        let captures = closure_captures(&args[0]);
        let items = match &args[1] {
            Value::List(l) => l.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("mapr: list arg must be a list, got {:?}", other),
                ));
            }
        };
        let mut result = Vec::with_capacity(items.len());
        for item in items.iter().cloned() {
            let mut call_args = vec![item];
            call_args.extend(captures.iter().cloned());
            match call_function(env, &fn_name, call_args)? {
                Value::Ok(inner) => result.push(*inner),
                Value::Err(e) => return Ok(Value::Err(e)),
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("mapr: fn must return a Result (~v or ^e), got {:?}", other),
                    ));
                }
            }
        }
        return Ok(Value::Ok(Box::new(Value::List(Arc::new(result)))));
    }
    if builtin == Some(Builtin::Flt) && (args.len() == 2 || args.len() == 3) {
        let fn_name = resolve_fn_ref(&args[0]).ok_or_else(|| {
            RuntimeError::new(
                "ILO-R009",
                format!(
                    "flt: first arg must be a function reference, got {:?}",
                    args[0]
                ),
            )
        })?;
        let captures = closure_captures(&args[0]);
        let mut args_iter = args.into_iter();
        let _ = args_iter.next(); // fn arg
        let (ctx, list_arg) = if args_iter.len() == 2 {
            let c = args_iter.next().unwrap();
            let l = args_iter.next().unwrap();
            (Some(c), l)
        } else {
            (None, args_iter.next().unwrap())
        };
        let items = match list_arg {
            Value::List(l) => l,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("flt: list arg must be a list, got {:?}", other),
                ));
            }
        };
        return flt_run(env, &fn_name, captures, ctx, items);
    }
    if builtin == Some(Builtin::Ct) && (args.len() == 2 || args.len() == 3) {
        // ct fn xs / ct fn ctx xs  → number of elements where fn returns true.
        // Mirrors flt's predicate semantics exactly; only the accumulator
        // shape differs (counter instead of pushing onto a result list).
        // Motivating shape: bioinformatics rerun6 `tm=ct has-tm seqs` saves
        // the L b allocation that `len (flt has-tm seqs)` pays for.
        //
        // Named `ct` (not `cnt`) because `cnt` is reserved as the loop
        // continue keyword — see src/parser/mod.rs:3507.
        let fn_name = resolve_fn_ref(&args[0]).ok_or_else(|| {
            RuntimeError::new(
                "ILO-R009",
                format!(
                    "ct: first arg must be a function reference, got {:?}",
                    args[0]
                ),
            )
        })?;
        let captures = closure_captures(&args[0]);
        let (ctx, list_arg) = if args.len() == 3 {
            (Some(args[1].clone()), &args[2])
        } else {
            (None, &args[1])
        };
        let items = match list_arg {
            Value::List(l) => l.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("ct: list arg must be a list, got {:?}", other),
                ));
            }
        };
        let mut count: i64 = 0;
        for item in items.iter() {
            let mut call_args = match &ctx {
                Some(c) => vec![item.clone(), c.clone()],
                None => vec![item.clone()],
            };
            call_args.extend(captures.iter().cloned());
            match call_function(env, &fn_name, call_args)? {
                Value::Bool(true) => count += 1,
                Value::Bool(false) => {}
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("ct: predicate must return bool, got {:?}", other),
                    ));
                }
            }
        }
        return Ok(Value::Number(count as f64));
    }
    if builtin == Some(Builtin::Fld) && (args.len() == 3 || args.len() == 4) {
        let fn_name = resolve_fn_ref(&args[0]).ok_or_else(|| {
            RuntimeError::new(
                "ILO-R009",
                format!(
                    "fld: first arg must be a function reference, got {:?}",
                    args[0]
                ),
            )
        })?;
        let captures = closure_captures(&args[0]);
        // closure-bind: fld fn ctx xs init
        let (ctx, list_arg, init) = if args.len() == 4 {
            (Some(args[1].clone()), &args[2], args[3].clone())
        } else {
            (None, &args[1], args[2].clone())
        };
        let items = match list_arg {
            Value::List(l) => l.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("fld: list arg must be a list, got {:?}", other),
                ));
            }
        };
        let mut acc = init;
        for item in items.iter() {
            let mut call_args = match &ctx {
                Some(c) => vec![acc, item.clone(), c.clone()],
                None => vec![acc, item.clone()],
            };
            call_args.extend(captures.iter().cloned());
            acc = call_function(env, &fn_name, call_args)?;
        }
        return Ok(acc);
    }

    if builtin == Some(Builtin::Partition) && args.len() == 2 {
        let fn_name = resolve_fn_ref(&args[0]).ok_or_else(|| {
            RuntimeError::new(
                "ILO-R009",
                format!(
                    "partition: first arg must be a function reference, got {:?}",
                    args[0]
                ),
            )
        })?;
        let captures = closure_captures(&args[0]);
        let items = match &args[1] {
            Value::List(l) => l.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("partition: second arg must be a list, got {:?}", other),
                ));
            }
        };
        let mut pass: Vec<Value> = Vec::new();
        let mut fail: Vec<Value> = Vec::new();
        for item in items.iter() {
            let mut call_args = vec![item.clone()];
            call_args.extend(captures.iter().cloned());
            match call_function(env, &fn_name, call_args)? {
                Value::Bool(true) => pass.push(item.clone()),
                Value::Bool(false) => fail.push(item.clone()),
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("partition: predicate must return bool, got {:?}", other),
                    ));
                }
            }
        }
        return Ok(Value::List(Arc::new(vec![
            Value::List(Arc::new(pass)),
            Value::List(Arc::new(fail)),
        ])));
    }

    if builtin == Some(Builtin::Flatmap) && args.len() == 2 {
        let fn_name = resolve_fn_ref(&args[0]).ok_or_else(|| {
            RuntimeError::new(
                "ILO-R009",
                format!(
                    "flatmap: first arg must be a function reference, got {:?}",
                    args[0]
                ),
            )
        })?;
        let captures = closure_captures(&args[0]);
        let items = match &args[1] {
            Value::List(l) => l.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("flatmap: second arg must be a list, got {:?}", other),
                ));
            }
        };
        let mut result: Vec<Value> = Vec::new();
        for item in items.iter().cloned() {
            let mut call_args = vec![item];
            call_args.extend(captures.iter().cloned());
            match call_function(env, &fn_name, call_args)? {
                Value::List(inner) => result.extend(inner.iter().cloned()),
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("flatmap: function must return a list, got {:?}", other),
                    ));
                }
            }
        }
        return Ok(Value::List(Arc::new(result)));
    }

    if builtin == Some(Builtin::Uniqby) && args.len() == 2 {
        let fn_name = resolve_fn_ref(&args[0]).ok_or_else(|| {
            RuntimeError::new(
                "ILO-R009",
                format!(
                    "uniqby: first arg must be a function reference, got {:?}",
                    args[0]
                ),
            )
        })?;
        let captures = closure_captures(&args[0]);
        let items = match &args[1] {
            Value::List(l) => l.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("uniqby: second arg must be a list, got {:?}", other),
                ));
            }
        };
        return uniqby_run(env, &fn_name, captures, items);
    }

    if builtin == Some(Builtin::Grp) && args.len() == 2 {
        let fn_name = resolve_fn_ref(&args[0]).ok_or_else(|| {
            RuntimeError::new(
                "ILO-R009",
                format!(
                    "grp: first arg must be a function reference, got {:?}",
                    args[0]
                ),
            )
        })?;
        let captures = closure_captures(&args[0]);
        let items = match &args[1] {
            Value::List(l) => l.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("grp: second arg must be a list, got {:?}", other),
                ));
            }
        };
        return grp_run(env, &fn_name, captures, items);
    }
    if builtin == Some(Builtin::Frq) && args.len() == 1 {
        let items = match &args[0] {
            Value::List(l) => l,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("frq: arg must be a list, got {:?}", other),
                ));
            }
        };
        let mut counts: std::collections::HashMap<MapKey, usize> = std::collections::HashMap::new();
        for item in items.iter() {
            // Build a typed `MapKey` so the resulting map preserves the element
            // type. Heterogeneous lists where text and number variants share a
            // print form (e.g. `Number(1)` and `Text("1")`) are now correctly
            // kept distinct — they were merged into a single key in the
            // pre-MapKey era.
            let map_key = match item {
                Value::Text(s) => MapKey::Text((**s).clone()),
                Value::Number(n) => {
                    if !n.is_finite() {
                        return Err(RuntimeError::new(
                            "ILO-R009",
                            format!("frq: numeric element must be finite, got {n}"),
                        ));
                    }
                    MapKey::Int(n.floor() as i64)
                }
                Value::Bool(b) => MapKey::Text(format!("{b}")),
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!(
                            "frq: list elements must be text, number, or bool, got {:?}",
                            other
                        ),
                    ));
                }
            };
            *counts.entry(map_key).or_insert(0) += 1;
        }
        let map: HashMap<MapKey, Value> = counts
            .into_iter()
            .map(|(k, v)| (k, Value::Number(v as f64)))
            .collect();
        return Ok(Value::Map(Arc::new(map)));
    }
    if builtin == Some(Builtin::Transpose) && args.len() == 1 {
        let rows = match &args[0] {
            Value::List(l) => l,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("transpose: arg must be a list of lists, got {:?}", other),
                ));
            }
        };
        if rows.is_empty() {
            return Ok(Value::List(Arc::new(vec![])));
        }
        let mut row_data: Vec<&Vec<Value>> = Vec::with_capacity(rows.len());
        let mut ncols: Option<usize> = None;
        for row in rows.iter() {
            match row {
                Value::List(r) => {
                    match ncols {
                        None => ncols = Some(r.len()),
                        Some(n) if n != r.len() => {
                            return Err(RuntimeError::new(
                                "ILO-R009",
                                format!(
                                    "transpose: ragged rows (expected {n} cols, got {})",
                                    r.len()
                                ),
                            ));
                        }
                        _ => {}
                    }
                    row_data.push(r);
                }
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("transpose: rows must be lists, got {:?}", other),
                    ));
                }
            }
        }
        let ncols = ncols.unwrap_or(0);
        let mut result: Vec<Value> = Vec::with_capacity(ncols);
        for j in 0..ncols {
            let mut col: Vec<Value> = Vec::with_capacity(row_data.len());
            for r in &row_data {
                col.push(r[j].clone());
            }
            result.push(Value::List(Arc::new(col)));
        }
        return Ok(Value::List(Arc::new(result)));
    }
    if builtin == Some(Builtin::Matmul) && args.len() == 2 {
        let a_rows = match &args[0] {
            Value::List(l) => l,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("matmul: first arg must be a list of lists, got {:?}", other),
                ));
            }
        };
        let b_rows = match &args[1] {
            Value::List(l) => l,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "matmul: second arg must be a list of lists, got {:?}",
                        other
                    ),
                ));
            }
        };
        return matmul_run(a_rows, b_rows);
    }
    if builtin == Some(Builtin::Matvec) && args.len() == 2 {
        // Out-of-line helper to keep this arm's frame off the giant
        // `call_function` stack frame. Same pattern as #506 (sha2/hmac),
        // #494 (caps fields), and the lstsq extraction in #515: each
        // additional inline arm grows the dispatch frame and trips
        // `cargo nextest`'s tighter stack budget on deep recursion tests.
        return matvec_run(&args[0], &args[1]);
    }
    if builtin == Some(Builtin::Dot) && args.len() == 2 {
        let xs = match &args[0] {
            Value::List(l) => l,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("dot: first arg must be a list, got {:?}", other),
                ));
            }
        };
        let ys = match &args[1] {
            Value::List(l) => l,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("dot: second arg must be a list, got {:?}", other),
                ));
            }
        };
        if xs.len() != ys.len() {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!(
                    "dot: length mismatch (xs has {}, ys has {})",
                    xs.len(),
                    ys.len()
                ),
            ));
        }
        let mut total = 0.0_f64;
        for (x, y) in xs.iter().zip(ys.iter()) {
            match (x, y) {
                (Value::Number(a), Value::Number(b)) => total += a * b,
                _ => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        "dot: list elements must be numbers".to_string(),
                    ));
                }
            }
        }
        return Ok(Value::Number(total));
    }
    if builtin == Some(Builtin::Sum) && args.len() == 1 {
        let items = match &args[0] {
            Value::List(l) => l,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("sum: arg must be a list, got {:?}", other),
                ));
            }
        };
        let mut total = 0.0_f64;
        for item in items.iter() {
            match item {
                Value::Number(n) => total += n,
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("sum: list elements must be numbers, got {:?}", other),
                    ));
                }
            }
        }
        return Ok(Value::Number(total));
    }
    if builtin == Some(Builtin::Prod) && args.len() == 1 {
        let items = match &args[0] {
            Value::List(l) => l,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("prod: arg must be a list, got {:?}", other),
                ));
            }
        };
        let mut total = 1.0_f64;
        for item in items.iter() {
            match item {
                Value::Number(n) => total *= n,
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("prod: list elements must be numbers, got {:?}", other),
                    ));
                }
            }
        }
        return Ok(Value::Number(total));
    }
    if builtin == Some(Builtin::Cumsum) && args.len() == 1 {
        let items = match &args[0] {
            Value::List(l) => l,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("cumsum: arg must be a list, got {:?}", other),
                ));
            }
        };
        let mut total = 0.0_f64;
        let mut out: Vec<Value> = Vec::with_capacity(items.len());
        for item in items.iter() {
            match item {
                Value::Number(n) => {
                    total += n;
                    out.push(Value::Number(total));
                }
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("cumsum: list elements must be numbers, got {:?}", other),
                    ));
                }
            }
        }
        return Ok(Value::List(Arc::new(out)));
    }
    if builtin == Some(Builtin::Cprod) && args.len() == 1 {
        let items = match &args[0] {
            Value::List(l) => l,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("cprod: arg must be a list, got {:?}", other),
                ));
            }
        };
        let mut total = 1.0_f64;
        let mut out: Vec<Value> = Vec::with_capacity(items.len());
        for item in items.iter() {
            match item {
                Value::Number(n) => {
                    total *= n;
                    out.push(Value::Number(total));
                }
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("cprod: list elements must be numbers, got {:?}", other),
                    ));
                }
            }
        }
        return Ok(Value::List(Arc::new(out)));
    }
    if builtin == Some(Builtin::Ewm) && args.len() == 2 {
        let a = match &args[1] {
            Value::Number(n) => *n,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("ewm: second arg a must be a number, got {:?}", other),
                ));
            }
        };
        return ewm_run(&args[0], a);
    }
    // Rolling-window reducers — rsum / ravg / rmin (n, xs).
    // Explicit per-verb guards so check-dispatch-arms measures each arm individually.
    if builtin == Some(Builtin::Rsum) && args.len() == 2 {
        let n_f = match &args[0] {
            Value::Number(n) => *n,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("rsum: first arg n must be a number, got {:?}", other),
                ));
            }
        };
        return rolling_window_run("rsum", Builtin::Rsum, n_f, &args[1]);
    }
    if builtin == Some(Builtin::Ravg) && args.len() == 2 {
        let n_f = match &args[0] {
            Value::Number(n) => *n,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("ravg: first arg n must be a number, got {:?}", other),
                ));
            }
        };
        return rolling_window_run("ravg", Builtin::Ravg, n_f, &args[1]);
    }
    if builtin == Some(Builtin::Rmin) && args.len() == 2 {
        let n_f = match &args[0] {
            Value::Number(n) => *n,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("rmin: first arg n must be a number, got {:?}", other),
                ));
            }
        };
        return rolling_window_run("rmin", Builtin::Rmin, n_f, &args[1]);
    }
    if builtin == Some(Builtin::Where) && args.len() == 3 {
        // where cond xs ys > L a — parallel-list conditional select.
        // For each i: output[i] = xs[i] if cond[i] else ys[i].
        // All three lists must have the same length.
        let cond = match &args[0] {
            Value::List(items) => items,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("where: first arg (cond) must be a list, got {:?}", other),
                ));
            }
        };
        let xs = match &args[1] {
            Value::List(items) => items,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("where: second arg (xs) must be a list, got {:?}", other),
                ));
            }
        };
        let ys = match &args[2] {
            Value::List(items) => items,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("where: third arg (ys) must be a list, got {:?}", other),
                ));
            }
        };
        return where_run(cond, xs, ys);
    }
    if builtin == Some(Builtin::Avg) && args.len() == 1 {
        let items = match &args[0] {
            Value::List(l) => l,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("avg: arg must be a list, got {:?}", other),
                ));
            }
        };
        if items.is_empty() {
            return Err(RuntimeError::new(
                "ILO-R009",
                "avg: cannot average an empty list".to_string(),
            ));
        }
        let mut total = 0.0_f64;
        for item in items.iter() {
            match item {
                Value::Number(n) => total += n,
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("avg: list elements must be numbers, got {:?}", other),
                    ));
                }
            }
        }
        return Ok(Value::Number(total / items.len() as f64));
    }
    if builtin == Some(Builtin::Median) && args.len() == 1 {
        let items = match &args[0] {
            Value::List(l) => l,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("median: arg must be a list, got {:?}", other),
                ));
            }
        };
        return median_run(items);
    }
    if builtin == Some(Builtin::Quantile) && args.len() == 2 {
        let items = match &args[0] {
            Value::List(l) => l,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("quantile: first arg must be a list, got {:?}", other),
                ));
            }
        };
        let p = match &args[1] {
            Value::Number(n) => *n,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("quantile: second arg p must be a number, got {:?}", other),
                ));
            }
        };
        return quantile_run(items, p);
    }
    if builtin == Some(Builtin::Variance) && args.len() == 1 {
        let items = match &args[0] {
            Value::List(l) => l,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("variance: arg must be a list, got {:?}", other),
                ));
            }
        };
        return variance_run(items);
    }
    if builtin == Some(Builtin::Stdev) && args.len() == 1 {
        let items = match &args[0] {
            Value::List(l) => l,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("stdev: arg must be a list, got {:?}", other),
                ));
            }
        };
        return stdev_run(items);
    }
    if builtin == Some(Builtin::Rgx) && args.len() == 2 {
        let pattern = match &args[0] {
            Value::Text(s) => s.as_str(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("rgx: first arg must be a string pattern, got {:?}", other),
                ));
            }
        };
        let input = match &args[1] {
            Value::Text(s) => s.as_str(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("rgx: second arg must be a string, got {:?}", other),
                ));
            }
        };
        return rgx_run(pattern, input);
    }
    if builtin == Some(Builtin::Rgxall) && args.len() == 2 {
        let pattern = match &args[0] {
            Value::Text(s) => s.as_str(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "rgxall: first arg must be a string pattern, got {:?}",
                        other
                    ),
                ));
            }
        };
        let input = match &args[1] {
            Value::Text(s) => s.as_str(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("rgxall: second arg must be a string, got {:?}", other),
                ));
            }
        };
        return rgxall_run(pattern, input);
    }
    if builtin == Some(Builtin::Rgxall1) && args.len() == 2 {
        let pattern = match &args[0] {
            Value::Text(s) => s.as_str(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "rgxall1: first arg must be a string pattern, got {:?}",
                        other
                    ),
                ));
            }
        };
        let input = match &args[1] {
            Value::Text(s) => s.as_str(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("rgxall1: second arg must be a string, got {:?}", other),
                ));
            }
        };
        return rgxall1_run(pattern, input);
    }
    if builtin == Some(Builtin::RgxallMulti) && args.len() == 2 {
        // rgxall-multi pats:L t line:t > L t
        //
        // For each pattern in `pats`, run rgxall1 semantics (0 groups →
        // whole matches; 1 group → capture-1 strings) and concatenate the
        // results in pattern order into a single flat list.
        //
        // This is equivalent to:
        //   flat (map (p:t>L t;rgxall1 p line) pats)
        // but saves ~20 tokens per call site. The cron-explainer and
        // historical-archeologist personas both reached for exactly this shape.
        //
        // Error conditions follow rgxall1:
        //   - non-list first arg → ILO-R009
        //   - non-text element in pats → ILO-R009
        //   - invalid regex → ILO-R009
        //   - pattern with 2+ capture groups → ILO-R009 (use rgxall per group)
        let pats = match &args[0] {
            Value::List(xs) => xs.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "rgxall-multi: first arg must be a list of patterns, got {:?}",
                        other
                    ),
                ));
            }
        };
        let input = match &args[1] {
            Value::Text(s) => s.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("rgxall-multi: second arg must be a string, got {:?}", other),
                ));
            }
        };
        return rgxall_multi_run(&pats, &input);
    }
    if builtin == Some(Builtin::Rgxsub) && args.len() == 3 {
        let pattern = match &args[0] {
            Value::Text(s) => s.as_str(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "rgxsub: first arg must be a string pattern, got {:?}",
                        other
                    ),
                ));
            }
        };
        let replacement = match &args[1] {
            Value::Text(s) => s.as_str(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "rgxsub: second arg must be a string replacement, got {:?}",
                        other
                    ),
                ));
            }
        };
        let subject = match &args[2] {
            Value::Text(s) => s.as_str(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!(
                        "rgxsub: third arg must be a string subject, got {:?}",
                        other
                    ),
                ));
            }
        };
        return rgxsub_run(pattern, replacement, subject);
    }
    if builtin == Some(Builtin::Flat) && args.len() == 1 {
        let items = match &args[0] {
            Value::List(l) => l.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("flat: arg must be a list, got {:?}", other),
                ));
            }
        };
        let mut result: Vec<Value> = Vec::new();
        for item in items.iter() {
            match item {
                Value::List(inner) => result.extend(inner.iter().cloned()),
                other => result.push(other.clone()),
            }
        }
        return Ok(Value::List(Arc::new(result)));
    }
    if builtin == Some(Builtin::Fft) && args.len() == 1 {
        let items = match &args[0] {
            Value::List(l) => l,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("fft: arg must be a list of numbers, got {:?}", other),
                ));
            }
        };
        if items.is_empty() {
            return Err(RuntimeError::new(
                "ILO-R009",
                "fft: input list must not be empty".to_string(),
            ));
        }
        let mut reals: Vec<f64> = Vec::with_capacity(items.len());
        for item in items.iter() {
            match item {
                Value::Number(n) => reals.push(*n),
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("fft: list elements must be numbers, got {:?}", other),
                    ));
                }
            }
        }
        let n = next_pow2(reals.len());
        let mut re = reals;
        re.resize(n, 0.0);
        let mut im = vec![0.0_f64; n];
        cooley_tukey(&mut re, &mut im, false);
        let result: Vec<Value> = re
            .into_iter()
            .zip(im)
            .map(|(r, i)| Value::List(Arc::new(vec![Value::Number(r), Value::Number(i)])))
            .collect();
        return Ok(Value::List(Arc::new(result)));
    }
    if builtin == Some(Builtin::Ifft) && args.len() == 1 {
        let items = match &args[0] {
            Value::List(l) => l,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("ifft: arg must be a list of pairs, got {:?}", other),
                ));
            }
        };
        return ifft_run(items);
    }

    // Sum type variant constructor: `Circle 5.0` or `red` (no payload)
    if let Some((type_name, has_payload)) = env.sum_variants.get(name).cloned() {
        return if has_payload {
            if args.len() != 1 {
                return Err(RuntimeError::new(
                    "ILO-R004",
                    format!(
                        "{name}: variant constructor expects 1 argument, got {}",
                        args.len()
                    ),
                ));
            }
            Ok(Value::Variant {
                type_name,
                tag: name.to_string(),
                payload: Some(Box::new(args.into_iter().next().unwrap())),
            })
        } else {
            if !args.is_empty() {
                return Err(RuntimeError::new(
                    "ILO-R004",
                    format!(
                        "{name}: variant constructor takes no arguments, got {}",
                        args.len()
                    ),
                ));
            }
            Ok(Value::Variant {
                type_name,
                tag: name.to_string(),
                payload: None,
            })
        };
    }

    // ── Signal/math cluster (0.13.0) ────────────────────────────────────────
    if builtin == Some(Builtin::Convolve) && args.len() == 2 {
        return run_convolve(&args[0], &args[1]);
    }
    if builtin == Some(Builtin::Searchsorted) && args.len() == 2 {
        return run_searchsorted(&args[0], &args[1]);
    }
    if builtin == Some(Builtin::Cabs) && args.len() == 1 {
        return run_cabs(&args[0]);
    }
    if builtin == Some(Builtin::Cmul) && args.len() == 2 {
        return run_cmul(&args[0], &args[1]);
    }
    // `pairwise f xs > L b` — apply binary f to each adjacent pair.
    // Has a FnRef arg so dispatches inline like map/flt rather than through a
    // top-level helper. Output length = len xs - 1; empty/singleton → [].
    if builtin == Some(Builtin::Pairwise) && args.len() == 2 {
        let fn_name = resolve_fn_ref(&args[0]).ok_or_else(|| {
            RuntimeError::new(
                "ILO-R009",
                format!(
                    "pairwise: first arg must be a function reference, got {:?}",
                    args[0]
                ),
            )
        })?;
        let captures = closure_captures(&args[0]);
        let items = match &args[1] {
            Value::List(l) => l.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("pairwise: second arg must be a list, got {:?}", other),
                ));
            }
        };
        if items.len() < 2 {
            return Ok(Value::List(Arc::new(vec![])));
        }
        let mut result = Vec::with_capacity(items.len() - 1);
        for i in 0..items.len() - 1 {
            let mut call_args = vec![items[i].clone(), items[i + 1].clone()];
            call_args.extend(captures.iter().cloned());
            result.push(call_function(env, &fn_name, call_args)?);
        }
        return Ok(Value::List(Arc::new(result)));
    }
    if builtin == Some(Builtin::Pdist2) && args.len() == 2 {
        return run_pdist2(&args[0], &args[1]);
    }

    // Dynamic dispatch: callee resolved to a FnRef at runtime
    // (e.g. calling a function passed as a parameter: `fn x` where fn:F n n)
    // This is handled by looking up `name` in scope within eval_expr, not here.

    let decl = env.function(name)?;
    match decl {
        Decl::Function {
            params,
            body,
            name: func_name,
            ..
        } => {
            // Trampoline: tail-position user-fn calls inside the body surface
            // as `BodyResult::TailCall { callee, args }`. We pick those up
            // here and rebind parameters in place instead of recursing into
            // Rust. Effect: a function that recurses only in tail position
            // runs to arbitrary depth on the tree interpreter without
            // touching the host call stack. VM and Cranelift backends gain
            // matching support in subsequent PRs.
            if args.len() != params.len() {
                return Err(RuntimeError::new(
                    "ILO-R004",
                    format!(
                        "{}: expected {} args, got {}",
                        name,
                        params.len(),
                        args.len()
                    ),
                ));
            }
            let saved_vars = std::mem::take(&mut env.vars);
            let saved_marks = std::mem::replace(&mut env.scope_marks, vec![0]);
            // Save and reset the defer stack for this call frame.
            let saved_defers = std::mem::take(&mut env.defer_stack);

            let mut cur_params = params;
            let mut cur_body = body;
            let mut cur_args = args;
            let mut cur_func_name = func_name;

            let body_result = loop {
                env.vars.clear();
                env.scope_marks.clear();
                env.scope_marks.push(0);
                for (param, arg) in cur_params.iter().zip(cur_args) {
                    env.define(&param.name, arg);
                }
                env.call_stack.push(cur_func_name.clone());
                let result = eval_body(env, &cur_body, true);
                env.call_stack.pop();

                match result {
                    Err(e) => break Err(e),
                    Ok(BodyResult::Value(v))
                    | Ok(BodyResult::Return(v))
                    | Ok(BodyResult::Break(v)) => break Ok(v),
                    Ok(BodyResult::Continue) => break Ok(Value::Nil),
                    Ok(BodyResult::TailCall {
                        callee,
                        args: ta_args,
                    }) => match env.function(&callee) {
                        Ok(Decl::Function {
                            params: np,
                            body: nb,
                            name: nn,
                            ..
                        }) => {
                            if ta_args.len() != np.len() {
                                break Err(RuntimeError::new(
                                    "ILO-R004",
                                    format!(
                                        "{}: expected {} args, got {}",
                                        callee,
                                        np.len(),
                                        ta_args.len()
                                    ),
                                ));
                            }
                            cur_params = np;
                            cur_body = nb;
                            cur_args = ta_args;
                            cur_func_name = nn;
                            continue;
                        }
                        _ => {
                            // Fallback: callee isn't a Decl::Function (e.g.
                            // a tool). try_synthesize_tail_call should have
                            // ruled this out at synth time, but defending
                            // against drift between synth and resolve.
                            break call_function(env, &callee, ta_args);
                        }
                    },
                }
            };

            // Run deferred expressions LIFO.  `defer` always fires; `errdefer`
            // fires only on the error path.  The error path covers both:
            //   1. A Rust-level RuntimeError (e.g. ILO-R004 arity mismatch).
            //   2. The function returning a `Value::Err(...)` ilo error value
            //      (e.g. `ret ^"msg"` or a `!`-propagated error).
            // Defer errors are silently ignored so a failing defer doesn't
            // hide the original error.
            let is_error = matches!(&body_result, Err(_) | Ok(Value::Err(_)));
            let frame_defers = std::mem::take(&mut env.defer_stack);
            for (defer_expr, defer_kind) in frame_defers.into_iter().rev() {
                let should_run = match defer_kind {
                    crate::ast::DeferKind::Always => true,
                    crate::ast::DeferKind::OnError => is_error,
                };
                if should_run {
                    let _ = eval_expr(env, &defer_expr);
                }
            }

            env.vars = saved_vars;
            env.scope_marks = saved_marks;
            env.defer_stack = saved_defers;
            body_result
        }
        Decl::Tool { name, .. } => {
            if let Some(ref _provider) = env.tool_provider {
                #[cfg(feature = "tools")]
                {
                    if let Some(ref rt) = env.tokio_runtime {
                        return rt
                            .block_on(_provider.call(&name, args))
                            .map_err(|e| RuntimeError::new("ILO-R099", e.to_string()));
                    }
                }
                // No async runtime available (or `tools` feature disabled);
                // fall through to stub.
                let args_str: Vec<String> = args.iter().map(|a| format!("{a}")).collect();
                eprintln!("tool call (no runtime): {}({})", name, args_str.join(", "));
                Ok(Value::Ok(Box::new(Value::Nil)))
            } else {
                // No provider: stub behaviour (matches original)
                let args_str: Vec<String> = args.iter().map(|a| format!("{a}")).collect();
                eprintln!("tool call: {}({})", name, args_str.join(", "));
                Ok(Value::Ok(Box::new(Value::Nil)))
            }
        }
        Decl::TypeDef { .. } => Err(RuntimeError::new(
            "ILO-R004",
            format!("{} is a type, not callable", name),
        )),
        Decl::Alias { .. } => Err(RuntimeError::new(
            "ILO-R004",
            format!("{} is a type alias, not callable", name),
        )),
        Decl::Use { .. } => Err(RuntimeError::new(
            "ILO-R002",
            format!("{} is an unresolved import", name),
        )),
        Decl::Error { .. } => Err(RuntimeError::new(
            "ILO-R002",
            format!("{} failed to parse", name),
        )),
        Decl::SumType { .. } => Err(RuntimeError::new(
            "ILO-R002",
            format!("{} is a sum type, not a callable function", name),
        )),
    }
}

fn value_to_json(val: &Value) -> serde_json::Value {
    match val {
        Value::Number(n) => {
            if n.fract() == 0.0 && n.abs() < 1e15 {
                serde_json::Value::Number(serde_json::Number::from(*n as i64))
            } else {
                serde_json::Number::from_f64(*n)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null)
            }
        }
        Value::Text(s) => serde_json::Value::String((**s).clone()),
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Nil => serde_json::Value::Null,
        Value::List(items) => serde_json::Value::Array(items.iter().map(value_to_json).collect()),
        Value::Record { fields, .. } => {
            let map: serde_json::Map<String, serde_json::Value> = fields
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            serde_json::Value::Object(map)
        }
        Value::Map(m) => {
            let map: serde_json::Map<String, serde_json::Value> = m
                .iter()
                .map(|(k, v)| (k.to_display_string(), value_to_json(v)))
                .collect();
            serde_json::Value::Object(map)
        }
        Value::Ok(inner) => value_to_json(inner),
        Value::Err(inner) => value_to_json(inner),
        Value::FnRef(name) => serde_json::Value::String(format!("<fn:{}>", name)),
        Value::Closure { fn_name, .. } => {
            serde_json::Value::String(format!("<closure:{}>", fn_name))
        }
        Value::Variant { tag, payload, .. } => {
            let mut map = serde_json::Map::new();
            map.insert("tag".to_string(), serde_json::Value::String(tag.clone()));
            if let Some(p) = payload {
                map.insert("payload".to_string(), value_to_json(p));
            }
            serde_json::Value::Object(map)
        }
        Value::LazyStdinLines(_) => serde_json::Value::String("<stdin-lines>".to_string()),
        Value::World {
            net,
            read,
            write,
            run,
        } => {
            let mut map = serde_json::Map::with_capacity(4);
            map.insert("net".to_string(), serde_json::Value::Bool(*net));
            map.insert("read".to_string(), serde_json::Value::Bool(*read));
            map.insert("write".to_string(), serde_json::Value::Bool(*write));
            map.insert("run".to_string(), serde_json::Value::Bool(*run));
            serde_json::Value::Object(map)
        }
    }
}

fn serde_json_to_value(v: serde_json::Value) -> Value {
    match v {
        serde_json::Value::Object(map) => {
            let fields: HashMap<String, Value> = map
                .into_iter()
                .map(|(k, v)| (k, serde_json_to_value(v)))
                .collect();
            Value::Record {
                type_name: "json".to_string(),
                fields,
            }
        }
        serde_json::Value::Array(arr) => {
            Value::List(Arc::new(arr.into_iter().map(serde_json_to_value).collect()))
        }
        serde_json::Value::String(s) => Value::Text(Arc::new(s)),
        serde_json::Value::Number(n) => Value::Number(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Null => Value::Nil,
    }
}

/// If `expr` is a direct `Expr::Call name args` with no auto-unwrap, the args
/// evaluate successfully, and `name` resolves to a user-defined function in
/// `env`, evaluate the args and return `(name, arg_values)` so the caller can
/// synthesise a `BodyResult::TailCall`.
///
/// Returns `None` when the expression isn't shaped like a TCO-eligible call,
/// in which case the caller falls back to regular `eval_expr`. Returns
/// `Some(Err(_))` if arg evaluation itself fails (e.g. nested call errored or
/// propagated via `!`); propagating that error rather than swallowing it
/// preserves the no-TCO semantics for failure paths.
///
/// Constraints (intentionally narrow for the tree-interpreter trampoline):
/// - `unwrap` must be `None` — `!`/`!!` inspect the call result before
///   propagating, so they can't be TCO'd without re-checking inside the
///   trampoline (left for a follow-up).
/// - `function` must be a direct user-fn name, not a scope-bound FnRef or
///   Closure. FnRef-via-scope tail calls remain on the host stack (rare in
///   practice; the common `fac n=...fac -n 1` pattern hits the direct path).
/// - Tools (`Decl::Tool`) intentionally do not TCO — they're an effect
///   boundary, not a recursive computation.
///
/// `#[inline(never)]` so the helper's frame stays separate from
/// `eval_stmt`'s. `eval_stmt` is on the hot path for every statement; if
/// this helper inlined, its `Vec<Value>` arg-buffer would bloat every
/// `eval_stmt` frame and tip moderately-deep non-tail recursion (e.g.
/// `fib 10`'s 177 nested frames) into stack overflow on tight-limit CI
/// builds.
#[inline(never)]
fn try_synthesize_tail_call(env: &mut Env, expr: &Expr) -> Option<Result<(String, Vec<Value>)>> {
    let Expr::Call {
        function,
        args,
        unwrap,
    } = expr
    else {
        return None;
    };
    if unwrap.is_any() {
        return None;
    }
    // Reject if the callee name is shadowed in local scope by a non-fn
    // binding, or by a FnRef/Closure (those use the dynamic-dispatch path
    // in eval_expr and aren't worth duplicating here).
    if env.vars.iter().rev().any(|(k, _)| k == function.as_str()) {
        return None;
    }
    // Resolve callee as a user-fn. Builtins, tools, type defs etc. fall back
    // to the normal call path. Builtin shadowing of a user fn name isn't a
    // thing in ilo (verifier rejects it), but we still check function() first
    // — the trampoline only handles Decl::Function payloads.
    let decl = env.functions.get(function.as_str())?;
    if !matches!(decl, Decl::Function { .. }) {
        return None;
    }
    // Evaluate args. Any error here is surfaced as Some(Err(_)) so the
    // caller propagates it the same way eval_expr would.
    let mut arg_vals = Vec::with_capacity(args.len());
    for arg in args {
        match eval_expr(env, arg) {
            Ok(v) => arg_vals.push(v),
            Err(e) => return Some(Err(e)),
        }
    }
    Some(Ok((function.clone(), arg_vals)))
}

fn eval_body(env: &mut Env, stmts: &[Spanned<Stmt>], is_tail: bool) -> Result<BodyResult> {
    let mut last = Value::Nil;
    let n = stmts.len();
    for (i, spanned) in stmts.iter().enumerate() {
        // Tail position only propagates to the LAST statement. Earlier
        // statements are not in tail position by definition.
        let stmt_is_tail = is_tail && i + 1 == n;
        // Update current span so sub-expression trace events can report a line.
        CURRENT_STMT_SPAN.with(|s| *s.borrow_mut() = spanned.span);
        match eval_stmt(env, &spanned.node, stmt_is_tail) {
            Ok(Some(BodyResult::Return(v))) => {
                fire_trace_event(env, spanned, v.clone());
                return Ok(BodyResult::Return(v));
            }
            Ok(Some(BodyResult::Break(v))) => {
                fire_trace_event(env, spanned, v.clone());
                return Ok(BodyResult::Break(v));
            }
            Ok(Some(BodyResult::Continue)) => {
                fire_trace_event(env, spanned, Value::Nil);
                return Ok(BodyResult::Continue);
            }
            Ok(Some(BodyResult::TailCall { callee, args })) => {
                fire_trace_event(env, spanned, Value::Nil);
                return Ok(BodyResult::TailCall { callee, args });
            }
            Ok(Some(BodyResult::Value(v))) => {
                fire_trace_event(env, spanned, v.clone());
                last = v;
            }
            Ok(None) => {
                // For Let statements the assigned value is available in env.
                // Use it as the result so the trace shows what was bound.
                let result = if let Stmt::Let { name, .. } = &spanned.node {
                    env.vars
                        .iter()
                        .rev()
                        .find(|(k, _)| k == name)
                        .map(|(_, v)| v.clone())
                        .unwrap_or(Value::Nil)
                } else {
                    Value::Nil
                };
                fire_trace_event(env, spanned, result);
            }
            Err(mut e) => {
                // Auto-unwrap propagation: convert to early return
                if let Some(val) = e.propagate_value.take() {
                    return Ok(BodyResult::Return(*val));
                }
                if e.span.is_none() {
                    e.span = Some(spanned.span);
                }
                if e.call_stack.is_empty() {
                    e.call_stack = env.call_stack.clone();
                }
                return Err(e);
            }
        }
    }
    Ok(BodyResult::Value(last))
}

/// Fire the TRACE_HOOK (if installed) after a statement executes.
/// Extracts line number from the span and collects current bindings.
#[inline]
fn fire_trace_event(env: &Env, spanned: &Spanned<Stmt>, result: Value) {
    let has_hook = TRACE_HOOK.with(|h| h.borrow().is_some());
    if !has_hook {
        return;
    }

    let span = spanned.span;

    // Resolve 1-based line number from the span.
    let (line, stmt_text) = TRACE_SOURCE.with(|src| {
        if let Some(ref source) = *src.borrow() {
            let sm = crate::ast::SourceMap::new(source);
            let (line, _col) = sm.lookup(span.start);
            let text = sm.line_text(source, line).trim().to_string();
            (line, text)
        } else {
            (0, String::new())
        }
    });

    // Snapshot current bindings.
    let bindings: Vec<(String, Value)> = env
        .vars
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    TRACE_HOOK.with(|h| {
        if let Some(ref mut hook) = *h.borrow_mut() {
            hook(TraceEvent {
                line,
                stmt: stmt_text,
                bindings,
                result,
            });
        }
    });
}

/// Fire the EXPR_TRACE_HOOK (if installed) after a sub-expression evaluates.
/// `span` is the byte span of the expression; `expr_text` is the source slice.
#[inline]
fn fire_expr_trace_event(expr: &Expr, span: Span, result: &Value) {
    let has_hook = EXPR_TRACE_HOOK.with(|h| h.borrow().is_some());
    if !has_hook {
        return;
    }

    // Collect Ref names touched by this expression.
    let refs = collect_refs(expr);

    let (line, expr_text) = TRACE_SOURCE.with(|src| {
        if let Some(ref source) = *src.borrow() {
            let sm = crate::ast::SourceMap::new(source);
            let (line, _col) = sm.lookup(span.start);
            // Use the span slice when available, else fall back to line text.
            let text = source
                .get(span.start..span.end)
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| sm.line_text(source, line).trim().to_string());
            (line, text)
        } else {
            (0, String::new())
        }
    });

    EXPR_TRACE_HOOK.with(|h| {
        if let Some(ref mut hook) = *h.borrow_mut() {
            hook(ExprTraceEvent {
                line,
                expr: expr_text,
                refs,
                result: result.clone(),
            });
        }
    });
}

/// Collect all `Ref` names reachable from an expression (shallow, non-recursive
/// into function bodies / closures).
fn collect_refs(expr: &Expr) -> Vec<String> {
    let mut out = Vec::new();
    collect_refs_inner(expr, &mut out);
    out
}

fn collect_refs_inner(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::Ref(name) => out.push(name.clone()),
        Expr::Field { object, .. } => collect_refs_inner(object, out),
        Expr::Index { object, .. } => collect_refs_inner(object, out),
        Expr::Call { args, .. } => {
            for a in args {
                collect_refs_inner(a, out);
            }
        }
        Expr::BinOp { left, right, .. } => {
            collect_refs_inner(left, out);
            collect_refs_inner(right, out);
        }
        Expr::UnaryOp { operand, .. } => collect_refs_inner(operand, out),
        _ => {}
    }
}

/// If `value` is the self-rebind accumulator shape `name = mset name k v`,
/// return `Some((key_expr, val_expr))`. Returns `None` for any other shape
/// (different target name, nested calls, unwrap form, wrong arity), which
/// then falls through to the general assignment path.
fn match_self_rebind_mset<'a>(name: &str, value: &'a Expr) -> Option<(&'a Expr, &'a Expr)> {
    if let Expr::Call {
        function,
        args,
        unwrap,
    } = value
        && !unwrap.is_any()
        && function == "mset"
        && args.len() == 3
        && let Expr::Ref(arg_name) = &args[0]
        && arg_name == name
    {
        return Some((&args[1], &args[2]));
    }
    None
}

/// Fast-path executor for the self-rebind shape. The caller has already taken
/// the previous binding out of env, leaving `Value::Nil` in its place. We
/// evaluate the key and value expressions, then drive `mset` with the moved
/// `prev` so the Arc has refcount=1 and `Arc::make_mut` mutates in place.
fn eval_self_rebind_mset(
    env: &mut Env,
    key_expr: &Expr,
    val_expr: &Expr,
    prev: Value,
) -> Result<Value> {
    let key_val = eval_expr(env, key_expr)?;
    let val_val = eval_expr(env, val_expr)?;
    let args = vec![prev, key_val, val_val];
    call_function(env, "mset", args)
}

/// Returns `true` if `expr` contains a `Ref(name)` anywhere in its subtree.
/// Used by the self-rebind peepholes to detect aliasing: if the RHS reads the
/// same binding we're about to take, the fast path would observe Nil and we
/// must fall back to the general (cloning) path.
fn expr_refers_to(name: &str, expr: &Expr) -> bool {
    match expr {
        Expr::Ref(n) => n == name,
        Expr::Field { object, .. } => expr_refers_to(name, object),
        Expr::Index { object, .. } => expr_refers_to(name, object),
        Expr::Call { args, .. } => args.iter().any(|a| expr_refers_to(name, a)),
        Expr::BinOp { left, right, .. } => {
            expr_refers_to(name, left) || expr_refers_to(name, right)
        }
        Expr::UnaryOp { operand, .. } => expr_refers_to(name, operand),
        Expr::Ok(inner) | Expr::Err(inner) => expr_refers_to(name, inner),
        Expr::List(items) => items.iter().any(|e| expr_refers_to(name, e)),
        Expr::Record { fields, .. } | Expr::AnonRecord { fields } => {
            fields.iter().any(|(_, e)| expr_refers_to(name, e))
        }
        // Conservative: assume Match arms might reference `name`. Falls back
        // to the general path, which is correct (just slower) in the rare
        // case where a self-rebind RHS is wrapped in a match.
        Expr::Match { .. } => true,
        Expr::NilCoalesce { value, default } => {
            expr_refers_to(name, value) || expr_refers_to(name, default)
        }
        Expr::With { object, updates } => {
            expr_refers_to(name, object) || updates.iter().any(|(_, e)| expr_refers_to(name, e))
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_refers_to(name, condition)
                || expr_refers_to(name, then_expr)
                || expr_refers_to(name, else_expr)
        }
        Expr::MakeClosure { captures, .. } => captures.iter().any(|c| expr_refers_to(name, c)),
        Expr::Todo(inner) | Expr::Panic(inner) => expr_refers_to(name, inner),
        Expr::Literal(_) => false,
    }
}

/// If `value` is the self-rebind list-append shape `name = +=name v`, return
/// `Some(val_expr)`. Returns `None` for any other shape (different target,
/// different operator, wrong operand order, or RHS that aliases `name`),
/// which falls through to the general path.
fn match_self_rebind_append<'a>(name: &str, value: &'a Expr) -> Option<&'a Expr> {
    if let Expr::BinOp { op, left, right } = value
        && matches!(op, BinOp::Append)
        && let Expr::Ref(left_name) = left.as_ref()
        && left_name == name
        && !expr_refers_to(name, right)
    {
        return Some(right.as_ref());
    }
    None
}

/// Fast-path executor for `xs = +=xs v`. Caller has taken the previous binding
/// out of env (leaving Value::Nil), so the Arc<Vec<Value>> in `prev` is the
/// sole reference. `Arc::make_mut` then mutates the inner Vec in place,
/// turning the classic `xs = +=xs v` loop accumulator from O(n²) to O(n)
/// amortised on the tree-walker.
fn eval_self_rebind_append(env: &mut Env, val_expr: &Expr, prev: Value) -> Result<Value> {
    let val_val = eval_expr(env, val_expr)?;
    match prev {
        Value::List(mut items) => {
            let inner = Arc::make_mut(&mut items);
            inner.push(val_val);
            Ok(Value::List(items))
        }
        // Non-list `prev` is impossible in well-typed code, but if it occurs
        // (e.g. the binding was previously nil) we surface the same error as
        // the general BinOp::Append path would.
        other => Err(RuntimeError::new(
            "ILO-R004",
            format!(
                "unsupported operation: {:?} on {:?} and {:?}",
                BinOp::Append,
                other,
                val_val
            ),
        )),
    }
}

/// If `value` is the self-rebind list/text-concat shape `name = name + rhs`,
/// return `Some(rhs_expr)`. Drives both the List and Text branches of
/// `eval_self_rebind_concat`. Returns `None` for any other shape, including
/// any case where the rhs references `name` (e.g. `s = s + s` self-concat),
/// which would observe Nil after the fast path takes the binding.
fn match_self_rebind_concat<'a>(name: &str, value: &'a Expr) -> Option<&'a Expr> {
    if let Expr::BinOp { op, left, right } = value
        && matches!(op, BinOp::Add)
        && let Expr::Ref(left_name) = left.as_ref()
        && left_name == name
        && !expr_refers_to(name, right)
    {
        return Some(right.as_ref());
    }
    None
}

/// Fast-path executor for `xs = xs + ys`. Caller has taken the previous
/// binding out of env. If both sides are lists, we use `Arc::make_mut` on the
/// prev (which now has refcount=1) and `extend` from ys, again giving O(n)
/// amortised behaviour for repeated concatenation. If both sides are text,
/// we do the same trick with `Arc::make_mut` and `push_str` to fold O(n^2)
/// string-accumulator loops to O(n) amortised. If the values aren't both
/// lists or both text (e.g. numeric add), we fall back to `apply_binop` so
/// semantics match the general path exactly.
fn eval_self_rebind_concat(env: &mut Env, rhs_expr: &Expr, prev: Value) -> Result<Value> {
    let rhs = eval_expr(env, rhs_expr)?;
    match (prev, rhs) {
        (Value::List(mut items), Value::List(other)) => {
            let inner = Arc::make_mut(&mut items);
            inner.extend(other.iter().cloned());
            Ok(Value::List(items))
        }
        (Value::Text(mut s), Value::Text(other)) => {
            // `match_self_rebind_concat` already rejects RHSes that reference
            // `name`, so `prev` and `other` cannot be the same Arc here. That
            // means `Arc::make_mut(&mut s)` is free to mutate in place (the
            // env's binding was taken to Nil, so refcount=1) and reading
            // `other` after the mutation observes the unchanged source.
            let inner = Arc::make_mut(&mut s);
            inner.push_str(&other);
            Ok(Value::Text(s))
        }
        (prev, rhs) => eval_binop(&BinOp::Add, &prev, &rhs),
    }
}

/// Run and remove all block-scope defers that were pushed since `saved_len`.
///
/// Called at every exit point of a block (normal, break, continue, return).
/// `is_error` mirrors the function-scope defer semantics: `errdefer` fires
/// only when true.  Defer errors are silently ignored so a failing defer
/// doesn't mask the original result.
fn run_block_defers(env: &mut Env, saved_len: usize, is_error: bool) {
    // Drain the entries added since we entered the block (LIFO order).
    let block_defers: Vec<_> = env.defer_stack.drain(saved_len..).rev().collect();
    for (defer_expr, defer_kind) in block_defers {
        let should_run = match defer_kind {
            crate::ast::DeferKind::Always => true,
            crate::ast::DeferKind::OnError => is_error,
        };
        if should_run {
            let _ = eval_expr(env, &defer_expr);
        }
    }
}

fn eval_stmt(env: &mut Env, stmt: &Stmt, is_tail: bool) -> Result<Option<BodyResult>> {
    match stmt {
        Stmt::Let { name, value } => {
            // Peephole: `m = mset m k v` self-rebind. Drop env's binding to Nil
            // before evaluating the RHS so the Arc<HashMap> inside `args[0]`
            // becomes the sole reference. `Arc::make_mut` then mutates in place
            // instead of cloning, giving O(n) amortised accumulator behaviour
            // on the tree-walker (mirrors VM compiler peephole from PR #249).
            if let Some((key_expr, val_expr)) = match_self_rebind_mset(name, value)
                && let Some(prev) = env.take(name)
            {
                // `take` left Value::Nil in env's slot so the Arc<HashMap>
                // moved into `prev` is the sole reference (refcount=1).
                // `Arc::make_mut` inside the mset builtin then mutates the
                // HashMap in place rather than cloning, giving O(n)
                // amortised behaviour for the `m=mset m k v` accumulator.
                //
                // Cloning `prev` for error-recovery would defeat the
                // refcount=1 invariant, so on Err we leave Nil in the
                // slot. This is safe: errors here propagate to the
                // function boundary unconditionally (ilo has no
                // catch/recover form), so user code never observes the
                // intermediate Nil.
                let val = eval_self_rebind_mset(env, key_expr, val_expr, prev)?;
                env.set(name, val);
                return Ok(None);
            }
            // Peephole: `xs = +=xs v` self-rebind list append. Same trick as
            // the Map mset peephole: take prev so Arc<Vec<Value>> refcount=1,
            // then `Arc::make_mut` mutates the inner Vec in place. Turns the
            // classic accumulator loop from O(n²) (clone-per-push) to O(n)
            // amortised. Phase 2b.2 of the RC-aware mutation rollout.
            if let Some(val_expr) = match_self_rebind_append(name, value)
                && let Some(prev) = env.take(name)
            {
                let val = eval_self_rebind_append(env, val_expr, prev)?;
                env.set(name, val);
                return Ok(None);
            }
            // Peephole: `xs = xs + ys` self-rebind list concat. Mirrors the
            // append peephole but extends from the rhs list. Falls back to the
            // general apply_binop path if either side isn't a list (so numeric
            // `x = x + y` works unchanged).
            if let Some(rhs_expr) = match_self_rebind_concat(name, value)
                && let Some(prev) = env.take(name)
            {
                let val = eval_self_rebind_concat(env, rhs_expr, prev)?;
                env.set(name, val);
                return Ok(None);
            }
            // `_=expr` — explicit discard bind. Evaluate for side effects only;
            // do not allocate a slot for `_` (it's a sigil, not a real binding).
            if name == "_" {
                eval_expr(env, value)?;
                return Ok(None);
            }
            let val = eval_expr(env, value)?;
            env.set(name, val);
            Ok(None)
        }
        Stmt::Destructure { bindings, value } => {
            let val = eval_expr(env, value)?;
            match val {
                Value::Record { fields, .. } => {
                    for binding in bindings {
                        let field_val = fields.get(binding).cloned().ok_or_else(|| {
                            RuntimeError::new(
                                "ILO-R005",
                                format!("no field '{}' on record", binding),
                            )
                        })?;
                        env.set(binding, field_val);
                    }
                    Ok(None)
                }
                _ => Err(RuntimeError::new(
                    "ILO-R005",
                    "destructure requires a record".to_string(),
                )),
            }
        }
        Stmt::Guard {
            condition,
            negated,
            body,
            else_body,
            braceless,
        } => {
            let cond = eval_expr(env, condition)?;
            let truth = is_truthy(&cond);
            let should_run = if *negated { !truth } else { truth };
            if let Some(else_b) = else_body {
                // Ternary: cond{then}{else} — produces value, no early
                // return. The chosen branch inherits the outer tail
                // position: if the ternary is the last stmt of a function
                // body, both branches are in tail position.
                let chosen = if should_run { body } else { else_b };
                env.push_scope();
                let defer_mark = env.defer_stack.len();
                let result = eval_body(env, chosen, is_tail);
                let is_err = matches!(result, Err(_) | Ok(BodyResult::Return(_)));
                run_block_defers(env, defer_mark, is_err);
                env.pop_scope();
                match result? {
                    BodyResult::Break(v) => Ok(Some(BodyResult::Break(v))),
                    BodyResult::Continue => Ok(Some(BodyResult::Continue)),
                    BodyResult::TailCall { callee, args } => {
                        Ok(Some(BodyResult::TailCall { callee, args }))
                    }
                    BodyResult::Value(v) | BodyResult::Return(v) => Ok(Some(BodyResult::Value(v))),
                }
            } else if should_run && *braceless {
                // Braceless guard `cond expr`: early return from the
                // enclosing function. The body is always in tail position
                // relative to the function — the value it produces becomes
                // the function's return, so a tail call inside trampolines.
                env.push_scope();
                let defer_mark = env.defer_stack.len();
                let result = eval_body(env, body, true);
                let is_err = matches!(result, Err(_) | Ok(BodyResult::Return(_)));
                run_block_defers(env, defer_mark, is_err);
                env.pop_scope();
                match result? {
                    BodyResult::Break(v) => Ok(Some(BodyResult::Break(v))),
                    BodyResult::Continue => Ok(Some(BodyResult::Continue)),
                    BodyResult::TailCall { callee, args } => {
                        Ok(Some(BodyResult::TailCall { callee, args }))
                    }
                    BodyResult::Value(v) | BodyResult::Return(v) => Ok(Some(BodyResult::Return(v))),
                }
            } else if should_run {
                // Braced guard `cond{body}`: conditional execution. The body
                // runs but the function does NOT early-return. The body's
                // tail value becomes the surrounding body's `last` (so a
                // braced guard at the tail of a function or match arm yields
                // its body value), but execution continues to subsequent
                // statements. `ret` inside the body still propagates as
                // Return; brk/cnt still propagate to the enclosing loop.
                //
                // Inheriting `is_tail`: if the braced guard is the last stmt
                // of a function body AND its branch is taken, the branch's
                // tail call is the function's tail call. Safe to pass
                // through.
                env.push_scope();
                let defer_mark = env.defer_stack.len();
                let result = eval_body(env, body, is_tail);
                let is_err = matches!(result, Err(_) | Ok(BodyResult::Return(_)));
                run_block_defers(env, defer_mark, is_err);
                env.pop_scope();
                match result? {
                    BodyResult::Break(v) => Ok(Some(BodyResult::Break(v))),
                    BodyResult::Continue => Ok(Some(BodyResult::Continue)),
                    BodyResult::TailCall { callee, args } => {
                        Ok(Some(BodyResult::TailCall { callee, args }))
                    }
                    BodyResult::Return(v) => Ok(Some(BodyResult::Return(v))),
                    BodyResult::Value(v) => Ok(Some(BodyResult::Value(v))),
                }
            } else {
                Ok(None)
            }
        }
        Stmt::Match { subject, arms } => {
            let subj = match subject {
                Some(e) => eval_expr(env, e)?,
                None => Value::Nil,
            };
            for arm in arms {
                if let Some(bindings) = match_pattern(&arm.pattern, &subj) {
                    env.push_scope();
                    for (name, val) in bindings {
                        env.define(&name, val);
                    }
                    // Arm body inherits the match's tail position: a tail
                    // call in the taken arm of a tail-position match
                    // trampolines through.
                    let defer_mark = env.defer_stack.len();
                    let result = eval_body(env, &arm.body, is_tail);
                    let is_err = matches!(result, Err(_) | Ok(BodyResult::Return(_)));
                    run_block_defers(env, defer_mark, is_err);
                    env.pop_scope();
                    match result? {
                        BodyResult::Return(v) => return Ok(Some(BodyResult::Return(v))),
                        BodyResult::Break(v) => return Ok(Some(BodyResult::Break(v))),
                        BodyResult::Continue => return Ok(Some(BodyResult::Continue)),
                        BodyResult::TailCall { callee, args } => {
                            return Ok(Some(BodyResult::TailCall { callee, args }));
                        }
                        BodyResult::Value(v) => return Ok(Some(BodyResult::Value(v))),
                    }
                }
            }
            Ok(None)
        }
        Stmt::ForEach {
            binding,
            collection,
            body,
        } => {
            let coll = eval_expr(env, collection)?;
            match coll {
                Value::List(items) => {
                    let mut last = Value::Nil;
                    for item in items.iter().cloned() {
                        env.push_scope();
                        env.define(binding, item);
                        // Loop bodies are never in tail position: control
                        // returns to the loop header after each iteration,
                        // so a "tail call" inside a loop must materialise as
                        // a normal call. Pass `false`.
                        let defer_mark = env.defer_stack.len();
                        let result = eval_body(env, body, false);
                        let is_err = matches!(result, Err(_) | Ok(BodyResult::Return(_)));
                        run_block_defers(env, defer_mark, is_err);
                        env.pop_scope();
                        match result? {
                            BodyResult::Return(v) => {
                                return Ok(Some(BodyResult::Return(v)));
                            }
                            BodyResult::Break(v) => {
                                last = v;
                                break;
                            }
                            BodyResult::Continue => continue,
                            BodyResult::TailCall { .. } => {
                                // Unreachable: loop body is_tail = false, so
                                // try_synthesize_tail_call is never invoked
                                // in this branch. Fall through with Nil to
                                // keep the match exhaustive without panic.
                                unreachable!("TailCall escaping non-tail loop body");
                            }
                            BodyResult::Value(v) => last = v,
                        }
                    }
                    Ok(Some(BodyResult::Value(last)))
                }
                // `for-line stdin` produces a lazy stdin iterator.
                // We read one line at a time so the loop can process
                // unbounded streams (e.g. `tail -f`) without buffering.
                // Partial trailing lines at EOF are emitted unchanged.
                // I/O errors terminate the loop via RuntimeError.
                Value::LazyStdinLines(handle) => {
                    let mut last = Value::Nil;
                    loop {
                        let line = handle.next_line();
                        match line {
                            None => break,
                            Some(Err(e)) => {
                                return Err(RuntimeError::new(
                                    "ILO-R012",
                                    format!("for-line: stdin read error: {}", e),
                                ));
                            }
                            Some(Ok(s)) => {
                                env.push_scope();
                                env.define(binding, Value::Text(Arc::new(s)));
                                let result = eval_body(env, body, false);
                                env.pop_scope();
                                match result? {
                                    BodyResult::Return(v) => {
                                        return Ok(Some(BodyResult::Return(v)));
                                    }
                                    BodyResult::Break(v) => {
                                        last = v;
                                        break;
                                    }
                                    BodyResult::Continue => continue,
                                    BodyResult::TailCall { .. } => {
                                        unreachable!("TailCall escaping non-tail loop body");
                                    }
                                    BodyResult::Value(v) => last = v,
                                }
                            }
                        }
                    }
                    Ok(Some(BodyResult::Value(last)))
                }
                _ => Err(RuntimeError::new("ILO-R007", "foreach requires a list")),
            }
        }
        Stmt::ForRange {
            binding,
            start,
            end,
            step,
            body,
        } => {
            let start_val = eval_expr(env, start)?;
            let end_val = eval_expr(env, end)?;
            let s = match start_val {
                Value::Number(n) => n as i64,
                _ => {
                    return Err(RuntimeError::new(
                        "ILO-R007",
                        "range start must be a number",
                    ));
                }
            };
            let e = match end_val {
                Value::Number(n) => n as i64,
                _ => return Err(RuntimeError::new("ILO-R007", "range end must be a number")),
            };
            let st: i64 = if let Some(step_expr) = step {
                match eval_expr(env, step_expr)? {
                    Value::Number(n) => n as i64,
                    _ => return Err(RuntimeError::new("ILO-R007", "range step must be a number")),
                }
            } else {
                1
            };
            let mut last = Value::Nil;
            let mut i = s;
            while i < e {
                env.push_scope();
                env.define(binding, Value::Number(i as f64));
                // Range body is not in tail position; see ForEach above.
                let defer_mark = env.defer_stack.len();
                let result = eval_body(env, body, false);
                let is_err = matches!(result, Err(_) | Ok(BodyResult::Return(_)));
                run_block_defers(env, defer_mark, is_err);
                env.pop_scope();
                match result? {
                    BodyResult::Return(v) => {
                        return Ok(Some(BodyResult::Return(v)));
                    }
                    BodyResult::Break(v) => {
                        last = v;
                        break;
                    }
                    BodyResult::Continue => {
                        i += st;
                        continue;
                    }
                    BodyResult::TailCall { .. } => {
                        unreachable!("TailCall escaping non-tail range body");
                    }
                    BodyResult::Value(v) => last = v,
                }
                i += st;
            }
            Ok(Some(BodyResult::Value(last)))
        }
        Stmt::While { condition, body } => {
            let mut last = Value::Nil;
            loop {
                let cond = eval_expr(env, condition)?;
                if !is_truthy(&cond) {
                    break;
                }
                // While body is not in tail position; see ForEach above.
                let defer_mark = env.defer_stack.len();
                let result = eval_body(env, body, false);
                let is_err = matches!(result, Err(_) | Ok(BodyResult::Return(_)));
                run_block_defers(env, defer_mark, is_err);
                match result? {
                    BodyResult::Return(v) => {
                        return Ok(Some(BodyResult::Return(v)));
                    }
                    BodyResult::Break(v) => {
                        last = v;
                        break;
                    }
                    BodyResult::Continue => continue,
                    BodyResult::TailCall { .. } => {
                        unreachable!("TailCall escaping non-tail while body");
                    }
                    BodyResult::Value(v) => last = v,
                }
            }
            Ok(Some(BodyResult::Value(last)))
        }
        Stmt::Return(expr) => eval_return_stmt(env, expr),
        Stmt::Break(expr) => {
            let val = match expr {
                Some(e) => eval_expr(env, e)?,
                None => Value::Nil,
            };
            Ok(Some(BodyResult::Break(val)))
        }
        Stmt::Continue => Ok(Some(BodyResult::Continue)),
        Stmt::Defer { expr, kind } => {
            // Register the cleanup expression onto the per-frame defer stack.
            // Execution happens at function exit (LIFO), handled by call_function.
            env.defer_stack.push((expr.clone(), *kind));
            Ok(None)
        }
        Stmt::Expr(expr) => {
            // Tail context: dispatch via the helper so the TailCall
            // synthesis locals (Option<Result<(String, Vec<Value>)>>) stay
            // out of eval_stmt's frame on the non-tail hot path.
            if is_tail {
                eval_tail_expr_stmt(env, expr)
            } else {
                let val = eval_expr(env, expr)?;
                Ok(Some(BodyResult::Value(val)))
            }
        }
    }
}

/// Tail-position `ret expr` handler. Extracted from `eval_stmt`'s match
/// arm so the TailCall synth's locals (Option<Result<(String, Vec<Value>)>>,
/// ~56 bytes) stay out of `eval_stmt`'s frame on the non-tail hot path.
/// Matters for moderately-deep non-tail recursion on debug builds with
/// tight test-thread stacks (~2MB): every saved byte per eval_stmt frame
/// multiplies across hundreds of nested frames.
#[inline(never)]
fn eval_return_stmt(env: &mut Env, expr: &Expr) -> Result<Option<BodyResult>> {
    if let Some(result) = try_synthesize_tail_call(env, expr) {
        let (callee, args) = result?;
        return Ok(Some(BodyResult::TailCall { callee, args }));
    }
    let val = eval_expr(env, expr)?;
    Ok(Some(BodyResult::Return(val)))
}

/// Tail-position bare-expression-statement handler. Same frame-isolation
/// rationale as `eval_return_stmt`. Only reached when `eval_stmt`'s
/// `is_tail` arg is true (last stmt of a body in tail position).
#[inline(never)]
fn eval_tail_expr_stmt(env: &mut Env, expr: &Expr) -> Result<Option<BodyResult>> {
    if let Some(result) = try_synthesize_tail_call(env, expr) {
        let (callee, args) = result?;
        return Ok(Some(BodyResult::TailCall { callee, args }));
    }
    let val = eval_expr(env, expr)?;
    Ok(Some(BodyResult::Value(val)))
}

fn eval_expr(env: &mut Env, expr: &Expr) -> Result<Value> {
    match expr {
        Expr::Literal(lit) => Ok(eval_literal(lit)),
        Expr::Ref(name) => env.get(name),
        Expr::Field {
            object,
            field,
            safe,
        } => {
            let obj = eval_expr(env, object)?;
            if *safe && matches!(obj, Value::Nil) {
                return Ok(Value::Nil);
            }
            match obj {
                Value::Record { fields, .. } => match fields.get(field).cloned() {
                    Some(v) => Ok(v),
                    None if *safe => Ok(Value::Nil),
                    None => Err(RuntimeError::new(
                        "ILO-R005",
                        format!("no field '{}' on record", field),
                    )),
                },
                // World field access: .net .read .write .run → Bool
                Value::World {
                    net,
                    read,
                    write,
                    run,
                } => {
                    let v = match field.as_str() {
                        "net" => Value::Bool(net),
                        "read" => Value::Bool(read),
                        "write" => Value::Bool(write),
                        "run" => Value::Bool(run),
                        _other if *safe => Value::Nil,
                        other => {
                            return Err(RuntimeError::new(
                                "ILO-R005",
                                format!(
                                    "no field '{other}' on World (known: net, read, write, run)"
                                ),
                            ));
                        }
                    };
                    Ok(v)
                }
                // Safe access on a non-record value (list, text, number, ...)
                // returns nil to match the VM and Cranelift backends. The
                // strict `.field` path below still errors on type mismatch.
                _ if *safe => Ok(Value::Nil),
                _ => Err(RuntimeError::new(
                    "ILO-R005",
                    format!("cannot access field '{}' on non-record", field),
                )),
            }
        }
        Expr::Index {
            object,
            index,
            safe,
        } => {
            let obj = eval_expr(env, object)?;
            if *safe && matches!(obj, Value::Nil) {
                return Ok(Value::Nil);
            }
            match obj {
                Value::List(items) => items.get(*index).cloned().ok_or_else(|| {
                    RuntimeError::new(
                        "ILO-R006",
                        format!("list index {} out of bounds (len {})", index, items.len()),
                    )
                }),
                _ => Err(RuntimeError::new("ILO-R006", "index access on non-list")),
            }
        }
        Expr::Call {
            function,
            args,
            unwrap,
        } => {
            let mut arg_vals = Vec::new();
            for arg in args {
                arg_vals.push(eval_expr(env, arg)?);
            }
            // If `function` is a local variable holding a FnRef (or a Text that names a
            // function), resolve dynamically. This enables user-defined HOFs and CLI usage.
            let callee_from_scope = env
                .vars
                .iter()
                .rev()
                .find(|(k, _)| k == function.as_str())
                .map(|(_, v)| v.clone());
            // A Closure value in scope dispatches to its lifted fn with the
            // captured values appended after the user-supplied args (same shape
            // as the HOF call sites). FnRef/Text keep their original behaviour.
            let (callee, extra_captures) = match callee_from_scope {
                Some(Value::FnRef(name)) => (name, Vec::new()),
                Some(Value::Text(name)) if env.functions.contains_key(name.as_str()) => {
                    ((*name).clone(), Vec::new())
                }
                Some(Value::Closure { fn_name, captures }) => (fn_name, captures),
                _ => (function.clone(), Vec::new()),
            };
            arg_vals.extend(extra_captures);
            let result = call_function(env, &callee, arg_vals)?;
            // Fire sub-expression event for this call (depth=expr mode).
            {
                let span = CURRENT_STMT_SPAN.with(|s| *s.borrow());
                fire_expr_trace_event(expr, span, &result);
            }
            match *unwrap {
                UnwrapMode::None => Ok(result),
                UnwrapMode::Propagate => match result {
                    Value::Ok(v) => Ok(*v),
                    Value::Err(e) => Err(RuntimeError {
                        propagate_value: Some(Box::new(Value::Err(e))),
                        ..RuntimeError::new("ILO-R014", "auto-unwrap propagating Err")
                    }),
                    // Optional auto-unwrap: nil propagates as the function's return.
                    // Non-nil values pass through (Optional<T> is represented inline,
                    // so Some(v) is just v at runtime).
                    Value::Nil => Err(RuntimeError {
                        propagate_value: Some(Box::new(Value::Nil)),
                        ..RuntimeError::new("ILO-R014", "auto-unwrap propagating nil")
                    }),
                    other => Ok(other), // non-Result/non-nil values pass through
                },
                // `!!` panic-unwrap: on Err/nil, abort with diagnostic + exit 1.
                // Surfaces as a regular RuntimeError (no propagate_value), which
                // bubbles to the CLI runner and produces stderr + exit 1, matching
                // the cross-engine error-channel contract established in #254.
                UnwrapMode::Panic => match result {
                    Value::Ok(v) => Ok(*v),
                    Value::Err(e) => Err(RuntimeError::new(
                        "ILO-R026",
                        format!("panic-unwrap: {}", *e),
                    )),
                    Value::Nil => Err(RuntimeError::new(
                        "ILO-R026",
                        "panic-unwrap: expected value, got nil".to_string(),
                    )),
                    other => Ok(other), // non-Result/non-nil pass through (matches `!`)
                },
            }
        }
        Expr::BinOp { op, left, right } => {
            // Short-circuit for logical ops
            if *op == BinOp::And {
                let l = eval_expr(env, left)?;
                return if !is_truthy(&l) {
                    Ok(l)
                } else {
                    eval_expr(env, right)
                };
            }
            if *op == BinOp::Or {
                let l = eval_expr(env, left)?;
                return if is_truthy(&l) {
                    Ok(l)
                } else {
                    eval_expr(env, right)
                };
            }
            let l = eval_expr(env, left)?;
            let r = eval_expr(env, right)?;
            let result = eval_binop(op, &l, &r)?;
            // Fire sub-expression event for this binary op (depth=expr mode).
            {
                let span = CURRENT_STMT_SPAN.with(|s| *s.borrow());
                fire_expr_trace_event(expr, span, &result);
            }
            Ok(result)
        }
        Expr::UnaryOp { op, operand } => {
            let val = eval_expr(env, operand)?;
            match op {
                UnaryOp::Not => Ok(Value::Bool(!is_truthy(&val))),
                UnaryOp::Negate => match val {
                    Value::Number(n) => Ok(Value::Number(-n)),
                    _ => Err(RuntimeError::new("ILO-R004", "cannot negate non-number")),
                },
            }
        }
        Expr::Ok(inner) => {
            let val = eval_expr(env, inner)?;
            Ok(Value::Ok(Box::new(val)))
        }
        Expr::Err(inner) => {
            let val = eval_expr(env, inner)?;
            Ok(Value::Err(Box::new(val)))
        }
        Expr::List(items) => {
            let mut vals = Vec::new();
            for item in items {
                vals.push(eval_expr(env, item)?);
            }
            Ok(Value::List(Arc::new(vals)))
        }
        Expr::AnonRecord { fields } => {
            let mut field_map = HashMap::new();
            for (name, val_expr) in fields {
                field_map.insert(name.clone(), eval_expr(env, val_expr)?);
            }
            Ok(Value::Record {
                type_name: "__anon".to_string(),
                fields: field_map,
            })
        }
        Expr::Record { type_name, fields } => {
            let mut field_map = HashMap::new();
            for (name, val_expr) in fields {
                field_map.insert(name.clone(), eval_expr(env, val_expr)?);
            }
            Ok(Value::Record {
                type_name: type_name.clone(),
                fields: field_map,
            })
        }
        Expr::Match { subject, arms } => {
            let subj = match subject {
                Some(e) => eval_expr(env, e)?,
                None => Value::Nil,
            };
            for arm in arms {
                if let Some(bindings) = match_pattern(&arm.pattern, &subj) {
                    env.push_scope();
                    for (name, val) in bindings {
                        env.define(&name, val);
                    }
                    // Value-producing match: the arm body is mid-expression,
                    // not in tail position of any function. Pass false so no
                    // TailCall is synthesised — the resulting Value flows
                    // back into the surrounding expression normally.
                    let result = eval_body(env, &arm.body, false);
                    env.pop_scope();
                    return match result? {
                        BodyResult::Value(v) | BodyResult::Return(v) | BodyResult::Break(v) => {
                            Ok(v)
                        }
                        BodyResult::Continue => Ok(Value::Nil),
                        BodyResult::TailCall { .. } => {
                            unreachable!("TailCall escaping value-producing Match arm")
                        }
                    };
                }
            }
            Ok(Value::Nil)
        }
        Expr::NilCoalesce { value, default } => {
            let val = eval_expr(env, value)?;
            if matches!(val, Value::Nil) {
                eval_expr(env, default)
            } else {
                Ok(val)
            }
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            let cond = eval_expr(env, condition)?;
            if is_truthy(&cond) {
                eval_expr(env, then_expr)
            } else {
                eval_expr(env, else_expr)
            }
        }
        Expr::With { object, updates } => {
            let obj = eval_expr(env, object)?;
            match obj {
                Value::Record {
                    type_name,
                    mut fields,
                } => {
                    for (name, val_expr) in updates {
                        fields.insert(name.clone(), eval_expr(env, val_expr)?);
                    }
                    Ok(Value::Record { type_name, fields })
                }
                _ => Err(RuntimeError::new("ILO-R008", "'with' requires a record")),
            }
        }
        Expr::MakeClosure { fn_name, captures } => {
            let mut cap_vals = Vec::with_capacity(captures.len());
            for c in captures {
                cap_vals.push(eval_expr(env, c)?);
            }
            Ok(Value::Closure {
                fn_name: fn_name.clone(),
                captures: cap_vals,
            })
        }
        Expr::Todo(reason) => {
            let msg = match eval_expr(env, reason)? {
                Value::Text(s) => s.to_string(),
                v => format!("{v}"),
            };
            Err(RuntimeError::new("ILO-R020", format!("todo: {msg}")))
        }
        Expr::Panic(reason) => {
            let msg = match eval_expr(env, reason)? {
                Value::Text(s) => s.to_string(),
                v => format!("{v}"),
            };
            Err(RuntimeError::new("ILO-R021", format!("panic: {msg}")))
        }
    }
}

fn eval_literal(lit: &Literal) -> Value {
    match lit {
        Literal::Number(n) => Value::Number(*n),
        Literal::Text(s) => Value::Text(Arc::new(s.clone())),
        Literal::Bool(b) => Value::Bool(*b),
        Literal::Nil => Value::Nil,
    }
}

fn eval_binop(op: &BinOp, left: &Value, right: &Value) -> Result<Value> {
    match (op, left, right) {
        // Numeric ops
        (BinOp::Add, Value::Number(a), Value::Number(b)) => Ok(Value::Number(a + b)),
        (BinOp::Subtract, Value::Number(a), Value::Number(b)) => Ok(Value::Number(a - b)),
        (BinOp::Multiply, Value::Number(a), Value::Number(b)) => Ok(Value::Number(a * b)),
        (BinOp::Divide, Value::Number(a), Value::Number(b)) => {
            if *b == 0.0 {
                Err(RuntimeError::new("ILO-R003", "division by zero"))
            } else {
                Ok(Value::Number(a / b))
            }
        }
        // String concatenation with +
        //
        // Slow path: `a` and `b` are borrowed (`&Arc<String>`), so we always
        // allocate a fresh String here. The hot accumulator pattern
        // `s = +s c` is short-circuited in eval_stmt via the self-rebind
        // peephole, which owns the Arc and uses `Arc::make_mut` for O(1)
        // amortised in-place push_str.
        (BinOp::Add, Value::Text(a), Value::Text(b)) => {
            let mut out = String::with_capacity(a.len() + b.len());
            out.push_str(a);
            out.push_str(b);
            Ok(Value::Text(Arc::new(out)))
        }
        // List concatenation with +
        (BinOp::Add, Value::List(a), Value::List(b)) => {
            let mut out = Vec::with_capacity(a.len() + b.len());
            out.extend_from_slice(a);
            out.extend_from_slice(b);
            Ok(Value::List(Arc::new(out)))
        }
        // Comparisons on numbers
        (BinOp::GreaterThan, Value::Number(a), Value::Number(b)) => Ok(Value::Bool(a > b)),
        (BinOp::LessThan, Value::Number(a), Value::Number(b)) => Ok(Value::Bool(a < b)),
        (BinOp::GreaterOrEqual, Value::Number(a), Value::Number(b)) => Ok(Value::Bool(a >= b)),
        (BinOp::LessOrEqual, Value::Number(a), Value::Number(b)) => Ok(Value::Bool(a <= b)),
        // Comparisons on text (lexicographic)
        (BinOp::GreaterThan, Value::Text(a), Value::Text(b)) => Ok(Value::Bool(a > b)),
        (BinOp::LessThan, Value::Text(a), Value::Text(b)) => Ok(Value::Bool(a < b)),
        (BinOp::GreaterOrEqual, Value::Text(a), Value::Text(b)) => Ok(Value::Bool(a >= b)),
        (BinOp::LessOrEqual, Value::Text(a), Value::Text(b)) => Ok(Value::Bool(a <= b)),
        // List append
        (BinOp::Append, Value::List(items), val) => {
            // Slow path: items is borrowed (`&Arc<Vec<Value>>`), so we always
            // clone the inner Vec here. The hot accumulator pattern
            // `xs = +=xs v` is short-circuited in eval_stmt via the
            // self-rebind peephole, which owns the Arc and uses
            // `Arc::make_mut` for O(1) amortised in-place push.
            let mut new_items = (**items).clone();
            new_items.push(val.clone());
            Ok(Value::List(Arc::new(new_items)))
        }
        // Equality
        (BinOp::Equals, a, b) => Ok(Value::Bool(values_equal(a, b))),
        (BinOp::NotEquals, a, b) => Ok(Value::Bool(!values_equal(a, b))),
        _ => Err(RuntimeError::new(
            "ILO-R004",
            format!(
                "unsupported operation: {:?} on {:?} and {:?}",
                op, left, right
            ),
        )),
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => (a - b).abs() < f64::EPSILON,
        (Value::Text(a), Value::Text(b)) => a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Nil, Value::Nil) => true,
        _ => false,
    }
}

fn is_truthy(val: &Value) -> bool {
    match val {
        Value::Bool(b) => *b,
        Value::Nil => false,
        Value::Number(n) => *n != 0.0,
        Value::Text(s) => !s.is_empty(),
        Value::List(l) => !l.is_empty(),
        _ => true,
    }
}

fn match_pattern(pattern: &Pattern, value: &Value) -> Option<Vec<(String, Value)>> {
    // `_` is always bound — to the inner value for Ok/Err/TypeIs, to the
    // subject itself for Wildcard. SPEC.md line 1069's `~_:~_` relies on this:
    // wildcard arms compose like named arms at zero extra tokens. Bodies that
    // never reference `_` are unaffected.
    match pattern {
        Pattern::Wildcard => Some(vec![("_".to_string(), value.clone())]),
        Pattern::Ok(binding) => {
            if let Value::Ok(inner) = value {
                Some(vec![(binding.clone(), *inner.clone())])
            } else {
                None
            }
        }
        Pattern::Err(binding) => {
            if let Value::Err(inner) = value {
                Some(vec![(binding.clone(), *inner.clone())])
            } else {
                None
            }
        }
        Pattern::Literal(lit) => {
            let expected = eval_literal(lit);
            if values_equal(&expected, value) {
                Some(vec![])
            } else {
                None
            }
        }
        Pattern::TypeIs { ty, binding } => {
            let matches = match ty {
                Type::Number => matches!(value, Value::Number(_)),
                Type::Text => matches!(value, Value::Text(_)),
                Type::Bool => matches!(value, Value::Bool(_)),
                Type::List(_) => matches!(value, Value::List(_)),
                _ => false,
            };
            if matches {
                Some(vec![(binding.clone(), value.clone())])
            } else {
                None
            }
        }
        Pattern::Variant { tag, binding } => {
            // `nil:` in a match arm is emitted as Pattern::Variant { tag: "nil" }
            // so that it can match both the built-in nil value (Optional/nil) and
            // a sum-type variant named `nil`.
            if tag == "nil" && matches!(value, Value::Nil) {
                return Some(vec![]);
            }
            if let Value::Variant {
                tag: vtag, payload, ..
            } = value
            {
                if vtag == tag {
                    let mut bindings = vec![];
                    if let Some(b) = binding {
                        if b != "_" {
                            let pval = payload.as_deref().cloned().unwrap_or(Value::Nil);
                            bindings.push((b.clone(), pval));
                        }
                    }
                    Some(bindings)
                } else {
                    None
                }
            } else {
                None
            }
        }
        Pattern::Or(alts) => {
            // Matches if any alternative matches; bindings from the first matching alt.
            for alt in alts {
                if let Some(bindings) = match_pattern(alt, value) {
                    return Some(bindings);
                }
            }
            None
        }
    }
}

/// Smallest power of 2 >= n. Used to zero-pad FFT input.
fn next_pow2(n: usize) -> usize {
    if n <= 1 {
        return 1;
    }
    let mut p = 1usize;
    while p < n {
        p <<= 1;
    }
    p
}

/// In-place iterative Cooley-Tukey radix-2 FFT.
/// `re.len() == im.len()` and must be a power of 2.
/// If `inverse` is true, applies the inverse transform (divides by N at the end).
pub(crate) fn cooley_tukey(re: &mut [f64], im: &mut [f64], inverse: bool) {
    let n = re.len();
    debug_assert_eq!(n, im.len());
    if n <= 1 {
        return;
    }
    debug_assert!(n.is_power_of_two());

    // Bit-reversal permutation.
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }

    // Butterfly stages.
    let sign: f64 = if inverse { 1.0 } else { -1.0 };
    let mut len = 2usize;
    while len <= n {
        let half = len / 2;
        let theta = sign * 2.0 * std::f64::consts::PI / (len as f64);
        let w_re = theta.cos();
        let w_im = theta.sin();
        let mut i = 0usize;
        while i < n {
            let mut cur_re = 1.0_f64;
            let mut cur_im = 0.0_f64;
            for k in 0..half {
                let a_re = re[i + k];
                let a_im = im[i + k];
                let b_re = re[i + k + half] * cur_re - im[i + k + half] * cur_im;
                let b_im = re[i + k + half] * cur_im + im[i + k + half] * cur_re;
                re[i + k] = a_re + b_re;
                im[i + k] = a_im + b_im;
                re[i + k + half] = a_re - b_re;
                im[i + k + half] = a_im - b_im;
                let new_re = cur_re * w_re - cur_im * w_im;
                let new_im = cur_re * w_im + cur_im * w_re;
                cur_re = new_re;
                cur_im = new_im;
            }
            i += len;
        }
        len <<= 1;
    }

    if inverse {
        let scale = 1.0 / (n as f64);
        for x in re.iter_mut() {
            *x *= scale;
        }
        for x in im.iter_mut() {
            *x *= scale;
        }
    }
}

/// Maximum number of concurrent HTTP GET requests for `get-many`.
/// Caps fan-out so a 10k-url list does not spawn 10k threads.
pub(crate) const GET_MANY_MAX_CONCURRENCY: usize = 10;

/// Fan-out concurrent HTTP GETs and collect one Result per URL, preserving order.
///
/// Each successful fetch (any 2xx-5xx response with valid UTF-8 body) becomes
/// `Ok(body)`; transport, DNS, or UTF-8 failures become `Err(message)`.
///
/// The function uses `std::thread::scope` to spawn worker threads chunked
/// `GET_MANY_MAX_CONCURRENCY` at a time. Each chunk runs in parallel and
/// joins before the next chunk starts. When the `http` feature is disabled,
/// every URL becomes `Err("http feature not enabled")`.
/// Per-stream cap on captured child output, in bytes. Hit on either stream
/// produces an Err result rather than a partial Map — agent scripts that
/// reach this limit are almost always misconfigured (tailing a log, piping
/// a binary), and a typed Err lets the caller surface the configuration
/// bug cleanly instead of silently truncating downstream JSON.
pub(crate) const RUN_OUTPUT_CAP: usize = 10 * 1024 * 1024;

/// Spawn `cmd` with `argv` via `std::process::Command` and return the result
/// as a typed `Value::Ok(Map[Text, Text])` on success, or `Value::Err(text)`
/// on spawn failure / output cap exceeded.
///
/// **No shell.** The argv list is passed directly to `Command::args` — there
/// is no `sh -c`, no string concatenation, and no glob expansion. This is
/// the principled choice that makes `run` safer than bash for agent
/// orchestration: ilo refuses to provide an injection vector, but does
/// provide controlled exec.
///
/// **Inherits parent env + cwd.** No env or cwd override in this first
/// version — that's a follow-up if real workloads need it.
///
/// **Captures stdout + stderr separately** as Text. Either stream exceeding
/// `RUN_OUTPUT_CAP` bytes triggers an Err rather than partial capture, so
/// downstream JSON pipelines never see a truncated payload.
///
/// **Stdin is /dev/null.** Stdin piping is a follow-up (4-arity form taking
/// optional input Text).
/// `rdin` implementation — reads all of stdin into a text string.
/// Returns `R t t`: `Ok(text)` on success, `Err(message)` on I/O failure.
/// On WASM targets stdin is unavailable; returns `Err` immediately.
fn rdin_impl() -> Result<Value> {
    #[cfg(target_family = "wasm")]
    {
        return Ok(Value::Err(Box::new(Value::Text(Arc::new(
            "rdin: stdin not available on wasm".to_string(),
        )))));
    }
    #[cfg(not(target_family = "wasm"))]
    {
        use std::io::Read;
        let mut buf = String::new();
        Ok(match std::io::stdin().read_to_string(&mut buf) {
            Ok(_) => Value::Ok(Box::new(Value::Text(Arc::new(buf)))),
            Err(e) => Value::Err(Box::new(Value::Text(Arc::new(e.to_string())))),
        })
    }
}

/// `rdinl` implementation — reads stdin line by line, stripping newlines.
/// Returns `R (L t) t`: `Ok([line, ...])` on success, `Err(message)` on failure.
/// On WASM targets stdin is unavailable; returns `Err` immediately.
fn rdinl_impl() -> Result<Value> {
    #[cfg(target_family = "wasm")]
    {
        return Ok(Value::Err(Box::new(Value::Text(Arc::new(
            "rdinl: stdin not available on wasm".to_string(),
        )))));
    }
    #[cfg(not(target_family = "wasm"))]
    {
        use std::io::BufRead;
        let stdin = std::io::stdin();
        let lines: std::result::Result<Vec<String>, std::io::Error> =
            stdin.lock().lines().collect();
        Ok(match lines {
            Ok(ls) => {
                let items: Vec<Value> = ls.into_iter().map(|l| Value::Text(Arc::new(l))).collect();
                Value::Ok(Box::new(Value::List(Arc::new(items))))
            }
            Err(e) => Value::Err(Box::new(Value::Text(Arc::new(e.to_string())))),
        })
    }
}

/// `for-line` implementation — returns a lazy stdin line iterator.
///
/// Takes one argument which must be the text "stdin". Returns
/// `Value::LazyStdinLines` so callers can iterate with `@binding` foreach.
/// On WASM stdin is unavailable; returns `Err` immediately.
fn for_line_impl(source: &Value) -> Result<Value> {
    match source {
        Value::Text(s) if s.as_str() == "stdin" => {
            #[cfg(target_family = "wasm")]
            {
                return Ok(Value::Err(Box::new(Value::Text(Arc::new(
                    "for-line: stdin not available on wasm".to_string(),
                )))));
            }
            #[cfg(not(target_family = "wasm"))]
            {
                Ok(Value::LazyStdinLines(StdinLinesHandle::new()))
            }
        }
        other => Err(RuntimeError::new(
            "ILO-R009",
            format!(
                "for-line: argument must be the text \"stdin\", got {:?}",
                other
            ),
        )),
    }
}

///
/// **Non-zero exit is NOT an error.** Matches Python's `subprocess.run`:
/// the caller inspects `code` in the returned Map. Spawn failures (cmd
/// not found, permission denied, etc.) ARE errors and surface as Err.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn run_spawn(cmd: &str, argv: &[String]) -> Value {
    use std::process::{Command, Stdio};

    let mut command = Command::new(cmd);
    command
        .args(argv)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Value::Err(Box::new(Value::Text(Arc::new(format!(
                "run: failed to spawn {cmd:?}: {e}"
            )))));
        }
    };

    // Read both streams concurrently so a child that fills its stderr pipe
    // while we drain stdout doesn't deadlock. Per-stream cap enforced inside
    // the reader; on cap-exceeded we drop the child and surface an Err.
    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();

    let (stdout_res, stderr_res) = std::thread::scope(|s| {
        let so = s.spawn(|| -> std::result::Result<Vec<u8>, String> {
            let mut buf = Vec::new();
            if let Some(p) = stdout_pipe.as_mut() {
                read_capped(p, &mut buf, RUN_OUTPUT_CAP)?;
            }
            Ok(buf)
        });
        let se = s.spawn(|| -> std::result::Result<Vec<u8>, String> {
            let mut buf = Vec::new();
            if let Some(p) = stderr_pipe.as_mut() {
                read_capped(p, &mut buf, RUN_OUTPUT_CAP)?;
            }
            Ok(buf)
        });
        let so = so
            .join()
            .unwrap_or_else(|_| Err("stdout reader panicked".to_string()));
        let se = se
            .join()
            .unwrap_or_else(|_| Err("stderr reader panicked".to_string()));
        (so, se)
    });

    let stdout_buf = match stdout_res {
        Ok(b) => b,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Value::Err(Box::new(Value::Text(Arc::new(format!(
                "run: stdout capture failed: {e}"
            )))));
        }
    };
    let stderr_buf = match stderr_res {
        Ok(b) => b,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Value::Err(Box::new(Value::Text(Arc::new(format!(
                "run: stderr capture failed: {e}"
            )))));
        }
    };

    let status = match child.wait() {
        Ok(s) => s,
        Err(e) => {
            return Value::Err(Box::new(Value::Text(Arc::new(format!(
                "run: wait failed: {e}"
            )))));
        }
    };

    // UTF-8 conversion is lossy — agents asking `run` to drive a binary
    // protocol should reach for a different tool. Lossy keeps the Map
    // schema honest as Text/Text without forcing an Err on every utf-8
    // boundary trip.
    let stdout = String::from_utf8_lossy(&stdout_buf).into_owned();
    let stderr = String::from_utf8_lossy(&stderr_buf).into_owned();
    let code = status.code().map(|c| c.to_string()).unwrap_or_else(|| {
        // On unix, .code() is None when the process was killed by a signal.
        // Surface the signal as a negative-style string so the caller can
        // still branch on it; we deliberately do not raise Err here because
        // signal termination is a normal outcome for `kill -9` style flows.
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if let Some(sig) = status.signal() {
                return format!("signal:{sig}");
            }
        }
        "unknown".to_string()
    });

    let mut m: HashMap<MapKey, Value> = HashMap::with_capacity(3);
    m.insert(
        MapKey::Text("stdout".to_string()),
        Value::Text(Arc::new(stdout)),
    );
    m.insert(
        MapKey::Text("stderr".to_string()),
        Value::Text(Arc::new(stderr)),
    );
    m.insert(
        MapKey::Text("code".to_string()),
        Value::Text(Arc::new(code)),
    );
    Value::Ok(Box::new(Value::Map(Arc::new(m))))
}

/// `run2 cmd argv > R RunResult t` — structured process spawn.
///
/// Same concurrency / cap / UTF-8-lossy policy as `run_spawn`. Returns a
/// typed Record{stdout:t; stderr:t; exit:n} wrapped in `Ok`. The `exit`
/// field is an f64 (ilo's number type): normal exit codes are non-negative
/// integers; signal-killed processes on Unix surface as -1.0 so callers
/// can branch on `r.exit < 0`.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn run_spawn_structured(cmd: &str, argv: &[String]) -> Value {
    use std::process::{Command, Stdio};

    let mut command = Command::new(cmd);
    command
        .args(argv)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Value::Err(Box::new(Value::Text(Arc::new(format!(
                "run2: failed to spawn {cmd:?}: {e}"
            )))));
        }
    };

    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();

    let (stdout_res, stderr_res) = std::thread::scope(|s| {
        let so = s.spawn(|| -> std::result::Result<Vec<u8>, String> {
            let mut buf = Vec::new();
            if let Some(p) = stdout_pipe.as_mut() {
                read_capped(p, &mut buf, RUN_OUTPUT_CAP)?;
            }
            Ok(buf)
        });
        let se = s.spawn(|| -> std::result::Result<Vec<u8>, String> {
            let mut buf = Vec::new();
            if let Some(p) = stderr_pipe.as_mut() {
                read_capped(p, &mut buf, RUN_OUTPUT_CAP)?;
            }
            Ok(buf)
        });
        let so = so
            .join()
            .unwrap_or_else(|_| Err("stdout reader panicked".to_string()));
        let se = se
            .join()
            .unwrap_or_else(|_| Err("stderr reader panicked".to_string()));
        (so, se)
    });

    let stdout_buf = match stdout_res {
        Ok(b) => b,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Value::Err(Box::new(Value::Text(Arc::new(format!(
                "run2: stdout capture failed: {e}"
            )))));
        }
    };
    let stderr_buf = match stderr_res {
        Ok(b) => b,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Value::Err(Box::new(Value::Text(Arc::new(format!(
                "run2: stderr capture failed: {e}"
            )))));
        }
    };

    let status = match child.wait() {
        Ok(s) => s,
        Err(e) => {
            return Value::Err(Box::new(Value::Text(Arc::new(format!(
                "run2: wait failed: {e}"
            )))));
        }
    };

    let stdout = String::from_utf8_lossy(&stdout_buf).into_owned();
    let stderr = String::from_utf8_lossy(&stderr_buf).into_owned();

    // exit as f64: normal code or -1 for signal-killed processes.
    let exit_code: f64 = status.code().map(|c| c as f64).unwrap_or_else(|| {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if status.signal().is_some() {
                return -1.0;
            }
        }
        -1.0
    });

    let mut fields = HashMap::with_capacity(3);
    fields.insert("stdout".to_string(), Value::Text(Arc::new(stdout)));
    fields.insert("stderr".to_string(), Value::Text(Arc::new(stderr)));
    fields.insert("exit".to_string(), Value::Number(exit_code));
    Value::Ok(Box::new(Value::Record {
        type_name: "RunResult".to_string(),
        fields,
    }))
}

#[cfg(target_family = "wasm")]
pub(crate) fn run_spawn(_cmd: &str, _argv: &[String]) -> Value {
    Value::Err(Box::new(Value::Text(Arc::new(
        "run: process spawn not available on wasm".to_string(),
    ))))
}

#[cfg(target_family = "wasm")]
pub(crate) fn run_spawn_structured(_cmd: &str, _argv: &[String]) -> Value {
    Value::Err(Box::new(Value::Text(Arc::new(
        "run2: process spawn not available on wasm".to_string(),
    ))))
}

/// `run cmd argv stdin_text > R (M t t) t` — like `run_spawn` but pipes
/// `stdin_text` into the child's stdin instead of /dev/null.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn run_spawn_with_stdin(cmd: &str, argv: &[String], stdin_text: &str) -> Value {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut command = Command::new(cmd);
    command
        .args(argv)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Value::Err(Box::new(Value::Text(Arc::new(format!(
                "run: failed to spawn {cmd:?}: {e}"
            )))));
        }
    };

    // Write stdin synchronously before draining stdout/stderr to avoid
    // deadlock on small inputs (the child reads stdin then closes it).
    // For large stdin blobs a dedicated thread would be safer; the 10 MiB
    // output cap already bounds child output, and stdin writes > pipe buffer
    // will block here — acceptable for the 0.13.0 initial shape.
    if let Some(mut stdin_pipe) = child.stdin.take() {
        if let Err(e) = stdin_pipe.write_all(stdin_text.as_bytes()) {
            let _ = child.kill();
            let _ = child.wait();
            return Value::Err(Box::new(Value::Text(Arc::new(format!(
                "run: failed to write stdin: {e}"
            )))));
        }
        // Drop closes the pipe, signalling EOF to the child.
    }

    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();

    let (stdout_res, stderr_res) = std::thread::scope(|s| {
        let so = s.spawn(|| -> std::result::Result<Vec<u8>, String> {
            let mut buf = Vec::new();
            if let Some(p) = stdout_pipe.as_mut() {
                read_capped(p, &mut buf, RUN_OUTPUT_CAP)?;
            }
            Ok(buf)
        });
        let se = s.spawn(|| -> std::result::Result<Vec<u8>, String> {
            let mut buf = Vec::new();
            if let Some(p) = stderr_pipe.as_mut() {
                read_capped(p, &mut buf, RUN_OUTPUT_CAP)?;
            }
            Ok(buf)
        });
        let so = so
            .join()
            .unwrap_or_else(|_| Err("stdout reader panicked".to_string()));
        let se = se
            .join()
            .unwrap_or_else(|_| Err("stderr reader panicked".to_string()));
        (so, se)
    });

    let stdout_buf = match stdout_res {
        Ok(b) => b,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Value::Err(Box::new(Value::Text(Arc::new(format!(
                "run: stdout capture failed: {e}"
            )))));
        }
    };
    let stderr_buf = match stderr_res {
        Ok(b) => b,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Value::Err(Box::new(Value::Text(Arc::new(format!(
                "run: stderr capture failed: {e}"
            )))));
        }
    };

    let status = match child.wait() {
        Ok(s) => s,
        Err(e) => {
            return Value::Err(Box::new(Value::Text(Arc::new(format!(
                "run: wait failed: {e}"
            )))));
        }
    };

    let stdout = String::from_utf8_lossy(&stdout_buf).into_owned();
    let stderr = String::from_utf8_lossy(&stderr_buf).into_owned();
    let code = status.code().map(|c| c.to_string()).unwrap_or_else(|| {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if let Some(sig) = status.signal() {
                return format!("signal:{sig}");
            }
        }
        "unknown".to_string()
    });

    let mut m: HashMap<MapKey, Value> = HashMap::with_capacity(3);
    m.insert(
        MapKey::Text("stdout".to_string()),
        Value::Text(Arc::new(stdout)),
    );
    m.insert(
        MapKey::Text("stderr".to_string()),
        Value::Text(Arc::new(stderr)),
    );
    m.insert(
        MapKey::Text("code".to_string()),
        Value::Text(Arc::new(code)),
    );
    Value::Ok(Box::new(Value::Map(Arc::new(m))))
}

/// `run2 cmd argv stdin_text > R RunResult t` — like `run_spawn_structured`
/// but pipes `stdin_text` into the child's stdin.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn run_spawn_structured_with_stdin(
    cmd: &str,
    argv: &[String],
    stdin_text: &str,
) -> Value {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut command = Command::new(cmd);
    command
        .args(argv)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Value::Err(Box::new(Value::Text(Arc::new(format!(
                "run2: failed to spawn {cmd:?}: {e}"
            )))));
        }
    };

    if let Some(mut stdin_pipe) = child.stdin.take() {
        if let Err(e) = stdin_pipe.write_all(stdin_text.as_bytes()) {
            let _ = child.kill();
            let _ = child.wait();
            return Value::Err(Box::new(Value::Text(Arc::new(format!(
                "run2: failed to write stdin: {e}"
            )))));
        }
    }

    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();

    let (stdout_res, stderr_res) = std::thread::scope(|s| {
        let so = s.spawn(|| -> std::result::Result<Vec<u8>, String> {
            let mut buf = Vec::new();
            if let Some(p) = stdout_pipe.as_mut() {
                read_capped(p, &mut buf, RUN_OUTPUT_CAP)?;
            }
            Ok(buf)
        });
        let se = s.spawn(|| -> std::result::Result<Vec<u8>, String> {
            let mut buf = Vec::new();
            if let Some(p) = stderr_pipe.as_mut() {
                read_capped(p, &mut buf, RUN_OUTPUT_CAP)?;
            }
            Ok(buf)
        });
        let so = so
            .join()
            .unwrap_or_else(|_| Err("stdout reader panicked".to_string()));
        let se = se
            .join()
            .unwrap_or_else(|_| Err("stderr reader panicked".to_string()));
        (so, se)
    });

    let stdout_buf = match stdout_res {
        Ok(b) => b,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Value::Err(Box::new(Value::Text(Arc::new(format!(
                "run2: stdout capture failed: {e}"
            )))));
        }
    };
    let stderr_buf = match stderr_res {
        Ok(b) => b,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Value::Err(Box::new(Value::Text(Arc::new(format!(
                "run2: stderr capture failed: {e}"
            )))));
        }
    };

    let status = match child.wait() {
        Ok(s) => s,
        Err(e) => {
            return Value::Err(Box::new(Value::Text(Arc::new(format!(
                "run2: wait failed: {e}"
            )))));
        }
    };

    let stdout = String::from_utf8_lossy(&stdout_buf).into_owned();
    let stderr = String::from_utf8_lossy(&stderr_buf).into_owned();
    let exit_code: f64 = status.code().map(|c| c as f64).unwrap_or_else(|| {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if status.signal().is_some() {
                return -1.0;
            }
        }
        -1.0
    });

    let mut fields = HashMap::with_capacity(3);
    fields.insert("stdout".to_string(), Value::Text(Arc::new(stdout)));
    fields.insert("stderr".to_string(), Value::Text(Arc::new(stderr)));
    fields.insert("exit".to_string(), Value::Number(exit_code));
    Value::Ok(Box::new(Value::Record {
        type_name: "RunResult".to_string(),
        fields,
    }))
}

/// `run-bg cmd argv > R n t` — fire-and-forget background spawn.
///
/// Spawns the child and immediately returns `Ok(pid:n)` without waiting.
/// Child inherits the parent's stdout and stderr. stdin is /dev/null.
/// Err only on spawn failure (cmd not found, permission denied, etc.).
#[cfg(not(target_family = "wasm"))]
pub(crate) fn run_spawn_bg(cmd: &str, argv: &[String]) -> Value {
    use std::process::{Command, Stdio};

    let mut command = Command::new(cmd);
    command
        .args(argv)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    match command.spawn() {
        Ok(child) => {
            let pid = child.id() as f64;
            // Detach: drop the Child handle without waiting so the child
            // runs independently. The OS will reap it as an orphan.
            Value::Ok(Box::new(Value::Number(pid)))
        }
        Err(e) => Value::Err(Box::new(Value::Text(Arc::new(format!(
            "run-bg: failed to spawn {cmd:?}: {e}"
        ))))),
    }
}

#[cfg(target_family = "wasm")]
pub(crate) fn run_spawn_with_stdin(_cmd: &str, _argv: &[String], _stdin: &str) -> Value {
    Value::Err(Box::new(Value::Text(Arc::new(
        "run: process spawn not available on wasm".to_string(),
    ))))
}

#[cfg(target_family = "wasm")]
pub(crate) fn run_spawn_structured_with_stdin(_cmd: &str, _argv: &[String], _stdin: &str) -> Value {
    Value::Err(Box::new(Value::Text(Arc::new(
        "run2: process spawn not available on wasm".to_string(),
    ))))
}

#[cfg(target_family = "wasm")]
pub(crate) fn run_spawn_bg(_cmd: &str, _argv: &[String]) -> Value {
    Value::Err(Box::new(Value::Text(Arc::new(
        "run-bg: process spawn not available on wasm".to_string(),
    ))))
}

/// Drain `reader` into `buf` while enforcing `cap` bytes per call. Returns
/// Err(message) when the cap is exceeded so the caller can surface an Err
/// rather than partial capture.
#[cfg(not(target_family = "wasm"))]
fn read_capped<R: std::io::Read>(
    reader: &mut R,
    buf: &mut Vec<u8>,
    cap: usize,
) -> std::result::Result<(), String> {
    let mut chunk = [0u8; 8192];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => return Ok(()),
            Ok(n) => {
                if buf.len() + n > cap {
                    return Err(format!(
                        "captured output exceeded cap ({cap} bytes); kill child + abort"
                    ));
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            Err(e) => return Err(format!("read error: {e}")),
        }
    }
}

/// Convert a `minreq::Response` to an ilo Ok-map with `status`, `headers`,
/// and `body` keys. Body decoded as UTF-8; non-UTF-8 surfaces as Err.
/// Shape: `R (M t _) t` — status:n, headers:M t t, body:t.
#[cfg(feature = "http")]
pub(crate) fn http_response_to_ok_map(resp: &minreq::Response) -> Value {
    let body = match resp.as_str() {
        Ok(b) => b.to_string(),
        Err(e) => {
            return Value::Err(Box::new(Value::Text(Arc::new(format!(
                "response is not valid UTF-8: {e}"
            )))));
        }
    };
    let mut headers_map: HashMap<MapKey, Value> = HashMap::with_capacity(resp.headers.len());
    for (k, v) in resp.headers.iter() {
        headers_map.insert(MapKey::Text(k.clone()), Value::Text(Arc::new(v.clone())));
    }
    let mut m: HashMap<MapKey, Value> = HashMap::with_capacity(3);
    m.insert(
        MapKey::Text("status".to_string()),
        Value::Number(resp.status_code as f64),
    );
    m.insert(
        MapKey::Text("headers".to_string()),
        Value::Map(Arc::new(headers_map)),
    );
    m.insert(
        MapKey::Text("body".to_string()),
        Value::Text(Arc::new(body)),
    );
    Value::Ok(Box::new(Value::Map(Arc::new(m))))
}

pub(crate) fn get_many_fetch(urls: &[String]) -> Vec<Value> {
    if urls.is_empty() {
        return Vec::new();
    }
    let mut results: Vec<Value> = (0..urls.len()).map(|_| Value::Nil).collect();
    #[cfg(feature = "http")]
    {
        let chunks: Vec<(usize, &[String])> = urls
            .chunks(GET_MANY_MAX_CONCURRENCY)
            .enumerate()
            .map(|(i, c)| (i * GET_MANY_MAX_CONCURRENCY, c))
            .collect();
        for (base, chunk) in chunks {
            std::thread::scope(|s| {
                let mut handles = Vec::with_capacity(chunk.len());
                for url in chunk.iter() {
                    let u = url.clone();
                    handles.push(s.spawn(move || match minreq::get(u.as_str()).send() {
                        Ok(resp) => match resp.as_str() {
                            Ok(body) => {
                                Value::Ok(Box::new(Value::Text(Arc::new(body.to_string()))))
                            }
                            Err(e) => Value::Err(Box::new(Value::Text(Arc::new(format!(
                                "response is not valid UTF-8: {e}"
                            ))))),
                        },
                        Err(e) => Value::Err(Box::new(Value::Text(Arc::new(e.to_string())))),
                    }));
                }
                for (i, h) in handles.into_iter().enumerate() {
                    let v = h.join().unwrap_or_else(|_| {
                        Value::Err(Box::new(Value::Text(Arc::new(
                            "worker thread panicked".to_string(),
                        ))))
                    });
                    results[base + i] = v;
                }
            });
        }
    }
    #[cfg(not(feature = "http"))]
    {
        for slot in results.iter_mut() {
            *slot = Value::Err(Box::new(Value::Text(
                "http feature not enabled".to_string().into(),
            )));
        }
    }
    results
}

/// Default concurrency for `par-map` when no explicit `n` is given.
///
/// Reads the `ILO_PAR_MAP_CONCURRENCY` environment variable first; falls back
/// to the number of logical CPUs reported by the OS (via `std::thread::available_parallelism`).
/// A zero or invalid env value is ignored in favour of the CPU count.
fn par_map_default_concurrency() -> usize {
    if let Ok(s) = std::env::var("ILO_PAR_MAP_CONCURRENCY") {
        if let Ok(n) = s.trim().parse::<usize>() {
            if n > 0 {
                return n;
            }
        }
    }
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

/// Compute the auto-tuned chunk size for `par_map_run`.
///
/// With `n_threads` threads and `n_items` items, each thread processes
/// `ceil(n_items / n_threads)` items, keeping thread-creation overhead
/// constant regardless of list length. Returns at least 1.
fn par_map_chunk_size(n_items: usize, n_threads: usize) -> usize {
    let t = n_threads.max(1);
    (n_items + t - 1) / t
}

/// Apply `fn_name` to each element of `items` using up to `concurrency`
/// threads, collecting results in input order as `Value::Ok(_)` / `Value::Err(_)`.
///
/// ### Chunking
/// Instead of spawning one thread per item, we spawn at most `concurrency`
/// threads and distribute items evenly across them
/// (`chunk_size = ceil(len / concurrency)`). This keeps thread-creation
/// overhead constant for large lists of small items and avoids the
/// wave-by-wave serialisation of the previous implementation.
///
/// ### Cancellation
/// A shared atomic flag (`cancelled`) is set to `true` the first time any
/// worker produces an `Err`. Subsequent items inside the same worker are
/// skipped and filled with a cancellation sentinel
/// `Err("par-map: cancelled due to earlier error")`. Items in other threads
/// that have not yet started processing also respect this flag. This means
/// that a single error causes remaining unstarted work to be abandoned
/// quickly while already-running calls complete naturally.
///
/// Worker threads each get a fresh `Env` built from the function-table snapshot
/// (`fns`) and the capability policy (`caps`) captured from the caller's `Env`.
///
/// `#[inline(never)]` keeps this body out of `call_function`'s already-huge
/// frame, following the dispatch-arm-size convention from #494 / ILO-289.
#[inline(never)]
fn par_map_run(
    fn_name: &str,
    captures: Vec<Value>,
    items: &[Value],
    concurrency: usize,
    fns: HashMap<String, Decl>,
    caps: Arc<Caps>,
) -> Vec<Value> {
    use std::sync::atomic::{AtomicBool, Ordering};

    if items.is_empty() {
        return Vec::new();
    }
    let n_threads = concurrency.max(1);
    let chunk_size = par_map_chunk_size(items.len(), n_threads);

    // Pre-fill results so threads can write their slice independently.
    let mut results: Vec<Value> = vec![Value::Nil; items.len()];

    // Shared cancellation flag: set to true when any worker encounters an Err.
    let cancelled = Arc::new(AtomicBool::new(false));

    std::thread::scope(|s| {
        let mut handles = Vec::new();
        for (chunk_idx, chunk) in items.chunks(chunk_size).enumerate() {
            let base = chunk_idx * chunk_size;
            let chunk_items: Vec<Value> = chunk.to_vec();
            let fn_name_owned = fn_name.to_string();
            let captures = captures.clone();
            let fns = fns.clone();
            let caps = caps.clone();
            let cancelled = cancelled.clone();
            handles.push((
                base,
                chunk_items.len(),
                s.spawn(move || {
                    let mut worker_env = Env::with_caps(caps);
                    worker_env.functions = fns;
                    let mut local: Vec<Value> = Vec::with_capacity(chunk_items.len());
                    for item in chunk_items {
                        // Check cancellation before starting each item.
                        if cancelled.load(Ordering::Relaxed) {
                            local.push(Value::Err(Box::new(Value::Text(Arc::new(
                                "par-map: cancelled due to earlier error".to_string(),
                            )))));
                            continue;
                        }
                        let mut call_args = vec![item];
                        call_args.extend(captures.iter().cloned());
                        let result = match call_function(&mut worker_env, &fn_name_owned, call_args)
                        {
                            Ok(v) => Value::Ok(Box::new(v)),
                            Err(e) => {
                                // Signal other workers to cancel.
                                cancelled.store(true, Ordering::Relaxed);
                                Value::Err(Box::new(Value::Text(Arc::new(e.message.clone()))))
                            }
                        };
                        local.push(result);
                    }
                    local
                }),
            ));
        }
        for (base, len, handle) in handles {
            let local = handle.join().unwrap_or_else(|_| {
                vec![
                    Value::Err(Box::new(Value::Text(Arc::new(
                        "par-map worker thread panicked".to_string(),
                    ))));
                    len
                ]
            });
            for (i, v) in local.into_iter().enumerate() {
                results[base + i] = v;
            }
        }
    });
    results
}

/// Parse a relative-date phrase into a Unix epoch (seconds), anchored at
/// `now_epoch`.
///
/// Supports:
///   today / yesterday / tomorrow
///   N days ago / in N days
///   N weeks ago / in N weeks
///   N months ago / in N months
///   last <weekday> / next <weekday> / this <weekday>
///   ISO-8601 date literals (YYYY-MM-DD) — delegates to chrono
///
/// Returns `Value::Ok(Number(epoch))` on success, `Value::Err(Text(msg))` on
/// failure. Never panics.
fn dtparse_rel(phrase: &str, now_epoch: f64) -> Value {
    use chrono::{Datelike, Duration, NaiveDate, TimeZone, Utc, Weekday};

    let make_ok = |epoch: i64| Value::Ok(Box::new(Value::Number(epoch as f64)));
    let make_err = |msg: String| Value::Err(Box::new(Value::Text(Arc::new(msg))));

    // Anchor date (UTC, floored to whole seconds).
    let now_secs = if now_epoch.is_finite() {
        now_epoch as i64
    } else {
        return make_err(format!(
            "dtparse-rel: now epoch is not finite ({now_epoch})"
        ));
    };
    let now_dt = match Utc.timestamp_opt(now_secs, 0).single() {
        Some(dt) => dt,
        None => return make_err(format!("dtparse-rel: now epoch out of range ({now_secs})")),
    };
    let today: NaiveDate = now_dt.date_naive();

    let s = phrase.trim().to_ascii_lowercase();

    // Simple keywords.
    if s == "today" {
        let epoch = today.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();
        return make_ok(epoch);
    }
    if s == "yesterday" {
        let d = today - Duration::days(1);
        return make_ok(d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp());
    }
    if s == "tomorrow" {
        let d = today + Duration::days(1);
        return make_ok(d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp());
    }

    // Helper: parse a weekday name (long or short).
    fn parse_weekday(name: &str) -> Option<Weekday> {
        match name {
            "monday" | "mon" => Some(Weekday::Mon),
            "tuesday" | "tue" => Some(Weekday::Tue),
            "wednesday" | "wed" => Some(Weekday::Wed),
            "thursday" | "thu" => Some(Weekday::Thu),
            "friday" | "fri" => Some(Weekday::Fri),
            "saturday" | "sat" => Some(Weekday::Sat),
            "sunday" | "sun" => Some(Weekday::Sun),
            _ => None,
        }
    }

    // "last <weekday>" — the most recent past occurrence, never today.
    if let Some(day_name) = s.strip_prefix("last ") {
        return match parse_weekday(day_name.trim()) {
            None => make_err(format!("dtparse-rel: unknown weekday '{day_name}'")),
            Some(target) => {
                let today_num = today.weekday().num_days_from_monday(); // 0=Mon
                let target_num = target.num_days_from_monday();
                // Days back: at least 1, at most 7.
                let diff = (today_num + 7 - target_num) % 7;
                let diff = if diff == 0 { 7 } else { diff };
                let d = today - Duration::days(diff as i64);
                make_ok(d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp())
            }
        };
    }

    // "next <weekday>" — the next future occurrence, never today.
    if let Some(day_name) = s.strip_prefix("next ") {
        return match parse_weekday(day_name.trim()) {
            None => make_err(format!("dtparse-rel: unknown weekday '{day_name}'")),
            Some(target) => {
                let today_num = today.weekday().num_days_from_monday();
                let target_num = target.num_days_from_monday();
                let diff = (target_num + 7 - today_num) % 7;
                let diff = if diff == 0 { 7 } else { diff };
                let d = today + Duration::days(diff as i64);
                make_ok(d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp())
            }
        };
    }

    // "this <weekday>" — the occurrence within the current week (Mon-Sun).
    // If today is that weekday, returns today. If already past in this week,
    // returns that past day. If not yet reached, returns the future day.
    if let Some(day_name) = s.strip_prefix("this ") {
        return match parse_weekday(day_name.trim()) {
            None => make_err(format!("dtparse-rel: unknown weekday '{day_name}'")),
            Some(target) => {
                let today_num = today.weekday().num_days_from_monday() as i64;
                let target_num = target.num_days_from_monday() as i64;
                let d = today + Duration::days(target_num - today_num);
                make_ok(d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp())
            }
        };
    }

    // Helper: only treat a stripped remainder as a count phrase if it's
    // an all-digit non-empty token. Without this, a leftover word fragment
    // (e.g. "wednes" from a hypothetical "wednesday" mis-strip, or "this"
    // from "in this day") would fall through to the int parser and surface
    // as a misleading "invalid <unit> count" error instead of the
    // unrecognised-phrase fallback. Digit-only also gates negatives out
    // (the `-` sign is rejected before parse, so the `n >= 0` branch in
    // each arm only fires for genuine non-negative integers).
    fn parse_unsigned_count(rest: &str) -> Option<i64> {
        let t = rest.trim();
        if t.is_empty() || !t.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        t.parse::<i64>().ok()
    }

    // "N day(s) ago" / "in N day(s)"
    let ago_days = s
        .strip_suffix(" days ago")
        .or_else(|| s.strip_suffix(" day ago"));
    if let Some(rest) = ago_days
        && let Some(n) = parse_unsigned_count(rest)
    {
        let d = today - Duration::days(n);
        return make_ok(d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp());
    }
    let in_days = s
        .strip_prefix("in ")
        .and_then(|r| r.strip_suffix(" days").or_else(|| r.strip_suffix(" day")));
    if let Some(rest) = in_days
        && let Some(n) = parse_unsigned_count(rest)
    {
        let d = today + Duration::days(n);
        return make_ok(d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp());
    }

    // "N week(s) ago" / "in N week(s)"
    let ago_weeks = s
        .strip_suffix(" weeks ago")
        .or_else(|| s.strip_suffix(" week ago"));
    if let Some(rest) = ago_weeks
        && let Some(n) = parse_unsigned_count(rest)
    {
        let d = today - Duration::weeks(n);
        return make_ok(d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp());
    }
    let in_weeks = s
        .strip_prefix("in ")
        .and_then(|r| r.strip_suffix(" weeks").or_else(|| r.strip_suffix(" week")));
    if let Some(rest) = in_weeks
        && let Some(n) = parse_unsigned_count(rest)
    {
        let d = today + Duration::weeks(n);
        return make_ok(d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp());
    }

    // "N month(s) ago" / "in N month(s)"
    // Month arithmetic: add/subtract calendar months, clamping to last day of month.
    fn add_months(date: NaiveDate, months: i32) -> Option<NaiveDate> {
        let total_months = date.year() * 12 + (date.month() as i32 - 1) + months;
        let y = total_months.div_euclid(12);
        let m = (total_months.rem_euclid(12) + 1) as u32;
        let max_day = days_in_month(y, m);
        let d = date.day().min(max_day);
        NaiveDate::from_ymd_opt(y, m, d)
    }
    fn days_in_month(year: i32, month: u32) -> u32 {
        let next = if month == 12 {
            NaiveDate::from_ymd_opt(year + 1, 1, 1)
        } else {
            NaiveDate::from_ymd_opt(year, month + 1, 1)
        };
        (next.unwrap() - NaiveDate::from_ymd_opt(year, month, 1).unwrap()).num_days() as u32
    }

    let ago_months = s
        .strip_suffix(" months ago")
        .or_else(|| s.strip_suffix(" month ago"));
    if let Some(rest) = ago_months
        && let Some(n) = parse_unsigned_count(rest)
    {
        let n = n as i32;
        return match add_months(today, -n) {
            Some(d) => make_ok(d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp()),
            None => make_err(format!(
                "dtparse-rel: month arithmetic out of range in '{phrase}'"
            )),
        };
    }
    let in_months = s.strip_prefix("in ").and_then(|r| {
        r.strip_suffix(" months")
            .or_else(|| r.strip_suffix(" month"))
    });
    if let Some(rest) = in_months
        && let Some(n) = parse_unsigned_count(rest)
    {
        let n = n as i32;
        return match add_months(today, n) {
            Some(d) => make_ok(d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp()),
            None => make_err(format!(
                "dtparse-rel: month arithmetic out of range in '{phrase}'"
            )),
        };
    }

    // ISO-8601 date literal passthrough (YYYY-MM-DD).
    if let Ok(nd) = chrono::NaiveDate::parse_from_str(phrase.trim(), "%Y-%m-%d") {
        let epoch = nd.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();
        return make_ok(epoch);
    }

    make_err(format!(
        "dtparse-rel: unrecognised phrase '{phrase}' — expected: today/yesterday/tomorrow, \
N days/weeks/months ago, in N days/weeks/months, last/next/this <weekday>, or YYYY-MM-DD"
    ))
}

#[cfg(test)]
#[allow(clippy::approx_constant)]
mod tests {
    use super::*;
    use crate::lexer;
    use crate::parser;

    static ENV_TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn parse_program(source: &str) -> Program {
        let tokens = lexer::lex(source).unwrap();
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
        let (prog, errors) = parser::parse(token_spans);
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        prog
    }

    fn run_str(source: &str, func: Option<&str>, args: Vec<Value>) -> Value {
        let prog = parse_program(source);
        run(&prog, func, args).unwrap()
    }

    #[test]
    fn interpret_tot() {
        // tot p:n q:n r:n>n;s=*p q;t=*s r;+s t
        let source = std::fs::read_to_string("examples/01-simple-function.ilo").unwrap();
        let result = run_str(
            &source,
            Some("tot"),
            vec![
                Value::Number(10.0),
                Value::Number(20.0),
                Value::Number(30.0),
            ],
        );
        assert_eq!(result, Value::Number(6200.0));
    }

    #[test]
    fn interpret_tot_different_args() {
        let source = "tot p:n q:n r:n>n;s=*p q;t=*s r;+s t";
        let result = run_str(
            source,
            Some("tot"),
            vec![Value::Number(2.0), Value::Number(3.0), Value::Number(4.0)],
        );
        // s = 2*3 = 6, t = 6*4 = 24, s+t = 30
        assert_eq!(result, Value::Number(30.0));
    }

    #[test]
    fn interpret_cls_gold() {
        // Braceless guards: early return
        let source = r#"cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze""#;
        let result = run_str(source, Some("cls"), vec![Value::Number(1000.0)]);
        assert_eq!(result, Value::Text(Arc::new("gold".to_string())));
    }

    #[test]
    fn interpret_cls_silver() {
        let source = r#"cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze""#;
        let result = run_str(source, Some("cls"), vec![Value::Number(500.0)]);
        assert_eq!(result, Value::Text(Arc::new("silver".to_string())));
    }

    #[test]
    fn interpret_cls_bronze() {
        let source = r#"cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze""#;
        let result = run_str(source, Some("cls"), vec![Value::Number(100.0)]);
        assert_eq!(result, Value::Text(Arc::new("bronze".to_string())));
    }

    #[test]
    fn interpret_match_stmt() {
        let source = r#"f x:t>n;?x{"a":1;"b":2;_:0}"#;
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::Text(Arc::new("a".to_string()))]
            ),
            Value::Number(1.0)
        );
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::Text(Arc::new("b".to_string()))]
            ),
            Value::Number(2.0)
        );
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::Text(Arc::new("z".to_string()))]
            ),
            Value::Number(0.0)
        );
    }

    #[test]
    fn interpret_ok_err() {
        let source = "f x:n>R n t;~x";
        let result = run_str(source, Some("f"), vec![Value::Number(42.0)]);
        assert_eq!(result, Value::Ok(Box::new(Value::Number(42.0))));
    }

    #[test]
    fn interpret_err_constructor() {
        let source = r#"f x:n>R n t;^"bad""#;
        let result = run_str(source, Some("f"), vec![Value::Number(0.0)]);
        assert_eq!(
            result,
            Value::Err(Box::new(Value::Text(Arc::new("bad".to_string()))))
        );
    }

    #[test]
    fn interpret_match_ok_err_patterns() {
        let source = r#"f x:R n t>n;?x{^er:0;~v:v}"#;
        let ok_result = run_str(
            source,
            Some("f"),
            vec![Value::Ok(Box::new(Value::Number(42.0)))],
        );
        assert_eq!(ok_result, Value::Number(42.0));

        let err_result = run_str(
            source,
            Some("f"),
            vec![Value::Err(Box::new(Value::Text(Arc::new(
                "oops".to_string(),
            ))))],
        );
        assert_eq!(err_result, Value::Number(0.0));
    }

    #[test]
    fn interpret_negated_guard() {
        // Ternary form: negated guard with else — produces value, no early return
        let source = r#"f x:b>t;!x{"nope"}{"yes"}"#;
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Bool(false)]),
            Value::Text(Arc::new("nope".to_string()))
        );
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Bool(true)]),
            Value::Text(Arc::new("yes".to_string()))
        );
    }

    #[test]
    fn interpret_logical_not() {
        let source = "f x:b>b;!x";
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Bool(true)]),
            Value::Bool(false)
        );
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Bool(false)]),
            Value::Bool(true)
        );
    }

    #[test]
    fn interpret_record_and_field() {
        let source = "f x:n>n;r=point x:x y:10;r.y";
        let result = run_str(source, Some("f"), vec![Value::Number(5.0)]);
        assert_eq!(result, Value::Number(10.0));
    }

    #[test]
    fn interpret_with_expr() {
        let source = "f>n;r=point x:1 y:2;r2=r with y:10;r2.y";
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(result, Value::Number(10.0));
    }

    #[test]
    fn interpret_string_concat() {
        let source = r#"f a:t b:t>t;+a b"#;
        let result = run_str(
            source,
            Some("f"),
            vec![
                Value::Text(Arc::new("hello ".to_string())),
                Value::Text(Arc::new("world".to_string())),
            ],
        );
        assert_eq!(result, Value::Text(Arc::new("hello world".to_string())));
    }

    #[test]
    fn interpret_string_comparison() {
        let gt = r#"f a:t b:t>b;>a b"#;
        assert_eq!(
            run_str(
                gt,
                Some("f"),
                vec![
                    Value::Text(Arc::new("banana".to_string())),
                    Value::Text(Arc::new("apple".to_string()))
                ]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            run_str(
                gt,
                Some("f"),
                vec![
                    Value::Text(Arc::new("apple".to_string())),
                    Value::Text(Arc::new("banana".to_string()))
                ]
            ),
            Value::Bool(false)
        );

        let lt = r#"f a:t b:t>b;<a b"#;
        assert_eq!(
            run_str(
                lt,
                Some("f"),
                vec![
                    Value::Text(Arc::new("apple".to_string())),
                    Value::Text(Arc::new("banana".to_string()))
                ]
            ),
            Value::Bool(true)
        );

        let ge = r#"f a:t b:t>b;>=a b"#;
        assert_eq!(
            run_str(
                ge,
                Some("f"),
                vec![
                    Value::Text(Arc::new("apple".to_string())),
                    Value::Text(Arc::new("apple".to_string()))
                ]
            ),
            Value::Bool(true)
        );

        let le = r#"f a:t b:t>b;<=a b"#;
        assert_eq!(
            run_str(
                le,
                Some("f"),
                vec![
                    Value::Text(Arc::new("zebra".to_string())),
                    Value::Text(Arc::new("banana".to_string()))
                ]
            ),
            Value::Bool(false)
        );
    }

    #[test]
    fn interpret_match_expr_in_let() {
        let source = r#"f x:t>n;y=?x{"a":1;"b":2;_:0};y"#;
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Text(Arc::new("b".to_string()))],
        );
        assert_eq!(result, Value::Number(2.0));
    }

    #[test]
    fn interpret_default_first_function() {
        let source = "f>n;42";
        let result = run_str(source, None, vec![]);
        assert_eq!(result, Value::Number(42.0));
    }

    #[test]
    fn interpret_division_by_zero() {
        let source = "f x:n>n;/x 0";
        let prog = parse_program(source);
        let result = run(&prog, Some("f"), vec![Value::Number(10.0)]);
        assert!(result.is_err());
    }

    // ── Error paths for the new transcendental math builtins ─────────────
    // The tree-walker accepts any Value at runtime; verify catches the type
    // mismatch at compile time but does not run here. These tests cover the
    // `other => Err(...)` arms in the Sqrt|Log|Exp|Sin|Cos and Pow handlers.
    #[test]
    fn interpret_sqrt_non_number_errors() {
        let source = "f x:t>n;sqrt x";
        let prog = parse_program(source);
        let err = run(
            &prog,
            Some("f"),
            vec![Value::Text(Arc::new("nope".to_string()))],
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("sqrt") && err.to_string().contains("requires a number"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn interpret_log_non_number_errors() {
        let prog = parse_program("f x:t>n;log x");
        let err = run(
            &prog,
            Some("f"),
            vec![Value::Text(Arc::new("nope".to_string()))],
        )
        .unwrap_err();
        assert!(err.to_string().contains("log"), "unexpected error: {err}");
    }

    #[test]
    fn interpret_exp_non_number_errors() {
        let prog = parse_program("f x:t>n;exp x");
        let err = run(
            &prog,
            Some("f"),
            vec![Value::Text(Arc::new("nope".to_string()))],
        )
        .unwrap_err();
        assert!(err.to_string().contains("exp"), "unexpected error: {err}");
    }

    #[test]
    fn interpret_sin_non_number_errors() {
        let prog = parse_program("f x:t>n;sin x");
        let err = run(
            &prog,
            Some("f"),
            vec![Value::Text(Arc::new("nope".to_string()))],
        )
        .unwrap_err();
        assert!(err.to_string().contains("sin"), "unexpected error: {err}");
    }

    #[test]
    fn interpret_cos_non_number_errors() {
        let prog = parse_program("f x:t>n;cos x");
        let err = run(
            &prog,
            Some("f"),
            vec![Value::Text(Arc::new("nope".to_string()))],
        )
        .unwrap_err();
        assert!(err.to_string().contains("cos"), "unexpected error: {err}");
    }

    #[test]
    fn interpret_pow_non_number_errors() {
        let prog = parse_program("f x:t y:t>n;pow x y");
        let err = run(
            &prog,
            Some("f"),
            vec![
                Value::Text(Arc::new("a".to_string())),
                Value::Text(Arc::new("b".to_string())),
            ],
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("pow") && err.to_string().contains("two numbers"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn interpret_logical_and() {
        let source = "f a:b b:b>b;&a b";
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::Bool(true), Value::Bool(true)]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::Bool(true), Value::Bool(false)]
            ),
            Value::Bool(false)
        );
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::Bool(false), Value::Bool(true)]
            ),
            Value::Bool(false)
        );
    }

    #[test]
    fn interpret_logical_or() {
        let source = "f a:b b:b>b;|a b";
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::Bool(false), Value::Bool(false)]
            ),
            Value::Bool(false)
        );
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::Bool(true), Value::Bool(false)]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::Bool(false), Value::Bool(true)]
            ),
            Value::Bool(true)
        );
    }

    #[test]
    fn interpret_len_string() {
        let source = r#"f s:t>n;len s"#;
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::Text(Arc::new("hello".to_string()))]
            ),
            Value::Number(5.0)
        );
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::Text(Arc::new("".to_string()))]
            ),
            Value::Number(0.0)
        );
    }

    #[test]
    fn interpret_len_list() {
        let source = "f>n;xs=[1, 2, 3];len xs";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn interpret_list_append() {
        let source = "f>L n;xs=[1, 2];+=xs 3";
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::List(Arc::new(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0)
            ]))
        );
    }

    #[test]
    fn interpret_list_append_empty() {
        let source = "f>L n;xs=[];+=xs 42";
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::List(Arc::new(vec![Value::Number(42.0)]))
        );
    }

    #[test]
    fn interpret_list_concat() {
        let source = "f>L n;a=[1, 2];b=[3, 4];+a b";
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::List(Arc::new(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
                Value::Number(4.0)
            ]))
        );
    }

    #[test]
    fn interpret_str_integer() {
        let source = "f>t;str 42";
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::Text(Arc::new("42".to_string()))
        );
    }

    #[test]
    fn interpret_str_float() {
        let source = "f>t;str 3.14";
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::Text(Arc::new("3.14".to_string()))
        );
    }

    #[test]
    fn interpret_num_ok() {
        let source = "f>R n t;num \"42\"";
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::Ok(Box::new(Value::Number(42.0)))
        );
    }

    #[test]
    fn interpret_num_err() {
        let source = "f>R n t;num \"abc\"";
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::Err(Box::new(Value::Text(Arc::new("abc".to_string()))))
        );
    }

    #[test]
    fn interpret_abs() {
        let source = "f>n;abs -7";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(7.0));
    }

    #[test]
    fn interpret_min() {
        let source = "f>n;min 3 7";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn interpret_max() {
        let source = "f>n;max 3 7";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(7.0));
    }

    #[test]
    fn interpret_flr() {
        let source = "f>n;flr 3.7";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn interpret_cel() {
        let source = "f>n;cel 3.2";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(4.0));
    }

    #[test]
    fn interpret_index_access() {
        let source = "f>n;xs=[10, 20, 30];xs.1";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(20.0));
    }

    #[test]
    fn interpret_index_access_string() {
        let source = "f>t;xs=[\"hello\", \"world\"];xs.0";
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::Text(Arc::new("hello".to_string()))
        );
    }

    #[test]
    fn interpret_multi_function() {
        let source = "double x:n>n;*x 2\nf x:n>n;double x";
        let result = run_str(source, Some("f"), vec![Value::Number(5.0)]);
        assert_eq!(result, Value::Number(10.0));
    }

    #[test]
    fn interpret_nested_multiply_add() {
        // +*a b c → (a * b) + c
        let source = "f a:n b:n c:n>n;+*a b c";
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Number(2.0), Value::Number(3.0), Value::Number(4.0)],
        );
        assert_eq!(result, Value::Number(10.0));
    }

    #[test]
    fn interpret_nested_compare() {
        // >=+x y 100 → (x + y) >= 100
        let source = "f x:n y:n>b;>=+x y 100";
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Number(60.0), Value::Number(50.0)],
        );
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn interpret_not_as_and_operand() {
        // &!x y → (!x) & y
        let source = "f x:b y:b>b;&!x y";
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Bool(false), Value::Bool(true)],
        );
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn interpret_negate_product() {
        // -*a b → -(a * b)
        let source = "f a:n b:n>n;-*a b";
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Number(3.0), Value::Number(4.0)],
        );
        assert_eq!(result, Value::Number(-12.0));
    }

    // ── Helper for error tests ──────────────────────────────────────────

    fn run_str_err(source: &str, func: Option<&str>, args: Vec<Value>) -> String {
        let prog = parse_program(source);
        run(&prog, func, args).unwrap_err().to_string()
    }

    // ── Value::fmt Display tests ────────────────────────────────────────

    #[test]
    fn display_float() {
        assert_eq!(format!("{}", Value::Number(3.14)), "3.14");
    }

    #[test]
    fn display_integer_number() {
        assert_eq!(format!("{}", Value::Number(42.0)), "42");
    }

    #[test]
    fn display_text() {
        assert_eq!(
            format!("{}", Value::Text(Arc::new("hello".to_string()))),
            "hello"
        );
    }

    #[test]
    fn display_bool() {
        assert_eq!(format!("{}", Value::Bool(true)), "true");
        assert_eq!(format!("{}", Value::Bool(false)), "false");
    }

    #[test]
    fn display_nil() {
        assert_eq!(format!("{}", Value::Nil), "nil");
    }

    #[test]
    fn display_list() {
        let list = Value::List(Arc::new(vec![
            Value::Number(1.0),
            Value::Number(2.0),
            Value::Number(3.0),
        ]));
        assert_eq!(format!("{}", list), "[1, 2, 3]");
    }

    #[test]
    fn display_list_empty() {
        assert_eq!(format!("{}", Value::List(Arc::new(vec![]))), "[]");
    }

    #[test]
    fn display_record() {
        let mut fields = HashMap::new();
        fields.insert("x".to_string(), Value::Number(1.0));
        let rec = Value::Record {
            type_name: "point".into(),
            fields,
        };
        assert_eq!(format!("{}", rec), "point {x: 1}");
    }

    #[test]
    fn display_record_multiple_fields() {
        let mut fields = HashMap::new();
        fields.insert("a".to_string(), Value::Number(1.0));
        fields.insert("b".to_string(), Value::Number(2.0));
        let rec = Value::Record {
            type_name: "pair".into(),
            fields,
        };
        let s = format!("{}", rec);
        assert!(s.starts_with("pair {"));
        assert!(s.contains("a: 1"));
        assert!(s.contains("b: 2"));
        assert!(s.ends_with("}"));
    }

    #[test]
    fn display_ok() {
        assert_eq!(
            format!("{}", Value::Ok(Box::new(Value::Number(42.0)))),
            "~42"
        );
    }

    #[test]
    fn display_err() {
        assert_eq!(
            format!(
                "{}",
                Value::Err(Box::new(Value::Text(Arc::new("bad".to_string()))))
            ),
            "^bad"
        );
    }

    // ── Error path tests ────────────────────────────────────────────────

    #[test]
    fn err_undefined_variable() {
        let err = run_str_err("f>n;x", Some("f"), vec![]);
        assert!(err.contains("undefined variable"));
    }

    #[test]
    fn err_undefined_function() {
        let err = run_str_err("f>n;nope 1", Some("f"), vec![]);
        assert!(err.contains("undefined function"));
    }

    #[test]
    fn err_wrong_arity() {
        let err = run_str_err("f x:n>n;x", Some("f"), vec![]);
        assert!(err.contains("expected 1 args, got 0"));
    }

    #[test]
    fn err_len_wrong_arg_count() {
        let err = run_str_err("f>n;len 1 2", Some("f"), vec![]);
        assert!(err.contains("len: expected 1 arg"));
    }

    #[test]
    fn err_len_wrong_type() {
        let err = run_str_err("f x:n>n;len x", Some("f"), vec![Value::Number(1.0)]);
        assert!(err.contains("len requires string, list, or map"));
    }

    #[test]
    fn err_str_wrong_arg_count() {
        let err = run_str_err("f>t;str 1 2", Some("f"), vec![]);
        assert!(err.contains("str: expected 1 arg"));
    }

    #[test]
    fn err_str_wrong_type() {
        // str now accepts text (identity) and number; bool triggers the error
        let err = run_str_err(r#"f x:_ >t;str x"#, Some("f"), vec![Value::Bool(true)]);
        assert!(err.contains("str requires"));
    }

    #[test]
    fn err_num_wrong_arg_count() {
        let err = run_str_err(r#"f>R n t;num "1" "2""#, Some("f"), vec![]);
        assert!(err.contains("num: expected 1 arg"));
    }

    #[test]
    fn err_num_wrong_type() {
        // Post-polymorphism, only non-text-non-number args still error at
        // runtime. Bool is the canonical "neither" case.
        let err = run_str_err("f x:b>R n t;num x", Some("f"), vec![Value::Bool(true)]);
        assert!(
            err.contains("num requires text or number"),
            "expected polymorphic num error, got: {err}"
        );
    }

    #[test]
    fn num_on_number_is_identity() {
        // Polymorphic widening: numeric input is identity-wrapped Ok.
        let result = run_str("f x:n>R n t;num x", Some("f"), vec![Value::Number(42.0)]);
        assert_eq!(result, Value::Ok(Box::new(Value::Number(42.0))));
    }

    #[test]
    fn err_abs_wrong_arg_count() {
        let err = run_str_err("f>n;abs 1 2", Some("f"), vec![]);
        assert!(err.contains("abs: expected 1 arg"));
    }

    // ── num trims leading/trailing ASCII whitespace ────────────────────────
    #[test]
    fn num_trims_leading_whitespace() {
        let result = run_str(r#"f>R n t;num " 77516""#, Some("f"), vec![]);
        assert_eq!(result, Value::Ok(Box::new(Value::Number(77516.0))));
    }

    #[test]
    fn num_trims_trailing_whitespace() {
        let result = run_str(r#"f>R n t;num "77516 ""#, Some("f"), vec![]);
        assert_eq!(result, Value::Ok(Box::new(Value::Number(77516.0))));
    }

    #[test]
    fn num_trims_both_sides_signed_float() {
        let result = run_str(r#"f>R n t;num "  -3.14  ""#, Some("f"), vec![]);
        assert_eq!(result, Value::Ok(Box::new(Value::Number(-3.14))));
    }

    #[test]
    fn num_trims_scientific_notation() {
        let result = run_str(r#"f>R n t;num " 1e10 ""#, Some("f"), vec![]);
        assert_eq!(result, Value::Ok(Box::new(Value::Number(1e10))));
    }

    #[test]
    fn num_internal_whitespace_still_errors() {
        let result = run_str(r#"f>R n t;num "1 2""#, Some("f"), vec![]);
        match result {
            Value::Err(_) => {}
            other => panic!("expected Err for internal whitespace, got {:?}", other),
        }
    }

    #[test]
    fn num_empty_string_errors() {
        let result = run_str(r#"f>R n t;num """#, Some("f"), vec![]);
        match result {
            Value::Err(_) => {}
            other => panic!("expected Err for empty string, got {:?}", other),
        }
    }

    #[test]
    fn num_whitespace_only_errors() {
        let result = run_str(r#"f>R n t;num "   ""#, Some("f"), vec![]);
        match result {
            Value::Err(_) => {}
            other => panic!("expected Err for whitespace-only, got {:?}", other),
        }
    }

    #[test]
    fn err_abs_wrong_type() {
        let err = run_str_err(
            r#"f x:t>n;abs x"#,
            Some("f"),
            vec![Value::Text(Arc::new("hi".to_string()))],
        );
        assert!(err.contains("abs requires a number"));
    }

    #[test]
    fn err_min_non_number() {
        let err = run_str_err(
            r#"f a:t b:t>n;min a b"#,
            Some("f"),
            vec![
                Value::Text(Arc::new("a".to_string())),
                Value::Text(Arc::new("b".to_string())),
            ],
        );
        assert!(err.contains("min requires two numbers"));
    }

    #[test]
    fn err_max_non_number() {
        let err = run_str_err(
            r#"f a:t b:t>n;max a b"#,
            Some("f"),
            vec![
                Value::Text(Arc::new("a".to_string())),
                Value::Text(Arc::new("b".to_string())),
            ],
        );
        assert!(err.contains("max requires two numbers"));
    }

    #[test]
    fn err_flr_non_number() {
        let err = run_str_err(
            r#"f x:t>n;flr x"#,
            Some("f"),
            vec![Value::Text(Arc::new("a".to_string()))],
        );
        assert!(err.contains("flr requires a number"));
    }

    #[test]
    fn err_cel_non_number() {
        let err = run_str_err(
            r#"f x:t>n;cel x"#,
            Some("f"),
            vec![Value::Text(Arc::new("a".to_string()))],
        );
        assert!(err.contains("cel requires a number"));
    }

    #[test]
    fn err_field_not_found_on_record() {
        let err = run_str_err("f>n;r=point x:1 y:2;r.z", Some("f"), vec![]);
        assert!(err.contains("no field 'z' on record"));
    }

    #[test]
    fn err_field_access_on_non_record() {
        let err = run_str_err("f x:n>n;x.y", Some("f"), vec![Value::Number(1.0)]);
        assert!(err.contains("cannot access field"));
    }

    #[test]
    fn err_index_out_of_bounds() {
        let err = run_str_err("f>n;xs=[1, 2];xs.5", Some("f"), vec![]);
        assert!(err.contains("out of bounds"));
    }

    #[test]
    fn err_index_on_non_list() {
        let err = run_str_err("f x:n>n;x.0", Some("f"), vec![Value::Number(1.0)]);
        // x.0 is an index access; on a number it should error
        assert!(
            err.contains("index access on non-list") || err.contains("cannot access field"),
            "got: {}",
            err
        );
    }

    #[test]
    fn err_negate_non_number() {
        let err = run_str_err(r#"f>n;-"hello""#, Some("f"), vec![]);
        assert!(err.contains("cannot negate non-number"));
    }

    #[test]
    fn err_with_on_non_record() {
        let err = run_str_err("f x:n>n;x with y:1", Some("f"), vec![Value::Number(1.0)]);
        assert!(err.contains("'with' requires a record"));
    }

    // ── Missing operational tests ───────────────────────────────────────

    #[test]
    fn interpret_foreach() {
        // Sum the list by calling an accumulator pattern
        // Simple: foreach that returns last value (last element * 2)
        let source = "f>n;s=0;@x [1, 2, 3]{+s x}";
        let result = run_str(source, Some("f"), vec![]);
        // ForEach returns the last body value: 0 + 3 = 3
        // (each iteration: s is still 0 because we don't reassign, body is +s x)
        // iteration 1: +0 1 = 1, iteration 2: +0 2 = 2, iteration 3: +0 3 = 3
        assert_eq!(result, Value::Number(3.0));
    }

    #[test]
    fn interpret_subtract() {
        let source = "f a:n b:n>n;-a b";
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Number(10.0), Value::Number(3.0)],
        );
        assert_eq!(result, Value::Number(7.0));
    }

    #[test]
    fn interpret_divide() {
        let source = "f a:n b:n>n;/a b";
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Number(10.0), Value::Number(4.0)],
        );
        assert_eq!(result, Value::Number(2.5));
    }

    #[test]
    fn interpret_equals() {
        let source = "f a:n b:n>b;=a b";
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::Number(1.0), Value::Number(1.0)]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::Number(1.0), Value::Number(2.0)]
            ),
            Value::Bool(false)
        );
    }

    #[test]
    fn interpret_not_equals() {
        let source = "f a:n b:n>b;!=a b";
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::Number(1.0), Value::Number(2.0)]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::Number(1.0), Value::Number(1.0)]
            ),
            Value::Bool(false)
        );
    }

    #[test]
    fn values_equal_numbers() {
        assert!(values_equal(&Value::Number(1.0), &Value::Number(1.0)));
        assert!(!values_equal(&Value::Number(1.0), &Value::Number(2.0)));
    }

    #[test]
    fn values_equal_bools() {
        assert!(values_equal(&Value::Bool(true), &Value::Bool(true)));
        assert!(!values_equal(&Value::Bool(true), &Value::Bool(false)));
    }

    #[test]
    fn values_equal_nil() {
        assert!(values_equal(&Value::Nil, &Value::Nil));
    }

    #[test]
    fn values_equal_mismatched() {
        assert!(!values_equal(
            &Value::Number(1.0),
            &Value::Text(Arc::new("1".to_string()))
        ));
        assert!(!values_equal(&Value::Nil, &Value::Bool(false)));
    }

    #[test]
    fn is_truthy_nil() {
        assert!(!is_truthy(&Value::Nil));
    }

    #[test]
    fn is_truthy_number_zero() {
        assert!(!is_truthy(&Value::Number(0.0)));
    }

    #[test]
    fn is_truthy_number_nonzero() {
        assert!(is_truthy(&Value::Number(1.0)));
        assert!(is_truthy(&Value::Number(-5.0)));
    }

    #[test]
    fn is_truthy_text() {
        assert!(!is_truthy(&Value::Text(Arc::new("".to_string()))));
        assert!(is_truthy(&Value::Text(Arc::new("hello".to_string()))));
    }

    #[test]
    fn is_truthy_list() {
        assert!(!is_truthy(&Value::List(Arc::new(vec![]))));
        assert!(is_truthy(&Value::List(Arc::new(vec![Value::Number(1.0)]))));
    }

    #[test]
    fn is_truthy_other() {
        // Records, Ok, Err are always truthy
        assert!(is_truthy(&Value::Ok(Box::new(Value::Nil))));
        assert!(is_truthy(&Value::Err(Box::new(Value::Nil))));
    }

    #[test]
    fn interpret_literal_bool() {
        let source = "f>b;true";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Bool(true));
        let source2 = "f>b;false";
        assert_eq!(run_str(source2, Some("f"), vec![]), Value::Bool(false));
    }

    #[test]
    fn interpret_match_no_subject() {
        // ?{...} — match with no subject means subject is Nil
        let source = r#"f>n;?{_:42}"#;
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(result, Value::Number(42.0));
    }

    #[test]
    fn interpret_match_expr_with_bindings() {
        // Match expression that binds a value from Ok pattern
        let source = "f x:R n t>n;y=?x{~v:v;_:0};y";
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Ok(Box::new(Value::Number(99.0)))],
        );
        assert_eq!(result, Value::Number(99.0));
    }

    #[test]
    fn interpret_match_expr_no_arm_matches() {
        // No arm matches in a match expression → returns Nil
        let source = r#"f>n;y=?1{2:99};y"#;
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(result, Value::Nil);
    }

    #[test]
    fn interpret_typedef_in_declarations() {
        // TypeDef should be silently skipped during registration
        let source = "type point{x:n;y:n}\nf>n;42";
        let result = run_str(source, None, vec![]);
        assert_eq!(result, Value::Number(42.0));
    }

    #[test]
    fn interpret_pattern_literal_no_match() {
        // A literal pattern that does not match falls through
        let source = r#"f x:n>n;?x{1:10;2:20;_:0}"#;
        let result = run_str(source, Some("f"), vec![Value::Number(5.0)]);
        assert_eq!(result, Value::Number(0.0));
    }

    #[test]
    fn interpret_foreach_on_non_list() {
        let err = run_str_err("f x:n>n;@i x{i}", Some("f"), vec![Value::Number(1.0)]);
        assert!(err.contains("foreach requires a list"));
    }

    #[test]
    fn interpret_tool_call() {
        let source =
            "tool fetch\"HTTP GET\" url:t>R _ t timeout:30\nf>R _ t;fetch \"http://example.com\"";
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(result, Value::Ok(Box::new(Value::Nil)));
    }

    #[test]
    fn interpret_typedef_not_callable() {
        // TypeDef names are not registered as functions, so calling one
        // results in an "undefined function" error
        let source = "type point{x:n;y:n}\nf>n;point 1 2";
        let err = run_str_err(source, Some("f"), vec![]);
        assert!(
            err.contains("undefined function")
                || err.contains("type")
                || err.contains("not callable"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn interpret_greater_than() {
        let source = "f a:n b:n>b;>a b";
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::Number(5.0), Value::Number(3.0)]
            ),
            Value::Bool(true)
        );
    }

    #[test]
    fn interpret_less_than() {
        let source = "f a:n b:n>b;<a b";
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::Number(3.0), Value::Number(5.0)]
            ),
            Value::Bool(true)
        );
    }

    #[test]
    fn interpret_less_or_equal() {
        let source = "f a:n b:n>b;<=a b";
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::Number(3.0), Value::Number(3.0)]
            ),
            Value::Bool(true)
        );
    }

    #[test]
    fn interpret_unsupported_binop() {
        let source = "f a:b b:b>b;-a b";
        let err = run_str_err(
            source,
            Some("f"),
            vec![Value::Bool(true), Value::Bool(false)],
        );
        assert!(
            err.contains("unsupported operation"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn interpret_foreach_early_return() {
        // Use `ret` inside braced guard for early return from loop
        let source = "f xs:L n>n;@x xs{>=x 3{ret x}};0";
        let result = run_str(
            source,
            Some("f"),
            vec![Value::List(Arc::new(vec![
                Value::Number(1.0),
                Value::Number(5.0),
                Value::Number(2.0),
            ]))],
        );
        assert_eq!(result, Value::Number(5.0));
    }

    #[test]
    fn interpret_match_not_last_stmt() {
        let source = "f x:n>n;?x{0:x;_:x};+x 1";
        let result = run_str(source, Some("f"), vec![Value::Number(5.0)]);
        assert_eq!(result, Value::Number(6.0));
    }

    #[test]
    fn interpret_match_expr_no_subject() {
        let source = r#"f>t;x=?{_:"always"};x"#;
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(result, Value::Text(Arc::new("always".to_string())));
    }

    #[test]
    fn interpret_pattern_ok_no_match() {
        let source = r#"f>t;x=^"err";?x{~v:v;_:"default"}"#;
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(result, Value::Text(Arc::new("default".to_string())));
    }

    #[test]
    fn interpret_match_stmt_no_arm_matches() {
        // Standalone match statement (Stmt::Match) where no arm matches → Ok(None) at L307
        // The match is not the last stmt; function continues to 0 after no match.
        let source = "f x:n>n;?x{1:99};0";
        let result = run_str(source, Some("f"), vec![Value::Number(5.0)]);
        assert_eq!(result, Value::Number(0.0));
    }

    #[test]
    fn interpret_match_arm_body_with_guard_return() {
        // Match arm body contains a guard that fires → BodyResult::Return propagates (L297)
        // When x=1: pattern 1 matches, arm body has guard >=x 0 which is true → returns 42
        // The match is not the last stmt (y=0 is first), so BodyResult::Return propagation matters
        // Note: arm body syntax uses `;` not braces: `1:>=x 0{42}` means guard in arm 1 body
        let source = "f x:n>n;y=0;?x{1:>=x 0{42};_:0}";
        let result = run_str(source, Some("f"), vec![Value::Number(1.0)]);
        assert_eq!(result, Value::Number(42.0));
    }

    // L239: call_function with Decl::TypeDef → "is a type, not callable"
    #[test]
    fn call_typedef_as_function() {
        let mut env = Env::new();
        // Manually insert a TypeDef into the env's functions map
        env.functions.insert(
            "point".to_string(),
            Decl::TypeDef {
                name: "point".to_string(),
                fields: vec![],
                span: Span::UNKNOWN,
            },
        );
        let result = call_function(&mut env, "point", vec![]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("is a type, not callable"),
            "got: {}",
            err
        );
    }

    // L242: call_function with Decl::Error → "failed to parse"
    #[test]
    fn call_error_decl_as_function() {
        let mut env = Env::new();
        // Manually insert a Decl::Error into the env's functions map
        env.functions.insert(
            "broken".to_string(),
            Decl::Error {
                span: Span::UNKNOWN,
            },
        );
        let result = call_function(&mut env, "broken", vec![]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("failed to parse"), "got: {}", err);
    }

    fn make_result_program(inner_body: Vec<Spanned<Stmt>>) -> Program {
        // Build: inner x:n>R n t;{inner_body}  outer x:n>R n t;d=inner! x;~d
        Program {
            declarations: vec![
                Decl::Function {
                    type_params: vec![],
                    name: "inner".to_string(),
                    params: vec![Param {
                        name: "x".to_string(),
                        ty: Type::Number,
                    }],
                    return_type: Type::Result(Box::new(Type::Number), Box::new(Type::Text)),
                    effect_set: None,
                    body: inner_body,
                    span: Span::UNKNOWN,
                },
                Decl::Function {
                    type_params: vec![],
                    name: "outer".to_string(),
                    params: vec![Param {
                        name: "x".to_string(),
                        ty: Type::Number,
                    }],
                    return_type: Type::Result(Box::new(Type::Number), Box::new(Type::Text)),
                    effect_set: None,
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
            parse_failed_fns: Default::default(),
        }
    }

    #[test]
    fn unwrap_ok_path() {
        let prog = make_result_program(vec![Spanned::unknown(Stmt::Expr(Expr::Ok(Box::new(
            Expr::Ref("x".to_string()),
        ))))]);
        let result = run(&prog, Some("outer"), vec![Value::Number(42.0)]).unwrap();
        assert_eq!(result, Value::Ok(Box::new(Value::Number(42.0))));
    }

    #[test]
    fn unwrap_err_path() {
        let prog = make_result_program(vec![Spanned::unknown(Stmt::Expr(Expr::Err(Box::new(
            Expr::Literal(Literal::Text("fail".to_string())),
        ))))]);
        let result = run(&prog, Some("outer"), vec![Value::Number(42.0)]).unwrap();
        assert_eq!(
            result,
            Value::Err(Box::new(Value::Text(Arc::new("fail".to_string()))))
        );
    }

    #[test]
    fn unwrap_nested_propagation() {
        // c returns Err, b uses ! to call c, a uses ! to call b
        let unwrap_body = |callee: &str| {
            vec![
                Spanned::unknown(Stmt::Let {
                    name: "d".to_string(),
                    value: Expr::Call {
                        function: callee.to_string(),
                        args: vec![Expr::Ref("x".to_string())],
                        unwrap: UnwrapMode::Propagate,
                    },
                }),
                Spanned::unknown(Stmt::Expr(Expr::Ok(Box::new(Expr::Ref("d".to_string()))))),
            ]
        };
        let rnt = Type::Result(Box::new(Type::Number), Box::new(Type::Text));
        let prog = Program {
            declarations: vec![
                Decl::Function {
                    type_params: vec![],
                    name: "c".to_string(),
                    params: vec![Param {
                        name: "x".to_string(),
                        ty: Type::Number,
                    }],
                    return_type: rnt.clone(),
                    effect_set: None,
                    body: vec![Spanned::unknown(Stmt::Expr(Expr::Err(Box::new(
                        Expr::Literal(Literal::Text("deep".to_string())),
                    ))))],
                    span: Span::UNKNOWN,
                },
                Decl::Function {
                    type_params: vec![],
                    name: "b".to_string(),
                    params: vec![Param {
                        name: "x".to_string(),
                        ty: Type::Number,
                    }],
                    return_type: rnt.clone(),
                    effect_set: None,
                    body: unwrap_body("c"),
                    span: Span::UNKNOWN,
                },
                Decl::Function {
                    type_params: vec![],
                    name: "a".to_string(),
                    params: vec![Param {
                        name: "x".to_string(),
                        ty: Type::Number,
                    }],
                    return_type: rnt,
                    effect_set: None,
                    body: unwrap_body("b"),
                    span: Span::UNKNOWN,
                },
            ],
            source: None,
            parse_failed_fns: Default::default(),
        };
        let result = run(&prog, Some("a"), vec![Value::Number(1.0)]).unwrap();
        assert_eq!(
            result,
            Value::Err(Box::new(Value::Text(Arc::new("deep".to_string()))))
        );
    }

    // ---- Braceless guards ----

    #[test]
    fn interpret_braceless_guard() {
        let source = r#"cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze""#;
        assert_eq!(
            run_str(source, Some("cls"), vec![Value::Number(1500.0)]),
            Value::Text(Arc::new("gold".to_string()))
        );
        assert_eq!(
            run_str(source, Some("cls"), vec![Value::Number(750.0)]),
            Value::Text(Arc::new("silver".to_string()))
        );
        assert_eq!(
            run_str(source, Some("cls"), vec![Value::Number(100.0)]),
            Value::Text(Arc::new("bronze".to_string()))
        );
    }

    #[test]
    fn interpret_braceless_guard_factorial() {
        let source = "fac n:n>n;<=n 1 1;r=fac -n 1;*n r";
        assert_eq!(
            run_str(source, Some("fac"), vec![Value::Number(5.0)]),
            Value::Number(120.0)
        );
    }

    #[test]
    fn interpret_braceless_guard_fibonacci() {
        // fib(10) recurses deeply enough to blow the 2 MiB default test-thread
        // stack on some platforms. Run it on an explicit 8 MiB stack so the
        // test passes independently of RUST_MIN_STACK. Mirrors the pattern used
        // in tests/parser_depth_cap.rs.
        std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(|| {
                let source = "fib n:n>n;<=n 1 n;a=fib -n 1;b=fib -n 2;+a b";
                assert_eq!(
                    run_str(source, Some("fib"), vec![Value::Number(10.0)]),
                    Value::Number(55.0)
                );
            })
            .expect("spawn test thread")
            .join()
            .expect("thread panicked");
    }

    #[test]
    fn interpret_spl_basic() {
        let source = r#"f>L t;spl "a,b,c" ",""#;
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::List(Arc::new(vec![
                Value::Text(Arc::new("a".to_string())),
                Value::Text(Arc::new("b".to_string())),
                Value::Text(Arc::new("c".to_string())),
            ]))
        );
    }

    #[test]
    fn interpret_spl_empty() {
        let source = r#"f>L t;spl "" ",""#;
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::List(Arc::new(vec![Value::Text(Arc::new("".to_string()))]))
        );
    }

    #[test]
    fn interpret_cat_basic() {
        let source = "f items:L t>t;cat items \",\"";
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::List(Arc::new(vec![
                    Value::Text(Arc::new("a".to_string())),
                    Value::Text(Arc::new("b".to_string())),
                    Value::Text(Arc::new("c".to_string())),
                ]))]
            ),
            Value::Text(Arc::new("a,b,c".to_string()))
        );
    }

    #[test]
    fn interpret_cat_empty_list() {
        let source = "f items:L t>t;cat items \"-\"";
        assert_eq!(
            run_str(source, Some("f"), vec![Value::List(Arc::new(vec![]))]),
            Value::Text(Arc::new("".to_string()))
        );
    }

    #[test]
    fn interpret_has_list() {
        let source = "f xs:L n x:n>b;has xs x";
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![
                    Value::List(Arc::new(vec![Value::Number(1.0), Value::Number(2.0)])),
                    Value::Number(2.0)
                ]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![
                    Value::List(Arc::new(vec![Value::Number(1.0)])),
                    Value::Number(5.0)
                ]
            ),
            Value::Bool(false)
        );
    }

    #[test]
    fn interpret_has_text() {
        let source = r#"f s:t needle:t>b;has s needle"#;
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![
                    Value::Text(Arc::new("hello world".to_string())),
                    Value::Text(Arc::new("world".to_string()))
                ]
            ),
            Value::Bool(true)
        );
    }

    #[test]
    fn interpret_hd_list() {
        let source = "f>n;xs=[10, 20, 30];hd xs";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(10.0));
    }

    #[test]
    fn interpret_tl_list() {
        let source = "f>L n;xs=[10, 20, 30];tl xs";
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::List(Arc::new(vec![Value::Number(20.0), Value::Number(30.0)]))
        );
    }

    #[test]
    fn interpret_hd_text() {
        let source = r#"f s:t>t;hd s"#;
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::Text(Arc::new("hello".to_string()))]
            ),
            Value::Text(Arc::new("h".to_string()))
        );
    }

    #[test]
    fn interpret_tl_text() {
        let source = r#"f s:t>t;tl s"#;
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::Text(Arc::new("hello".to_string()))]
            ),
            Value::Text(Arc::new("ello".to_string()))
        );
    }

    #[test]
    fn interpret_rev_list() {
        let source = "f>L n;rev [1, 2, 3]";
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::List(Arc::new(vec![
                Value::Number(3.0),
                Value::Number(2.0),
                Value::Number(1.0)
            ]))
        );
    }

    #[test]
    fn interpret_rev_text() {
        let source = r#"f>t;rev "abc""#;
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::Text(Arc::new("cba".to_string()))
        );
    }

    #[test]
    fn interpret_srt_numbers() {
        let source = "f>L n;srt [3, 1, 2]";
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::List(Arc::new(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0)
            ]))
        );
    }

    #[test]
    fn interpret_srt_text_list() {
        let source = r#"f>L t;srt ["c", "a", "b"]"#;
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::List(Arc::new(vec![
                Value::Text(Arc::new("a".to_string())),
                Value::Text(Arc::new("b".to_string())),
                Value::Text(Arc::new("c".to_string()))
            ]))
        );
    }

    #[test]
    fn interpret_srt_text_string() {
        let source = r#"f>t;srt "cab""#;
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::Text(Arc::new("abc".to_string()))
        );
    }

    #[test]
    fn interpret_slc_list() {
        let source = "f>L n;slc [1, 2, 3, 4, 5] 1 3";
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::List(Arc::new(vec![Value::Number(2.0), Value::Number(3.0)]))
        );
    }

    #[test]
    fn interpret_slc_text() {
        let source = r#"f>t;slc "hello" 1 4"#;
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::Text(Arc::new("ell".to_string()))
        );
    }

    #[test]
    fn interpret_slc_clamped() {
        let source = "f>L n;slc [1, 2, 3] 1 100";
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::List(Arc::new(vec![Value::Number(2.0), Value::Number(3.0)]))
        );
    }

    #[test]
    fn interpret_ternary_true() {
        let source = r#"f x:n>t;=x 1{"yes"}{"no"}"#;
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Number(1.0)]),
            Value::Text(Arc::new("yes".to_string()))
        );
    }

    #[test]
    fn interpret_ternary_false() {
        let source = r#"f x:n>t;=x 1{"yes"}{"no"}"#;
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Number(2.0)]),
            Value::Text(Arc::new("no".to_string()))
        );
    }

    #[test]
    fn interpret_ternary_no_early_return() {
        let source = r#"f x:n>n;=x 0{10}{20};+x 1"#;
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Number(0.0)]),
            Value::Number(1.0)
        );
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Number(6.0)
        );
    }

    #[test]
    fn interpret_braced_guard_no_early_return() {
        // Braced guard is conditional execution. The body value is discarded
        // from the function's return path; the function falls through to the
        // next statement.
        let source = "f x:n>n;=x 0{99};+x 1";
        // x=0: =x 0 true, {99} runs, value discarded, falls through to +0 1 = 1
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Number(0.0)]),
            Value::Number(1.0)
        );
        // x=5: guard false, falls through to +5 1 = 6
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Number(6.0)
        );
    }

    #[test]
    fn interpret_braceless_guard_still_returns_early() {
        // Braceless guard causes early return.
        let source = "f x:n>n;=x 0 99;+x 1";
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Number(0.0)]),
            Value::Number(99.0)
        );
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Number(6.0)
        );
    }

    #[test]
    fn interpret_braced_guard_in_loop_no_early_return() {
        // Braced guard inside a loop does NOT early-return — the canonical
        // find-max idiom `m=xs.0;@x xs{>x m{m=x}};m` works as written.
        let source = "mx xs:L n>n;m=xs.0;@x xs{>x m{m=x}};+m 0";
        let result = run_str(
            source,
            Some("mx"),
            vec![Value::List(Arc::new(vec![
                Value::Number(3.0),
                Value::Number(1.0),
                Value::Number(5.0),
            ]))],
        );
        assert_eq!(result, Value::Number(5.0));
    }

    #[test]
    fn interpret_braceless_guard_early_return_factorial() {
        // Braceless guard still early-returns — factorial
        let source = "f x:n>n;<=x 1 1;r=f -x 1;*x r";
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Number(120.0)
        );
    }

    #[test]
    fn interpret_ternary_let_binding() {
        // Ternary let binding: v=cond{then}{else}
        let source = "f x:n>n;v=<x 0{- 0 x}{x};v";
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Number(-3.0)]),
            Value::Number(3.0)
        );
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Number(7.0)]),
            Value::Number(7.0)
        );
    }

    #[test]
    fn interpret_ternary_negated() {
        let source = r#"f x:n>t;!=x 1{"not one"}{"one"}"#;
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Number(1.0)]),
            Value::Text(Arc::new("one".to_string()))
        );
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Number(2.0)]),
            Value::Text(Arc::new("not one".to_string()))
        );
    }

    #[test]
    fn interpret_ret_early_return() {
        let source = r#"f x:n>n;>x 0{ret x};0"#;
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Number(5.0)
        );
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Number(-1.0)]),
            Value::Number(0.0)
        );
    }

    #[test]
    fn interpret_pipe_simple() {
        // str x>>len desugars to len(str(x))
        let source = "f x:n>n;str x>>len";
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Number(42.0)]),
            Value::Number(2.0)
        );
    }

    #[test]
    fn interpret_pipe_chain() {
        let source = "dbl x:n>n;*x 2\nadd1 x:n>n;+x 1\nf x:n>n;dbl x>>add1";
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Number(11.0)
        );
    }

    #[test]
    fn interpret_pipe_with_extra_args() {
        // add x 1>>add 2 → add(2, add(x, 1))
        let source = "add a:n b:n>n;+a b\nf x:n>n;add x 1>>add 2";
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Number(8.0)
        );
    }

    #[test]
    fn interpret_ret_in_foreach() {
        let source = "f xs:L n>n;@x xs{>=x 10{ret x}};0";
        let list = Value::List(Arc::new(vec![
            Value::Number(1.0),
            Value::Number(15.0),
            Value::Number(3.0),
        ]));
        assert_eq!(run_str(source, Some("f"), vec![list]), Value::Number(15.0));
    }

    #[test]
    fn interpret_while_basic() {
        // Sum 1..5 using while loop
        let source = "f>n;i=0;s=0;wh <i 5{i=+i 1;s=+s i};s";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(15.0));
    }

    #[test]
    fn interpret_while_zero_iterations() {
        let source = "f>n;wh false{42};0";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(0.0));
    }

    #[test]
    fn interpret_nil_coalesce_nil() {
        // Function returns nil when guard doesn't fire, ?? falls back
        let source = "mk x:n>n;>=x 1{x}\nf>n;x=mk 0;x??42";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(42.0));
    }

    #[test]
    fn interpret_nil_coalesce_non_nil() {
        // Non-nil value passes through
        let source = "f>n;x=10;x??42";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(10.0));
    }

    #[test]
    fn interpret_nil_coalesce_chain() {
        let source = "mk x:n>n;>=x 1{x}\nf>n;a=mk 0;b=mk 0;a??b??99";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(99.0));
    }

    #[test]
    fn interpret_safe_field_on_nil() {
        // Safe field access on nil returns nil
        let source = "mk x:n>n;>=x 1{x}\nf>n;v=mk 0;v.?name??99";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(99.0));
    }

    #[test]
    fn interpret_safe_field_on_value() {
        // Safe field access on record returns field value
        let source = "f>n;p=pt x:5;p.?x\ntype pt{x:n}";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(5.0));
    }

    #[test]
    fn interpret_safe_field_chained() {
        // Chained safe navigation: nil propagates through chain
        let source = "mk x:n>n;>=x 1{x}\nf>n;v=mk 0;v.?a.?b??77";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(77.0));
    }

    #[test]
    fn interpret_while_with_ret() {
        // Early return from while loop
        let source = "f>n;i=0;wh true{i=+i 1;>=i 3{ret i}};0";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn interpret_while_brk() {
        // brk exits while loop
        let source = "f>n;i=0;wh true{i=+i 1;>=i 3{brk}};i";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn interpret_while_brk_value() {
        // brk with value — value is discarded, loop exits
        let source = "f>n;i=0;wh true{i=+i 1;>=i 3{brk 99}};i";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn interpret_while_cnt() {
        // cnt skips rest of body, continues loop
        let source = "f>n;i=0;s=0;wh <i 5{i=+i 1;>=i 3{cnt};s=+s i};s";
        // i goes 1,2,3,4,5 — cnt when i>=3 so s += i only for i=1,2 → s=3
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn interpret_foreach_brk() {
        // brk with value exits foreach, foreach returns the break value
        let source = "f>n;@x [1,2,3,4,5]{>=x 3{brk x};x}";
        // x=1 → value 1, x=2 → value 2, x=3 → brk 3
        // Break value (3) becomes last, foreach returns 3
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn interpret_foreach_cnt() {
        // cnt in foreach skips rest of body for that iteration
        // Body value: x*2 — but when x>=3, cnt skips it
        // Last non-skipped value = 2*2 = 4 (from x=2)... but then x=3,4,5 continue with no value
        // Actually: last = Nil from unfinished iterations? No — continue doesn't update last.
        // x=1 → value 2, x=2 → value 4, x=3 → cnt (last stays 4), x=4 → cnt, x=5 → cnt
        let source = "f>n;@x [1,2,3,4,5]{>=x 3{cnt};*x 2}";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(4.0));
    }

    #[test]
    fn interpret_rnd_no_args() {
        let source = "f>n;rnd";
        let result = run_str(source, Some("f"), vec![]);
        let Value::Number(n) = result else {
            panic!("expected Number")
        };
        assert!((0.0..1.0).contains(&n), "rnd should be in [0,1), got {n}");
    }

    #[test]
    fn interpret_rnd_two_args() {
        let source = "f>n;rnd 1 10";
        let result = run_str(source, Some("f"), vec![]);
        let Value::Number(n) = result else {
            panic!("expected Number")
        };
        assert!(
            (1.0..=10.0).contains(&n),
            "rnd 1 10 should be in [1,10], got {n}"
        );
        assert_eq!(n, n.floor(), "rnd with two args should return integer");
    }

    #[test]
    fn interpret_rnd_same_bounds() {
        let source = "f>n;rnd 5 5";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(5.0));
    }

    #[test]
    fn interpret_now() {
        let source = "f>n;now";
        let result = run_str(source, Some("f"), vec![]);
        let Value::Number(n) = result else {
            panic!("expected Number")
        };
        assert!(
            n > 1_000_000_000.0,
            "now should be a reasonable unix timestamp, got {n}"
        );
    }

    // ── env builtin tests ─────────────────────────────────────────────

    #[test]
    fn interpret_env_existing_var() {
        let _guard = ENV_TEST_MUTEX.lock().unwrap();
        unsafe {
            std::env::set_var("ILO_TEST_VAR", "hello");
        }
        let source = r#"f k:t>R t t;env k"#;
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Text(Arc::new("ILO_TEST_VAR".to_string()))],
        );
        assert_eq!(
            result,
            Value::Ok(Box::new(Value::Text(Arc::new("hello".to_string()))))
        );
        unsafe {
            std::env::remove_var("ILO_TEST_VAR");
        }
    }

    #[test]
    fn interpret_env_missing_var() {
        let _guard = ENV_TEST_MUTEX.lock().unwrap();
        let source = r#"f k:t>R t t;env k"#;
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Text(Arc::new("ILO_NONEXISTENT_12345".to_string()))],
        );
        let Value::Err(inner) = result else {
            panic!("expected Err")
        };
        let Value::Text(s) = *inner else {
            panic!("expected Text")
        };
        assert!(s.contains("not set"), "got: {s}");
    }

    #[test]
    fn interpret_env_unwrap() {
        let _guard = ENV_TEST_MUTEX.lock().unwrap();
        unsafe {
            std::env::set_var("ILO_TEST_UNWRAP", "world");
        }
        let source = r#"f k:t>R t t;~(env! k)"#;
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Text(Arc::new("ILO_TEST_UNWRAP".to_string()))],
        );
        assert_eq!(
            result,
            Value::Ok(Box::new(Value::Text(Arc::new("world".to_string()))))
        );
        unsafe {
            std::env::remove_var("ILO_TEST_UNWRAP");
        }
    }

    // ── Range iteration tests ───────────────────────────────────────────

    #[test]
    fn interpret_range_basic() {
        // @i 0..3{i} → iterates 0, 1, 2; last value is 2
        let source = "f>n;@i 0..3{i}";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(2.0));
    }

    #[test]
    fn interpret_range_accumulate() {
        // Last body value: +0 i where i goes 0,1,2 → last is +0 2 = 2
        // s is in outer scope, s=+s i creates s in inner scope each time
        // So just check the body expression result
        let source = "f>n;@i 0..3{+i 1}";
        // last body val: +2 1 = 3
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn interpret_range_empty() {
        // start >= end → never executes, loop returns Nil
        let source = "f>n;@i 5..3{99}";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Nil);
    }

    #[test]
    fn interpret_range_dynamic_end() {
        // Dynamic end from parameter; body returns i
        let source = "f n:n>n;@i 0..n{i}";
        // n=4, iterates 0,1,2,3 → last body value is 3
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Number(4.0)]),
            Value::Number(3.0)
        );
    }

    #[test]
    fn interpret_range_brk() {
        // Break at i >= 3 with value
        let source = "f>n;@i 0..10{>=i 3{brk i};i}";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn interpret_range_cnt() {
        // cnt skips rest of body. Body is: =i 2{cnt};*i 10
        // i=0: *0 10 = 0, i=1: *1 10 = 10, i=2: cnt (skip), i=3: *3 10 = 30, i=4: *4 10 = 40
        // last body value = 40
        let source = "f>n;@i 0..5{=i 2{cnt};*i 10}";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(40.0));
    }

    #[test]
    fn interpret_range_as_index() {
        // Use range variable to index a list: xs.i doesn't work with dynamic i
        // Index access is only for literals. So just test basic indexing pattern.
        let source = "f>n;@i 0..3{*i i}";
        // i=0: 0, i=1: 1, i=2: 4 → last = 4
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(4.0));
    }

    // ---- Builtin error-path coverage tests ----

    #[test]
    fn err_spl_non_text_first() {
        let err = run_str_err(
            "f x:n y:t>L t;spl x y",
            Some("f"),
            vec![Value::Number(1.0), Value::Text(Arc::new("a".to_string()))],
        );
        assert!(err.contains("spl requires two text args"), "got: {err}");
    }

    #[test]
    fn err_spl_non_text_second() {
        let err = run_str_err(
            "f x:t y:n>L t;spl x y",
            Some("f"),
            vec![Value::Text(Arc::new("a-b".to_string())), Value::Number(1.0)],
        );
        assert!(err.contains("spl requires two text args"), "got: {err}");
    }

    #[test]
    fn err_cat_non_text_items() {
        let err = run_str_err("f>t;cat [1,2,3] \",\"", Some("f"), vec![]);
        assert!(err.contains("cat: list items must be text"), "got: {err}");
    }

    #[test]
    fn err_cat_wrong_arg_types() {
        let err = run_str_err(
            "f x:n y:n>t;cat x y",
            Some("f"),
            vec![Value::Number(1.0), Value::Number(2.0)],
        );
        assert!(
            err.contains("cat requires a list and text separator"),
            "got: {err}"
        );
    }

    #[test]
    fn err_has_text_non_text_needle() {
        let err = run_str_err(
            "f x:t y:n>b;has x y",
            Some("f"),
            vec![
                Value::Text(Arc::new("hello".to_string())),
                Value::Number(1.0),
            ],
        );
        assert!(
            err.contains("text search requires text needle"),
            "got: {err}"
        );
    }

    #[test]
    fn err_has_wrong_first_arg() {
        let err = run_str_err(
            "f x:n y:n>b;has x y",
            Some("f"),
            vec![Value::Number(1.0), Value::Number(2.0)],
        );
        assert!(err.contains("has requires a list or text"), "got: {err}");
    }

    #[test]
    fn err_hd_empty_list() {
        let err = run_str_err("f>n;hd []", Some("f"), vec![]);
        assert!(err.contains("hd: empty list"), "got: {err}");
    }

    #[test]
    fn err_hd_empty_text() {
        let err = run_str_err("f>t;hd \"\"", Some("f"), vec![]);
        assert!(err.contains("hd: empty text"), "got: {err}");
    }

    #[test]
    fn err_hd_wrong_type() {
        let err = run_str_err("f x:n>n;hd x", Some("f"), vec![Value::Number(1.0)]);
        assert!(err.contains("hd requires a list or text"), "got: {err}");
    }

    #[test]
    fn err_tl_empty_list() {
        let err = run_str_err("f>L n;tl []", Some("f"), vec![]);
        assert!(err.contains("tl: empty list"), "got: {err}");
    }

    #[test]
    fn err_tl_empty_text() {
        let err = run_str_err("f>t;tl \"\"", Some("f"), vec![]);
        assert!(err.contains("tl: empty text"), "got: {err}");
    }

    #[test]
    fn err_tl_wrong_type() {
        let err = run_str_err("f x:n>n;tl x", Some("f"), vec![Value::Number(1.0)]);
        assert!(err.contains("tl requires a list or text"), "got: {err}");
    }

    #[test]
    fn err_rev_wrong_type() {
        let err = run_str_err("f x:n>n;rev x", Some("f"), vec![Value::Number(1.0)]);
        assert!(err.contains("rev requires a list or text"), "got: {err}");
    }

    #[test]
    fn err_srt_mixed_types() {
        let err = run_str_err("f>L n;srt [1,\"a\"]", Some("f"), vec![]);
        assert!(
            err.contains("srt: list must contain all numbers or all text"),
            "got: {err}"
        );
    }

    #[test]
    fn err_srt_wrong_type() {
        let err = run_str_err("f x:n>n;srt x", Some("f"), vec![Value::Number(1.0)]);
        assert!(err.contains("srt requires a list or text"), "got: {err}");
    }

    #[test]
    fn err_slc_wrong_first_arg() {
        let err = run_str_err("f x:n>n;slc x 0 1", Some("f"), vec![Value::Number(1.0)]);
        assert!(err.contains("slc requires a list or text"), "got: {err}");
    }

    #[test]
    fn err_slc_non_number_start() {
        let err = run_str_err(
            "f x:t y:t>t;slc x y 1",
            Some("f"),
            vec![
                Value::Text(Arc::new("hi".to_string())),
                Value::Text(Arc::new("a".to_string())),
            ],
        );
        assert!(
            err.contains("slc: start index must be a number"),
            "got: {err}"
        );
    }

    #[test]
    fn err_slc_non_number_end() {
        let err = run_str_err(
            "f x:t y:t>t;slc x 0 y",
            Some("f"),
            vec![
                Value::Text(Arc::new("hi".to_string())),
                Value::Text(Arc::new("a".to_string())),
            ],
        );
        assert!(
            err.contains("slc: end index must be a number"),
            "got: {err}"
        );
    }

    #[test]
    fn err_rnd_lower_gt_upper() {
        let err = run_str_err("f>n;rnd 10 1", Some("f"), vec![]);
        assert!(err.contains("rnd: lower bound"), "got: {err}");
        assert!(err.contains("upper bound"), "got: {err}");
    }

    #[test]
    fn err_rnd_wrong_arg_types() {
        let err = run_str_err(
            "f x:t y:t>n;rnd x y",
            Some("f"),
            vec![
                Value::Text(Arc::new("a".to_string())),
                Value::Text(Arc::new("b".to_string())),
            ],
        );
        assert!(err.contains("rnd requires two numbers"), "got: {err}");
    }

    #[test]
    fn err_get_non_text_arg() {
        let err = run_str_err("f x:n>R t t;get x", Some("f"), vec![Value::Number(1.0)]);
        assert!(err.contains("get requires text"), "got: {err}");
    }

    #[test]
    fn ok_srt_empty_list() {
        let source = "f>L n;srt []";
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::List(Arc::new(vec![]))
        );
    }

    // ---- Destructuring bind tests ----

    #[test]
    fn destructure_basic() {
        let source = "type pt{x:n;y:n} f>n;p=pt x:3 y:4;{x;y}=p;+x y";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(7.0));
    }

    #[test]
    fn destructure_single_field() {
        let source = "type pt{x:n;y:n} f>n;p=pt x:10 y:20;{x}=p;x";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(10.0));
    }

    #[test]
    fn destructure_with_text_fields() {
        let source =
            "type usr{name:t;email:t} f>t;u=usr name:\"alice\" email:\"a@b\";{name;email}=u;name";
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::Text(Arc::new("alice".to_string()))
        );
    }

    #[test]
    fn destructure_in_loop() {
        // Destructure inside a foreach — last iteration value is returned
        let source = "type pt{x:n;y:n} f>n;ps=[pt x:1 y:2,pt x:3 y:4];@p ps{{x;y}=p;+x y}";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Number(7.0));
    }

    #[test]
    fn destructure_non_record_error() {
        let err = run_str_err("f x:n>n;{a}=x;a", Some("f"), vec![Value::Number(5.0)]);
        assert!(
            err.contains("destructure requires a record"),
            "got: {}",
            err
        );
    }

    #[test]
    fn destructure_missing_field_error() {
        let source = "type pt{x:n;y:n} f>n;p=pt x:3 y:4;{x;z}=p;x";
        let err = run_str_err(source, Some("f"), vec![]);
        assert!(err.contains("no field 'z'"), "got: {}", err);
    }

    // ── JSON builtins ───────────────────────────────────────────────────

    #[test]
    fn interp_jp_object() {
        let source = r#"f j:t p:t>R t t;jpth j p"#;
        let result = run_str(
            source,
            Some("f"),
            vec![
                Value::Text(Arc::new(r#"{"name":"alice"}"#.to_string())),
                Value::Text(Arc::new("name".to_string())),
            ],
        );
        assert_eq!(
            result,
            Value::Ok(Box::new(Value::Text(Arc::new("alice".to_string()))))
        );
    }

    #[test]
    fn interp_jp_nested() {
        let source = r#"f j:t p:t>R t t;jpth j p"#;
        let result = run_str(
            source,
            Some("f"),
            vec![
                Value::Text(Arc::new(r#"{"user":{"name":"bob"}}"#.to_string())),
                Value::Text(Arc::new("user.name".to_string())),
            ],
        );
        assert_eq!(
            result,
            Value::Ok(Box::new(Value::Text(Arc::new("bob".to_string()))))
        );
    }

    #[test]
    fn interp_jp_array_index() {
        // jpth on a numeric leaf now returns Number, not Text — the 0.12.1
        // typed-jpth change. Same source / args; only the asserted leaf
        // shape changes (was Text("20"), now Number(20.0)).
        let source = r#"f j:t p:t>R _ t;jpth j p"#;
        let result = run_str(
            source,
            Some("f"),
            vec![
                Value::Text(Arc::new(r#"{"items":[10,20,30]}"#.to_string())),
                Value::Text(Arc::new("items.1".to_string())),
            ],
        );
        assert_eq!(result, Value::Ok(Box::new(Value::Number(20.0))));
    }

    #[test]
    fn interp_jp_missing_key() {
        let source = r#"f j:t p:t>R t t;jpth j p"#;
        let result = run_str(
            source,
            Some("f"),
            vec![
                Value::Text(Arc::new(r#"{"a":1}"#.to_string())),
                Value::Text(Arc::new("b".to_string())),
            ],
        );
        let Value::Err(e) = result else {
            panic!("expected Err")
        };
        assert!(e.to_string().contains("key not found"), "got: {}", e);
    }

    #[test]
    fn interp_jp_invalid_json() {
        let source = r#"f j:t p:t>R t t;jpth j p"#;
        let result = run_str(
            source,
            Some("f"),
            vec![
                Value::Text(Arc::new("not json".to_string())),
                Value::Text(Arc::new("x".to_string())),
            ],
        );
        assert!(matches!(result, Value::Err(_)));
    }

    #[test]
    fn interp_jp_unwrap() {
        let source = r#"f j:t p:t>t;jpth! j p"#;
        let result = run_str(
            source,
            Some("f"),
            vec![
                Value::Text(Arc::new(r#"{"x":"hello"}"#.to_string())),
                Value::Text(Arc::new("x".to_string())),
            ],
        );
        assert_eq!(result, Value::Text(Arc::new("hello".to_string())));
    }

    #[test]
    fn interp_jd_number() {
        let source = "f x:n>t;jdmp x";
        let result = run_str(source, Some("f"), vec![Value::Number(42.0)]);
        assert_eq!(result, Value::Text(Arc::new("42".to_string())));
    }

    #[test]
    fn interp_jd_text() {
        let source = r#"f x:t>t;jdmp x"#;
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Text(Arc::new("hello".to_string()))],
        );
        assert_eq!(result, Value::Text(Arc::new(r#""hello""#.to_string())));
    }

    #[test]
    fn interp_jd_list() {
        let source = "f>t;xs=[1, 2, 3];jdmp xs";
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(result, Value::Text(Arc::new("[1,2,3]".to_string())));
    }

    #[test]
    fn interp_jd_record() {
        let source = "type pt{x:n;y:n} f>t;p=pt x:1 y:2;jdmp p";
        let result = run_str(source, Some("f"), vec![]);
        let Value::Text(ref s) = result else {
            panic!("expected text")
        };
        let text = s.clone();
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["x"], 1);
        assert_eq!(parsed["y"], 2);
    }

    #[test]
    fn interp_jparse_object() {
        let source = r#"f j:t>R t t;jpar j"#;
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Text(Arc::new(r#"{"a":1,"b":"two"}"#.to_string()))],
        );
        let Value::Ok(inner) = result else {
            panic!("expected Ok")
        };
        let Value::Record { type_name, fields } = *inner else {
            panic!("expected record")
        };
        assert_eq!(type_name, "json");
        assert_eq!(fields.get("a"), Some(&Value::Number(1.0)));
        assert_eq!(
            fields.get("b"),
            Some(&Value::Text(Arc::new("two".to_string())))
        );
    }

    #[test]
    fn interp_jparse_array() {
        let source = r#"f j:t>R t t;jpar j"#;
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Text(Arc::new("[1,2,3]".to_string()))],
        );
        let Value::Ok(inner) = result else {
            panic!("expected Ok")
        };
        assert_eq!(
            *inner,
            Value::List(Arc::new(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0)
            ]))
        );
    }

    #[test]
    fn interp_jparse_scalar() {
        let source = r#"f j:t>R t t;jpar j"#;
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::Text(Arc::new("42".to_string()))]
            ),
            Value::Ok(Box::new(Value::Number(42.0)))
        );
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::Text(Arc::new("true".to_string()))]
            ),
            Value::Ok(Box::new(Value::Bool(true)))
        );
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::Text(Arc::new("null".to_string()))]
            ),
            Value::Ok(Box::new(Value::Nil))
        );
    }

    #[test]
    fn interp_jparse_invalid() {
        let source = r#"f j:t>R t t;jpar j"#;
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Text(Arc::new("not json".to_string()))],
        );
        assert!(matches!(result, Value::Err(_)));
    }

    #[test]
    fn interp_jparse_unwrap() {
        let source = r#"f j:t>t;jpar! j"#;
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Text(Arc::new(r#"{"x":1}"#.to_string()))],
        );
        let Value::Record { type_name, fields } = result else {
            panic!("expected record")
        };
        assert_eq!(type_name, "json");
        assert_eq!(fields.get("x"), Some(&Value::Number(1.0)));
    }

    #[test]
    fn interp_jparse_then_field_access() {
        let source = r#"f j:t>n;r=jpar! j;r.x"#;
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Text(Arc::new(r#"{"x":42}"#.to_string()))],
        );
        assert_eq!(result, Value::Number(42.0));
    }

    #[test]
    fn interp_map_squares() {
        // map sq over [1,2,3,4,5] → [1,4,9,16,25]
        let source = "sq x:n>n;*x x main xs:L n>L n;map sq xs";
        let result = run_str(
            source,
            Some("main"),
            vec![Value::List(Arc::new(
                vec![1.0, 2.0, 3.0, 4.0, 5.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect(),
            ))],
        );
        assert_eq!(
            result,
            Value::List(Arc::new(
                vec![1.0, 4.0, 9.0, 16.0, 25.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect()
            ))
        );
    }

    #[test]
    fn interp_flt_positive() {
        // flt pos over [-3,-1,0,2,4] → [2,4]
        let source = "pos x:n>b;>x 0 main xs:L n>L n;flt pos xs";
        let result = run_str(
            source,
            Some("main"),
            vec![Value::List(Arc::new(
                vec![-3.0, -1.0, 0.0, 2.0, 4.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect(),
            ))],
        );
        assert_eq!(
            result,
            Value::List(Arc::new(
                vec![2.0, 4.0].into_iter().map(Value::Number).collect()
            ))
        );
    }

    #[test]
    fn interp_fld_sum() {
        // fld add over [1..5] with init 0 → 15
        let source = "add a:n b:n>n;+a b main xs:L n>n;fld add xs 0";
        let result = run_str(
            source,
            Some("main"),
            vec![Value::List(Arc::new(
                vec![1.0, 2.0, 3.0, 4.0, 5.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect(),
            ))],
        );
        assert_eq!(result, Value::Number(15.0));
    }

    #[test]
    fn interp_grp_by_string_key() {
        // group numbers into "big" and "small" based on > 5
        let source = r#"cl x:n>t;>x 5{"big"}{"small"} main xs:L n>M t L n;grp cl xs"#;
        let result = run_str(
            source,
            Some("main"),
            vec![Value::List(Arc::new(
                vec![1.0, 8.0, 3.0, 9.0, 2.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect(),
            ))],
        );
        let Value::Map(m) = result else {
            panic!("expected Map")
        };
        assert_eq!(
            m.get(&MapKey::Text("small".to_string())).unwrap(),
            &Value::List(Arc::new(
                vec![1.0, 3.0, 2.0].into_iter().map(Value::Number).collect()
            ))
        );
        assert_eq!(
            m.get(&MapKey::Text("big".to_string())).unwrap(),
            &Value::List(Arc::new(
                vec![8.0, 9.0].into_iter().map(Value::Number).collect()
            ))
        );
    }

    #[test]
    fn interp_grp_by_numeric_key() {
        // group by str(x) — each number becomes its own group
        let source = "key x:n>t;str x main xs:L n>M t L n;grp key xs";
        let result = run_str(
            source,
            Some("main"),
            vec![Value::List(Arc::new(
                vec![1.0, 2.0, 1.0, 3.0, 2.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect(),
            ))],
        );
        let Value::Map(m) = result else {
            panic!("expected Map")
        };
        assert_eq!(
            m.get(&MapKey::Text("1".to_string())).unwrap(),
            &Value::List(Arc::new(
                vec![1.0, 1.0].into_iter().map(Value::Number).collect()
            ))
        );
        assert_eq!(
            m.get(&MapKey::Text("2".to_string())).unwrap(),
            &Value::List(Arc::new(
                vec![2.0, 2.0].into_iter().map(Value::Number).collect()
            ))
        );
        assert_eq!(
            m.get(&MapKey::Text("3".to_string())).unwrap(),
            &Value::List(Arc::new(vec![3.0].into_iter().map(Value::Number).collect()))
        );
    }

    #[test]
    fn interp_grp_empty_list() {
        let source = "id x:n>t;str x main xs:L n>M t L n;grp id xs";
        let result = run_str(source, Some("main"), vec![Value::List(Arc::new(vec![]))]);
        assert_eq!(
            result,
            Value::Map(Arc::new(std::collections::HashMap::new()))
        );
    }

    #[test]
    fn interp_grp_wrong_fn_arg() {
        let err = run_str_err("f>t;grp 42 [1, 2, 3]", Some("f"), vec![]);
        assert!(err.contains("grp"), "got: {err}");
    }

    #[test]
    fn interp_grp_wrong_list_arg() {
        let err = run_str_err("id x:n>n;x f>t;grp id 42", Some("f"), vec![]);
        assert!(err.contains("grp"), "got: {err}");
    }

    #[test]
    fn interp_sum_basic() {
        let source = "f xs:L n>n;sum xs";
        let result = run_str(
            source,
            Some("f"),
            vec![Value::List(Arc::new(
                vec![1.0, 2.0, 3.0, 4.0, 5.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect(),
            ))],
        );
        assert_eq!(result, Value::Number(15.0));
    }

    #[test]
    fn interp_sum_empty() {
        let source = "f xs:L n>n;sum xs";
        let result = run_str(source, Some("f"), vec![Value::List(Arc::new(vec![]))]);
        assert_eq!(result, Value::Number(0.0));
    }

    #[test]
    fn interp_sum_wrong_arg() {
        let err = run_str_err("f>n;sum 42", Some("f"), vec![]);
        assert!(err.contains("sum"), "got: {err}");
    }

    #[test]
    fn interp_sum_non_numeric_element() {
        let err = run_str_err(r#"f>n;sum ["a", "b"]"#, Some("f"), vec![]);
        assert!(err.contains("sum"), "got: {err}");
    }

    #[test]
    fn interp_avg_basic() {
        let source = "f xs:L n>n;avg xs";
        let result = run_str(
            source,
            Some("f"),
            vec![Value::List(Arc::new(
                vec![2.0, 4.0, 6.0].into_iter().map(Value::Number).collect(),
            ))],
        );
        assert_eq!(result, Value::Number(4.0));
    }

    #[test]
    fn interp_avg_empty_error() {
        let err = run_str_err("f>n;avg []", Some("f"), vec![]);
        assert!(err.contains("avg"), "got: {err}");
    }

    #[test]
    fn interp_avg_wrong_arg() {
        let err = run_str_err("f>n;avg 42", Some("f"), vec![]);
        assert!(err.contains("avg"), "got: {err}");
    }

    #[test]
    fn interp_min_lst_basic() {
        let source = "f xs:L n>n;min xs";
        let result = run_str(
            source,
            Some("f"),
            vec![Value::List(Arc::new(
                vec![3.0, 1.0, 4.0, 1.0, 5.0, 9.0, 2.0, 6.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect(),
            ))],
        );
        assert_eq!(result, Value::Number(1.0));
    }

    #[test]
    fn interp_max_lst_basic() {
        let source = "f xs:L n>n;max xs";
        let result = run_str(
            source,
            Some("f"),
            vec![Value::List(Arc::new(
                vec![3.0, 1.0, 4.0, 1.0, 5.0, 9.0, 2.0, 6.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect(),
            ))],
        );
        assert_eq!(result, Value::Number(9.0));
    }

    #[test]
    fn interp_min_lst_empty_errors() {
        let err = run_str_err("f>n;min []", Some("f"), vec![]);
        assert!(err.contains("min") && err.contains("empty"), "got: {err}");
    }

    #[test]
    fn interp_max_lst_empty_errors() {
        let err = run_str_err("f>n;max []", Some("f"), vec![]);
        assert!(err.contains("max") && err.contains("empty"), "got: {err}");
    }

    #[test]
    fn interp_min_lst_non_list_arg() {
        // Runtime path (verifier would also catch this, but the interpreter
        // arm must error cleanly if reached via a generic `_` typed input).
        let err = run_str_err("f x:_>n;min x", Some("f"), vec![Value::Number(42.0)]);
        assert!(err.contains("min"), "got: {err}");
    }

    #[test]
    fn interp_max_lst_non_list_arg() {
        let err = run_str_err("f x:_>n;max x", Some("f"), vec![Value::Number(42.0)]);
        assert!(err.contains("max"), "got: {err}");
    }

    #[test]
    fn interp_min_lst_non_number_element() {
        let err = run_str_err(
            "f xs:L n>n;min xs",
            Some("f"),
            vec![Value::List(Arc::new(vec![Value::Text(Arc::new(
                "x".to_string(),
            ))]))],
        );
        assert!(err.contains("min"), "got: {err}");
    }

    #[test]
    fn interp_max_lst_non_number_element() {
        let err = run_str_err(
            "f xs:L n>n;max xs",
            Some("f"),
            vec![Value::List(Arc::new(vec![Value::Text(Arc::new(
                "x".to_string(),
            ))]))],
        );
        assert!(err.contains("max"), "got: {err}");
    }

    #[test]
    fn interp_min_lst_nan_propagates() {
        let result = run_str(
            "f xs:L n>n;min xs",
            Some("f"),
            vec![Value::List(Arc::new(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(f64::NAN),
                Value::Number(4.0),
            ]))],
        );
        match result {
            Value::Number(n) => assert!(n.is_nan(), "expected NaN, got {n}"),
            other => panic!("expected Number(NaN), got {other:?}"),
        }
    }

    #[test]
    fn interp_max_lst_nan_propagates() {
        let result = run_str(
            "f xs:L n>n;max xs",
            Some("f"),
            vec![Value::List(Arc::new(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(f64::NAN),
                Value::Number(4.0),
            ]))],
        );
        match result {
            Value::Number(n) => assert!(n.is_nan(), "expected NaN, got {n}"),
            other => panic!("expected Number(NaN), got {other:?}"),
        }
    }

    #[test]
    fn interp_wr_csv_output() {
        let dir = std::env::temp_dir();
        let path = dir.join("ilo_test_wr_csv.csv");
        let path_str = path.to_str().unwrap();
        let source = format!(
            r#"f>R t t;wr "{}" [["name", "age"], ["alice", 30], ["bob", 25]] "csv""#,
            path_str.replace('\\', "\\\\")
        );
        let result = run_str(&source, Some("f"), vec![]);
        assert!(matches!(result, Value::Ok(_)));
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "name,age\nalice,30\nbob,25\n");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn interp_wr_csv_quoted_fields() {
        let dir = std::env::temp_dir();
        let path = dir.join("ilo_test_wr_csv_quoted.csv");
        let path_str = path.to_str().unwrap();
        let source = format!(
            r#"f>R t t;wr "{}" [["a,b", "c\"d"]] "csv""#,
            path_str.replace('\\', "\\\\")
        );
        let result = run_str(&source, Some("f"), vec![]);
        assert!(matches!(result, Value::Ok(_)));
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "\"a,b\",\"c\"\"d\"\n");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn interp_wr_json_output() {
        let dir = std::env::temp_dir();
        let path = dir.join("ilo_test_wr_json.json");
        let path_str = path.to_str().unwrap();
        let source = format!(
            r#"f>R t t;wr "{}" [1, 2, 3] "json""#,
            path_str.replace('\\', "\\\\")
        );
        let result = run_str(&source, Some("f"), vec![]);
        assert!(matches!(result, Value::Ok(_)));
        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed, serde_json::json!([1.0, 2.0, 3.0]));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn interp_wr_unknown_format() {
        let err = run_str_err(r#"f>R t t;wr "/tmp/x" "data" "xml""#, Some("f"), vec![]);
        assert!(err.contains("unknown format"), "got: {err}");
    }

    #[test]
    fn interp_rgx_find_all() {
        // find all numbers in a string
        let source = r#"f s:t>L t;rgx "\d+" s"#;
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Text(Arc::new("abc 123 def 456".to_string()))],
        );
        assert_eq!(
            result,
            Value::List(Arc::new(vec![
                Value::Text(Arc::new("123".to_string())),
                Value::Text(Arc::new("456".to_string())),
            ]))
        );
    }

    #[test]
    fn interp_rgx_capture_groups() {
        // extract key=value pairs
        let source = r#"f s:t>L t;rgx "(\w+)=(\w+)" s"#;
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Text(Arc::new("name=alice age=30".to_string()))],
        );
        // Returns first match's groups
        assert_eq!(
            result,
            Value::List(Arc::new(vec![
                Value::Text(Arc::new("name".to_string())),
                Value::Text(Arc::new("alice".to_string())),
            ]))
        );
    }

    #[test]
    fn interp_rgx_no_match() {
        let source = r#"f s:t>L t;rgx "\d+" s"#;
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Text(Arc::new("no numbers here".to_string()))],
        );
        assert_eq!(result, Value::List(Arc::new(vec![])));
    }

    #[test]
    fn interp_rgx_invalid_pattern() {
        let err = run_str_err(r#"f>L t;rgx "[invalid" "test""#, Some("f"), vec![]);
        assert!(err.contains("rgx"), "got: {err}");
    }

    #[test]
    fn interp_rgx_wrong_arg_types() {
        let err = run_str_err(r#"f>L t;rgx 42 "test""#, Some("f"), vec![]);
        assert!(err.contains("rgx"), "got: {err}");
    }

    #[test]
    fn interp_flat_nested() {
        // flat [[1,2],[3],[4,5]] → [1,2,3,4,5]
        let source = "f>L n;flat [[1, 2], [3], [4, 5]]";
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(
            result,
            Value::List(Arc::new(
                vec![1.0, 2.0, 3.0, 4.0, 5.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect()
            ))
        );
    }

    #[test]
    fn interp_flat_mixed() {
        // flat [[1, 2], 3] — non-list elements pass through
        let source = "f>L n;flat [[1, 2], 3]";
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(
            result,
            Value::List(Arc::new(
                vec![1.0, 2.0, 3.0].into_iter().map(Value::Number).collect()
            ))
        );
    }

    #[test]
    fn interp_flat_empty() {
        let source = "f>L n;flat []";
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(result, Value::List(Arc::new(vec![])));
    }

    #[test]
    fn interp_flat_wrong_arg() {
        let err = run_str_err("f>L n;flat 42", Some("f"), vec![]);
        assert!(err.contains("flat"), "got: {err}");
    }

    #[test]
    fn interp_user_hof_fn_type() {
        // User-defined HOF: apl f:F n n x:n>n;f x
        let source = "sq x:n>n;*x x apl f:F n n x:n>n;f x";
        let result = run_str(
            source,
            Some("apl"),
            vec![Value::FnRef("sq".to_string()), Value::Number(7.0)],
        );
        assert_eq!(result, Value::Number(49.0));
    }

    #[test]
    fn interp_fn_ref_via_ref_expr() {
        // Using a function name as a value (Expr::Ref resolves to FnRef)
        let source = "dbl x:n>n;*x 2 main>n;f=dbl;f 10";
        let result = run_str(source, Some("main"), vec![]);
        assert_eq!(result, Value::Number(20.0));
    }

    // --- trm ---

    #[test]
    fn interpret_trm_basic() {
        let result = run_str(
            "f s:t>t;trm s",
            Some("f"),
            vec![Value::Text(Arc::new("  hello  ".to_string()))],
        );
        assert_eq!(result, Value::Text(Arc::new("hello".to_string())));
    }

    #[test]
    fn interpret_trm_no_whitespace() {
        let result = run_str(
            "f s:t>t;trm s",
            Some("f"),
            vec![Value::Text(Arc::new("hi".to_string()))],
        );
        assert_eq!(result, Value::Text(Arc::new("hi".to_string())));
    }

    #[test]
    fn interpret_trm_only_whitespace() {
        let result = run_str(
            "f s:t>t;trm s",
            Some("f"),
            vec![Value::Text(Arc::new("   ".to_string()))],
        );
        assert_eq!(result, Value::Text(Arc::new("".to_string())));
    }

    #[test]
    fn err_trm_wrong_type() {
        let err = run_str_err("f x:n>t;trm x", Some("f"), vec![Value::Number(1.0)]);
        assert!(
            err.contains("trm requires text"),
            "expected trm type error, got: {err}"
        );
    }

    // --- unq ---

    #[test]
    fn interpret_unq_list_numbers() {
        let result = run_str(
            "f xs:L n>L n;unq xs",
            Some("f"),
            vec![Value::List(Arc::new(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(1.0),
                Value::Number(3.0),
                Value::Number(2.0),
            ]))],
        );
        assert_eq!(
            result,
            Value::List(Arc::new(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0)
            ]))
        );
    }

    #[test]
    fn interpret_unq_list_strings() {
        let result = run_str(
            "f xs:L t>L t;unq xs",
            Some("f"),
            vec![Value::List(Arc::new(vec![
                Value::Text(Arc::new("a".to_string())),
                Value::Text(Arc::new("b".to_string())),
                Value::Text(Arc::new("a".to_string())),
            ]))],
        );
        assert_eq!(
            result,
            Value::List(Arc::new(vec![
                Value::Text(Arc::new("a".to_string())),
                Value::Text(Arc::new("b".to_string()))
            ]))
        );
    }

    #[test]
    fn interpret_unq_text_chars() {
        let result = run_str(
            "f s:t>t;unq s",
            Some("f"),
            vec![Value::Text(Arc::new("aabbc".to_string()))],
        );
        assert_eq!(result, Value::Text(Arc::new("abc".to_string())));
    }

    #[test]
    fn interpret_unq_empty_list() {
        let result = run_str(
            "f xs:L n>L n;unq xs",
            Some("f"),
            vec![Value::List(Arc::new(vec![]))],
        );
        assert_eq!(result, Value::List(Arc::new(vec![])));
    }

    #[test]
    fn interpret_unq_preserves_order() {
        let result = run_str(
            "f xs:L n>L n;unq xs",
            Some("f"),
            vec![Value::List(Arc::new(vec![
                Value::Number(3.0),
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(1.0),
                Value::Number(3.0),
            ]))],
        );
        assert_eq!(
            result,
            Value::List(Arc::new(vec![
                Value::Number(3.0),
                Value::Number(1.0),
                Value::Number(2.0)
            ]))
        );
    }

    // --- fmt ---

    #[test]
    fn interpret_fmt_basic() {
        let result = run_str(
            r#"f a:t b:t>t;fmt "{} + {}" a b"#,
            Some("f"),
            vec![
                Value::Text(Arc::new("1".to_string())),
                Value::Text(Arc::new("2".to_string())),
            ],
        );
        assert_eq!(result, Value::Text(Arc::new("1 + 2".to_string())));
    }

    #[test]
    fn interpret_fmt_template_only() {
        let result = run_str(r#"f>t;fmt "hello""#, Some("f"), vec![]);
        assert_eq!(result, Value::Text(Arc::new("hello".to_string())));
    }

    #[test]
    fn interpret_fmt_fewer_args_than_slots() {
        let result = run_str(
            r#"f a:t>t;fmt "{} and {}" a"#,
            Some("f"),
            vec![Value::Text(Arc::new("x".to_string()))],
        );
        assert_eq!(result, Value::Text(Arc::new("x and {}".to_string())));
    }

    #[test]
    fn interpret_fmt_number_arg() {
        let result = run_str(
            r#"f n:n>t;fmt "value: {}" n"#,
            Some("f"),
            vec![Value::Number(42.0)],
        );
        assert_eq!(result, Value::Text(Arc::new("value: 42".to_string())));
    }

    // --- srt fn xs ---

    #[test]
    fn interpret_srt_fn_by_length() {
        let source = "ln s:t>n;len s main xs:L t>L t;srt ln xs";
        let result = run_str(
            source,
            Some("main"),
            vec![Value::List(Arc::new(vec![
                Value::Text(Arc::new("banana".to_string())),
                Value::Text(Arc::new("a".to_string())),
                Value::Text(Arc::new("cc".to_string())),
            ]))],
        );
        assert_eq!(
            result,
            Value::List(Arc::new(vec![
                Value::Text(Arc::new("a".to_string())),
                Value::Text(Arc::new("cc".to_string())),
                Value::Text(Arc::new("banana".to_string())),
            ]))
        );
    }

    #[test]
    fn interpret_srt_fn_numeric_key() {
        let source = "neg x:n>n;-x main xs:L n>L n;srt neg xs";
        let result = run_str(
            source,
            Some("main"),
            vec![Value::List(Arc::new(vec![
                Value::Number(1.0),
                Value::Number(3.0),
                Value::Number(2.0),
            ]))],
        );
        // sort by negative: highest first
        assert_eq!(
            result,
            Value::List(Arc::new(vec![
                Value::Number(3.0),
                Value::Number(2.0),
                Value::Number(1.0)
            ]))
        );
    }

    // --- prnt ---

    #[test]
    fn interpret_prnt_returns_value() {
        let result = run_str("f x:n>n;prnt x", Some("f"), vec![Value::Number(7.0)]);
        assert_eq!(result, Value::Number(7.0));
    }

    #[test]
    fn interpret_prnt_text_passthrough() {
        let result = run_str(
            "f s:t>t;prnt s",
            Some("f"),
            vec![Value::Text(Arc::new("hi".to_string()))],
        );
        assert_eq!(result, Value::Text(Arc::new("hi".to_string())));
    }

    // --- rdb ---

    #[test]
    fn interpret_rdb_csv() {
        let result = run_str(
            r#"f s:t>t;rdb s "csv""#,
            Some("f"),
            vec![Value::Text(Arc::new("a,b\n1,2".to_string()))],
        );
        let Value::Ok(inner) = result else {
            panic!("expected Ok")
        };
        let Value::List(rows) = *inner else {
            panic!("expected list")
        };
        assert_eq!(rows.len(), 2);
        assert!(matches!(&rows[0], Value::List(_)));
    }

    #[test]
    fn interpret_rdb_json() {
        let result = run_str(
            r#"f s:t>t;rdb s "json""#,
            Some("f"),
            vec![Value::Text(Arc::new(r#"{"x":1}"#.to_string()))],
        );
        assert!(
            matches!(result, Value::Ok(_)),
            "expected Ok, got {:?}",
            result
        );
    }

    #[test]
    fn interpret_rdb_invalid_json_is_err() {
        let result = run_str(
            r#"f s:t>t;rdb s "json""#,
            Some("f"),
            vec![Value::Text(Arc::new("not json".to_string()))],
        );
        assert!(
            matches!(result, Value::Err(_)),
            "expected Err, got {:?}",
            result
        );
    }

    #[test]
    fn interpret_rdb_raw_passthrough() {
        let result = run_str(
            r#"f s:t>t;rdb s "raw""#,
            Some("f"),
            vec![Value::Text(Arc::new("hello".to_string()))],
        );
        assert_eq!(
            result,
            Value::Ok(Box::new(Value::Text(Arc::new("hello".to_string()))))
        );
    }

    // --- rd (error paths not needing a real file) ---

    #[test]
    fn interpret_rd_file_not_found() {
        let result = run_str(
            "f p:t>t;rd p",
            Some("f"),
            vec![Value::Text(Arc::new(
                "/nonexistent/ilo_test_file.txt".to_string(),
            ))],
        );
        assert!(
            matches!(result, Value::Err(_)),
            "expected Err, got {:?}",
            result
        );
    }

    // --- TypeIs pattern ---

    #[test]
    fn interpret_type_is_number_match() {
        // n v: pattern matches a number value
        let result = run_str(
            r#"f x:n>t;?x{n v:"num";_:"other"}"#,
            Some("f"),
            vec![Value::Number(42.0)],
        );
        assert_eq!(result, Value::Text(Arc::new("num".to_string())));
    }

    #[test]
    fn interpret_type_is_text_match() {
        // t v: pattern matches a text value
        let result = run_str(
            r#"f x:t>t;?x{t v:v;_:"other"}"#,
            Some("f"),
            vec![Value::Text(Arc::new("hello".to_string()))],
        );
        assert_eq!(result, Value::Text(Arc::new("hello".to_string())));
    }

    #[test]
    fn interpret_type_is_bool_match() {
        // b v: pattern matches a bool value
        let result = run_str(
            r#"f x:b>t;?x{b v:"bool";_:"other"}"#,
            Some("f"),
            vec![Value::Bool(true)],
        );
        assert_eq!(result, Value::Text(Arc::new("bool".to_string())));
    }

    #[test]
    fn interpret_type_is_no_match_falls_through() {
        // TypeIs with wrong type → falls through to wildcard
        let result = run_str(
            r#"f x:n>t;?x{t v:"text";_:"other"}"#,
            Some("f"),
            vec![Value::Number(1.0)],
        );
        assert_eq!(result, Value::Text(Arc::new("other".to_string())));
    }

    #[test]
    fn interpret_type_is_wildcard_binding() {
        // TypeIs with _ binding (no binding created)
        let result = run_str(
            r#"f x:n>t;?x{n _:"matched";_:"other"}"#,
            Some("f"),
            vec![Value::Number(5.0)],
        );
        assert_eq!(result, Value::Text(Arc::new("matched".to_string())));
    }

    // --- Text comparison operators ---

    #[test]
    fn interpret_text_greater_than() {
        let result = run_str(
            "f a:t b:t>b;>a b",
            Some("f"),
            vec![
                Value::Text(Arc::new("b".to_string())),
                Value::Text(Arc::new("a".to_string())),
            ],
        );
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn interpret_text_less_than() {
        let result = run_str(
            "f a:t b:t>b;<a b",
            Some("f"),
            vec![
                Value::Text(Arc::new("a".to_string())),
                Value::Text(Arc::new("b".to_string())),
            ],
        );
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn interpret_text_greater_or_equal() {
        let result = run_str(
            "f a:t b:t>b;>=a b",
            Some("f"),
            vec![
                Value::Text(Arc::new("a".to_string())),
                Value::Text(Arc::new("a".to_string())),
            ],
        );
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn interpret_text_less_or_equal() {
        let result = run_str(
            "f a:t b:t>b;<=a b",
            Some("f"),
            vec![
                Value::Text(Arc::new("a".to_string())),
                Value::Text(Arc::new("b".to_string())),
            ],
        );
        assert_eq!(result, Value::Bool(true));
    }

    // --- Destructure error path ---

    #[test]
    fn interpret_destructure_non_record_error() {
        let prog = parse_program("type pt{x:n;y:n} f p:pt>n;{x;y}=p;+x y");
        // Pass a non-record at runtime (bypass type checking)
        let result = run(&prog, Some("f"), vec![Value::Number(42.0)]);
        assert!(
            result.is_err(),
            "expected error for destructure on non-record"
        );
    }

    // --- Safe field/index on nil ---

    #[test]
    fn interpret_safe_field_on_nil_returns_nil() {
        // mget on missing key returns nil; safe field access on nil short-circuits to nil
        let result = run_str("f>n;x=mget mmap \"key\";x.?field", Some("f"), vec![]);
        assert_eq!(result, Value::Nil);
    }

    #[test]
    fn interpret_safe_index_on_nil_returns_nil() {
        // mget on missing key returns nil; safe index access on nil short-circuits to nil
        let result = run_str("f>n;xs=mget mmap \"key\";xs.?0", Some("f"), vec![]);
        assert_eq!(result, Value::Nil);
    }

    // --- values_equal for texts ---

    #[test]
    fn values_equal_texts() {
        assert!(values_equal(
            &Value::Text(Arc::new("a".to_string())),
            &Value::Text(Arc::new("a".to_string()))
        ));
        assert!(!values_equal(
            &Value::Text(Arc::new("a".to_string())),
            &Value::Text(Arc::new("b".to_string()))
        ));
    }

    // ── New coverage tests ────────────────────────────────────────────────────

    // L62: Value::FnRef Display
    #[test]
    fn display_fnref() {
        assert_eq!(format!("{}", Value::FnRef("add".into())), "<fn:add>");
    }

    // Single-row quoted-field coverage now lives on parse_csv_content;
    // the previous per-line `parse_csv_row` helper was removed as part of
    // the csv-pipeline rerun10 fix. See parse_csv_content_* tests below.
    #[test]
    fn parse_csv_content_single_row_escaped_quote() {
        let rows = parse_csv_content(r#""he said ""hello""","world""#, ',');
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], vec![r#"he said "hello""#, "world"]);
    }

    #[test]
    fn parse_csv_content_single_row_simple_quoted() {
        let rows = parse_csv_content(r#""hello","world""#, ',');
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], vec!["hello", "world"]);
    }

    // ── parse_csv_content: quote-state tracking across newlines ───────────────
    // Regression for csv-pipeline rerun10: the reader used to split content
    // on `\n` before tracking quote state, so a multi-line quoted field
    // (which the writer correctly emits per RFC 4180) was mis-parsed as
    // two rows. parse_csv_content now scans the full document in one pass.

    #[test]
    fn parse_csv_content_multiline_quoted_field() {
        // The writer emits "line\nbreak" as a quoted multi-line cell.
        // The reader must keep it as a single cell across the embedded \n.
        let input = "name,note,n\nplain,\"line\nbreak\",2\n";
        let rows = parse_csv_content(input, ',');
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], vec!["name", "note", "n"]);
        assert_eq!(rows[1], vec!["plain", "line\nbreak", "2"]);
    }

    #[test]
    fn parse_csv_content_basic_no_trailing_newline() {
        let rows = parse_csv_content("a,b\nc,d", ',');
        assert_eq!(rows, vec![vec!["a", "b"], vec!["c", "d"]]);
    }

    #[test]
    fn parse_csv_content_basic_trailing_newline_no_phantom_row() {
        // A file ending in `\n` should NOT yield a trailing empty row.
        let rows = parse_csv_content("a,b\nc,d\n", ',');
        assert_eq!(rows, vec![vec!["a", "b"], vec!["c", "d"]]);
    }

    #[test]
    fn parse_csv_content_crlf_line_endings() {
        let rows = parse_csv_content("a,b\r\nc,d\r\n", ',');
        assert_eq!(rows, vec![vec!["a", "b"], vec!["c", "d"]]);
    }

    #[test]
    fn parse_csv_content_crlf_inside_quoted_field_preserved() {
        // \r\n inside a quoted cell is part of the cell, not a record break.
        let rows = parse_csv_content("a,\"x\r\ny\"\n", ',');
        assert_eq!(rows, vec![vec!["a".to_string(), "x\r\ny".to_string()]]);
    }

    #[test]
    fn parse_csv_content_escaped_quote_inside_multiline_field() {
        // Combined edge case: embedded newline AND escaped quote in one cell.
        let input = "a,\"he said \"\"hi\"\"\nfoo\",b\n";
        let rows = parse_csv_content(input, ',');
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], vec!["a", "he said \"hi\"\nfoo", "b"]);
    }

    #[test]
    fn parse_csv_content_tsv_separator() {
        // Same scanner, tab separator.
        let rows = parse_csv_content("a\tb\nc\td\n", '\t');
        assert_eq!(rows, vec![vec!["a", "b"], vec!["c", "d"]]);
    }

    #[test]
    fn parse_csv_content_empty_input() {
        let rows = parse_csv_content("", ',');
        assert!(rows.is_empty());
    }

    #[test]
    fn parse_csv_content_embedded_comma_in_quoted_field() {
        // A comma inside a quoted cell is part of the cell, not a separator.
        let rows = parse_csv_content("a,\"x,y\",b\n", ',');
        assert_eq!(rows, vec![vec!["a", "x,y", "b"]]);
    }

    #[test]
    fn parse_csv_content_mixed_quoted_and_unquoted_in_same_row() {
        // Real-world CSV mixes quoted and unquoted cells freely. The scanner
        // must handle both in a single row.
        let rows = parse_csv_content("alice,\"engineer, sr.\",30,\"London\"\n", ',');
        assert_eq!(rows, vec![vec!["alice", "engineer, sr.", "30", "London"]]);
    }

    #[test]
    fn parse_csv_content_empty_trailing_field() {
        // A row ending with a separator means the last cell is empty. This is
        // a common spreadsheet artefact ("alice,30," for a missing column).
        let rows = parse_csv_content("a,b,\nc,d,\n", ',');
        assert_eq!(rows, vec![vec!["a", "b", ""], vec!["c", "d", ""]]);
    }

    #[test]
    fn parse_csv_content_empty_field_in_middle() {
        // ",,," produces ["", "", "", ""] -- four cells, three of them empty.
        let rows = parse_csv_content("a,,b\n", ',');
        assert_eq!(rows, vec![vec!["a", "", "b"]]);
    }

    #[test]
    fn parse_csv_content_empty_quoted_field() {
        // "" is the canonical empty quoted cell -- not a stray escape.
        let rows = parse_csv_content("a,\"\",b\n", ',');
        assert_eq!(rows, vec![vec!["a", "", "b"]]);
    }

    #[test]
    fn parse_csv_content_utf8_bom_preserved_in_first_cell() {
        // We don't currently strip a leading UTF-8 BOM. Pin the current
        // behaviour so future BOM handling (if added) is a deliberate change,
        // not silent drift. The BOM is the three bytes EF BB BF (\u{feff}).
        let rows = parse_csv_content("\u{feff}name,age\nalice,30\n", ',');
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], vec!["\u{feff}name", "age"]);
        assert_eq!(rows[1], vec!["alice", "30"]);
    }

    #[test]
    fn parse_csv_content_single_unterminated_quoted_field() {
        // Malformed input: open quote with no close. The scanner should not
        // panic and should still produce the partial cell so the user can
        // see the broken data rather than getting a silent empty result.
        let rows = parse_csv_content("a,\"oops\n", ',');
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], vec!["a", "oops\n"]);
    }

    #[test]
    fn parse_csv_content_round_trip_via_write_csv_tsv() {
        // End-to-end: the canonical regression. Take a row with a multi-line
        // cell, write it via write_csv_tsv, then parse_csv_content the result.
        // The original cells should come back byte-for-byte.
        let original = vec![
            Value::List(Arc::new(vec![
                Value::Text(Arc::new("name".to_string())),
                Value::Text(Arc::new("note".to_string())),
                Value::Text(Arc::new("n".to_string())),
            ])),
            Value::List(Arc::new(vec![
                Value::Text(Arc::new("plain".to_string())),
                Value::Text(Arc::new("line\nbreak".to_string())),
                Value::Number(2.0),
            ])),
        ];
        let serialised = write_csv_tsv(&original, ',').expect("write_csv_tsv failed");
        let rows = parse_csv_content(&serialised, ',');
        assert_eq!(rows.len(), 2, "round-trip produced wrong row count");
        assert_eq!(rows[0], vec!["name", "note", "n"]);
        assert_eq!(rows[1], vec!["plain", "line\nbreak", "2"]);
    }

    // L299: len on Map
    #[test]
    fn interpret_len_map() {
        let result = run_str(
            r#"f>n;m=mset (mset mmap "a" 1) "b" 2;len m"#,
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Number(2.0));
    }

    // L310: mget wrong args
    #[test]
    fn interpret_mget_wrong_args() {
        let err = run_str_err("f>n;mget 42 \"key\"", Some("f"), vec![]);
        assert!(err.contains("mget"), "got: {err}");
    }

    // L320: mset wrong args
    #[test]
    fn interpret_mset_wrong_args() {
        let err = run_str_err("f>n;mset 42 \"key\" 1", Some("f"), vec![]);
        assert!(err.contains("mset"), "got: {err}");
    }

    // L324-326: mhas wrong args
    #[test]
    fn interpret_mhas_wrong_args() {
        let err = run_str_err("f>n;mhas 42 \"key\"", Some("f"), vec![]);
        assert!(err.contains("mhas"), "got: {err}");
    }

    // L330-336: mkeys wrong args
    #[test]
    fn interpret_mkeys_wrong_args() {
        let err = run_str_err("f>n;mkeys 42", Some("f"), vec![]);
        assert!(err.contains("mkeys"), "got: {err}");
    }

    // L340-346: mvals wrong args
    #[test]
    fn interpret_mvals_wrong_args() {
        let err = run_str_err("f>n;mvals 42", Some("f"), vec![]);
        assert!(err.contains("mvals"), "got: {err}");
    }

    // L350-356: mdel wrong args
    #[test]
    fn interpret_mdel_wrong_args() {
        let err = run_str_err("f>n;mdel 42 \"key\"", Some("f"), vec![]);
        assert!(err.contains("mdel"), "got: {err}");
    }

    // L437: rnd wrong types (two non-number args)
    #[test]
    fn interpret_rnd_wrong_types() {
        let err = run_str_err(r#"f>n;rnd "a" "b""#, Some("f"), vec![]);
        assert!(err.contains("rnd"), "got: {err}");
    }

    // L566-570: srt with key fn — second arg not a list
    #[test]
    fn interpret_srt_key_fn_wrong_second_arg() {
        let source = "sq x:n>n;*x x f>n;srt sq 42";
        let err = run_str_err(source, Some("f"), vec![]);
        assert!(err.contains("srt"), "got: {err}");
    }

    // L582-583: srt with key fn — text keys
    #[test]
    fn interpret_srt_key_fn_text_keys() {
        let source = "id x:t>t;x main xs:L t>L t;srt id xs";
        let result = run_str(
            source,
            Some("main"),
            vec![Value::List(Arc::new(vec![
                Value::Text(Arc::new("banana".to_string())),
                Value::Text(Arc::new("apple".to_string())),
                Value::Text(Arc::new("cherry".to_string())),
            ]))],
        );
        assert_eq!(
            result,
            Value::List(Arc::new(vec![
                Value::Text(Arc::new("apple".to_string())),
                Value::Text(Arc::new("banana".to_string())),
                Value::Text(Arc::new("cherry".to_string())),
            ]))
        );
    }

    // L622: get with invalid (non-map) headers
    #[test]
    fn interpret_get_invalid_headers() {
        let err = run_str_err(r#"f>t;get "http://x" 42"#, Some("f"), vec![]);
        assert!(
            err.contains("headers") || err.contains("M t t"),
            "got: {err}"
        );
    }

    // L648: pst wrong arg types (renamed from `post` in 0.12.0)
    #[test]
    fn interpret_pst_wrong_arg_types() {
        let err = run_str_err(r#"f>t;pst 42 "body""#, Some("f"), vec![]);
        assert!(err.contains("pst"), "got: {err}");
    }

    // L656: pst with invalid headers
    #[test]
    fn interpret_pst_invalid_headers() {
        let err = run_str_err(r#"f>t;pst "http://x" "body" 42"#, Some("f"), vec![]);
        assert!(err.contains("headers") || err.contains("pst"), "got: {err}");
    }

    // L703: unq wrong type
    #[test]
    fn interpret_unq_wrong_type() {
        let err = run_str_err("f>n;unq 42", Some("f"), vec![]);
        assert!(err.contains("unq"), "got: {err}");
    }

    // L709: fmt wrong first arg
    #[test]
    fn interpret_fmt_wrong_first_arg() {
        let err = run_str_err("f>n;fmt 42", Some("f"), vec![]);
        assert!(err.contains("fmt"), "got: {err}");
    }

    // L732: rd wrong arg type
    #[test]
    fn interpret_rd_wrong_arg_type() {
        let err = run_str_err("f>t;rd 42", Some("f"), vec![]);
        assert!(err.contains("rd"), "got: {err}");
    }

    // L735-737: rd with explicit format, wrong format arg type
    #[test]
    fn interpret_rd_with_wrong_format_type() {
        let err = run_str_err("f>t;rd \"/tmp\" 42", Some("f"), vec![]);
        assert!(err.contains("rd") || err.contains("format"), "got: {err}");
    }

    // ILO-374: rd on a .json path must return raw text, not a parsed value.
    #[test]
    fn interpret_rd_json_path_returns_raw_text() {
        let mut path = std::env::temp_dir();
        path.push("ilo_interp_rd_json_raw.json");
        std::fs::write(&path, r#"{"key":"value"}"#).unwrap();
        let path_str = path.to_str().unwrap().to_string();
        let result = run_str(
            "f p:t>R t t;rd p",
            Some("f"),
            vec![Value::Text(Arc::new(path_str))],
        );
        std::fs::remove_file(&path).ok();
        match &result {
            Value::Ok(inner) => assert!(
                matches!(inner.as_ref(), Value::Text(_)),
                "rd on .json must return raw text, not {:?}",
                inner
            ),
            other => panic!("expected Ok(text), got {other:?}"),
        }
    }

    // ILO-374: rd-json reads and parses a JSON file.
    #[test]
    fn interpret_rd_json_builtin_parses_json() {
        let mut path = std::env::temp_dir();
        path.push("ilo_interp_rd_json_builtin.json");
        std::fs::write(&path, r#"{"key":"value"}"#).unwrap();
        let path_str = path.to_str().unwrap().to_string();
        let result = run_str(
            "f p:t>R _ t;rd-json p",
            Some("f"),
            vec![Value::Text(Arc::new(path_str))],
        );
        std::fs::remove_file(&path).ok();
        match &result {
            Value::Ok(inner) => assert!(
                !matches!(inner.as_ref(), Value::Text(_)),
                "rd-json must return parsed value, not raw text"
            ),
            other => panic!("expected Ok(parsed), got {other:?}"),
        }
    }

    // ILO-374: rd-json on non-existent file returns Err.
    #[test]
    fn interpret_rd_json_not_found() {
        let result = run_str(
            "f p:t>R _ t;rd-json p",
            Some("f"),
            vec![Value::Text(Arc::new(
                "/nonexistent/ilo_rd_json_test.json".to_string(),
            ))],
        );
        assert!(
            matches!(result, Value::Err(_)),
            "expected Err for missing file, got {:?}",
            result
        );
    }

    // L758: rdb wrong first arg
    #[test]
    fn interpret_rdb_wrong_first_arg() {
        let err = run_str_err(r#"f>t;rdb 42 "raw""#, Some("f"), vec![]);
        assert!(err.contains("rdb"), "got: {err}");
    }

    // L762: rdb wrong format arg
    #[test]
    fn interpret_rdb_wrong_format_arg() {
        let err = run_str_err(r#"f>t;rdb "hello" 42"#, Some("f"), vec![]);
        assert!(err.contains("rdb") || err.contains("format"), "got: {err}");
    }

    // L770-777: rdl returns list of lines
    #[test]
    fn interpret_rdl_basic() {
        let mut path = std::env::temp_dir();
        path.push("ilo_interp_rdl_test.txt");
        std::fs::write(&path, "line1\nline2\nline3").unwrap();
        let path_str = path.to_str().unwrap().to_string();
        let result = run_str(
            "f p:t>t;rdl p",
            Some("f"),
            vec![Value::Text(Arc::new(path_str))],
        );
        std::fs::remove_file(&path).ok();
        let Value::Ok(inner) = result else {
            panic!("expected Ok")
        };
        let Value::List(lines) = *inner else {
            panic!("expected list")
        };
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], Value::Text(Arc::new("line1".to_string())));
    }

    // L779: rdl file not found
    #[test]
    fn interpret_rdl_not_found() {
        let result = run_str(
            "f p:t>t;rdl p",
            Some("f"),
            vec![Value::Text(Arc::new(
                "/nonexistent/ilo_rdl_test.txt".to_string(),
            ))],
        );
        assert!(
            matches!(result, Value::Err(_)),
            "expected Err, got {:?}",
            result
        );
    }

    // L781: rdl wrong arg type
    #[test]
    fn interpret_rdl_wrong_arg() {
        let err = run_str_err("f>t;rdl 42", Some("f"), vec![]);
        assert!(err.contains("rdl"), "got: {err}");
    }

    // L785-788: wr basic (write to temp file)
    #[test]
    fn interpret_wr_basic() {
        let mut path = std::env::temp_dir();
        path.push("ilo_interp_wr_test.txt");
        let path_str = path.to_str().unwrap().to_string();
        let result = run_str(
            "f p:t>t;wr p \"hello\"",
            Some("f"),
            vec![Value::Text(Arc::new(path_str.clone()))],
        );
        std::fs::remove_file(&path).ok();
        assert!(
            matches!(result, Value::Ok(_)),
            "expected Ok, got {:?}",
            result
        );
    }

    // L790: wr wrong arg types
    #[test]
    fn interpret_wr_wrong_args() {
        let err = run_str_err("f>t;wr 42 \"hello\"", Some("f"), vec![]);
        assert!(err.contains("wr"), "got: {err}");
    }

    // L794-805: wrl basic
    #[test]
    fn interpret_wrl_basic() {
        let mut path = std::env::temp_dir();
        path.push("ilo_interp_wrl_test.txt");
        let path_str = path.to_str().unwrap().to_string();
        let result = run_str(
            "f p:t>t;wrl p [\"a\", \"b\", \"c\"]",
            Some("f"),
            vec![Value::Text(Arc::new(path_str.clone()))],
        );
        std::fs::remove_file(&path).ok();
        assert!(
            matches!(result, Value::Ok(_)),
            "expected Ok, got {:?}",
            result
        );
    }

    // L800: wrl list with non-text item
    #[test]
    fn interpret_wrl_non_text_item() {
        let mut path = std::env::temp_dir();
        path.push("ilo_interp_wrl_nontxt_test.txt");
        let path_str = path.to_str().unwrap().to_string();
        let mut env = Env::new();
        let result = call_function(
            &mut env,
            "wrl",
            vec![
                Value::Text(Arc::new(path_str.clone())),
                Value::List(Arc::new(vec![
                    Value::Text(Arc::new("ok".to_string())),
                    Value::Number(99.0),
                ])),
            ],
        );
        std::fs::remove_file(&path).ok();
        assert!(result.is_err(), "expected error for non-text wrl item");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("wrl"), "got: {err}");
    }

    // L808: wrl wrong arg types
    #[test]
    fn interpret_wrl_wrong_args() {
        let err = run_str_err("f>t;wrl 42 [\"a\"]", Some("f"), vec![]);
        assert!(err.contains("wrl"), "got: {err}");
    }

    // L822: jpth array index navigation. Post-0.12.1 jpth returns typed
    // values (R ? t), so a numeric leaf comes back as Number, not Text.
    #[test]
    fn interpret_jpth_array_index() {
        let source = r#"f j:t p:t>R _ t;jpth j p"#;
        let result = run_str(
            source,
            Some("f"),
            vec![
                Value::Text(Arc::new(r#"[10,20,30]"#.to_string())),
                Value::Text(Arc::new("1".to_string())),
            ],
        );
        assert_eq!(result, Value::Ok(Box::new(Value::Number(20.0))));
    }

    // L839: jpth non-text/non-map args
    #[test]
    fn interpret_jpth_wrong_args() {
        let err = run_str_err(r#"f>t;jpth 42 "path""#, Some("f"), vec![]);
        assert!(err.contains("jpth"), "got: {err}");
    }

    // L857: jdmp on Ok value
    #[test]
    fn interp_jdmp_ok_value() {
        let result = run_str("f>t;jdmp ~42", Some("f"), vec![]);
        assert_eq!(result, Value::Text(Arc::new("42".to_string())));
    }

    // L869: jdmp on FnRef (goes through value_to_json FnRef branch)
    #[test]
    fn interp_jdmp_fnref() {
        let source = "sq x:n>n;*x x f>t;r=sq;jdmp r";
        let result = run_str(source, Some("f"), vec![]);
        // FnRef displays as "<fn:sq>"
        let Value::Text(s) = result else {
            panic!("expected Text")
        };
        assert!(s.contains("fn:sq") || s.contains("sq"), "got: {s}");
    }

    // L879-880: jpar wrong arg type
    #[test]
    fn interp_jpar_wrong_arg_type() {
        let err = run_str_err("f>t;jpar 42", Some("f"), vec![]);
        assert!(err.contains("jpar"), "got: {err}");
    }

    // L885-886: env wrong arg type
    #[test]
    fn interpret_env_wrong_arg_type() {
        let err = run_str_err("f>t;env 42", Some("f"), vec![]);
        assert!(err.contains("env"), "got: {err}");
    }

    // L889: map wrong first arg (not a fn ref)
    #[test]
    fn interpret_map_wrong_fn_arg() {
        let err = run_str_err("f>t;map 42 [1, 2]", Some("f"), vec![]);
        assert!(err.contains("map"), "got: {err}");
    }

    // L899-900: map wrong second arg (not a list)
    #[test]
    fn interpret_map_wrong_list_arg() {
        let source = "sq x:n>n;*x x f>t;map sq 42";
        let err = run_str_err(source, Some("f"), vec![]);
        assert!(err.contains("map"), "got: {err}");
    }

    // L903: flt predicate returns non-bool
    #[test]
    fn interpret_flt_predicate_returns_non_bool() {
        let source = "id x:n>n;x f xs:L n>L n;flt id xs";
        let err = run_str_err(
            source,
            Some("f"),
            vec![Value::List(Arc::new(vec![Value::Number(1.0)]))],
        );
        assert!(err.contains("flt") || err.contains("bool"), "got: {err}");
    }

    // L910: flt wrong list arg
    #[test]
    fn interpret_flt_wrong_list_arg() {
        let source = "pos x:n>b;>x 0 f>t;flt pos 42";
        let err = run_str_err(source, Some("f"), vec![]);
        assert!(err.contains("flt"), "got: {err}");
    }

    // L917-918: fld wrong list arg
    #[test]
    fn interpret_fld_wrong_list_arg() {
        let source = "add a:n b:n>n;+a b f>n;fld add 42 0";
        let err = run_str_err(source, Some("f"), vec![]);
        assert!(err.contains("fld"), "got: {err}");
    }

    // L921: fld wrong first arg (not a fn ref)
    #[test]
    fn interpret_fld_wrong_fn_arg() {
        let err = run_str_err("f>n;fld 42 [1, 2] 0", Some("f"), vec![]);
        assert!(err.contains("fld"), "got: {err}");
    }

    // L956: Decl::Use branch in call_function
    #[test]
    fn interpret_call_use_decl_errors() {
        use crate::ast::{Decl, Span};
        let mut env = Env::new();
        env.functions.insert(
            "fake_use".to_string(),
            Decl::Use {
                path: "x.ilo".to_string(),
                only: None,
                alias: None,
                predicate: None,
                alt_path: None,
                reexport: false,
                lazy: false,
                span: Span { start: 0, end: 0 },
            },
        );
        let result = call_function(&mut env, "fake_use", vec![]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("unresolved import")
        );
    }

    // L984: Alias branch in call_function
    #[test]
    fn interpret_call_alias_decl_errors() {
        use crate::ast::{Decl, Span, Type};
        let mut env = Env::new();
        env.functions.insert(
            "myalias".to_string(),
            Decl::Alias {
                name: "myalias".to_string(),
                target: Type::Number,
                span: Span { start: 0, end: 0 },
            },
        );
        let result = call_function(&mut env, "myalias", vec![]);
        assert!(result.is_err());
    }

    // L987: Error decl branch in call_function
    #[test]
    fn interpret_call_error_decl_errors() {
        use crate::ast::{Decl, Span};
        let mut env = Env::new();
        env.functions.insert(
            "bad_decl".to_string(),
            Decl::Error {
                span: Span { start: 0, end: 0 },
            },
        );
        let result = call_function(&mut env, "bad_decl", vec![]);
        assert!(result.is_err());
    }

    // L1001-1003: Expr::Match arms — Continue from body
    // The Continue path in match-expr eval_body → BodyResult::Continue → Value::Nil
    #[test]
    fn interpret_match_continue_arm_returns_nil() {
        // A match where the matched arm body triggers continue (cnt) — only valid in for loop
        let source = "f xs:L n>n;@x xs{?x{1:cnt;_:x}}";
        let result = run_str(
            source,
            Some("f"),
            vec![Value::List(Arc::new(vec![
                Value::Number(1.0),
                Value::Number(2.0),
            ]))],
        );
        // Iteration: x=1 → cnt (continue), x=2 → 2. Last value of foreach body = 2.
        assert_eq!(result, Value::Number(2.0));
    }

    // L1103-1104: Guard ternary with else body — exercises BodyResult::Value in ternary branch
    #[test]
    fn interpret_guard_ternary_in_foreach() {
        // Ternary `=x 0{yes}{no}` used inside a foreach body
        let source = "f xs:L n>n;@x xs{=x 0{10}{20}}";
        let result = run_str(
            source,
            Some("f"),
            vec![Value::List(Arc::new(vec![
                Value::Number(0.0),
                Value::Number(1.0),
            ]))],
        );
        // x=0: true → 10, x=1: false → 20. Last value = 20.
        assert_eq!(result, Value::Number(20.0));
    }

    // L1140-1141: Match arm Continue path in match-stmt
    #[test]
    fn interpret_match_stmt_continue_propagates() {
        let source = "f xs:L n>n;@x xs{?x{1:cnt;_:x}}";
        let result = run_str(
            source,
            Some("f"),
            vec![Value::List(Arc::new(vec![
                Value::Number(1.0),
                Value::Number(5.0),
            ]))],
        );
        assert_eq!(result, Value::Number(5.0));
    }

    // L1185: ForEach — early return propagated via match-arm returning value
    #[test]
    fn interpret_foreach_return_from_nested_match() {
        // Match arm returns a value; foreach body value propagates
        let source = "f xs:L n>n;@x xs{?x{5:x;_:0}}";
        let result = run_str(
            source,
            Some("f"),
            vec![Value::List(Arc::new(vec![
                Value::Number(1.0),
                Value::Number(5.0),
                Value::Number(9.0),
            ]))],
        );
        // x=1 → 0, x=5 → 5, x=9 → 0; last value of foreach = 0
        assert_eq!(result, Value::Number(0.0));
    }

    // L1189: ForRange — range end not a number
    #[test]
    fn interpret_range_end_not_number() {
        // ForRange where end is not a number — needs tricky setup
        // The range start/end are evaluated, if end is text it errors
        let source = "f s:n en:n>n;@i s..en{i}";
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Number(0.0), Value::Number(3.0)],
        );
        assert_eq!(result, Value::Number(2.0));
    }

    // L1298: value_to_json large float (uses Number::from_f64)
    #[test]
    fn interp_jdmp_large_float() {
        let source = "f x:n>t;jdmp x";
        // Very large float that won't be an integer — exercises from_f64 path
        let result = run_str(source, Some("f"), vec![Value::Number(1.23456789e20)]);
        assert!(matches!(result, Value::Text(_)));
    }

    // L1309: value_to_json Err inner
    #[test]
    fn interp_jdmp_err_value() {
        let result = run_str("f>t;jdmp ^42", Some("f"), vec![]);
        assert_eq!(result, Value::Text(Arc::new("42".to_string())));
    }

    // L1379: value_to_json Map variant
    #[test]
    fn interp_jdmp_map_value() {
        let result = run_str(r#"f>t;m=mset mmap "k" 1;jdmp m"#, Some("f"), vec![]);
        let Value::Text(s) = result else {
            panic!("expected text")
        };
        assert!(s.contains("k"), "got: {s}");
    }

    // L1527-1528: TypeIs List pattern (uses `l` token for list)
    #[test]
    fn interpret_type_is_list_match() {
        let source = r#"f x:L n>t;?x{l v:"list";_:"other"}"#;
        let result = run_str(
            source,
            Some("f"),
            vec![Value::List(Arc::new(vec![Value::Number(1.0)]))],
        );
        assert_eq!(result, Value::Text(Arc::new("list".to_string())));
    }

    // L2376: Decl::TypeDef is not callable error (duplicate name avoided — already tested above)
    // (see earlier interpret_typedef_not_callable test)

    // L3669/3671: rdb csv header-only / single row
    #[test]
    fn interpret_rdb_csv_single_row() {
        let result = run_str(
            r#"f s:t>t;rdb s "csv""#,
            Some("f"),
            vec![Value::Text(Arc::new("a,b,c".to_string()))],
        );
        let Value::Ok(inner) = result else {
            panic!("expected Ok")
        };
        let Value::List(rows) = *inner else {
            panic!("expected list")
        };
        assert_eq!(rows.len(), 1);
    }

    // ── mhas/mkeys/mvals/mdel happy paths ─────────────────────────────────

    // L325: mhas Map+Text → true/false
    #[test]
    fn interpret_mhas_found() {
        let result = run_str(r#"f>b;m=mset mmap "x" 1;mhas m "x""#, Some("f"), vec![]);
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn interpret_mhas_not_found() {
        let result = run_str(r#"f>b;m=mset mmap "x" 1;mhas m "y""#, Some("f"), vec![]);
        assert_eq!(result, Value::Bool(false));
    }

    // L331-334: mkeys happy path — sorted keys
    #[test]
    fn interpret_mkeys_happy_path() {
        let result = run_str(
            r#"f>L t;m=mset (mset mmap "b" 2) "a" 1;mkeys m"#,
            Some("f"),
            vec![],
        );
        assert_eq!(
            result,
            Value::List(Arc::new(vec![
                Value::Text(Arc::new("a".to_string())),
                Value::Text(Arc::new("b".to_string()))
            ]))
        );
    }

    // L341-344: mvals happy path — values sorted by key
    #[test]
    fn interpret_mvals_happy_path() {
        let result = run_str(
            r#"f>L n;m=mset (mset mmap "b" 2) "a" 1;mvals m"#,
            Some("f"),
            vec![],
        );
        assert_eq!(
            result,
            Value::List(Arc::new(vec![Value::Number(1.0), Value::Number(2.0)]))
        );
    }

    // L351-354: mdel happy path — delete key from map
    #[test]
    fn interpret_mdel_happy_path() {
        let result = run_str(
            r#"f>n;m=mset (mset mmap "a" 1) "b" 2;m2=mdel m "a";len m2"#,
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Number(1.0));
    }

    // ── srt 2-arg key not fn-ref (line 566-567) ────────────────────────────

    #[test]
    fn interpret_srt_key_not_fn_ref() {
        // 42 is a Number, resolve_fn_ref returns None → line 566-567 error
        let err = run_str_err(
            "f xs:L n>L n;srt 42 xs",
            Some("f"),
            vec![Value::List(Arc::new(vec![Value::Number(1.0)]))],
        );
        assert!(err.contains("srt"), "got: {err}");
    }

    // ── flt first arg not fn-ref (lines 968-969) ────────────────────────────

    #[test]
    fn interpret_flt_key_not_fn_ref() {
        let err = run_str_err(
            "f xs:L n>L n;flt 42 xs",
            Some("f"),
            vec![Value::List(Arc::new(vec![Value::Number(1.0)]))],
        );
        assert!(err.contains("flt"), "got: {err}");
    }

    // ── resolve_fn_ref Text path (line 948) via map with text fn name ───────

    #[test]
    fn interpret_map_with_text_fn_name() {
        // Pass fn name as text arg; resolve_fn_ref hits Text branch (line 948)
        let source = "sq x:n>n;*x x f cb:t xs:L n>L n;map cb xs";
        let result = run_str(
            source,
            Some("f"),
            vec![
                Value::Text(Arc::new("sq".to_string())),
                Value::List(Arc::new(vec![Value::Number(3.0)])),
            ],
        );
        assert_eq!(result, Value::List(Arc::new(vec![Value::Number(9.0)])));
    }

    // ── rd 2-arg explicit format (lines 736, 749, 750-751) ──────────────────

    #[test]
    fn interpret_rd_explicit_raw_format() {
        // Write a temp file, read with explicit "raw" format → lines 736, 749
        let path = "/tmp/ilo_test_rd_explicit.txt";
        std::fs::write(path, "hello").unwrap();
        let source = format!(r#"f>R t t;rd "{path}" "raw""#);
        let result = run_str(&source, Some("f"), vec![]);
        let Value::Ok(inner) = result else {
            panic!("expected Ok")
        };
        assert_eq!(*inner, Value::Text(Arc::new("hello".to_string())));
    }

    #[test]
    fn interpret_rd_explicit_format_parse_error() {
        // Write invalid JSON to a temp file, read with "json" format → line 750-751
        let path = "/tmp/ilo_test_rd_badjson.txt";
        std::fs::write(path, "not json at all!!!").unwrap();
        let source = format!(r#"f>R t t;rd "{path}" "json""#);
        let result = run_str(&source, Some("f"), vec![]);
        let Value::Err(_) = result else {
            panic!("expected Err")
        };
        // parse_format returns Err → line 750-751
    }

    // ── wr 3-arg csv/json (lines 792, 799, 819-820, 835-843) ───────────────

    #[test]
    fn interpret_wr_csv_format() {
        // wr path data "csv" — csv format path → lines 795, 804, 816-817, 824
        let path = "/tmp/ilo_test_wr.csv";
        let source = format!(r#"f>R t t;wr "{path}" [[1,2],[3,4]] "csv""#);
        let result = run_str(&source, Some("f"), vec![]);
        let Value::Ok(_) = result else {
            panic!("expected Ok")
        };
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("1,2"));
    }

    #[test]
    fn interpret_wr_csv_bool_field() {
        // Bool field in csv row → line 819
        let path = "/tmp/ilo_test_wr_bool.csv";
        let source = format!(r#"f>R t t;wr "{path}" [[true,false]] "csv""#);
        let result = run_str(&source, Some("f"), vec![]);
        let Value::Ok(_) = result else {
            panic!("expected Ok")
        };
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("true"));
    }

    #[test]
    fn interpret_wr_json_format() {
        // wr path data "json" → lines 831, 834-848
        let path = "/tmp/ilo_test_wr.json";
        let source = format!(r#"f>R t t;wr "{path}" [1,2,3] "json""#);
        let result = run_str(&source, Some("f"), vec![]);
        let Value::Ok(_) = result else {
            panic!("expected Ok")
        };
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("1"));
    }

    // ── grp Number/Bool key (lines 1012-1016, 1019-1020) ───────────────────

    #[test]
    fn interpret_grp_number_key() {
        // Key fn returns Number → lines 1012-1016
        let source = "id x:n>n;x g xs:L n>_;grp id xs";
        let result = run_str(
            source,
            Some("g"),
            vec![Value::List(Arc::new(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(1.0),
            ]))],
        );
        let Value::Map(m) = result else {
            panic!("expected map")
        };
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn interpret_grp_bool_key() {
        // Key fn returns Bool → lines 1019-1020
        let source = "pos x:n>b;>x 0 g xs:L n>_;grp pos xs";
        let result = run_str(
            source,
            Some("g"),
            vec![Value::List(Arc::new(vec![
                Value::Number(-1.0),
                Value::Number(1.0),
                Value::Number(2.0),
            ]))],
        );
        let Value::Map(m) = result else {
            panic!("expected map")
        };
        assert!(m.contains_key(&MapKey::Text("true".to_string())));
        assert!(m.contains_key(&MapKey::Text("false".to_string())));
    }

    // ── avg non-number element (line 1053) ──────────────────────────────────

    #[test]
    fn interpret_avg_non_number_element() {
        let err = run_str_err(
            "f xs:L n>n;avg xs",
            Some("f"),
            vec![Value::List(Arc::new(vec![Value::Text(Arc::new(
                "x".to_string(),
            ))]))],
        );
        assert!(err.contains("avg"), "got: {err}");
    }

    // ── rgx non-text second arg (line 1065) ─────────────────────────────────

    #[test]
    fn interpret_rgx_non_text_second_arg() {
        let err = run_str_err(r#"f>L t;rgx "." 42"#, Some("f"), vec![]);
        assert!(err.contains("rgx"), "got: {err}");
    }

    // ── jdmp Bool/Nil → value_to_json lines 1179-1180 ───────────────────────

    #[test]
    fn interpret_jdmp_bool_value() {
        // value_to_json Bool branch (line 1179)
        let result = run_str("f>t;jdmp true", Some("f"), vec![]);
        assert_eq!(result, Value::Text(Arc::new("true".to_string())));
    }

    #[test]
    fn interpret_jdmp_nil_value() {
        // value_to_json Nil branch (line 1180) — mget on empty map returns Nil
        let result = run_str(r#"f>t;jdmp (mget mmap "k")"#, Some("f"), vec![]);
        assert_eq!(result, Value::Text(Arc::new("null".to_string())));
    }

    // ── wr json — text/bool/map/nil value types (lines 835-843) ───────────────

    #[test]
    fn interpret_wr_json_text_value() {
        // value_to_json Text branch (line 835)
        let path = "/tmp/ilo_test_wr_json_text.json";
        let source = format!(r#"f>R t t;wr "{path}" "hello world" "json""#);
        let result = run_str(&source, Some("f"), vec![]);
        let Value::Ok(_) = result else {
            panic!("expected Ok")
        };
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("hello world"));
    }

    #[test]
    fn interpret_wr_json_bool_value() {
        // value_to_json Bool branch (line 836)
        let path = "/tmp/ilo_test_wr_json_bool.json";
        let source = format!(r#"f>R t t;wr "{path}" true "json""#);
        let result = run_str(&source, Some("f"), vec![]);
        let Value::Ok(_) = result else {
            panic!("expected Ok")
        };
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("true"));
    }

    #[test]
    fn interpret_wr_json_map_value() {
        // value_to_json Map branch (lines 838-841)
        let path = "/tmp/ilo_test_wr_json_map.json";
        let source = format!(r#"f>R t t;m=mset mmap "k" 42;wr "{path}" m "json""#);
        let result = run_str(&source, Some("f"), vec![]);
        let Value::Ok(_) = result else {
            panic!("expected Ok")
        };
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("\"k\""));
        assert!(content.contains("42"));
    }

    #[test]
    fn interpret_wr_json_nil_value() {
        // value_to_json Nil branch (line 842) — mget on missing key returns Nil
        let path = "/tmp/ilo_test_wr_json_nil.json";
        let source = format!(r#"f>R t t;v=mget mmap "x";wr "{path}" v "json""#);
        let result = run_str(&source, Some("f"), vec![]);
        let Value::Ok(_) = result else {
            panic!("expected Ok")
        };
        let content = std::fs::read_to_string(path).unwrap();
        assert_eq!(content.trim(), "null");
    }

    // ── wr — error paths (lines 792, 799, 826) ────────────────────────────────

    #[test]
    fn interpret_wr_non_text_format_arg_errors() {
        // wr format arg must be text (line 792)
        let path = "/tmp/ilo_test_wr_fmt_err.csv";
        let source = format!(r#"f>R t t;wr "{path}" [1] 42"#);
        let err = run_str_err(&source, Some("f"), vec![]);
        assert!(err.contains("wr"), "got: {err}");
    }

    #[test]
    fn interpret_wr_csv_non_list_data_errors() {
        // wr csv data must be a list (line 799)
        let path = "/tmp/ilo_test_wr_csv_nonlist.csv";
        let source = format!(r#"f>R t t;wr "{path}" 42 "csv""#);
        let err = run_str_err(&source, Some("f"), vec![]);
        assert!(err.contains("wr"), "got: {err}");
    }

    #[test]
    fn interpret_wr_csv_row_not_a_list_errors() {
        // each csv row must be a list (line 826)
        let path = "/tmp/ilo_test_wr_csv_row_err.csv";
        // [42] is a list with element 42 (number, not a list of fields)
        let source = format!(r#"f>R t t;wr "{path}" [42] "csv""#);
        let err = run_str_err(&source, Some("f"), vec![]);
        assert!(err.contains("wr"), "got: {err}");
    }

    // ── grp — float key (line 1016) ──────────────────────────────────────────

    #[test]
    fn interpret_grp_float_key() {
        // Key function returns a fractional number — under MapKey numeric
        // keys floor to i64 at the boundary, matching `at xs i`. So
        // /1 2 = 0.5 → 0, /2 2 = 1 → 1, /3 2 = 1.5 → 1. Two distinct groups.
        let source = "half x:n>n;/x 2 g xs:L n>_;grp half xs";
        let result = run_str(
            source,
            Some("g"),
            vec![Value::List(Arc::new(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ]))],
        );
        let Value::Map(m) = result else {
            panic!("expected Map")
        };
        assert_eq!(m.len(), 2);
        assert!(m.contains_key(&MapKey::Int(0)));
        assert!(m.contains_key(&MapKey::Int(1)));
    }

    // ── ForRange early return (lines 1370-1371) ───────────────────────────────

    #[test]
    fn interpret_for_range_early_return_via_guard() {
        // Use `ret` inside braced guard for early return from loop.
        // When i >= 3, ret returns i → BodyResult::Return propagates out of loop.
        // Syntax: @binding start..end{body}
        let result = run_str("f>n;@i 0..5{>=i 3{ret i};i}", Some("f"), vec![]);
        assert_eq!(result, Value::Number(3.0));
    }

    // ── wr csv with Nil field (line 820) ─────────────────────────────────────

    #[test]
    fn interpret_wr_csv_nil_field() {
        // Nil in a csv row → `other => format!("{other}")` path (line 820)
        // Pass Nil as a z-typed arg to bypass the verifier
        let path = "/tmp/ilo_test_wr_nil.csv";
        let source = format!(r#"f x:z>R t t;wr "{path}" [[x,1]] "csv""#);
        let result = run_str(&source, Some("f"), vec![Value::Nil]);
        let Value::Ok(_) = result else {
            panic!("expected Ok, got {:?}", result)
        };
        let content = std::fs::read_to_string(path).unwrap();
        assert!(!content.is_empty());
    }

    // ── wr json with Ok value (line 843) ─────────────────────────────────────

    #[test]
    fn interpret_wr_json_with_ok_value() {
        // `other => Value::from(format!("{other}"))` path in json value_to_json (line 843)
        // Pass Value::Ok as a z-typed arg to bypass the verifier
        let path = "/tmp/ilo_test_wr_ok.json";
        let source = format!(r#"f x:z>R t t;wr "{path}" x "json""#);
        let result = run_str(
            &source,
            Some("f"),
            vec![Value::Ok(Box::new(Value::Number(1.0)))],
        );
        let Value::Ok(_) = result else {
            panic!("expected Ok, got {:?}", result)
        };
    }

    // ── wr 2-arg non-text content (line 854) ─────────────────────────────────

    #[test]
    fn interpret_wr_two_arg_non_text_content_error() {
        // wr path 42 — second arg is a number, not text (line 854 other => Err)
        let err = run_str_err(
            r#"f>R t t;wr "/tmp/ilo_test_bad_wr.txt" 42"#,
            Some("f"),
            vec![],
        );
        assert!(
            err.contains("wr") || err.contains("text") || err.contains("content"),
            "got: {err}"
        );
    }

    // ── wr fs::write failure (line 859) ──────────────────────────────────────

    #[test]
    fn interpret_wr_write_failure_returns_err() {
        // Write to a non-existent directory → fs::write Err → Value::Err (line 859)
        let source = r#"f>R t t;wr "/no/such/dir/ilo_test.txt" "hello""#;
        let result = run_str(source, Some("f"), vec![]);
        let Value::Err(_) = result else {
            panic!("expected Err for bad path, got {:?}", result)
        };
    }

    // ── wrl fs::write failure (line 874) ─────────────────────────────────────

    #[test]
    fn interpret_wrl_write_failure_returns_err() {
        // Write to a non-existent directory → fs::write Err → Value::Err (line 874)
        let source = r#"f>R t t;wrl "/no/such/dir/ilo_test.txt" ["a","b"]"#;
        let result = run_str(source, Some("f"), vec![]);
        let Value::Err(_) = result else {
            panic!("expected Err for bad path, got {:?}", result)
        };
    }

    // ── jpth array index out of bounds (line 891) ────────────────────────────

    #[test]
    fn interpret_jpth_array_index_out_of_bounds() {
        // jpth where numeric key is out of bounds in array → Err (line 891)
        let source = r#"f>R t t;jpth "[1,2,3]" "5""#;
        let result = run_str(source, Some("f"), vec![]);
        let Value::Err(inner) = result else {
            panic!("expected Err, got {:?}", result)
        };
        let s = inner.to_string();
        assert!(s.contains("not found") || s.contains("5"), "got: {s}");
    }

    // ── grp key returns non-basic type (line 1020) ───────────────────────────

    #[test]
    fn interpret_grp_key_returns_list_error() {
        // Key function returns a List → grp errors at line 1020
        let source = "mk x:n>L n;[x] g xs:L n>_;grp mk xs";
        let err = run_str_err(
            source,
            Some("g"),
            vec![Value::List(Arc::new(vec![
                Value::Number(1.0),
                Value::Number(2.0),
            ]))],
        );
        assert!(
            err.contains("grp") || err.contains("key") || err.contains("string"),
            "got: {err}"
        );
    }

    // ── ForRange non-number start/end (lines 1357, 1361) ─────────────────────

    #[test]
    fn interpret_for_range_non_number_start_error() {
        // @i "a"..3{i} — start is text → error at line 1357
        let err = run_str_err(
            "f s:t>n;@i s..3{i}",
            Some("f"),
            vec![Value::Text(Arc::new("a".to_string()))],
        );
        assert!(
            err.contains("range") || err.contains("number") || err.contains("start"),
            "got: {err}"
        );
    }

    #[test]
    fn interpret_for_range_non_number_end_error() {
        // @i 0..z{i} — end is text → error at line 1361
        let err = run_str_err(
            "f en:t>n;@i 0..en{i}",
            Some("f"),
            vec![Value::Text(Arc::new("b".to_string()))],
        );
        assert!(
            err.contains("range") || err.contains("number") || err.contains("end"),
            "got: {err}"
        );
    }

    // ── FnRef callee from scope (line 1470) ──────────────────────────────────

    #[test]
    fn interpret_fnref_callee_from_scope() {
        // A FnRef stored in a variable is used as a callee (line 1470)
        let source = "sq x:n>n;*x x f cb:z>n;cb 3";
        let result = run_str(source, Some("f"), vec![Value::FnRef("sq".into())]);
        assert_eq!(result, Value::Number(9.0));
    }

    // ── bang on non-Result value passes through (line 1481) ──────────────────

    #[test]
    fn interpret_bang_on_non_result_passes_through() {
        // id! where id returns a Number (not Result) → `other => Ok(other)` (line 1481)
        // id has z return type so verifier doesn't reject !, result passes through
        let source = "id x:n>z;x f>z;id! 42";
        let result = run_str(source, Some("f"), vec![]);
        // id returns Number(42), bang passes it through via the `other` arm
        assert_eq!(result, Value::Number(42.0));
    }

    // ── TypeIs pattern _ => false (line 1700) ────────────────────────────────

    #[test]
    fn interpret_typeis_pattern_non_basic_type_no_match() {
        // TypeIs with a type other than n/t/b/l → `_ => false` (line 1700)
        // Pattern `?x{n _:true;_:false}` for a Record value
        let source = "f x:z>b;?x{n _:true;_:false}";
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Record {
                type_name: "pt".into(),
                fields: std::collections::HashMap::new(),
            }],
        );
        assert_eq!(result, Value::Bool(false));
    }

    // ── brk inside match arm propagates Break (line 1312) ────────────────────

    #[test]
    fn interpret_brk_inside_match_arm_propagates() {
        // ?x { 2: brk x; _ : x } — when x==2 break propagates out of match arm (L1312)
        // The match must NOT be the last stmt in the foreach body; otherwise the _:x arm
        // converts Value(1.0) → Return(1.0) on the first iteration, exiting the function
        // before x=2 is ever reached. Adding ;x as a trailing stmt keeps match non-last.
        let src = "f>n;@x [1,2,3]{?x{2:brk x;_:x};x}";
        let result = run_str(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(2.0));
    }

    // ── text variable used as callee (line 1470) ─────────────────────────────

    #[test]
    fn interpret_text_callee_from_scope() {
        // When a variable holds a Text naming a known function, it is used as the callee (L1470)
        let source = "sq x:n>n;*x x f cb:z>n;cb 3";
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Text(Arc::new("sq".to_string()))],
        );
        assert_eq!(result, Value::Number(9.0));
    }

    // ── srt with bool key hits _ => Equal arm (line 583) ─────────────────────

    #[test]
    fn interpret_srt_bool_key_equal_ordering() {
        // Key fn returns Bool → neither Number nor Text arm matches in sort_by → L583 _ => Equal
        let source = "pos x:n>b;> x 0 f>L n;srt pos [3,-1,2,-2]";
        let result = run_str(source, Some("f"), vec![]);
        // All elements are compared as Bool keys → Equal ordering → list unchanged
        let Value::List(items) = result else {
            panic!("expected List, got {:?}", result)
        };
        assert_eq!(items.len(), 4);
    }

    // ── brk inside guard body propagates Break (line 1287) ───────────────────

    #[test]
    fn interpret_brk_inside_guard_body_propagates() {
        // Guard body containing brk: when x>2, break with x → ForEach exits early (L1287)
        let src = "f>n;@x [1,2,3,4]{>x 2{brk x};x}";
        let result = run_str(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(3.0));
    }

    // ── cnt inside guard body propagates Continue (line 1288) ────────────────

    #[test]
    fn interpret_cnt_inside_guard_body_propagates() {
        // Guard body containing cnt: when x==1, skip iteration → ForEach gets last=3 (L1288)
        let src = "f>n;@x [1,2,3]{=x 1{cnt};x}";
        let result = run_str(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(3.0));
    }

    // ── brk inside ternary then-body propagates Break (line 1275) ─────────────

    #[test]
    fn interpret_brk_inside_ternary_body_propagates() {
        // Ternary cond{then}{else}: then-body contains brk → Break propagates (L1275)
        // When x==2: ternary true → brk x → Break(2.0) exits ForEach early
        let src = "f>n;@x [1,2,3]{=x 2{brk x}{0};0}";
        let result = run_str(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(2.0));
    }

    // ── cnt inside ternary then-body propagates Continue (line 1276) ──────────

    #[test]
    fn interpret_cnt_inside_ternary_body_propagates() {
        // Ternary cond{then}{else}: then-body contains cnt → Continue propagates (L1276)
        // When x==1: ternary true → cnt → Continue skips that iteration
        let src = "f>n;@x [1,2,3]{=x 1{cnt}{0};x}";
        let result = run_str(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(3.0));
    }

    // ── cnt inside match-expression arm returns Nil (line 1551) ──────────────

    #[test]
    fn interpret_cnt_in_match_expr_arm_returns_nil() {
        // Expr::Match arm body returns Continue → match expr yields Nil (L1551)
        // cnt inside match arm is "consumed" — the match expression returns Nil for that arm
        let src = "f>n;@x [1,2,3]{r=?x{1:cnt;_:x};r}";
        let result = run_str(src, Some("f"), vec![]);
        // x=1: match arm 1 runs cnt → Continue consumed → Nil, r=Nil
        // x=2: match arm _ matches → 2, r=2
        // x=3: match arm _ matches → 3, r=3 → foreach last=3
        assert_eq!(result, Value::Number(3.0));
    }

    // ── BodyResult::Continue in eval_call → Ok(Nil) (line 1128) ─────────────

    #[test]
    fn interpret_continue_in_function_body_returns_nil() {
        // cnt at top level of function body → eval_body returns BodyResult::Continue
        // eval_call L1128: BodyResult::Continue => Ok(Value::Nil)
        // Verifier rejects this pattern (ILO-T028), but run_str bypasses the verifier
        let result = run_str("f>_;cnt", Some("f"), vec![]);
        assert_eq!(result, Value::Nil);
    }

    // ── Builtin::Mod (L399-407) ──────────────────────────────────────────────

    #[test]
    fn interpret_mod_normal() {
        let result = run_str(
            "f a:n b:n>n;mod a b",
            Some("f"),
            vec![Value::Number(10.0), Value::Number(3.0)],
        );
        assert_eq!(result, Value::Number(1.0));
    }

    #[test]
    fn interpret_mod_by_zero() {
        let prog = parse_program("f a:n b:n>n;mod a b");
        let err = run(
            &prog,
            Some("f"),
            vec![Value::Number(10.0), Value::Number(0.0)],
        )
        .unwrap_err();
        assert!(err.to_string().contains("modulo by zero"), "got: {err}");
    }

    #[test]
    fn interpret_mod_non_numbers() {
        let prog = parse_program(r#"f a:t b:t>_;mod a b"#);
        let err = run(
            &prog,
            Some("f"),
            vec![
                Value::Text(Arc::new("a".to_string())),
                Value::Text(Arc::new("b".to_string())),
            ],
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("mod requires two numbers"),
            "got: {err}"
        );
    }

    // ── Builtin::Rou (round, L425) ──────────────────────────────────────────

    #[test]
    fn interpret_round() {
        let result = run_str("f x:n>n;rou x", Some("f"), vec![Value::Number(3.7)]);
        assert_eq!(result, Value::Number(4.0));
        let result2 = run_str("f x:n>n;rou x", Some("f"), vec![Value::Number(3.2)]);
        assert_eq!(result2, Value::Number(3.0));
    }

    // ── Ternary expression (L1583-1588) ─────────────────────────────────────

    #[test]
    fn interpret_ternary_then() {
        // Prefix ternary: ?=x 0 10 20 → if x==0 then 10 else 20
        let result = run_str("f x:n>n;?=x 0 10 20", Some("f"), vec![Value::Number(0.0)]);
        assert_eq!(result, Value::Number(10.0));
    }

    #[test]
    fn interpret_ternary_else() {
        let result = run_str("f x:n>n;?=x 0 10 20", Some("f"), vec![Value::Number(5.0)]);
        assert_eq!(result, Value::Number(20.0));
    }

    // ── Literal::Nil in eval_literal (L1611) ────────────────────────────────

    #[test]
    fn interpret_literal_nil() {
        let result = run_str("f>O n;nil", Some("f"), vec![]);
        assert_eq!(result, Value::Nil);
    }

    // ── Pattern::TypeIs wildcard fallback (L1727) ───────────────────────────

    #[test]
    fn interpret_type_is_no_match() {
        // Match a number against a text TypeIs pattern → falls through to wildcard
        let result = run_str(
            r#"f x:n>t;?x{t v:"text";_:"other"}"#,
            Some("f"),
            vec![Value::Number(42.0)],
        );
        assert_eq!(result, Value::Text(Arc::new("other".to_string())));
    }

    // ── Tool call with provider but no async runtime (L1160-1162) ───────────

    #[test]
    fn interpret_tool_call_with_provider_no_runtime() {
        let source = r#"tool greet"say hello" name:t>R _ t timeout:5"#;
        let prog = parse_program(source);
        let provider = std::sync::Arc::new(crate::tools::StubProvider);
        let result = run_with_tools(
            &prog,
            Some("greet"),
            vec![Value::Text(Arc::new("world".to_string()))],
            provider,
        )
        .unwrap();
        assert_eq!(result, Value::Ok(Box::new(Value::Nil)));
    }

    // ── Coverage: rnd builtin with valid bounds (L455) ──────────────────────

    #[test]
    fn interp_rnd_valid_bounds() {
        let result = run_str("f>n;rnd 1 10", None, vec![]);
        match result {
            Value::Number(n) => assert!((1.0..=10.0).contains(&n)),
            _ => panic!("expected number"),
        }
    }

    // ── Coverage: TypeIs pattern with non-primitive type (L1727) ─────────────

    #[test]
    fn interp_type_is_pattern_number() {
        let result = run_str(
            r#"f x:n>t;?x{n v:"num";_:"other"}"#,
            None,
            vec![Value::Number(5.0)],
        );
        assert_eq!(result, Value::Text(Arc::new("num".to_string())));
    }

    #[test]
    fn interp_type_is_pattern_text() {
        let result = run_str(
            r#"f x:t>t;?x{t v:v;_:"other"}"#,
            None,
            vec![Value::Text(Arc::new("hi".to_string()))],
        );
        assert_eq!(result, Value::Text(Arc::new("hi".to_string())));
    }

    #[test]
    fn interp_type_is_pattern_bool() {
        let result = run_str(
            r#"f x:b>t;?x{b v:"matched";_:"other"}"#,
            None,
            vec![Value::Bool(true)],
        );
        assert_eq!(result, Value::Text(Arc::new("matched".to_string())));
    }

    // ── Coverage round 2: TypeIs pattern matching for less common types ──────

    // ── TypeIs with List match (L1726) ──────────────────────────────────────

    #[test]
    fn interp_type_is_list_match_with_binding() {
        // TypeIs List pattern with binding — exercises L1726 and L1731-1732
        let source = r#"f x:L n>t;?x{l v:"list";_:"other"}"#;
        let result = run_str(
            source,
            Some("f"),
            vec![Value::List(Arc::new(vec![
                Value::Number(1.0),
                Value::Number(2.0),
            ]))],
        );
        assert_eq!(result, Value::Text(Arc::new("list".to_string())));
    }

    #[test]
    fn interp_type_is_list_no_match() {
        // TypeIs List pattern tested against a non-list value — falls through to wildcard
        let source = r#"f x:n>t;?x{l _:"list";_:"other"}"#;
        let result = run_str(source, Some("f"), vec![Value::Number(42.0)]);
        assert_eq!(result, Value::Text(Arc::new("other".to_string())));
    }

    // ── TypeIs with Map type → _ => false (L1727) ───────────────────────────

    #[test]
    fn interp_type_is_map_falls_through() {
        // Map type in TypeIs pattern hits the _ => false branch (L1727)
        // since Map is not explicitly matched in the TypeIs arms
        let source = r#"f x:M t n>t;?x{n _:"num";_:"other"}"#;
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Map(Arc::new(std::collections::HashMap::from([(
                MapKey::Text("a".to_string()),
                Value::Number(1.0),
            )])))],
        );
        assert_eq!(result, Value::Text(Arc::new("other".to_string())));
    }

    // ── TypeIs with Nil value → no match on any typed pattern (L1727) ───────

    #[test]
    fn interp_type_is_nil_falls_through() {
        // Nil value doesn't match n/t/b/l patterns — exercises _ => false (L1727)
        let source = r#"f x:O n>t;?x{n _:"num";_:"nil"}"#;
        let result = run_str(source, Some("f"), vec![Value::Nil]);
        assert_eq!(result, Value::Text(Arc::new("nil".to_string())));
    }

    #[test]
    fn interp_type_is_nil_value_against_text() {
        // Nil tested against text TypeIs pattern → falls through
        let source = r#"f x:O t>t;?x{t v:v;_:"none"}"#;
        let result = run_str(source, Some("f"), vec![Value::Nil]);
        assert_eq!(result, Value::Text(Arc::new("none".to_string())));
    }

    // ── `!` auto-unwrap on Optional: nil propagates as the function's return ──

    #[test]
    fn interp_mget_bang_missing_propagates_nil() {
        // mget on an empty map returns nil; `!` propagates nil out of f.
        let source = r#"f>O n;m=mmap;v=mget! m "missing";+v 99"#;
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(result, Value::Nil);
    }

    #[test]
    fn interp_mget_bang_present_returns_inner() {
        // mget on a present key returns the inner value via `!`.
        let source = r#"f>O n;m=mset mmap "k" 5;v=mget! m "k";v"#;
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(result, Value::Number(5.0));
    }

    // ---- at xs i with negative indices (covers new interpreter arms) ----

    #[test]
    fn interp_at_list_negative_last() {
        let source = "f>n;xs=[10,20,30];at xs -1";
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(result, Value::Number(30.0));
    }

    #[test]
    fn interp_at_list_negative_first() {
        let source = "f>n;xs=[10,20,30];at xs -3";
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(result, Value::Number(10.0));
    }

    #[test]
    fn interp_at_text_negative_last() {
        let source = r#"f>t;at "abc" -1"#;
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(result, Value::Text(Arc::new("c".to_string())));
    }

    #[test]
    fn interp_at_list_negative_out_of_range() {
        let prog = parse_program("f>n;xs=[10,20,30];at xs -4");
        let err = run(&prog, Some("f"), vec![]).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("out of range"), "got {msg}");
    }

    #[test]
    fn interp_at_text_negative_out_of_range() {
        let prog = parse_program(r#"f>t;at "ab" -3"#);
        let err = run(&prog, Some("f"), vec![]).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("out of range"), "got {msg}");
    }

    #[test]
    fn interp_at_fractional_index_floors() {
        // Fractional indices auto-floor (was: strict integer guard).
        // 1.5 → 1 → middle element.
        let prog = parse_program("f>n;xs=[10,20,30];at xs 1.5");
        let v = run(&prog, Some("f"), vec![]).unwrap();
        assert_eq!(v, Value::Number(20.0));
    }

    #[test]
    fn interp_at_fractional_negative_index_floors() {
        // -0.5 floors to -1 → last element after negative-index resolution.
        let prog = parse_program("f>n;xs=[10,20,30];at xs -0.5");
        let v = run(&prog, Some("f"), vec![]).unwrap();
        assert_eq!(v, Value::Number(30.0));
    }

    #[test]
    fn interp_at_non_numeric_index_errors() {
        // Non-numeric index still errors (the type guard is preserved).
        let prog = parse_program("f>n;xs=[10,20,30];at xs \"a\"");
        let err = run(&prog, Some("f"), vec![]).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("number") || msg.contains("at"), "got {msg}");
    }

    // ---- lst xs i v: replace element at index, returning a new list ----

    #[test]
    fn interp_lst_happy() {
        let source = "f>L n;lst [10,20,30] 1 99";
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(
            result,
            Value::List(Arc::new(vec![
                Value::Number(10.0),
                Value::Number(99.0),
                Value::Number(30.0),
            ]))
        );
    }

    #[test]
    fn interp_lst_first_index() {
        let source = "f>L n;lst [10,20,30] 0 7";
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(
            result,
            Value::List(Arc::new(vec![
                Value::Number(7.0),
                Value::Number(20.0),
                Value::Number(30.0),
            ]))
        );
    }

    #[test]
    fn interp_lst_out_of_range_errors() {
        let prog = parse_program("f>L n;lst [1,2,3] 5 0");
        let err = run(&prog, Some("f"), vec![]).unwrap_err();
        assert!(format!("{err:?}").contains("out of range"));
    }

    #[test]
    fn interp_lst_negative_index_errors() {
        let prog = parse_program("f>L n;lst [1,2,3] -1 0");
        let err = run(&prog, Some("f"), vec![]).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("non-negative integer"), "got {msg}");
    }

    #[test]
    fn interp_lst_fractional_index_errors() {
        let prog = parse_program("f>L n;lst [1,2,3] 1.5 0");
        let err = run(&prog, Some("f"), vec![]).unwrap_err();
        assert!(format!("{err:?}").contains("non-negative integer"));
    }

    // fmt2 error arm: non-number args bypass the verifier when calling
    // call_function directly, exercising the runtime type guard.
    #[test]
    fn interp_fmt2_rejects_non_number_args() {
        let mut env = Env::new();
        let result = call_function(
            &mut env,
            "fmt2",
            vec![Value::Text(Arc::new("hi".to_string())), Value::Number(2.0)],
        );
        let err = result.unwrap_err();
        assert_eq!(err.code, "ILO-R009");
        assert!(
            err.message.contains("fmt2 requires two numbers"),
            "got: {}",
            err.message
        );
    }

    // ---- box_muller_normal + Builtin::Rndn coverage ----

    #[test]
    fn box_muller_sigma_zero_returns_mu() {
        // sigma == 0 short-circuit: must return exactly mu (no NaN, no jitter).
        assert_eq!(box_muller_normal(5.0, 0.0), 5.0);
        assert_eq!(box_muller_normal(-1.25, 0.0), -1.25);
        assert_eq!(box_muller_normal(0.0, 0.0), 0.0);
    }

    #[test]
    fn box_muller_finite_for_nonzero_sigma() {
        crate::rng::seed(42);
        for _ in 0..200 {
            let v = box_muller_normal(0.0, 1.0);
            assert!(v.is_finite(), "got non-finite {v}");
        }
    }

    #[test]
    fn interp_rndn_sigma_zero_returns_mu() {
        let source = "f>n;rndn 7 0";
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(result, Value::Number(7.0));
    }

    #[test]
    fn interp_rndn_negative_mu_sigma_zero() {
        let source = "f>n;rndn -3 0";
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(result, Value::Number(-3.0));
    }

    // --- Duration helper coverage --------------------------------------

    #[test]
    fn dur_parse_basic_abbreviations() {
        assert_eq!(super::dur_parse("3h 30m"), Ok(12_600.0));
        assert_eq!(super::dur_parse("1d"), Ok(86_400.0));
        assert_eq!(super::dur_parse("2w"), Ok(1_209_600.0));
    }

    #[test]
    fn dur_parse_full_unit_names() {
        assert_eq!(super::dur_parse("1 week 2 days"), Ok(777_600.0));
        assert_eq!(super::dur_parse("1 hour"), Ok(3_600.0));
        assert_eq!(super::dur_parse("30 seconds"), Ok(30.0));
        assert_eq!(super::dur_parse("5 minutes"), Ok(300.0));
    }

    #[test]
    fn dur_parse_decimal_quantity() {
        assert_eq!(super::dur_parse("1.5 hours"), Ok(5_400.0));
        assert_eq!(super::dur_parse("0.5s"), Ok(0.5));
    }

    #[test]
    fn dur_parse_negative_first_token_is_sticky() {
        // Sticky sign: the leading `-` applies to every following token so
        // "-1m 30s" parses as -90, not -30. This guarantees round-trip
        // symmetry with dur-fmt which emits a single leading minus for
        // negative durations.
        assert_eq!(super::dur_parse("-1m 30s"), Ok(-90.0));
        assert_eq!(super::dur_parse("-1h 30m"), Ok(-5_400.0));
    }

    #[test]
    fn dur_parse_explicit_sign_resets_sticky() {
        assert_eq!(super::dur_parse("-1m +30s"), Ok(-30.0));
        assert_eq!(super::dur_parse("+1h -10m"), Ok(3_000.0));
    }

    #[test]
    fn dur_parse_months_rejected() {
        // Months are not supported (variable length). "3mo", "3 months",
        // "3M" all fall through to the no-unit-matched error path.
        assert!(super::dur_parse("3mo").is_err());
        assert!(super::dur_parse("3 months").is_err());
        assert!(super::dur_parse("3 month").is_err());
    }

    #[test]
    fn dur_parse_unknown_unit_skipped() {
        // Unknown unit on its own is an error; mixed with a valid token
        // the unknown is dropped and the valid token wins.
        assert!(super::dur_parse("3xyz").is_err());
        assert_eq!(super::dur_parse("3xyz 5s"), Ok(5.0));
    }

    #[test]
    fn dur_parse_empty_and_whitespace() {
        assert!(super::dur_parse("").is_err());
        assert!(super::dur_parse("   ").is_err());
    }

    #[test]
    fn dur_parse_no_recognised_unit() {
        assert!(super::dur_parse("hello").is_err());
        assert!(super::dur_parse("42").is_err());
    }

    #[test]
    fn dur_fmt_basic() {
        assert_eq!(super::dur_fmt(0.0), "0s");
        assert_eq!(super::dur_fmt(90.0), "1m 30s");
        assert_eq!(super::dur_fmt(9_720.0), "2h 42m");
        assert_eq!(super::dur_fmt(86_400.0), "1 day");
        assert_eq!(super::dur_fmt(604_800.0), "1 week");
    }

    #[test]
    fn dur_fmt_preserves_fractional_seconds() {
        // Sub-second fractions are preserved, with trailing zeros stripped.
        assert_eq!(super::dur_fmt(0.5), "0.5s");
        // Fractions on top of whole seconds are also preserved (the prior
        // implementation silently truncated these).
        assert_eq!(super::dur_fmt(90.5), "1m 30.5s");
        assert_eq!(super::dur_fmt(1.75), "1.75s");
    }

    #[test]
    fn dur_fmt_negative_round_trips() {
        // Negative durations emit a single leading minus, and dur-parse's
        // sticky sign restores the full value on round-trip.
        assert_eq!(super::dur_fmt(-90.0), "-1m 30s");
        assert_eq!(super::dur_parse(&super::dur_fmt(-90.0)), Ok(-90.0));
        assert_eq!(super::dur_parse(&super::dur_fmt(-5_400.0)), Ok(-5_400.0));
    }

    #[test]
    fn dur_fmt_non_finite_passthrough() {
        assert_eq!(super::dur_fmt(f64::INFINITY), "inf");
        assert_eq!(super::dur_fmt(f64::NEG_INFINITY), "-inf");
        assert_eq!(super::dur_fmt(f64::NAN), "NaN");
    }

    #[test]
    fn dur_round_trip_examples() {
        for &secs in &[
            0.0_f64, 1.0, 30.0, 90.0, 3_600.0, 9_720.0, 86_400.0, 604_800.0,
        ] {
            let s = super::dur_fmt(secs);
            assert_eq!(
                super::dur_parse(&s),
                Ok(secs),
                "round-trip failed for {secs}"
            );
        }
    }

    // ── rand-bytes / base64url-no-pad encoder ────────────────────────────────
    //
    // The encoder is internal to the interpreter (no third-party base64
    // dep yet on main), so we pin it against known RFC 4648 §5 vectors here.
    // If anyone tweaks the byte-shuffle these break immediately.

    #[test]
    fn b64url_no_pad_empty() {
        assert_eq!(super::b64url_no_pad_encode(b""), "");
    }

    #[test]
    fn b64url_no_pad_one_byte() {
        // "f" -> "Zg" (single trailing-byte branch, rem.len() == 1)
        assert_eq!(super::b64url_no_pad_encode(b"f"), "Zg");
    }

    #[test]
    fn b64url_no_pad_two_bytes() {
        // "fo" -> "Zm8" (two trailing-bytes branch, rem.len() == 2)
        assert_eq!(super::b64url_no_pad_encode(b"fo"), "Zm8");
    }

    #[test]
    fn b64url_no_pad_three_bytes() {
        // "foo" -> "Zm9v" (exact 3-byte multiple, no remainder)
        assert_eq!(super::b64url_no_pad_encode(b"foo"), "Zm9v");
    }

    #[test]
    fn b64url_no_pad_canonical_hello() {
        // Standard b64 of "hello" is "aGVsbG8=" — strip the `=` for no-pad.
        assert_eq!(super::b64url_no_pad_encode(b"hello"), "aGVsbG8");
    }

    #[test]
    fn b64url_no_pad_url_safe_chars() {
        // Input chosen so the standard b64 output contains `+` and `/`,
        // which the url-safe alphabet must rewrite to `-` and `_`.
        // Bytes [0xfb, 0xff, 0xbf] -> std b64 "+/+/" -> url-safe "-_-_".
        assert_eq!(super::b64url_no_pad_encode(&[0xfb, 0xff, 0xbf]), "-_-_");
    }

    #[test]
    fn b64url_no_pad_length_formula() {
        // Output length = ceil(n * 4 / 3) for non-zero n, with the trailing
        // `=` chars dropped. Pins the length contract that callers rely on
        // (e.g. `len (rand-bytes 16) == 22` for a jti token).
        for n in 0..=64 {
            let bytes = vec![0xa5u8; n];
            let encoded = super::b64url_no_pad_encode(&bytes);
            let expected = if n == 0 {
                0
            } else {
                // ceil(n/3)*4 minus padding chars
                let pad = match n % 3 {
                    1 => 2,
                    2 => 1,
                    _ => 0,
                };
                n.div_ceil(3) * 4 - pad
            };
            assert_eq!(encoded.len(), expected, "n={n}: encoded={encoded:?}");
        }
    }

    #[test]
    fn rand_bytes_negative_returns_err() {
        let r = super::eval_rand_bytes(&Value::Number(-1.0));
        assert!(matches!(&r, Err(e) if e.code == "ILO-R009"), "got {r:?}");
    }

    #[test]
    fn rand_bytes_non_finite_returns_err() {
        for n in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let r = super::eval_rand_bytes(&Value::Number(n));
            assert!(
                matches!(&r, Err(e) if e.code == "ILO-R009"),
                "n={n} got {r:?}"
            );
        }
    }

    #[test]
    fn rand_bytes_over_cap_returns_err() {
        let r = super::eval_rand_bytes(&Value::Number(2.0 * 1024.0 * 1024.0));
        assert!(matches!(&r, Err(e) if e.code == "ILO-R009"), "got {r:?}");
    }

    #[test]
    fn rand_bytes_zero_returns_empty_text() {
        let r = super::eval_rand_bytes(&Value::Number(0.0)).expect("Ok");
        match r {
            Value::Text(s) => assert_eq!(s.as_str(), ""),
            other => panic!("expected Text(\"\"), got {other:?}"),
        }
    }

    #[test]
    fn rand_bytes_16_returns_22_char_text() {
        let r = super::eval_rand_bytes(&Value::Number(16.0)).expect("Ok");
        match r {
            Value::Text(s) => {
                assert_eq!(s.len(), 22, "got {s:?}");
                assert!(
                    s.chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                    "non-b64url char in {s:?}"
                );
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }
    // ── run2 cross-engine regression tests ──────────────────────────────

    #[test]
    #[cfg(not(target_family = "wasm"))]
    fn run2_echo_stdout_is_record() {
        // echo populates stdout; exit is 0; stderr is empty.
        let src = r#"f>_;r=run2!! "echo" ["hello"];r"#;
        let v = run_str(src, Some("f"), vec![]);
        match v {
            Value::Record {
                ref type_name,
                ref fields,
            } => {
                assert_eq!(type_name, "RunResult");
                assert_eq!(
                    fields.get("stdout"),
                    Some(&Value::Text(Arc::new("hello\n".to_string())))
                );
                assert_eq!(
                    fields.get("stderr"),
                    Some(&Value::Text(Arc::new(String::new())))
                );
                assert_eq!(fields.get("exit"), Some(&Value::Number(0.0)));
            }
            other => panic!("expected RunResult record, got {:?}", other),
        }
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    fn run2_false_exit_nonzero() {
        // `false` exits 1; not an Err -- exit field carries the code as a number.
        let src = r#"f>n;r=run2!! "false" [];r.exit"#;
        assert_eq!(run_str(src, Some("f"), vec![]), Value::Number(1.0));
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    fn run2_true_exit_zero() {
        let src = r#"f>n;r=run2!! "true" [];r.exit"#;
        assert_eq!(run_str(src, Some("f"), vec![]), Value::Number(0.0));
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    fn run2_nonexistent_is_err() {
        // Spawn failure surfaces as Err, not Ok.
        let src = r#"f>b;r=run2 "no-such-command-xyz-run2" [];?r{~v:false;^e:true}"#;
        assert_eq!(run_str(src, Some("f"), vec![]), Value::Bool(true));
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    fn run2_stderr_captured() {
        // Verify stdout and stderr are captured into separate fields.
        // echo puts text in stdout; stderr should be empty (len 0).
        let src = r#"f>n;r=run2!! "echo" ["hello"];len r.stdout"#;
        let v = run_str(src, Some("f"), vec![]);
        // "hello\n" is 6 bytes/chars
        assert_eq!(v, Value::Number(6.0));
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    fn run2_exit_is_number_not_text() {
        // Regression: exit must be Number, not Text (run uses Text for code).
        let src = r#"f>b;r=run2!! "true" [];?r.exit{0:true;_:false}"#;
        assert_eq!(run_str(src, Some("f"), vec![]), Value::Bool(true));
    }

    // ── ILO-62: Sum types (discriminated unions) ────────────────────────────

    #[test]
    fn sum_type_payload_less_variant_returns_variant_value() {
        let src = r#"type color = red | green | blue
f>t;c=red;?c{red:"r";green:"g";blue:"b"}"#;
        assert_eq!(
            run_str(src, Some("f"), vec![]),
            Value::Text(Arc::new("r".to_string()))
        );
    }

    #[test]
    fn sum_type_payload_variant_carries_value() {
        let src = r#"type shape = circle(n) | point
f>n;s=circle 5;?s{circle(r):r;point:0}"#;
        assert_eq!(run_str(src, Some("f"), vec![]), Value::Number(5.0));
    }

    #[test]
    fn sum_type_wildcard_arm_catches_remaining() {
        let src = r#"type shape = circle(n) | square(n) | point
f>t;s=point;?s{circle(r):"c";_:"other"}"#;
        assert_eq!(
            run_str(src, Some("f"), vec![]),
            Value::Text(Arc::new("other".to_string()))
        );
    }

    #[test]
    fn sum_type_multiple_payload_variants_inline() {
        let src = r#"type shape = circle(n) | square(n) | point
area s:shape>n;?s{circle(r):*3 r;square(side):*side side;point:0}
f>n;+area(circle 2) area(square 3)"#;
        assert_eq!(run_str(src, Some("f"), vec![]), Value::Number(6.0 + 9.0));
    }

    // ---- todo / panic typed expressions (ILO-410) ----

    #[test]
    fn todo_expr_produces_runtime_error() {
        let prog = parse_program(r#"f>n;todo "not yet""#);
        let result = run(&prog, None, vec![]);
        match result {
            Err(e) => {
                assert_eq!(e.code, "ILO-R020", "expected ILO-R020, got {}", e.code);
                assert!(e.message.contains("not yet"), "message was: {}", e.message);
            }
            Ok(v) => panic!("expected runtime error from todo, got value: {:?}", v),
        }
    }

    #[test]
    fn panic_expr_produces_runtime_error() {
        let prog = parse_program(r#"f>n;panic "unreachable""#);
        let result = run(&prog, None, vec![]);
        match result {
            Err(e) => {
                assert_eq!(e.code, "ILO-R021", "expected ILO-R021, got {}", e.code);
                assert!(
                    e.message.contains("unreachable"),
                    "message was: {}",
                    e.message
                );
            }
            Ok(v) => panic!("expected runtime error from panic, got value: {:?}", v),
        }
    }

    // par-map tests (ILO-67)

    #[test]
    fn par_map_applies_fn_to_each_element_in_order() {
        // double x = x * 2; par-map over [1,2,3] with concurrency 2 => [2,4,6]
        let src = r#"dbl x:n>n;*x 2  main>L n;xs=[1 2 3];ys=par-map dbl xs 2;map (y:_>n;?y{~v:v;^_:0}) ys"#;
        let result = run_str(src, Some("main"), vec![]);
        assert_eq!(
            result,
            Value::List(Arc::new(vec![
                Value::Number(2.0),
                Value::Number(4.0),
                Value::Number(6.0),
            ]))
        );
    }

    #[test]
    fn par_map_empty_list_returns_empty() {
        let src = r#"dbl x:n>n;*x 2  main>L n;par-map dbl [] 4"#;
        let result = run_str(src, Some("main"), vec![]);
        assert_eq!(result, Value::List(Arc::new(vec![])));
    }

    #[test]
    fn par_map_default_concurrency_two_arg_form() {
        // 2-arg form (no explicit n): should still work
        let src =
            r#"sq x:n>n;*x x  main>L n;xs=[1 2 3 4];ys=par-map sq xs;map (y:_>n;?y{~v:v;^_:0}) ys"#;
        let result = run_str(src, Some("main"), vec![]);
        assert_eq!(
            result,
            Value::List(Arc::new(vec![
                Value::Number(1.0),
                Value::Number(4.0),
                Value::Number(9.0),
                Value::Number(16.0),
            ]))
        );
    }

    // ILO-354: chunking strategy — large list of small items processed correctly
    // with auto-tuned chunk size (ceil(len / num_cpus) items per thread).
    #[test]
    fn par_map_large_list_chunking() {
        // 100 items [0..99], each doubled — verifies order-preservation across
        // multiple auto-sized chunks. Uses `range 0 100` (2-arg form).
        let src = r#"dbl x:n>n;*x 2  main>L n;xs=range 0 100;ys=par-map dbl xs;map (y:_>n;?y{~v:v;^_:0}) ys"#;
        let result = run_str(src, Some("main"), vec![]);
        if let Value::List(list) = result {
            assert_eq!(list.len(), 100);
            for (i, v) in list.iter().enumerate() {
                assert_eq!(*v, Value::Number((i * 2) as f64), "mismatch at index {i}");
            }
        } else {
            panic!("expected a list");
        }
    }

    // ILO-354: cancellation — an error in one item causes remaining items
    // to be short-circuited (filled with cancellation Err sentinel).
    #[test]
    fn par_map_error_cancels_remaining_workers() {
        // `boom` errors on x == 5 by performing `at [] 0` (out-of-bounds),
        // which is a RuntimeError (not a Value::Err), triggering cancellation.
        // Items after 5 should be Err (original error or cancellation sentinel).
        // We use concurrency=1 so items are processed strictly in order.
        let src = r#"boom x:n>n;=x 5{at [] 0};x  main>L n;xs=range 0 10;par-map boom xs 1"#;
        let result = run_str(src, Some("main"), vec![]);
        if let Value::List(list) = result {
            assert_eq!(list.len(), 10);
            // Items 0..5 should be Ok.
            for i in 0..5 {
                assert!(
                    matches!(&list[i], Value::Ok(_)),
                    "expected Ok at index {i}, got {:?}",
                    list[i]
                );
            }
            // Item 5 should be Err (the explicit failure).
            assert!(
                matches!(&list[5], Value::Err(_)),
                "expected Err at index 5, got {:?}",
                list[5]
            );
            // Items 6..10 should be Err (cancelled).
            for i in 6..10 {
                assert!(
                    matches!(&list[i], Value::Err(_)),
                    "expected cancelled Err at index {i}, got {:?}",
                    list[i]
                );
            }
        } else {
            panic!("expected a list");
        }
    }

    // ILO-354: par_map_chunk_size helper — unit test for the auto-tuning formula.
    #[test]
    fn par_map_chunk_size_formula() {
        use super::par_map_chunk_size;
        assert_eq!(par_map_chunk_size(100, 4), 25);
        assert_eq!(par_map_chunk_size(101, 4), 26); // ceil(101/4)
        assert_eq!(par_map_chunk_size(1, 8), 1);
        assert_eq!(par_map_chunk_size(0, 4), 0);
        assert_eq!(par_map_chunk_size(10, 0), 10); // 0 threads treated as 1
    }
}
