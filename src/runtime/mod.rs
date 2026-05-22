use crate::ast::*;
use crate::builtins::{Builtin, CharAtResult, char_at_signed};
use crate::caps::Caps;
use std::collections::HashMap;
use std::sync::Arc;

pub mod json;

// Trace hook (ILO-72) removed in PR E of ILO-45: it fired from `eval_body`,
// which no longer exists. VM/JIT trace paths tracked at ILO-343.

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

/// Builtin-dispatch context. After PR E of ILO-45 deleted the tree-walker
/// eval loop, this carries only what the bridge-entry builtin impls still
/// need: the program's function/tool table (so HOF callbacks the bridge
/// might dispatch can be resolved if PR E's audit ever drifts) and the
/// capability policy that IO/network/process builtins consult.
struct Env {
    functions: HashMap<String, Decl>,
    /// CLI capability policy — checked at IO builtin call sites.
    caps: Arc<Caps>,
}

thread_local! {
    /// Active capability policy for bridge-dispatched builtins. Set by the
    /// VM/Cranelift caps-aware bridge entry (`call_builtin_for_bridge_with_caps`
    /// or the program-aware variant), read by `Env::new()` so IO/network/
    /// process builtin impls observe the live caps even though the bridge
    /// itself constructs a fresh `Env` per call. Cleared by the RAII guard
    /// `ActiveCapsGuard` after dispatch returns.
    static ACTIVE_CAPS: std::cell::RefCell<Option<Arc<Caps>>> =
        const { std::cell::RefCell::new(None) };
}

struct ActiveCapsGuard;

impl ActiveCapsGuard {
    fn install(caps: Arc<Caps>) -> Self {
        ACTIVE_CAPS.with(|c| *c.borrow_mut() = Some(caps));
        ActiveCapsGuard
    }
}

impl Drop for ActiveCapsGuard {
    fn drop(&mut self) {
        ACTIVE_CAPS.with(|c| *c.borrow_mut() = None);
    }
}

impl Env {
    fn new() -> Self {
        let caps = ACTIVE_CAPS
            .with(|c| c.borrow().clone())
            .unwrap_or_else(|| Arc::new(Caps::default()));
        Env {
            functions: HashMap::new(),
            caps,
        }
    }
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

/// Caps-aware bridge entry. Installs the caller's capability policy into
/// `ACTIVE_CAPS` for the duration of dispatch so IO/network/process builtin
/// impls (`run`, `get`, `pst`, `rd`, `wr`, …) observe the live caps. RAII
/// guard clears the TLS slot on drop.
pub fn call_builtin_for_bridge_with_caps(
    name: &str,
    args: Vec<Value>,
    caps: Arc<Caps>,
) -> Result<Value> {
    let _guard = ActiveCapsGuard::install(caps);
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
            Decl::TypeDef { .. } | Decl::Alias { .. } | Decl::Use { .. } | Decl::Error { .. } => {}
        }
    }
    call_function(&mut env, name, args)
}

/// Program-aware AND caps-aware bridge entry. Same semantics as
/// `call_builtin_for_bridge_with_program` plus an `ACTIVE_CAPS` install.
pub fn call_builtin_for_bridge_with_program_and_caps(
    name: &str,
    args: Vec<Value>,
    program: &Program,
    caps: Arc<Caps>,
) -> Result<Value> {
    let _guard = ActiveCapsGuard::install(caps);
    call_builtin_for_bridge_with_program(name, args, program)
}

// box_muller_normal removed in PR E of ILO-45: was only exercised by the
// deleted `runtime::tests` module. Production code paths use `crate::rng`
// directly. The orphan doc-comment lines above were leftover from the
// deleted `parse_to_value` helper; restored here so `parse_to_value`'s
// definition below is annotated correctly.

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
    hex::decode(s.as_ref()).map_err(|e| {
        RuntimeError::new(
            "ILO-R009",
            format!("{caller}: invalid hex input: {e}"),
        )
    })
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
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("str requires a number, got {:?}", other),
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
            (Value::Number(x), Value::Number(lo), Value::Number(hi)) => {
                // Semantics: result = max(lo, min(hi, x)). When lo > hi the
                // outer max wins and returns lo, so the result is always >= lo.
                Ok(Value::Number(x.min(*hi).max(*lo)))
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "clamp requires three numbers".to_string(),
            )),
        };
    }
    if matches!(builtin, Some(Builtin::Min | Builtin::Max)) && args.len() == 2 {
        return match (&args[0], &args[1]) {
            (Value::Number(a), Value::Number(b)) => {
                let result = if builtin == Some(Builtin::Min) {
                    a.min(*b)
                } else {
                    a.max(*b)
                };
                Ok(Value::Number(result))
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                format!("{} requires two numbers", name),
            )),
        };
    }
    if matches!(builtin, Some(Builtin::Argmax | Builtin::Argmin)) && args.len() == 1 {
        // arg{max,min} xs:L n > n — index of the {max,min} element. numpy
        // convention: first occurrence wins on ties (strict `<`/`>`).
        // NaN-propagation: any NaN element makes the result NaN (mirrors
        // `max`/`min` 1-arg list form on NaN — see `vm_min_max_lst`).
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
        let is_min = builtin == Some(Builtin::Argmin);
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
        return Ok(Value::Number(best_idx as f64));
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
        if n_raw.fract() != 0.0 || n_raw <= 0.0 {
            return Err(RuntimeError::new(
                "ILO-R009",
                format!("chunks: size must be a positive integer, got {n_raw}"),
            ));
        }
        let n = n_raw as usize;
        let xs = match &args[1] {
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
        return Ok(Value::List(Arc::new(out)));
    }
    if matches!(
        builtin,
        Some(Builtin::Setunion) | Some(Builtin::Setinter) | Some(Builtin::Setdiff)
    ) && args.len() == 2
    {
        let op_name = match builtin {
            Some(Builtin::Setunion) => "setunion",
            Some(Builtin::Setinter) => "setinter",
            Some(Builtin::Setdiff) => "setdiff",
            _ => unreachable!(),
        };
        let xs = match &args[0] {
            Value::List(items) => items,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("{op_name} arg 1 requires a list, got {:?}", other),
                ));
            }
        };
        let ys = match &args[1] {
            Value::List(items) => items,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("{op_name} arg 2 requires a list, got {:?}", other),
                ));
            }
        };
        // Build type-prefixed string keys to avoid Number(5)/Text("5") collisions
        // (same precedent as uniqby post-hotfix). Restrict elements to t/n/b.
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
                // Need a combined lookup; clone into a single map.
                // Use a static-ish approach: merge into a_first below.
                let mut merged = a_first;
                for (k, v) in &b_first {
                    merged.entry(k.clone()).or_insert_with(|| v.clone());
                }
                keys.sort();
                // Return early with merged map by re-binding locally.
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
        return Ok(Value::List(Arc::new(out)));
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
            #[cfg(feature = "http")]
            {
                let mut req = minreq::get(url.as_str());
                for (k, v) in &headers {
                    req = req.with_header(k.as_str(), v.as_str());
                }
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
                let _ = (url, headers);
                Ok(Value::Err(Box::new(Value::Text(
                    "http feature not enabled".to_string().into(),
                ))))
            }
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
        };
    }
    // HTTP verb cluster (#5z). Same shape as `pst` (PUT, PATCH) or `get`
    // (DELETE, HEAD, OPTIONS) — optional 3rd-arg (PUT/PAT) or 2nd-arg
    // (DEL/HD/OPT) `M t t` headers map. Returns `R t t`.
    if matches!(builtin, Some(Builtin::Put) | Some(Builtin::Pat))
        && (args.len() == 2 || args.len() == 3)
    {
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
        return {
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
        };
    }
    if matches!(
        builtin,
        Some(Builtin::Del) | Some(Builtin::Hed) | Some(Builtin::Opt)
    ) && (args.len() == 1 || args.len() == 2)
    {
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
        return {
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
                let _ = (url, headers);
                Ok(Value::Err(Box::new(Value::Text(
                    "http feature not enabled".to_string().into(),
                ))))
            }
        };
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
        return match &args[0] {
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
        };
    }
    if (builtin == Some(Builtin::Padl) || builtin == Some(Builtin::Padr))
        && (args.len() == 2 || args.len() == 3)
    {
        let name = if builtin == Some(Builtin::Padl) {
            "padl"
        } else {
            "padr"
        };
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
        // Resolve pad char: explicit arg validated as a 1-Unicode-scalar string,
        // or ' ' when omitted (2-arg form). Single-char enforcement keeps width-in-chars
        // semantics meaningful — a multi-char pad would make the output not line up to `w`.
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
        let out = if builtin == Some(Builtin::Padl) {
            format!("{pad}{s}")
        } else {
            format!("{s}{pad}")
        };
        return Ok(Value::Text(Arc::new(out)));
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
                // Collect spec body up to '}'.
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
                    // Unterminated brace — leave as literal (matches the old
                    // permissive behaviour for `{a:1}` style non-placeholder
                    // text that just happens to start with `{`).
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
        return Ok(Value::Text(Arc::new(result)));
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
            // auto-detect from extension
            std::path::Path::new(path.as_str())
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("raw")
                .to_lowercase()
        };
        return match std::fs::read_to_string(path.as_str()) {
            Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
            Ok(content) => match parse_format(&fmt, &content) {
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
    if builtin == Some(Builtin::Wr) && (args.len() == 2 || args.len() == 3) {
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
                                format!(
                                    "wr: data for {fmt} must be a list of rows, got {:?}",
                                    other
                                ),
                            ));
                        }
                    };
                    write_csv_tsv(rows, sep)?
                }
                "json" => {
                    fn value_to_json(v: &Value) -> serde_json::Value {
                        match v {
                            Value::Number(n) => serde_json::Value::from(*n),
                            Value::Text(s) => serde_json::Value::from(s.as_str()),
                            Value::Bool(b) => serde_json::Value::from(*b),
                            Value::List(l) => {
                                serde_json::Value::Array(l.iter().map(value_to_json).collect())
                            }
                            Value::Map(m) => {
                                let obj: serde_json::Map<String, serde_json::Value> = m
                                    .iter()
                                    .map(|(k, v)| (k.to_display_string(), value_to_json(v)))
                                    .collect();
                                serde_json::Value::Object(obj)
                            }
                            Value::Nil => serde_json::Value::Null,
                            other => serde_json::Value::from(format!("{other}")),
                        }
                    }
                    serde_json::to_string_pretty(&value_to_json(&args[1]))
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
        return match std::fs::write(path.as_str(), &content) {
            Ok(()) => Ok(Value::Ok(Box::new(Value::Text(path)))),
            Err(e) => Ok(Value::Err(Box::new(Value::Text(Arc::new(e.to_string()))))),
        };
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
        // Extract a as Vec<Vec<f64>>
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
        return Ok(Value::List(Arc::new(out)));
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
        let items = match &args[0] {
            Value::List(l) => l,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("ewm: first arg must be a list, got {:?}", other),
                ));
            }
        };
        let a = match &args[1] {
            Value::Number(n) => *n,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("ewm: second arg a must be a number, got {:?}", other),
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
        return Ok(Value::List(Arc::new(out)));
    }
    // Rolling-window reducers — rsum / ravg / rmin (n, xs).
    if let Some(b) = builtin
        && matches!(b, Builtin::Rsum | Builtin::Ravg | Builtin::Rmin)
        && args.len() == 2
    {
        let name = b.name();
        let n_f = match &args[0] {
            Value::Number(n) => *n,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("{name}: first arg n must be a number, got {:?}", other),
                ));
            }
        };
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
        let items = match &args[1] {
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
        return Ok(Value::List(Arc::new(out)));
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
        return Ok(Value::List(Arc::new(out)));
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
        // Per the NaN contract for math builtins (PR #162): if any input is
        // NaN, propagate NaN rather than silently sorting it to an arbitrary
        // position via `partial_cmp(...).unwrap_or(Equal)`.
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
        return Ok(Value::Number(m));
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
        // NaN-propagation: if any input is NaN, return NaN (see median).
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
        return Ok(Value::Number(q));
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
        // NaN-propagation: any NaN input → NaN result.
        if nums.iter().any(|x| x.is_nan()) {
            return Ok(Value::Number(f64::NAN));
        }
        let mean = nums.iter().sum::<f64>() / n as f64;
        let sse: f64 = nums.iter().map(|x| (x - mean).powi(2)).sum();
        return Ok(Value::Number(sse / (n - 1) as f64));
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
        // NaN-propagation: any NaN input → NaN result.
        if nums.iter().any(|x| x.is_nan()) {
            return Ok(Value::Number(f64::NAN));
        }
        let mean = nums.iter().sum::<f64>() / n as f64;
        let sse: f64 = nums.iter().map(|x| (x - mean).powi(2)).sum();
        return Ok(Value::Number((sse / (n - 1) as f64).sqrt()));
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
        let re = regex::Regex::new(pattern).map_err(|e| {
            RuntimeError::new("ILO-R009", format!("rgx: invalid regex pattern: {e}"))
        })?;
        let result: Vec<Value> = if re.captures_len() > 1 {
            // Has capture groups — return list of captured group strings
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
            // No capture groups — return list of all matches
            re.find_iter(input)
                .map(|m| Value::Text(Arc::new(m.as_str().to_string())))
                .collect()
        };
        return Ok(Value::List(Arc::new(result)));
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
        let re = regex::Regex::new(pattern).map_err(|e| {
            RuntimeError::new("ILO-R009", format!("rgxall: invalid regex pattern: {e}"))
        })?;
        // Outer shape is always L (L t). Inner-list contents depend on the
        // pattern:
        // - No capture groups: inner list is [whole_match] (length 1).
        // - With capture groups: inner list holds only the groups that
        //   *participated* in this particular match, in declaration order.
        //   For straight patterns like `(\w+)=(\d+)` that means N declared
        //   groups = N inner-list entries on every match. For alternation
        //   patterns like `(a)|(b)`, only the branch that fired contributes,
        //   so the inner list can be shorter than the declared group count.
        //   This matches the `rgx` family's existing filter_map semantics
        //   and avoids the empty-string-sentinel ambiguity of an alternative
        //   "always emit N slots" design.
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
        return Ok(Value::List(Arc::new(result)));
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
        let re = regex::Regex::new(pattern).map_err(|e| {
            RuntimeError::new("ILO-R009", format!("rgxall1: invalid regex pattern: {e}"))
        })?;
        // Single-capture-group convenience over rgxall.
        // - 0 groups: returns the flat list of whole matches (L t).
        // - 1 group: returns the flat list of capture-1 strings (L t),
        //   skipping any match where group 1 did not participate
        //   (parallels rgxall's filter_map semantics for non-participating
        //   groups under alternation).
        // - 2+ groups: runtime error pointing the user back at rgxall, which
        //   returns L (L t) and preserves every group on every match.
        //   We surface this at runtime rather than verify time because the
        //   group-count check requires inspecting the literal pattern, and
        //   patterns often arrive as values from bindings; the verifier
        //   doesn't track regex group arity.
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
        return Ok(Value::List(Arc::new(result)));
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
        return Ok(Value::List(Arc::new(result)));
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
        let re = regex::Regex::new(pattern).map_err(|e| {
            RuntimeError::new("ILO-R009", format!("rgxsub: invalid regex pattern: {e}"))
        })?;
        return Ok(Value::Text(Arc::new(
            re.replace_all(subject, replacement).into_owned(),
        )));
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
        return Ok(Value::List(Arc::new(result)));
    }

    // The tree-walker engine was removed in PR E of ILO-45. call_function
    // is now a builtin-only dispatcher — every reachable callsite (the
    // VM/Cranelift tree-bridge) only ever passes builtin names. A
    // non-builtin name reaching here is a bridge bug, not user input, so
    // it surfaces as a structured runtime error rather than panicking.
    Err(RuntimeError::new(
        "ILO-R002",
        format!("call_function: unknown builtin '{}'", name),
    ))
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
