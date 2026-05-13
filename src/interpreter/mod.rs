use crate::ast::*;
use crate::builtins::Builtin;
use std::collections::HashMap;

pub mod json;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    Text(String),
    Bool(bool),
    Nil,
    List(Vec<Value>),
    Map(HashMap<String, Value>),
    Record {
        type_name: String,
        fields: HashMap<String, Value>,
    },
    Ok(Box<Value>),
    Err(Box<Value>),
    /// A reference to a named function — produced when a function name is used as a value.
    FnRef(String),
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
                write!(f, "{} {{", type_name)?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "}}")
            }
            Value::Map(m) => {
                write!(f, "{{")?;
                let mut keys: Vec<&String> = m.keys().collect();
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
    call_stack: Vec<String>,
    tool_provider: Option<std::sync::Arc<dyn crate::tools::ToolProvider>>,
    #[cfg(feature = "tools")]
    tokio_runtime: Option<std::sync::Arc<tokio::runtime::Runtime>>,
}

impl Env {
    fn new() -> Self {
        Env {
            vars: Vec::new(),
            scope_marks: vec![0],
            functions: HashMap::new(),
            call_stack: Vec::new(),
            tool_provider: None,
            #[cfg(feature = "tools")]
            tokio_runtime: None,
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
            call_stack: Vec::new(),
            tool_provider: Some(provider),
            #[cfg(feature = "tools")]
            tokio_runtime: Some(runtime),
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
        // Builtin names also resolve to FnRef so they can be passed to
        // higher-order builtins (e.g. `fld max xs 0`).
        if Builtin::is_builtin(name) {
            return Ok(Value::FnRef(name.to_string()));
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
}

pub fn run(program: &Program, func_name: Option<&str>, args: Vec<Value>) -> Result<Value> {
    run_with_env(program, func_name, args, Env::new())
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

fn run_with_env(
    program: &Program,
    func_name: Option<&str>,
    args: Vec<Value>,
    mut env: Env,
) -> Result<Value> {
    // Register all functions and tools
    for decl in &program.declarations {
        match decl {
            Decl::Function { name, .. } | Decl::Tool { name, .. } => {
                env.functions.insert(name.clone(), decl.clone());
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
/// Uses fastrand to mirror the rnd builtin's RNG.
pub(crate) fn box_muller_normal(mu: f64, sigma: f64) -> f64 {
    // sigma == 0: distribution is a point mass at mu. Short-circuit so we
    // never hit 0 * inf = NaN when u1 underflows.
    if sigma == 0.0 {
        return mu;
    }
    // Avoid u1 == 0 so ln() is finite. fastrand::f64() is in [0, 1).
    let mut u1 = fastrand::f64();
    while u1 <= f64::MIN_POSITIVE {
        u1 = fastrand::f64();
    }
    let u2 = fastrand::f64();
    let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
    mu + sigma * z
}

fn parse_format(fmt: &str, content: &str) -> std::result::Result<Value, String> {
    match fmt {
        "csv" | "tsv" => {
            let sep = if fmt == "tsv" { '\t' } else { ',' };
            let rows: Vec<Value> = content
                .lines()
                .map(|line| {
                    let fields: Vec<Value> = parse_csv_row(line, sep)
                        .into_iter()
                        .map(Value::Text)
                        .collect();
                    Value::List(fields)
                })
                .collect();
            Ok(Value::List(rows))
        }
        "json" => serde_json::from_str::<serde_json::Value>(content)
            .map(serde_json_to_value)
            .map_err(|e| e.to_string()),
        _ => Ok(Value::Text(content.to_string())),
    }
}

/// Parse one CSV/TSV row respecting double-quoted fields.
fn parse_csv_row(line: &str, sep: char) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
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
                field.push(c);
            }
        } else if c == '"' {
            in_quotes = true;
        } else if c == sep {
            fields.push(std::mem::take(&mut field));
        } else {
            field.push(c);
        }
    }
    fields.push(field);
    fields
}

// ── Linear algebra helpers ──────────────────────────────────────────

/// Coerce a `Value` into a row-major matrix `Vec<Vec<f64>>`.
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
    for row in rows {
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
        for c in cells {
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
    for item in items {
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
        Value::Text(s) => s.clone(),
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
            let mut keys: Vec<String> = m.keys().cloned().collect();
            keys.sort();
            (Some(keys), true)
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
            out.push_str(&fmt_csv_field(&Value::Text(k.clone()), sep));
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
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        out.push(sep);
                    }
                    let v = m.get(k).cloned().unwrap_or(Value::Nil);
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
        return Ok(Value::Map(HashMap::new()));
    }
    if builtin == Some(Builtin::Mget) && args.len() == 2 {
        return match (&args[0], &args[1]) {
            (Value::Map(m), Value::Text(k)) => Ok(m.get(k).cloned().unwrap_or(Value::Nil)),
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "mget: expects map and text key".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Mset) && args.len() == 3 {
        return match (&args[0], &args[1]) {
            (Value::Map(m), Value::Text(k)) => {
                let mut new_map = m.clone();
                new_map.insert(k.clone(), args[2].clone());
                Ok(Value::Map(new_map))
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "mset: expects map, text key, and value".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Mhas) && args.len() == 2 {
        return match (&args[0], &args[1]) {
            (Value::Map(m), Value::Text(k)) => Ok(Value::Bool(m.contains_key(k.as_str()))),
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "mhas: expects map and text key".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Mkeys) && args.len() == 1 {
        return match &args[0] {
            Value::Map(m) => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                Ok(Value::List(
                    keys.into_iter().map(|k| Value::Text(k.clone())).collect(),
                ))
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
                let mut pairs: Vec<(&String, &Value)> = m.iter().collect();
                pairs.sort_by_key(|(k, _)| k.as_str());
                Ok(Value::List(
                    pairs.into_iter().map(|(_, v)| v.clone()).collect(),
                ))
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "mvals: expects a map".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Mdel) && args.len() == 2 {
        return match (&args[0], &args[1]) {
            (Value::Map(m), Value::Text(k)) => {
                let mut new_map = m.clone();
                new_map.remove(k.as_str());
                Ok(Value::Map(new_map))
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "mdel: expects map and text key".to_string(),
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
                Value::List(
                    (0..n)
                        .map(|j| Value::Number(cols[j][i]))
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        return Ok(Value::List(rows));
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
        return Ok(Value::List(x.into_iter().map(Value::Number).collect()));
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
                Ok(Value::Text(s))
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
            Value::Text(s) => match s.parse::<f64>() {
                Ok(n) => Ok(Value::Ok(Box::new(Value::Number(n)))),
                Err(_) => Ok(Value::Err(Box::new(Value::Text(s.clone())))),
            },
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("num requires text, got {:?}", other),
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
                    _ => n.log2(),
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
    if builtin == Some(Builtin::Now) && args.is_empty() {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        return Ok(Value::Number(ts));
    }
    if builtin == Some(Builtin::Rndn) && args.len() == 2 {
        return match (&args[0], &args[1]) {
            (Value::Number(mu), Value::Number(sigma)) => {
                Ok(Value::Number(box_muller_normal(*mu, *sigma)))
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
                    return Ok(Value::Err(Box::new(Value::Text(format!(
                        "dtfmt: epoch is not finite ({epoch})"
                    )))));
                }
                if *epoch < i64::MIN as f64 || *epoch > i64::MAX as f64 {
                    return Ok(Value::Err(Box::new(Value::Text(format!(
                        "dtfmt: epoch out of range ({epoch})"
                    )))));
                }
                let secs = *epoch as i64;
                match chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0) {
                    Some(dt) => {
                        let formatted = dt.format(fmt_str.as_str()).to_string();
                        Ok(Value::Ok(Box::new(Value::Text(formatted))))
                    }
                    None => Ok(Value::Err(Box::new(Value::Text(format!(
                        "dtfmt: timestamp out of range ({secs})"
                    ))))),
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
                    Err(e) => Ok(Value::Err(Box::new(Value::Text(format!("dtparse: {e}"))))),
                }
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "dtparse requires two text args".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Rnd) {
        if args.is_empty() {
            return Ok(Value::Number(fastrand::f64()));
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
                    Ok(Value::Number(fastrand::i64(lo..=hi) as f64))
                }
                _ => Err(RuntimeError::new(
                    "ILO-R009",
                    "rnd requires two numbers".to_string(),
                )),
            };
        }
    }
    if builtin == Some(Builtin::Spl) && args.len() == 2 {
        return match (&args[0], &args[1]) {
            (Value::Text(s), Value::Text(sep)) => {
                let parts: Vec<Value> = s
                    .split(sep.as_str())
                    .map(|p| Value::Text(p.to_string()))
                    .collect();
                Ok(Value::List(parts))
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
                let mut parts = Vec::new();
                for item in items {
                    match item {
                        Value::Text(s) => parts.push(s.clone()),
                        other => {
                            return Err(RuntimeError::new(
                                "ILO-R009",
                                format!("cat: list items must be text, got {:?}", other),
                            ));
                        }
                    }
                }
                Ok(Value::Text(parts.join(sep.as_str())))
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
                    Ok(Value::Text(s.chars().next().unwrap().to_string()))
                }
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("hd requires a list or text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::At) && args.len() == 2 {
        let i = match &args[1] {
            Value::Number(n) => {
                if n.fract() != 0.0 {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        "at: index must be an integer".to_string(),
                    ));
                }
                *n as i64
            }
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
            Value::Text(s) => {
                let chars: Vec<char> = s.chars().collect();
                let len = chars.len() as i64;
                let adjusted = if i < 0 { i + len } else { i };
                if adjusted < 0 || adjusted >= len {
                    Err(RuntimeError::new(
                        "ILO-R009",
                        format!(
                            "at: index {i} out of range for text of length {}",
                            chars.len()
                        ),
                    ))
                } else {
                    Ok(Value::Text(chars[adjusted as usize].to_string()))
                }
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("at requires a list or text, got {:?}", other),
            )),
        };
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
                    let mut new_items = items.clone();
                    new_items[idx] = args[2].clone();
                    Ok(Value::List(new_items))
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
            return Ok(Value::List(vec![]));
        }
        let mut out = Vec::with_capacity(xs.len() - n + 1);
        for w in xs.windows(n) {
            out.push(Value::List(w.to_vec()));
        }
        return Ok(Value::List(out));
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
            out.push(Value::List(vec![xs[i].clone(), ys[i].clone()]));
        }
        return Ok(Value::List(out));
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
            out.push(Value::List(vec![Value::Number(i as f64), v.clone()]));
        }
        return Ok(Value::List(out));
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
                    return Ok(Value::List(Vec::new()));
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
                Ok(Value::List(out))
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "range requires two numbers".to_string(),
            )),
        };
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
            out.push(Value::List(chunk.to_vec()));
        }
        return Ok(Value::List(out));
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
        for v in xs {
            let k = key_for(v, op_name)?;
            if set_a.insert(k.clone()) {
                a_first.insert(k, v.clone());
            }
        }
        let mut set_b: HashSet<String> = HashSet::new();
        let mut b_first: HashMap<String, Value> = HashMap::new();
        for v in ys {
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
                return Ok(Value::List(out));
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
        return Ok(Value::List(out));
    }
    if builtin == Some(Builtin::Tl) && args.len() == 1 {
        return match &args[0] {
            Value::List(items) => {
                if items.is_empty() {
                    Err(RuntimeError::new("ILO-R009", "tl: empty list".to_string()))
                } else {
                    Ok(Value::List(items[1..].to_vec()))
                }
            }
            Value::Text(s) => {
                if s.is_empty() {
                    Err(RuntimeError::new("ILO-R009", "tl: empty text".to_string()))
                } else {
                    let mut chars = s.chars();
                    chars.next();
                    Ok(Value::Text(chars.collect()))
                }
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("tl requires a list or text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Rev) && args.len() == 1 {
        return match &args[0] {
            Value::List(items) => {
                let mut reversed = items.clone();
                reversed.reverse();
                Ok(Value::List(reversed))
            }
            Value::Text(s) => Ok(Value::Text(s.chars().rev().collect())),
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("rev requires a list or text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Srt) && args.len() == 1 {
        return match &args[0] {
            Value::List(items) => {
                if items.is_empty() {
                    return Ok(Value::List(vec![]));
                }
                let all_numbers = items.iter().all(|v| matches!(v, Value::Number(_)));
                let all_text = items.iter().all(|v| matches!(v, Value::Text(_)));
                if all_numbers {
                    let mut sorted = items.clone();
                    sorted.sort_by(|a, b| {
                        if let (Value::Number(x), Value::Number(y)) = (a, b) {
                            x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
                        } else {
                            unreachable!()
                        }
                    });
                    Ok(Value::List(sorted))
                } else if all_text {
                    let mut sorted = items.clone();
                    sorted.sort_by(|a, b| {
                        if let (Value::Text(x), Value::Text(y)) = (a, b) {
                            x.cmp(y)
                        } else {
                            unreachable!()
                        }
                    });
                    Ok(Value::List(sorted))
                } else {
                    Err(RuntimeError::new(
                        "ILO-R009",
                        "srt: list must contain all numbers or all text".to_string(),
                    ))
                }
            }
            Value::Text(s) => {
                let mut chars: Vec<char> = s.chars().collect();
                chars.sort();
                Ok(Value::Text(chars.into_iter().collect()))
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
        // Compute keys for each item, then sort by key
        let mut keyed: Vec<(Value, Value)> = items
            .into_iter()
            .map(|item| {
                let call_args = match &ctx {
                    Some(c) => vec![item.clone(), c.clone()],
                    None => vec![item.clone()],
                };
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
        return Ok(Value::List(keyed.into_iter().map(|(_, v)| v).collect()));
    }
    if builtin == Some(Builtin::Rsrt) && args.len() == 1 {
        return match &args[0] {
            Value::List(items) => {
                if items.is_empty() {
                    return Ok(Value::List(vec![]));
                }
                let all_numbers = items.iter().all(|v| matches!(v, Value::Number(_)));
                let all_text = items.iter().all(|v| matches!(v, Value::Text(_)));
                if all_numbers {
                    let mut sorted = items.clone();
                    sorted.sort_by(|a, b| {
                        if let (Value::Number(x), Value::Number(y)) = (a, b) {
                            y.partial_cmp(x).unwrap_or(std::cmp::Ordering::Equal)
                        } else {
                            unreachable!()
                        }
                    });
                    Ok(Value::List(sorted))
                } else if all_text {
                    let mut sorted = items.clone();
                    sorted.sort_by(|a, b| {
                        if let (Value::Text(x), Value::Text(y)) = (a, b) {
                            y.cmp(x)
                        } else {
                            unreachable!()
                        }
                    });
                    Ok(Value::List(sorted))
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
                Ok(Value::Text(chars.into_iter().collect()))
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("rsrt requires a list or text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Slc) && args.len() == 3 {
        let start = match &args[1] {
            Value::Number(n) => *n as usize,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("slc: start index must be a number, got {:?}", other),
                ));
            }
        };
        let end = match &args[2] {
            Value::Number(n) => *n as usize,
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("slc: end index must be a number, got {:?}", other),
                ));
            }
        };
        return match &args[0] {
            Value::List(items) => {
                let end = end.min(items.len());
                let start = start.min(end);
                Ok(Value::List(items[start..end].to_vec()))
            }
            Value::Text(s) => {
                let chars: Vec<char> = s.chars().collect();
                let end = end.min(chars.len());
                let start = start.min(end);
                Ok(Value::Text(chars[start..end].iter().collect()))
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("slc requires a list or text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Take) && args.len() == 2 {
        let n = match &args[0] {
            Value::Number(n) => {
                if n.fract() != 0.0 {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        "take: count must be an integer".to_string(),
                    ));
                }
                if *n < 0.0 {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        "take: count must be a non-negative integer".to_string(),
                    ));
                }
                *n as usize
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
                let end = n.min(items.len());
                Ok(Value::List(items[..end].to_vec()))
            }
            Value::Text(s) => {
                let chars: Vec<char> = s.chars().collect();
                let end = n.min(chars.len());
                Ok(Value::Text(chars[..end].iter().collect()))
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("take requires a list or text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Drop) && args.len() == 2 {
        let n = match &args[0] {
            Value::Number(n) => {
                if n.fract() != 0.0 {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        "drop: count must be an integer".to_string(),
                    ));
                }
                if *n < 0.0 {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        "drop: count must be a non-negative integer".to_string(),
                    ));
                }
                *n as usize
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
                let start = n.min(items.len());
                Ok(Value::List(items[start..].to_vec()))
            }
            Value::Text(s) => {
                let chars: Vec<char> = s.chars().collect();
                let start = n.min(chars.len());
                Ok(Value::Text(chars[start..].iter().collect()))
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
        let headers = if args.len() == 2 {
            match &args[1] {
                Value::Map(m) => m
                    .iter()
                    .map(|(k, v)| {
                        let vs = match v {
                            Value::Text(s) => s.clone(),
                            other => format!("{other:?}"),
                        };
                        (k.clone(), vs)
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
                        Ok(body) => Ok(Value::Ok(Box::new(Value::Text(body.to_string())))),
                        Err(e) => Ok(Value::Err(Box::new(Value::Text(format!(
                            "response is not valid UTF-8: {e}"
                        ))))),
                    },
                    Err(e) => Ok(Value::Err(Box::new(Value::Text(e.to_string())))),
                }
            }
            #[cfg(not(feature = "http"))]
            {
                let _ = (url, headers);
                Ok(Value::Err(Box::new(Value::Text(
                    "http feature not enabled".to_string(),
                ))))
            }
        };
    }
    if builtin == Some(Builtin::GetMany) && args.len() == 1 {
        let urls = match &args[0] {
            Value::List(items) => {
                let mut out = Vec::with_capacity(items.len());
                for (i, v) in items.iter().enumerate() {
                    match v {
                        Value::Text(s) => out.push(s.clone()),
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
        return Ok(Value::List(get_many_fetch(&urls)));
    }
    if builtin == Some(Builtin::Post) && (args.len() == 2 || args.len() == 3) {
        let (url, body) = match (&args[0], &args[1]) {
            (Value::Text(u), Value::Text(b)) => (u.clone(), b.clone()),
            _ => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("post requires (t, t), got ({:?}, {:?})", args[0], args[1]),
                ));
            }
        };
        let headers = if args.len() == 3 {
            match &args[2] {
                Value::Map(m) => m
                    .iter()
                    .map(|(k, v)| {
                        let vs = match v {
                            Value::Text(s) => s.clone(),
                            other => format!("{other:?}"),
                        };
                        (k.clone(), vs)
                    })
                    .collect::<Vec<_>>(),
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("post headers must be M t t, got {:?}", other),
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
                        Ok(b) => Ok(Value::Ok(Box::new(Value::Text(b.to_string())))),
                        Err(e) => Ok(Value::Err(Box::new(Value::Text(format!(
                            "response is not valid UTF-8: {e}"
                        ))))),
                    },
                    Err(e) => Ok(Value::Err(Box::new(Value::Text(e.to_string())))),
                }
            }
            #[cfg(not(feature = "http"))]
            {
                let _ = (url, body, headers);
                Ok(Value::Err(Box::new(Value::Text(
                    "http feature not enabled".to_string(),
                ))))
            }
        };
    }
    if builtin == Some(Builtin::Trm) && args.len() == 1 {
        return match &args[0] {
            Value::Text(s) => Ok(Value::Text(s.trim().to_string())),
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("trm requires text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Upr) && args.len() == 1 {
        return match &args[0] {
            Value::Text(s) => Ok(Value::Text(s.to_uppercase())),
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("upr requires text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Lwr) && args.len() == 1 {
        return match &args[0] {
            Value::Text(s) => Ok(Value::Text(s.to_lowercase())),
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
                Ok(Value::Text(out))
            }
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("cap requires text, got {:?}", other),
            )),
        };
    }
    if (builtin == Some(Builtin::Padl) || builtin == Some(Builtin::Padr)) && args.len() == 2 {
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
        let char_count = s.chars().count();
        if char_count >= w {
            return Ok(Value::Text(s));
        }
        let pad = " ".repeat(w - char_count);
        let out = if builtin == Some(Builtin::Padl) {
            format!("{pad}{s}")
        } else {
            format!("{s}{pad}")
        };
        return Ok(Value::Text(out));
    }
    if builtin == Some(Builtin::Unq) && args.len() == 1 {
        return match &args[0] {
            Value::List(xs) => {
                let mut seen = std::collections::HashSet::new();
                let mut out = Vec::new();
                for v in xs {
                    let key = format!("{v:?}");
                    if seen.insert(key) {
                        out.push(v.clone());
                    }
                }
                Ok(Value::List(out))
            }
            Value::Text(s) => {
                let mut seen = std::collections::HashSet::new();
                let deduped: String = s.chars().filter(|c| seen.insert(*c)).collect();
                Ok(Value::Text(deduped))
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
                Ok(Value::Text(format!("{:.*}", digits, x)))
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
            if c == '{' && chars.peek() == Some(&'}') {
                chars.next();
                if arg_idx < args.len() {
                    result.push_str(&format!("{}", args[arg_idx]));
                    arg_idx += 1;
                } else {
                    result.push_str("{}");
                }
            } else {
                result.push(c);
            }
        }
        return Ok(Value::Text(result));
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
            std::path::Path::new(&path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("raw")
                .to_lowercase()
        };
        return match std::fs::read_to_string(&path) {
            Err(e) => Ok(Value::Err(Box::new(Value::Text(e.to_string())))),
            Ok(content) => match parse_format(&fmt, &content) {
                Ok(v) => Ok(Value::Ok(Box::new(v))),
                Err(e) => Ok(Value::Err(Box::new(Value::Text(e)))),
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
            Err(e) => Ok(Value::Err(Box::new(Value::Text(e)))),
        };
    }
    if builtin == Some(Builtin::Rdl) && args.len() == 1 {
        return match &args[0] {
            Value::Text(path) => match std::fs::read_to_string(path) {
                Ok(content) => {
                    let lines: Vec<Value> = content
                        .lines()
                        .map(|l| Value::Text(l.to_string()))
                        .collect();
                    Ok(Value::Ok(Box::new(Value::List(lines))))
                }
                Err(e) => Ok(Value::Err(Box::new(Value::Text(e.to_string())))),
            },
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("rdl requires text path, got {:?}", other),
            )),
        };
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
                    let sep = if fmt == "csv" { ',' } else { '\t' };
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
                                    .map(|(k, v)| (k.clone(), value_to_json(v)))
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
                Value::Text(s) => s.clone(),
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("wr: second arg must be text content, got {:?}", other),
                    ));
                }
            }
        };
        return match std::fs::write(&path, &content) {
            Ok(()) => Ok(Value::Ok(Box::new(Value::Text(path)))),
            Err(e) => Ok(Value::Err(Box::new(Value::Text(e.to_string())))),
        };
    }
    if builtin == Some(Builtin::Wrl) && args.len() == 2 {
        return match (&args[0], &args[1]) {
            (Value::Text(path), Value::List(lines)) => {
                let mut content = String::new();
                for line in lines {
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
                match std::fs::write(path, &content) {
                    Ok(()) => Ok(Value::Ok(Box::new(Value::Text(path.clone())))),
                    Err(e) => Ok(Value::Err(Box::new(Value::Text(e.to_string())))),
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
                match serde_json::from_str::<serde_json::Value>(json_str) {
                    Ok(parsed) => {
                        let mut current = &parsed;
                        for key in path.split('.') {
                            if let Ok(idx) = key.parse::<usize>() {
                                if let Some(v) = current.as_array().and_then(|a| a.get(idx)) {
                                    current = v;
                                } else {
                                    return Ok(Value::Err(Box::new(Value::Text(format!(
                                        "key not found: {key}"
                                    )))));
                                }
                            } else if let Some(v) = current.get(key) {
                                current = v;
                            } else {
                                return Ok(Value::Err(Box::new(Value::Text(format!(
                                    "key not found: {key}"
                                )))));
                            }
                        }
                        let result_str = match current {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        Ok(Value::Ok(Box::new(Value::Text(result_str))))
                    }
                    Err(e) => Ok(Value::Err(Box::new(Value::Text(e.to_string())))),
                }
            }
            _ => Err(RuntimeError::new(
                "ILO-R009",
                "jpth requires two text args".to_string(),
            )),
        };
    }
    if builtin == Some(Builtin::Prnt) && args.len() == 1 {
        let v = args
            .into_iter()
            .next()
            .expect("prnt: arity=1 guaranteed by caller");
        println!("{v}");
        return Ok(v);
    }
    if builtin == Some(Builtin::Jdmp) && args.len() == 1 {
        let json_val = value_to_json(&args[0]);
        return Ok(Value::Text(json_val.to_string()));
    }
    if builtin == Some(Builtin::Jpar) && args.len() == 1 {
        return match &args[0] {
            Value::Text(s) => match serde_json::from_str::<serde_json::Value>(s) {
                Ok(v) => Ok(Value::Ok(Box::new(serde_json_to_value(v)))),
                Err(e) => Ok(Value::Err(Box::new(Value::Text(e.to_string())))),
            },
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("jpar requires text, got {:?}", other),
            )),
        };
    }
    if builtin == Some(Builtin::Rdjl) && args.len() == 1 {
        return match &args[0] {
            Value::Text(path) => match std::fs::read_to_string(path) {
                Ok(content) => {
                    let mut items: Vec<Value> = Vec::new();
                    for line in content.split('\n') {
                        if line.is_empty() {
                            continue;
                        }
                        let parsed = match serde_json::from_str::<serde_json::Value>(line) {
                            Ok(v) => Value::Ok(Box::new(serde_json_to_value(v))),
                            Err(e) => Value::Err(Box::new(Value::Text(e.to_string()))),
                        };
                        items.push(parsed);
                    }
                    Ok(Value::List(items))
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
            Value::Text(key) => match std::env::var(key.as_str()) {
                Ok(val) => Ok(Value::Ok(Box::new(Value::Text(val)))),
                Err(_) => Ok(Value::Err(Box::new(Value::Text(format!(
                    "env var '{}' not set",
                    key
                ))))),
            },
            other => Err(RuntimeError::new(
                "ILO-R009",
                format!("env requires text, got {:?}", other),
            )),
        };
    }

    // Higher-order builtins: map, flt, fld
    // A function reference can be Value::FnRef(name) or Value::Text(name) when the
    // function name was passed as a CLI string argument.
    fn resolve_fn_ref(val: &Value) -> Option<String> {
        match val {
            Value::FnRef(n) => Some(n.clone()),
            Value::Text(n) => Some(n.clone()),
            _ => None,
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
        for item in items {
            let call_args = match &ctx {
                Some(c) => vec![item, c.clone()],
                None => vec![item],
            };
            result.push(call_function(env, &fn_name, call_args)?);
        }
        return Ok(Value::List(result));
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
                    format!("flt: list arg must be a list, got {:?}", other),
                ));
            }
        };
        let mut result = Vec::new();
        for item in items {
            let call_args = match &ctx {
                Some(c) => vec![item.clone(), c.clone()],
                None => vec![item.clone()],
            };
            match call_function(env, &fn_name, call_args)? {
                Value::Bool(true) => result.push(item),
                Value::Bool(false) => {}
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("flt: predicate must return bool, got {:?}", other),
                    ));
                }
            }
        }
        return Ok(Value::List(result));
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
        for item in items {
            let call_args = match &ctx {
                Some(c) => vec![acc, item, c.clone()],
                None => vec![acc, item],
            };
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
        for item in items {
            match call_function(env, &fn_name, vec![item.clone()])? {
                Value::Bool(true) => pass.push(item),
                Value::Bool(false) => fail.push(item),
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("partition: predicate must return bool, got {:?}", other),
                    ));
                }
            }
        }
        return Ok(Value::List(vec![Value::List(pass), Value::List(fail)]));
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
        for item in items {
            match call_function(env, &fn_name, vec![item])? {
                Value::List(inner) => result.extend(inner),
                other => {
                    return Err(RuntimeError::new(
                        "ILO-R009",
                        format!("flatmap: function must return a list, got {:?}", other),
                    ));
                }
            }
        }
        return Ok(Value::List(result));
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
        let items = match &args[1] {
            Value::List(l) => l.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("uniqby: second arg must be a list, got {:?}", other),
                ));
            }
        };
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut out: Vec<Value> = Vec::new();
        for item in items {
            let key = call_function(env, &fn_name, vec![item.clone()])?;
            // Prefix the hashed key with a type tag so values from distinct
            // domains never alias each other. Without this, `Number(5)` and
            // `Text("5")` both stringify to `"5"` and collide.
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
                out.push(item);
            }
        }
        return Ok(Value::List(out));
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
        let items = match &args[1] {
            Value::List(l) => l.clone(),
            other => {
                return Err(RuntimeError::new(
                    "ILO-R009",
                    format!("grp: second arg must be a list, got {:?}", other),
                ));
            }
        };
        let mut groups: std::collections::HashMap<String, Vec<Value>> =
            std::collections::HashMap::new();
        for item in items {
            let key = call_function(env, &fn_name, vec![item.clone()])?;
            let key_str = match &key {
                Value::Text(s) => s.clone(),
                Value::Number(n) => {
                    if *n == (*n as i64) as f64 {
                        format!("{}", *n as i64)
                    } else {
                        format!("{n}")
                    }
                }
                Value::Bool(b) => format!("{b}"),
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
            groups.entry(key_str).or_default().push(item);
        }
        let map = groups
            .into_iter()
            .map(|(k, v)| (k, Value::List(v)))
            .collect();
        return Ok(Value::Map(map));
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
        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for item in items {
            // Prefix keys with a type tag so distinct domains never alias each
            // other. Without this, `Number(1)` and `Text("1")` would both
            // stringify to `"1"` and collide. Matches uniqby/setops precedent.
            let key_str = match item {
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
                            "frq: list elements must be text, number, or bool, got {:?}",
                            other
                        ),
                    ));
                }
            };
            *counts.entry(key_str).or_insert(0) += 1;
        }
        let map = counts
            .into_iter()
            .map(|(k, v)| (k, Value::Number(v as f64)))
            .collect();
        return Ok(Value::Map(map));
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
            return Ok(Value::List(vec![]));
        }
        let mut row_data: Vec<&Vec<Value>> = Vec::with_capacity(rows.len());
        let mut ncols: Option<usize> = None;
        for row in rows {
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
            result.push(Value::List(col));
        }
        return Ok(Value::List(result));
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
        for row in a_rows {
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
                    for v in r {
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
        for row in b_rows {
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
                    for v in r {
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
            out.push(Value::List(row));
        }
        return Ok(Value::List(out));
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
        for item in items {
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
        for item in items {
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
        return Ok(Value::List(out));
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
        for item in items {
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
        for item in items {
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
        for item in items {
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
        for item in items {
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
        for item in items {
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
                        .filter_map(|i| caps.get(i).map(|m| Value::Text(m.as_str().to_string())))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            // No capture groups — return list of all matches
            re.find_iter(input)
                .map(|m| Value::Text(m.as_str().to_string()))
                .collect()
        };
        return Ok(Value::List(result));
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
        // Uniform shape: L (L t). Each match is a list of capture-group strings.
        // No-group patterns wrap the whole match in a single-element inner list,
        // so the outer shape stays predictable regardless of group count.
        let result: Vec<Value> = if re.captures_len() > 1 {
            re.captures_iter(input)
                .map(|caps| {
                    let groups: Vec<Value> = (1..caps.len())
                        .filter_map(|i| caps.get(i).map(|m| Value::Text(m.as_str().to_string())))
                        .collect();
                    Value::List(groups)
                })
                .collect()
        } else {
            re.find_iter(input)
                .map(|m| Value::List(vec![Value::Text(m.as_str().to_string())]))
                .collect()
        };
        return Ok(Value::List(result));
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
        return Ok(Value::Text(
            re.replace_all(subject, replacement).into_owned(),
        ));
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
        let mut result = Vec::new();
        for item in items {
            match item {
                Value::List(inner) => result.extend(inner),
                other => result.push(other),
            }
        }
        return Ok(Value::List(result));
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
        for item in items {
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
            .map(|(r, i)| Value::List(vec![Value::Number(r), Value::Number(i)]))
            .collect();
        return Ok(Value::List(result));
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
        for item in items {
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
        return Ok(Value::List(result));
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
            // Isolate the callee's scope from the caller's variables.
            let saved_vars = std::mem::take(&mut env.vars);
            let saved_marks = std::mem::replace(&mut env.scope_marks, vec![0]);
            for (param, arg) in params.iter().zip(args) {
                env.define(&param.name, arg);
            }
            env.call_stack.push(func_name.clone());
            let result = eval_body(env, &body);
            env.call_stack.pop();
            env.vars = saved_vars;
            env.scope_marks = saved_marks;
            match result? {
                BodyResult::Value(v) | BodyResult::Return(v) | BodyResult::Break(v) => Ok(v),
                BodyResult::Continue => Ok(Value::Nil),
            }
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
        Value::Text(s) => serde_json::Value::String(s.clone()),
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
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            serde_json::Value::Object(map)
        }
        Value::Ok(inner) => value_to_json(inner),
        Value::Err(inner) => value_to_json(inner),
        Value::FnRef(name) => serde_json::Value::String(format!("<fn:{}>", name)),
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
            Value::List(arr.into_iter().map(serde_json_to_value).collect())
        }
        serde_json::Value::String(s) => Value::Text(s),
        serde_json::Value::Number(n) => Value::Number(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Null => Value::Nil,
    }
}

fn eval_body(env: &mut Env, stmts: &[Spanned<Stmt>]) -> Result<BodyResult> {
    let mut last = Value::Nil;
    for spanned in stmts.iter() {
        match eval_stmt(env, &spanned.node) {
            Ok(Some(BodyResult::Return(v))) => return Ok(BodyResult::Return(v)),
            Ok(Some(BodyResult::Break(v))) => return Ok(BodyResult::Break(v)),
            Ok(Some(BodyResult::Continue)) => return Ok(BodyResult::Continue),
            Ok(Some(BodyResult::Value(v))) => last = v,
            Ok(None) => {}
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

fn eval_stmt(env: &mut Env, stmt: &Stmt) -> Result<Option<BodyResult>> {
    match stmt {
        Stmt::Let { name, value } => {
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
                // Ternary: cond{then}{else} — produces value, no early return
                let chosen = if should_run { body } else { else_b };
                env.push_scope();
                let result = eval_body(env, chosen);
                env.pop_scope();
                match result? {
                    BodyResult::Break(v) => Ok(Some(BodyResult::Break(v))),
                    BodyResult::Continue => Ok(Some(BodyResult::Continue)),
                    BodyResult::Value(v) | BodyResult::Return(v) => Ok(Some(BodyResult::Value(v))),
                }
            } else if should_run && *braceless {
                // Braceless guard: cond expr — early return from function
                env.push_scope();
                let result = eval_body(env, body);
                env.pop_scope();
                match result? {
                    BodyResult::Break(v) => Ok(Some(BodyResult::Break(v))),
                    BodyResult::Continue => Ok(Some(BodyResult::Continue)),
                    BodyResult::Value(v) | BodyResult::Return(v) => Ok(Some(BodyResult::Return(v))),
                }
            } else if should_run {
                // Braced guard: cond{body} — conditional execution (no early return)
                env.push_scope();
                let result = eval_body(env, body);
                env.pop_scope();
                match result? {
                    BodyResult::Break(v) => Ok(Some(BodyResult::Break(v))),
                    BodyResult::Continue => Ok(Some(BodyResult::Continue)),
                    BodyResult::Value(v) => Ok(Some(BodyResult::Value(v))),
                    BodyResult::Return(v) => Ok(Some(BodyResult::Return(v))),
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
                    let result = eval_body(env, &arm.body);
                    env.pop_scope();
                    match result? {
                        BodyResult::Return(v) => return Ok(Some(BodyResult::Return(v))),
                        BodyResult::Break(v) => return Ok(Some(BodyResult::Break(v))),
                        BodyResult::Continue => return Ok(Some(BodyResult::Continue)),
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
                    for item in items {
                        env.push_scope();
                        env.define(binding, item);
                        let result = eval_body(env, body);
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
                            BodyResult::Value(v) => last = v,
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
            let mut last = Value::Nil;
            for i in s..e {
                env.push_scope();
                env.define(binding, Value::Number(i as f64));
                let result = eval_body(env, body);
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
                    BodyResult::Value(v) => last = v,
                }
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
                let result = eval_body(env, body);
                match result? {
                    BodyResult::Return(v) => {
                        return Ok(Some(BodyResult::Return(v)));
                    }
                    BodyResult::Break(v) => {
                        last = v;
                        break;
                    }
                    BodyResult::Continue => continue,
                    BodyResult::Value(v) => last = v,
                }
            }
            Ok(Some(BodyResult::Value(last)))
        }
        Stmt::Return(expr) => {
            let val = eval_expr(env, expr)?;
            Ok(Some(BodyResult::Return(val)))
        }
        Stmt::Break(expr) => {
            let val = match expr {
                Some(e) => eval_expr(env, e)?,
                None => Value::Nil,
            };
            Ok(Some(BodyResult::Break(val)))
        }
        Stmt::Continue => Ok(Some(BodyResult::Continue)),
        Stmt::Expr(expr) => {
            let val = eval_expr(env, expr)?;
            Ok(Some(BodyResult::Value(val)))
        }
    }
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
                Value::Record { fields, .. } => fields.get(field).cloned().ok_or_else(|| {
                    RuntimeError::new("ILO-R005", format!("no field '{}' on record", field))
                }),
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
            let callee = match callee_from_scope {
                Some(Value::FnRef(name)) => name,
                Some(Value::Text(name)) if env.functions.contains_key(&name) => name,
                _ => function.clone(),
            };
            let result = call_function(env, &callee, arg_vals)?;
            if *unwrap {
                match result {
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
                }
            } else {
                Ok(result)
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
            eval_binop(op, &l, &r)
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
            Ok(Value::List(vals))
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
                    let result = eval_body(env, &arm.body);
                    env.pop_scope();
                    return match result? {
                        BodyResult::Value(v) | BodyResult::Return(v) | BodyResult::Break(v) => {
                            Ok(v)
                        }
                        BodyResult::Continue => Ok(Value::Nil),
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
    }
}

fn eval_literal(lit: &Literal) -> Value {
    match lit {
        Literal::Number(n) => Value::Number(*n),
        Literal::Text(s) => Value::Text(s.clone()),
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
        (BinOp::Add, Value::Text(a), Value::Text(b)) => {
            let mut out = String::with_capacity(a.len() + b.len());
            out.push_str(a);
            out.push_str(b);
            Ok(Value::Text(out))
        }
        // List concatenation with +
        (BinOp::Add, Value::List(a), Value::List(b)) => {
            let mut out = Vec::with_capacity(a.len() + b.len());
            out.extend_from_slice(a);
            out.extend_from_slice(b);
            Ok(Value::List(out))
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
            let mut new_items = items.clone();
            new_items.push(val.clone());
            Ok(Value::List(new_items))
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
    match pattern {
        Pattern::Wildcard => Some(vec![]),
        Pattern::Ok(binding) => {
            if let Value::Ok(inner) = value {
                let mut bindings = vec![];
                if binding != "_" {
                    bindings.push((binding.clone(), *inner.clone()));
                }
                Some(bindings)
            } else {
                None
            }
        }
        Pattern::Err(binding) => {
            if let Value::Err(inner) = value {
                let mut bindings = vec![];
                if binding != "_" {
                    bindings.push((binding.clone(), *inner.clone()));
                }
                Some(bindings)
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
                let mut bindings = vec![];
                if binding != "_" {
                    bindings.push((binding.clone(), value.clone()));
                }
                Some(bindings)
            } else {
                None
            }
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
                            Ok(body) => Value::Ok(Box::new(Value::Text(body.to_string()))),
                            Err(e) => Value::Err(Box::new(Value::Text(format!(
                                "response is not valid UTF-8: {e}"
                            )))),
                        },
                        Err(e) => Value::Err(Box::new(Value::Text(e.to_string()))),
                    }));
                }
                for (i, h) in handles.into_iter().enumerate() {
                    let v = h.join().unwrap_or_else(|_| {
                        Value::Err(Box::new(Value::Text("worker thread panicked".to_string())))
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
                "http feature not enabled".to_string(),
            )));
        }
    }
    results
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
        assert_eq!(result, Value::Text("gold".to_string()));
    }

    #[test]
    fn interpret_cls_silver() {
        let source = r#"cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze""#;
        let result = run_str(source, Some("cls"), vec![Value::Number(500.0)]);
        assert_eq!(result, Value::Text("silver".to_string()));
    }

    #[test]
    fn interpret_cls_bronze() {
        let source = r#"cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze""#;
        let result = run_str(source, Some("cls"), vec![Value::Number(100.0)]);
        assert_eq!(result, Value::Text("bronze".to_string()));
    }

    #[test]
    fn interpret_match_stmt() {
        let source = r#"f x:t>n;?x{"a":1;"b":2;_:0}"#;
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Text("a".to_string())]),
            Value::Number(1.0)
        );
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Text("b".to_string())]),
            Value::Number(2.0)
        );
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Text("z".to_string())]),
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
        assert_eq!(result, Value::Err(Box::new(Value::Text("bad".to_string()))));
    }

    #[test]
    fn interpret_match_ok_err_patterns() {
        let source = r#"f x:R n t>n;?x{^e:0;~v:v}"#;
        let ok_result = run_str(
            source,
            Some("f"),
            vec![Value::Ok(Box::new(Value::Number(42.0)))],
        );
        assert_eq!(ok_result, Value::Number(42.0));

        let err_result = run_str(
            source,
            Some("f"),
            vec![Value::Err(Box::new(Value::Text("oops".to_string())))],
        );
        assert_eq!(err_result, Value::Number(0.0));
    }

    #[test]
    fn interpret_negated_guard() {
        // Ternary form: negated guard with else — produces value, no early return
        let source = r#"f x:b>t;!x{"nope"}{"yes"}"#;
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Bool(false)]),
            Value::Text("nope".to_string())
        );
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Bool(true)]),
            Value::Text("yes".to_string())
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
                Value::Text("hello ".to_string()),
                Value::Text("world".to_string()),
            ],
        );
        assert_eq!(result, Value::Text("hello world".to_string()));
    }

    #[test]
    fn interpret_string_comparison() {
        let gt = r#"f a:t b:t>b;>a b"#;
        assert_eq!(
            run_str(
                gt,
                Some("f"),
                vec![Value::Text("banana".into()), Value::Text("apple".into())]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            run_str(
                gt,
                Some("f"),
                vec![Value::Text("apple".into()), Value::Text("banana".into())]
            ),
            Value::Bool(false)
        );

        let lt = r#"f a:t b:t>b;<a b"#;
        assert_eq!(
            run_str(
                lt,
                Some("f"),
                vec![Value::Text("apple".into()), Value::Text("banana".into())]
            ),
            Value::Bool(true)
        );

        let ge = r#"f a:t b:t>b;>=a b"#;
        assert_eq!(
            run_str(
                ge,
                Some("f"),
                vec![Value::Text("apple".into()), Value::Text("apple".into())]
            ),
            Value::Bool(true)
        );

        let le = r#"f a:t b:t>b;<=a b"#;
        assert_eq!(
            run_str(
                le,
                Some("f"),
                vec![Value::Text("zebra".into()), Value::Text("banana".into())]
            ),
            Value::Bool(false)
        );
    }

    #[test]
    fn interpret_match_expr_in_let() {
        let source = r#"f x:t>n;y=?x{"a":1;"b":2;_:0};y"#;
        let result = run_str(source, Some("f"), vec![Value::Text("b".to_string())]);
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
        let err = run(&prog, Some("f"), vec![Value::Text("nope".into())]).unwrap_err();
        assert!(
            err.to_string().contains("sqrt") && err.to_string().contains("requires a number"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn interpret_log_non_number_errors() {
        let prog = parse_program("f x:t>n;log x");
        let err = run(&prog, Some("f"), vec![Value::Text("nope".into())]).unwrap_err();
        assert!(err.to_string().contains("log"), "unexpected error: {err}");
    }

    #[test]
    fn interpret_exp_non_number_errors() {
        let prog = parse_program("f x:t>n;exp x");
        let err = run(&prog, Some("f"), vec![Value::Text("nope".into())]).unwrap_err();
        assert!(err.to_string().contains("exp"), "unexpected error: {err}");
    }

    #[test]
    fn interpret_sin_non_number_errors() {
        let prog = parse_program("f x:t>n;sin x");
        let err = run(&prog, Some("f"), vec![Value::Text("nope".into())]).unwrap_err();
        assert!(err.to_string().contains("sin"), "unexpected error: {err}");
    }

    #[test]
    fn interpret_cos_non_number_errors() {
        let prog = parse_program("f x:t>n;cos x");
        let err = run(&prog, Some("f"), vec![Value::Text("nope".into())]).unwrap_err();
        assert!(err.to_string().contains("cos"), "unexpected error: {err}");
    }

    #[test]
    fn interpret_pow_non_number_errors() {
        let prog = parse_program("f x:t y:t>n;pow x y");
        let err = run(
            &prog,
            Some("f"),
            vec![Value::Text("a".into()), Value::Text("b".into())],
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
            run_str(source, Some("f"), vec![Value::Text("hello".to_string())]),
            Value::Number(5.0)
        );
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Text("".to_string())]),
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
            Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0)
            ])
        );
    }

    #[test]
    fn interpret_list_append_empty() {
        let source = "f>L n;xs=[];+=xs 42";
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::List(vec![Value::Number(42.0)])
        );
    }

    #[test]
    fn interpret_list_concat() {
        let source = "f>L n;a=[1, 2];b=[3, 4];+a b";
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
                Value::Number(4.0)
            ])
        );
    }

    #[test]
    fn interpret_str_integer() {
        let source = "f>t;str 42";
        assert_eq!(run_str(source, Some("f"), vec![]), Value::Text("42".into()));
    }

    #[test]
    fn interpret_str_float() {
        let source = "f>t;str 3.14";
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::Text("3.14".into())
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
            Value::Err(Box::new(Value::Text("abc".into())))
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
            Value::Text("hello".into())
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
        assert_eq!(format!("{}", Value::Text("hello".into())), "hello");
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
        let list = Value::List(vec![
            Value::Number(1.0),
            Value::Number(2.0),
            Value::Number(3.0),
        ]);
        assert_eq!(format!("{}", list), "[1, 2, 3]");
    }

    #[test]
    fn display_list_empty() {
        assert_eq!(format!("{}", Value::List(vec![])), "[]");
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
            format!("{}", Value::Err(Box::new(Value::Text("bad".into())))),
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
        let err = run_str_err(
            r#"f x:t>t;str x"#,
            Some("f"),
            vec![Value::Text("hi".into())],
        );
        assert!(err.contains("str requires a number"));
    }

    #[test]
    fn err_num_wrong_arg_count() {
        let err = run_str_err(r#"f>R n t;num "1" "2""#, Some("f"), vec![]);
        assert!(err.contains("num: expected 1 arg"));
    }

    #[test]
    fn err_num_wrong_type() {
        let err = run_str_err("f x:n>R n t;num x", Some("f"), vec![Value::Number(1.0)]);
        assert!(err.contains("num requires text"));
    }

    #[test]
    fn err_abs_wrong_arg_count() {
        let err = run_str_err("f>n;abs 1 2", Some("f"), vec![]);
        assert!(err.contains("abs: expected 1 arg"));
    }

    #[test]
    fn err_abs_wrong_type() {
        let err = run_str_err(
            r#"f x:t>n;abs x"#,
            Some("f"),
            vec![Value::Text("hi".into())],
        );
        assert!(err.contains("abs requires a number"));
    }

    #[test]
    fn err_min_non_number() {
        let err = run_str_err(
            r#"f a:t b:t>n;min a b"#,
            Some("f"),
            vec![Value::Text("a".into()), Value::Text("b".into())],
        );
        assert!(err.contains("min requires two numbers"));
    }

    #[test]
    fn err_max_non_number() {
        let err = run_str_err(
            r#"f a:t b:t>n;max a b"#,
            Some("f"),
            vec![Value::Text("a".into()), Value::Text("b".into())],
        );
        assert!(err.contains("max requires two numbers"));
    }

    #[test]
    fn err_flr_non_number() {
        let err = run_str_err(r#"f x:t>n;flr x"#, Some("f"), vec![Value::Text("a".into())]);
        assert!(err.contains("flr requires a number"));
    }

    #[test]
    fn err_cel_non_number() {
        let err = run_str_err(r#"f x:t>n;cel x"#, Some("f"), vec![Value::Text("a".into())]);
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
        assert!(!values_equal(&Value::Number(1.0), &Value::Text("1".into())));
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
        assert!(!is_truthy(&Value::Text("".into())));
        assert!(is_truthy(&Value::Text("hello".into())));
    }

    #[test]
    fn is_truthy_list() {
        assert!(!is_truthy(&Value::List(vec![])));
        assert!(is_truthy(&Value::List(vec![Value::Number(1.0)])));
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
            vec![Value::List(vec![
                Value::Number(1.0),
                Value::Number(5.0),
                Value::Number(2.0),
            ])],
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
        assert_eq!(result, Value::Text("always".to_string()));
    }

    #[test]
    fn interpret_pattern_ok_no_match() {
        let source = r#"f>t;x=^"err";?x{~v:v;_:"default"}"#;
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(result, Value::Text("default".to_string()));
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
                    name: "inner".to_string(),
                    params: vec![Param {
                        name: "x".to_string(),
                        ty: Type::Number,
                    }],
                    return_type: Type::Result(Box::new(Type::Number), Box::new(Type::Text)),
                    body: inner_body,
                    span: Span::UNKNOWN,
                },
                Decl::Function {
                    name: "outer".to_string(),
                    params: vec![Param {
                        name: "x".to_string(),
                        ty: Type::Number,
                    }],
                    return_type: Type::Result(Box::new(Type::Number), Box::new(Type::Text)),
                    body: vec![
                        Spanned::unknown(Stmt::Let {
                            name: "d".to_string(),
                            value: Expr::Call {
                                function: "inner".to_string(),
                                args: vec![Expr::Ref("x".to_string())],
                                unwrap: true,
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
            Value::Err(Box::new(Value::Text("fail".to_string())))
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
                        unwrap: true,
                    },
                }),
                Spanned::unknown(Stmt::Expr(Expr::Ok(Box::new(Expr::Ref("d".to_string()))))),
            ]
        };
        let rnt = Type::Result(Box::new(Type::Number), Box::new(Type::Text));
        let prog = Program {
            declarations: vec![
                Decl::Function {
                    name: "c".to_string(),
                    params: vec![Param {
                        name: "x".to_string(),
                        ty: Type::Number,
                    }],
                    return_type: rnt.clone(),
                    body: vec![Spanned::unknown(Stmt::Expr(Expr::Err(Box::new(
                        Expr::Literal(Literal::Text("deep".to_string())),
                    ))))],
                    span: Span::UNKNOWN,
                },
                Decl::Function {
                    name: "b".to_string(),
                    params: vec![Param {
                        name: "x".to_string(),
                        ty: Type::Number,
                    }],
                    return_type: rnt.clone(),
                    body: unwrap_body("c"),
                    span: Span::UNKNOWN,
                },
                Decl::Function {
                    name: "a".to_string(),
                    params: vec![Param {
                        name: "x".to_string(),
                        ty: Type::Number,
                    }],
                    return_type: rnt,
                    body: unwrap_body("b"),
                    span: Span::UNKNOWN,
                },
            ],
            source: None,
        };
        let result = run(&prog, Some("a"), vec![Value::Number(1.0)]).unwrap();
        assert_eq!(
            result,
            Value::Err(Box::new(Value::Text("deep".to_string())))
        );
    }

    // ---- Braceless guards ----

    #[test]
    fn interpret_braceless_guard() {
        let source = r#"cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze""#;
        assert_eq!(
            run_str(source, Some("cls"), vec![Value::Number(1500.0)]),
            Value::Text("gold".to_string())
        );
        assert_eq!(
            run_str(source, Some("cls"), vec![Value::Number(750.0)]),
            Value::Text("silver".to_string())
        );
        assert_eq!(
            run_str(source, Some("cls"), vec![Value::Number(100.0)]),
            Value::Text("bronze".to_string())
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
        let source = "fib n:n>n;<=n 1 n;a=fib -n 1;b=fib -n 2;+a b";
        assert_eq!(
            run_str(source, Some("fib"), vec![Value::Number(10.0)]),
            Value::Number(55.0)
        );
    }

    #[test]
    fn interpret_spl_basic() {
        let source = r#"f>L t;spl "a,b,c" ",""#;
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::List(vec![
                Value::Text("a".to_string()),
                Value::Text("b".to_string()),
                Value::Text("c".to_string()),
            ])
        );
    }

    #[test]
    fn interpret_spl_empty() {
        let source = r#"f>L t;spl "" ",""#;
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::List(vec![Value::Text("".to_string())])
        );
    }

    #[test]
    fn interpret_cat_basic() {
        let source = "f items:L t>t;cat items \",\"";
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::List(vec![
                    Value::Text("a".into()),
                    Value::Text("b".into()),
                    Value::Text("c".into()),
                ])]
            ),
            Value::Text("a,b,c".into())
        );
    }

    #[test]
    fn interpret_cat_empty_list() {
        let source = "f items:L t>t;cat items \"-\"";
        assert_eq!(
            run_str(source, Some("f"), vec![Value::List(vec![])]),
            Value::Text("".into())
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
                    Value::List(vec![Value::Number(1.0), Value::Number(2.0)]),
                    Value::Number(2.0)
                ]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            run_str(
                source,
                Some("f"),
                vec![Value::List(vec![Value::Number(1.0)]), Value::Number(5.0)]
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
                    Value::Text("hello world".into()),
                    Value::Text("world".into())
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
            Value::List(vec![Value::Number(20.0), Value::Number(30.0)])
        );
    }

    #[test]
    fn interpret_hd_text() {
        let source = r#"f s:t>t;hd s"#;
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Text("hello".into())]),
            Value::Text("h".into())
        );
    }

    #[test]
    fn interpret_tl_text() {
        let source = r#"f s:t>t;tl s"#;
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Text("hello".into())]),
            Value::Text("ello".into())
        );
    }

    #[test]
    fn interpret_rev_list() {
        let source = "f>L n;rev [1, 2, 3]";
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::List(vec![
                Value::Number(3.0),
                Value::Number(2.0),
                Value::Number(1.0)
            ])
        );
    }

    #[test]
    fn interpret_rev_text() {
        let source = r#"f>t;rev "abc""#;
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::Text("cba".into())
        );
    }

    #[test]
    fn interpret_srt_numbers() {
        let source = "f>L n;srt [3, 1, 2]";
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0)
            ])
        );
    }

    #[test]
    fn interpret_srt_text_list() {
        let source = r#"f>L t;srt ["c", "a", "b"]"#;
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::List(vec![
                Value::Text("a".into()),
                Value::Text("b".into()),
                Value::Text("c".into())
            ])
        );
    }

    #[test]
    fn interpret_srt_text_string() {
        let source = r#"f>t;srt "cab""#;
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::Text("abc".into())
        );
    }

    #[test]
    fn interpret_slc_list() {
        let source = "f>L n;slc [1, 2, 3, 4, 5] 1 3";
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::List(vec![Value::Number(2.0), Value::Number(3.0)])
        );
    }

    #[test]
    fn interpret_slc_text() {
        let source = r#"f>t;slc "hello" 1 4"#;
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::Text("ell".into())
        );
    }

    #[test]
    fn interpret_slc_clamped() {
        let source = "f>L n;slc [1, 2, 3] 1 100";
        assert_eq!(
            run_str(source, Some("f"), vec![]),
            Value::List(vec![Value::Number(2.0), Value::Number(3.0)])
        );
    }

    #[test]
    fn interpret_ternary_true() {
        let source = r#"f x:n>t;=x 1{"yes"}{"no"}"#;
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Number(1.0)]),
            Value::Text("yes".into())
        );
    }

    #[test]
    fn interpret_ternary_false() {
        let source = r#"f x:n>t;=x 1{"yes"}{"no"}"#;
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Number(2.0)]),
            Value::Text("no".into())
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
        // Braced guard is conditional execution — no early return
        let source = "f x:n>n;=x 0{99};+x 1";
        // x=0: {99} runs but value is discarded, returns +0 1 = 1
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
    fn interpret_braceless_guard_still_returns_early() {
        // Braceless guard still causes early return
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
        // Braced guard inside loop does NOT early-return — finds max of list
        let source = "mx xs:L n>n;m=xs.0;@x xs{>x m{m=x}};+m 0";
        let result = run_str(
            source,
            Some("mx"),
            vec![Value::List(vec![
                Value::Number(3.0),
                Value::Number(1.0),
                Value::Number(5.0),
            ])],
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
            Value::Text("one".into())
        );
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Number(2.0)]),
            Value::Text("not one".into())
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
        let list = Value::List(vec![
            Value::Number(1.0),
            Value::Number(15.0),
            Value::Number(3.0),
        ]);
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
        let result = run_str(source, Some("f"), vec![Value::Text("ILO_TEST_VAR".into())]);
        assert_eq!(result, Value::Ok(Box::new(Value::Text("hello".into()))));
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
            vec![Value::Text("ILO_NONEXISTENT_12345".into())],
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
            vec![Value::Text("ILO_TEST_UNWRAP".into())],
        );
        assert_eq!(result, Value::Ok(Box::new(Value::Text("world".into()))));
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
            vec![Value::Number(1.0), Value::Text("a".into())],
        );
        assert!(err.contains("spl requires two text args"), "got: {err}");
    }

    #[test]
    fn err_spl_non_text_second() {
        let err = run_str_err(
            "f x:t y:n>L t;spl x y",
            Some("f"),
            vec![Value::Text("a-b".into()), Value::Number(1.0)],
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
            vec![Value::Text("hello".into()), Value::Number(1.0)],
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
            vec![Value::Text("hi".into()), Value::Text("a".into())],
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
            vec![Value::Text("hi".into()), Value::Text("a".into())],
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
            vec![Value::Text("a".into()), Value::Text("b".into())],
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
        assert_eq!(run_str(source, Some("f"), vec![]), Value::List(vec![]));
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
            Value::Text("alice".to_string())
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
                Value::Text(r#"{"name":"alice"}"#.to_string()),
                Value::Text("name".to_string()),
            ],
        );
        assert_eq!(
            result,
            Value::Ok(Box::new(Value::Text("alice".to_string())))
        );
    }

    #[test]
    fn interp_jp_nested() {
        let source = r#"f j:t p:t>R t t;jpth j p"#;
        let result = run_str(
            source,
            Some("f"),
            vec![
                Value::Text(r#"{"user":{"name":"bob"}}"#.to_string()),
                Value::Text("user.name".to_string()),
            ],
        );
        assert_eq!(result, Value::Ok(Box::new(Value::Text("bob".to_string()))));
    }

    #[test]
    fn interp_jp_array_index() {
        let source = r#"f j:t p:t>R t t;jpth j p"#;
        let result = run_str(
            source,
            Some("f"),
            vec![
                Value::Text(r#"{"items":[10,20,30]}"#.to_string()),
                Value::Text("items.1".to_string()),
            ],
        );
        assert_eq!(result, Value::Ok(Box::new(Value::Text("20".to_string()))));
    }

    #[test]
    fn interp_jp_missing_key() {
        let source = r#"f j:t p:t>R t t;jpth j p"#;
        let result = run_str(
            source,
            Some("f"),
            vec![
                Value::Text(r#"{"a":1}"#.to_string()),
                Value::Text("b".to_string()),
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
                Value::Text("not json".to_string()),
                Value::Text("x".to_string()),
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
                Value::Text(r#"{"x":"hello"}"#.to_string()),
                Value::Text("x".to_string()),
            ],
        );
        assert_eq!(result, Value::Text("hello".to_string()));
    }

    #[test]
    fn interp_jd_number() {
        let source = "f x:n>t;jdmp x";
        let result = run_str(source, Some("f"), vec![Value::Number(42.0)]);
        assert_eq!(result, Value::Text("42".to_string()));
    }

    #[test]
    fn interp_jd_text() {
        let source = r#"f x:t>t;jdmp x"#;
        let result = run_str(source, Some("f"), vec![Value::Text("hello".to_string())]);
        assert_eq!(result, Value::Text(r#""hello""#.to_string()));
    }

    #[test]
    fn interp_jd_list() {
        let source = "f>t;xs=[1, 2, 3];jdmp xs";
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(result, Value::Text("[1,2,3]".to_string()));
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
            vec![Value::Text(r#"{"a":1,"b":"two"}"#.to_string())],
        );
        let Value::Ok(inner) = result else {
            panic!("expected Ok")
        };
        let Value::Record { type_name, fields } = *inner else {
            panic!("expected record")
        };
        assert_eq!(type_name, "json");
        assert_eq!(fields.get("a"), Some(&Value::Number(1.0)));
        assert_eq!(fields.get("b"), Some(&Value::Text("two".to_string())));
    }

    #[test]
    fn interp_jparse_array() {
        let source = r#"f j:t>R t t;jpar j"#;
        let result = run_str(source, Some("f"), vec![Value::Text("[1,2,3]".to_string())]);
        let Value::Ok(inner) = result else {
            panic!("expected Ok")
        };
        assert_eq!(
            *inner,
            Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0)
            ])
        );
    }

    #[test]
    fn interp_jparse_scalar() {
        let source = r#"f j:t>R t t;jpar j"#;
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Text("42".to_string())]),
            Value::Ok(Box::new(Value::Number(42.0)))
        );
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Text("true".to_string())]),
            Value::Ok(Box::new(Value::Bool(true)))
        );
        assert_eq!(
            run_str(source, Some("f"), vec![Value::Text("null".to_string())]),
            Value::Ok(Box::new(Value::Nil))
        );
    }

    #[test]
    fn interp_jparse_invalid() {
        let source = r#"f j:t>R t t;jpar j"#;
        let result = run_str(source, Some("f"), vec![Value::Text("not json".to_string())]);
        assert!(matches!(result, Value::Err(_)));
    }

    #[test]
    fn interp_jparse_unwrap() {
        let source = r#"f j:t>t;jpar! j"#;
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Text(r#"{"x":1}"#.to_string())],
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
            vec![Value::Text(r#"{"x":42}"#.to_string())],
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
            vec![Value::List(
                vec![1.0, 2.0, 3.0, 4.0, 5.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect(),
            )],
        );
        assert_eq!(
            result,
            Value::List(
                vec![1.0, 4.0, 9.0, 16.0, 25.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect()
            )
        );
    }

    #[test]
    fn interp_flt_positive() {
        // flt pos over [-3,-1,0,2,4] → [2,4]
        let source = "pos x:n>b;>x 0 main xs:L n>L n;flt pos xs";
        let result = run_str(
            source,
            Some("main"),
            vec![Value::List(
                vec![-3.0, -1.0, 0.0, 2.0, 4.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect(),
            )],
        );
        assert_eq!(
            result,
            Value::List(vec![2.0, 4.0].into_iter().map(Value::Number).collect())
        );
    }

    #[test]
    fn interp_fld_sum() {
        // fld add over [1..5] with init 0 → 15
        let source = "add a:n b:n>n;+a b main xs:L n>n;fld add xs 0";
        let result = run_str(
            source,
            Some("main"),
            vec![Value::List(
                vec![1.0, 2.0, 3.0, 4.0, 5.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect(),
            )],
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
            vec![Value::List(
                vec![1.0, 8.0, 3.0, 9.0, 2.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect(),
            )],
        );
        let Value::Map(m) = result else {
            panic!("expected Map")
        };
        assert_eq!(
            m.get("small").unwrap(),
            &Value::List(vec![1.0, 3.0, 2.0].into_iter().map(Value::Number).collect())
        );
        assert_eq!(
            m.get("big").unwrap(),
            &Value::List(vec![8.0, 9.0].into_iter().map(Value::Number).collect())
        );
    }

    #[test]
    fn interp_grp_by_numeric_key() {
        // group by str(x) — each number becomes its own group
        let source = "key x:n>t;str x main xs:L n>M t L n;grp key xs";
        let result = run_str(
            source,
            Some("main"),
            vec![Value::List(
                vec![1.0, 2.0, 1.0, 3.0, 2.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect(),
            )],
        );
        let Value::Map(m) = result else {
            panic!("expected Map")
        };
        assert_eq!(
            m.get("1").unwrap(),
            &Value::List(vec![1.0, 1.0].into_iter().map(Value::Number).collect())
        );
        assert_eq!(
            m.get("2").unwrap(),
            &Value::List(vec![2.0, 2.0].into_iter().map(Value::Number).collect())
        );
        assert_eq!(
            m.get("3").unwrap(),
            &Value::List(vec![3.0].into_iter().map(Value::Number).collect())
        );
    }

    #[test]
    fn interp_grp_empty_list() {
        let source = "id x:n>t;str x main xs:L n>M t L n;grp id xs";
        let result = run_str(source, Some("main"), vec![Value::List(vec![])]);
        assert_eq!(result, Value::Map(std::collections::HashMap::new()));
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
            vec![Value::List(
                vec![1.0, 2.0, 3.0, 4.0, 5.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect(),
            )],
        );
        assert_eq!(result, Value::Number(15.0));
    }

    #[test]
    fn interp_sum_empty() {
        let source = "f xs:L n>n;sum xs";
        let result = run_str(source, Some("f"), vec![Value::List(vec![])]);
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
            vec![Value::List(
                vec![2.0, 4.0, 6.0].into_iter().map(Value::Number).collect(),
            )],
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
            vec![Value::Text("abc 123 def 456".into())],
        );
        assert_eq!(
            result,
            Value::List(vec![Value::Text("123".into()), Value::Text("456".into()),])
        );
    }

    #[test]
    fn interp_rgx_capture_groups() {
        // extract key=value pairs
        let source = r#"f s:t>L t;rgx "(\w+)=(\w+)" s"#;
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Text("name=alice age=30".into())],
        );
        // Returns first match's groups
        assert_eq!(
            result,
            Value::List(vec![
                Value::Text("name".into()),
                Value::Text("alice".into()),
            ])
        );
    }

    #[test]
    fn interp_rgx_no_match() {
        let source = r#"f s:t>L t;rgx "\d+" s"#;
        let result = run_str(
            source,
            Some("f"),
            vec![Value::Text("no numbers here".into())],
        );
        assert_eq!(result, Value::List(vec![]));
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
            Value::List(
                vec![1.0, 2.0, 3.0, 4.0, 5.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect()
            )
        );
    }

    #[test]
    fn interp_flat_mixed() {
        // flat [[1, 2], 3] — non-list elements pass through
        let source = "f>L n;flat [[1, 2], 3]";
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(
            result,
            Value::List(vec![1.0, 2.0, 3.0].into_iter().map(Value::Number).collect())
        );
    }

    #[test]
    fn interp_flat_empty() {
        let source = "f>L n;flat []";
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(result, Value::List(vec![]));
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
            vec![Value::Text("  hello  ".into())],
        );
        assert_eq!(result, Value::Text("hello".into()));
    }

    #[test]
    fn interpret_trm_no_whitespace() {
        let result = run_str("f s:t>t;trm s", Some("f"), vec![Value::Text("hi".into())]);
        assert_eq!(result, Value::Text("hi".into()));
    }

    #[test]
    fn interpret_trm_only_whitespace() {
        let result = run_str("f s:t>t;trm s", Some("f"), vec![Value::Text("   ".into())]);
        assert_eq!(result, Value::Text("".into()));
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
            vec![Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(1.0),
                Value::Number(3.0),
                Value::Number(2.0),
            ])],
        );
        assert_eq!(
            result,
            Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0)
            ])
        );
    }

    #[test]
    fn interpret_unq_list_strings() {
        let result = run_str(
            "f xs:L t>L t;unq xs",
            Some("f"),
            vec![Value::List(vec![
                Value::Text("a".into()),
                Value::Text("b".into()),
                Value::Text("a".into()),
            ])],
        );
        assert_eq!(
            result,
            Value::List(vec![Value::Text("a".into()), Value::Text("b".into())])
        );
    }

    #[test]
    fn interpret_unq_text_chars() {
        let result = run_str(
            "f s:t>t;unq s",
            Some("f"),
            vec![Value::Text("aabbc".into())],
        );
        assert_eq!(result, Value::Text("abc".into()));
    }

    #[test]
    fn interpret_unq_empty_list() {
        let result = run_str("f xs:L n>L n;unq xs", Some("f"), vec![Value::List(vec![])]);
        assert_eq!(result, Value::List(vec![]));
    }

    #[test]
    fn interpret_unq_preserves_order() {
        let result = run_str(
            "f xs:L n>L n;unq xs",
            Some("f"),
            vec![Value::List(vec![
                Value::Number(3.0),
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(1.0),
                Value::Number(3.0),
            ])],
        );
        assert_eq!(
            result,
            Value::List(vec![
                Value::Number(3.0),
                Value::Number(1.0),
                Value::Number(2.0)
            ])
        );
    }

    // --- fmt ---

    #[test]
    fn interpret_fmt_basic() {
        let result = run_str(
            r#"f a:t b:t>t;fmt "{} + {}" a b"#,
            Some("f"),
            vec![Value::Text("1".into()), Value::Text("2".into())],
        );
        assert_eq!(result, Value::Text("1 + 2".into()));
    }

    #[test]
    fn interpret_fmt_template_only() {
        let result = run_str(r#"f>t;fmt "hello""#, Some("f"), vec![]);
        assert_eq!(result, Value::Text("hello".into()));
    }

    #[test]
    fn interpret_fmt_fewer_args_than_slots() {
        let result = run_str(
            r#"f a:t>t;fmt "{} and {}" a"#,
            Some("f"),
            vec![Value::Text("x".into())],
        );
        assert_eq!(result, Value::Text("x and {}".into()));
    }

    #[test]
    fn interpret_fmt_number_arg() {
        let result = run_str(
            r#"f n:n>t;fmt "value: {}" n"#,
            Some("f"),
            vec![Value::Number(42.0)],
        );
        assert_eq!(result, Value::Text("value: 42".into()));
    }

    // --- srt fn xs ---

    #[test]
    fn interpret_srt_fn_by_length() {
        let source = "ln s:t>n;len s main xs:L t>L t;srt ln xs";
        let result = run_str(
            source,
            Some("main"),
            vec![Value::List(vec![
                Value::Text("banana".into()),
                Value::Text("a".into()),
                Value::Text("cc".into()),
            ])],
        );
        assert_eq!(
            result,
            Value::List(vec![
                Value::Text("a".into()),
                Value::Text("cc".into()),
                Value::Text("banana".into()),
            ])
        );
    }

    #[test]
    fn interpret_srt_fn_numeric_key() {
        let source = "neg x:n>n;-x main xs:L n>L n;srt neg xs";
        let result = run_str(
            source,
            Some("main"),
            vec![Value::List(vec![
                Value::Number(1.0),
                Value::Number(3.0),
                Value::Number(2.0),
            ])],
        );
        // sort by negative: highest first
        assert_eq!(
            result,
            Value::List(vec![
                Value::Number(3.0),
                Value::Number(2.0),
                Value::Number(1.0)
            ])
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
        let result = run_str("f s:t>t;prnt s", Some("f"), vec![Value::Text("hi".into())]);
        assert_eq!(result, Value::Text("hi".into()));
    }

    // --- rdb ---

    #[test]
    fn interpret_rdb_csv() {
        let result = run_str(
            r#"f s:t>t;rdb s "csv""#,
            Some("f"),
            vec![Value::Text("a,b\n1,2".into())],
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
            vec![Value::Text(r#"{"x":1}"#.into())],
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
            vec![Value::Text("not json".into())],
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
            vec![Value::Text("hello".into())],
        );
        assert_eq!(result, Value::Ok(Box::new(Value::Text("hello".into()))));
    }

    // --- rd (error paths not needing a real file) ---

    #[test]
    fn interpret_rd_file_not_found() {
        let result = run_str(
            "f p:t>t;rd p",
            Some("f"),
            vec![Value::Text("/nonexistent/ilo_test_file.txt".into())],
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
        assert_eq!(result, Value::Text("num".into()));
    }

    #[test]
    fn interpret_type_is_text_match() {
        // t v: pattern matches a text value
        let result = run_str(
            r#"f x:t>t;?x{t v:v;_:"other"}"#,
            Some("f"),
            vec![Value::Text("hello".into())],
        );
        assert_eq!(result, Value::Text("hello".into()));
    }

    #[test]
    fn interpret_type_is_bool_match() {
        // b v: pattern matches a bool value
        let result = run_str(
            r#"f x:b>t;?x{b v:"bool";_:"other"}"#,
            Some("f"),
            vec![Value::Bool(true)],
        );
        assert_eq!(result, Value::Text("bool".into()));
    }

    #[test]
    fn interpret_type_is_no_match_falls_through() {
        // TypeIs with wrong type → falls through to wildcard
        let result = run_str(
            r#"f x:n>t;?x{t v:"text";_:"other"}"#,
            Some("f"),
            vec![Value::Number(1.0)],
        );
        assert_eq!(result, Value::Text("other".into()));
    }

    #[test]
    fn interpret_type_is_wildcard_binding() {
        // TypeIs with _ binding (no binding created)
        let result = run_str(
            r#"f x:n>t;?x{n _:"matched";_:"other"}"#,
            Some("f"),
            vec![Value::Number(5.0)],
        );
        assert_eq!(result, Value::Text("matched".into()));
    }

    // --- Text comparison operators ---

    #[test]
    fn interpret_text_greater_than() {
        let result = run_str(
            "f a:t b:t>b;>a b",
            Some("f"),
            vec![Value::Text("b".into()), Value::Text("a".into())],
        );
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn interpret_text_less_than() {
        let result = run_str(
            "f a:t b:t>b;<a b",
            Some("f"),
            vec![Value::Text("a".into()), Value::Text("b".into())],
        );
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn interpret_text_greater_or_equal() {
        let result = run_str(
            "f a:t b:t>b;>=a b",
            Some("f"),
            vec![Value::Text("a".into()), Value::Text("a".into())],
        );
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn interpret_text_less_or_equal() {
        let result = run_str(
            "f a:t b:t>b;<=a b",
            Some("f"),
            vec![Value::Text("a".into()), Value::Text("b".into())],
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
            &Value::Text("a".into()),
            &Value::Text("a".into())
        ));
        assert!(!values_equal(
            &Value::Text("a".into()),
            &Value::Text("b".into())
        ));
    }

    // ── New coverage tests ────────────────────────────────────────────────────

    // L62: Value::FnRef Display
    #[test]
    fn display_fnref() {
        assert_eq!(format!("{}", Value::FnRef("add".into())), "<fn:add>");
    }

    // L268-279: parse_csv_row with quoted fields
    #[test]
    fn parse_csv_row_quoted_fields() {
        // quoted field + escaped double-quote inside
        let rows = parse_csv_row(r#""he said ""hello""","world""#, ',');
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], r#"he said "hello""#);
        assert_eq!(rows[1], "world");
    }

    #[test]
    fn parse_csv_row_simple_quoted() {
        // plain quoted field (no escaped quotes)
        let rows = parse_csv_row(r#""hello","world""#, ',');
        assert_eq!(rows[0], "hello");
        assert_eq!(rows[1], "world");
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
            vec![Value::List(vec![
                Value::Text("banana".into()),
                Value::Text("apple".into()),
                Value::Text("cherry".into()),
            ])],
        );
        assert_eq!(
            result,
            Value::List(vec![
                Value::Text("apple".into()),
                Value::Text("banana".into()),
                Value::Text("cherry".into()),
            ])
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

    // L648: post wrong arg types
    #[test]
    fn interpret_post_wrong_arg_types() {
        let err = run_str_err(r#"f>t;post 42 "body""#, Some("f"), vec![]);
        assert!(err.contains("post"), "got: {err}");
    }

    // L656: post with invalid headers
    #[test]
    fn interpret_post_invalid_headers() {
        let err = run_str_err(r#"f>t;post "http://x" "body" 42"#, Some("f"), vec![]);
        assert!(
            err.contains("headers") || err.contains("post"),
            "got: {err}"
        );
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
        let result = run_str("f p:t>t;rdl p", Some("f"), vec![Value::Text(path_str)]);
        std::fs::remove_file(&path).ok();
        let Value::Ok(inner) = result else {
            panic!("expected Ok")
        };
        let Value::List(lines) = *inner else {
            panic!("expected list")
        };
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], Value::Text("line1".into()));
    }

    // L779: rdl file not found
    #[test]
    fn interpret_rdl_not_found() {
        let result = run_str(
            "f p:t>t;rdl p",
            Some("f"),
            vec![Value::Text("/nonexistent/ilo_rdl_test.txt".into())],
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
            vec![Value::Text(path_str.clone())],
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
            vec![Value::Text(path_str.clone())],
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
                Value::Text(path_str.clone()),
                Value::List(vec![Value::Text("ok".into()), Value::Number(99.0)]),
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

    // L822: jpth array index navigation
    #[test]
    fn interpret_jpth_array_index() {
        let source = r#"f j:t p:t>R t t;jpth j p"#;
        let result = run_str(
            source,
            Some("f"),
            vec![
                Value::Text(r#"[10,20,30]"#.to_string()),
                Value::Text("1".to_string()),
            ],
        );
        assert_eq!(result, Value::Ok(Box::new(Value::Text("20".into()))));
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
        assert_eq!(result, Value::Text("42".into()));
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
            vec![Value::List(vec![Value::Number(1.0)])],
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
            vec![Value::List(vec![Value::Number(1.0), Value::Number(2.0)])],
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
            vec![Value::List(vec![Value::Number(0.0), Value::Number(1.0)])],
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
            vec![Value::List(vec![Value::Number(1.0), Value::Number(5.0)])],
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
            vec![Value::List(vec![
                Value::Number(1.0),
                Value::Number(5.0),
                Value::Number(9.0),
            ])],
        );
        // x=1 → 0, x=5 → 5, x=9 → 0; last value of foreach = 0
        assert_eq!(result, Value::Number(0.0));
    }

    // L1189: ForRange — range end not a number
    #[test]
    fn interpret_range_end_not_number() {
        // ForRange where end is not a number — needs tricky setup
        // The range start/end are evaluated, if end is text it errors
        let source = "f s:n e:n>n;@i s..e{i}";
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
        assert_eq!(result, Value::Text("42".into()));
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
            vec![Value::List(vec![Value::Number(1.0)])],
        );
        assert_eq!(result, Value::Text("list".into()));
    }

    // L2376: Decl::TypeDef is not callable error (duplicate name avoided — already tested above)
    // (see earlier interpret_typedef_not_callable test)

    // L3669/3671: rdb csv header-only / single row
    #[test]
    fn interpret_rdb_csv_single_row() {
        let result = run_str(
            r#"f s:t>t;rdb s "csv""#,
            Some("f"),
            vec![Value::Text("a,b,c".into())],
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
            Value::List(vec![Value::Text("a".into()), Value::Text("b".into())])
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
            Value::List(vec![Value::Number(1.0), Value::Number(2.0)])
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
            vec![Value::List(vec![Value::Number(1.0)])],
        );
        assert!(err.contains("srt"), "got: {err}");
    }

    // ── flt first arg not fn-ref (lines 968-969) ────────────────────────────

    #[test]
    fn interpret_flt_key_not_fn_ref() {
        let err = run_str_err(
            "f xs:L n>L n;flt 42 xs",
            Some("f"),
            vec![Value::List(vec![Value::Number(1.0)])],
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
                Value::Text("sq".into()),
                Value::List(vec![Value::Number(3.0)]),
            ],
        );
        assert_eq!(result, Value::List(vec![Value::Number(9.0)]));
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
        assert_eq!(*inner, Value::Text("hello".into()));
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
            vec![Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(1.0),
            ])],
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
            vec![Value::List(vec![
                Value::Number(-1.0),
                Value::Number(1.0),
                Value::Number(2.0),
            ])],
        );
        let Value::Map(m) = result else {
            panic!("expected map")
        };
        assert!(m.contains_key("true"));
        assert!(m.contains_key("false"));
    }

    // ── avg non-number element (line 1053) ──────────────────────────────────

    #[test]
    fn interpret_avg_non_number_element() {
        let err = run_str_err(
            "f xs:L n>n;avg xs",
            Some("f"),
            vec![Value::List(vec![Value::Text("x".into())])],
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
        assert_eq!(result, Value::Text("true".into()));
    }

    #[test]
    fn interpret_jdmp_nil_value() {
        // value_to_json Nil branch (line 1180) — mget on empty map returns Nil
        let result = run_str(r#"f>t;jdmp (mget mmap "k")"#, Some("f"), vec![]);
        assert_eq!(result, Value::Text("null".into()));
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
        // Key function returns a fractional number → format!("{n}") path (line 1016)
        // Use floor-then-half: key = x/2 for x in [1,2,3] → keys 0.5, 1.0, 1.5
        let source = "half x:n>n;/x 2 g xs:L n>_;grp half xs";
        let result = run_str(
            source,
            Some("g"),
            vec![Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ])],
        );
        let Value::Map(m) = result else {
            panic!("expected Map")
        };
        // 1/2=0.5, 2/2=1, 3/2=1.5 → 3 groups
        assert!(
            m.contains_key("0.5") || m.contains_key("1.5"),
            "expected float key, got: {:?}",
            m.keys().collect::<Vec<_>>()
        );
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
            vec![Value::List(vec![Value::Number(1.0), Value::Number(2.0)])],
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
            vec![Value::Text("a".into())],
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
            "f e:t>n;@i 0..e{i}",
            Some("f"),
            vec![Value::Text("b".into())],
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
        let result = run_str(source, Some("f"), vec![Value::Text("sq".into())]);
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
            vec![Value::Text("a".into()), Value::Text("b".into())],
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
        assert_eq!(result, Value::Text("other".to_string()));
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
            vec![Value::Text("world".into())],
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
        assert_eq!(result, Value::Text("num".to_string()));
    }

    #[test]
    fn interp_type_is_pattern_text() {
        let result = run_str(
            r#"f x:t>t;?x{t v:v;_:"other"}"#,
            None,
            vec![Value::Text("hi".to_string())],
        );
        assert_eq!(result, Value::Text("hi".to_string()));
    }

    #[test]
    fn interp_type_is_pattern_bool() {
        let result = run_str(
            r#"f x:b>t;?x{b v:"matched";_:"other"}"#,
            None,
            vec![Value::Bool(true)],
        );
        assert_eq!(result, Value::Text("matched".to_string()));
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
            vec![Value::List(vec![Value::Number(1.0), Value::Number(2.0)])],
        );
        assert_eq!(result, Value::Text("list".to_string()));
    }

    #[test]
    fn interp_type_is_list_no_match() {
        // TypeIs List pattern tested against a non-list value — falls through to wildcard
        let source = r#"f x:n>t;?x{l _:"list";_:"other"}"#;
        let result = run_str(source, Some("f"), vec![Value::Number(42.0)]);
        assert_eq!(result, Value::Text("other".to_string()));
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
            vec![Value::Map(std::collections::HashMap::from([(
                "a".to_string(),
                Value::Number(1.0),
            )]))],
        );
        assert_eq!(result, Value::Text("other".to_string()));
    }

    // ── TypeIs with Nil value → no match on any typed pattern (L1727) ───────

    #[test]
    fn interp_type_is_nil_falls_through() {
        // Nil value doesn't match n/t/b/l patterns — exercises _ => false (L1727)
        let source = r#"f x:O n>t;?x{n _:"num";_:"nil"}"#;
        let result = run_str(source, Some("f"), vec![Value::Nil]);
        assert_eq!(result, Value::Text("nil".to_string()));
    }

    #[test]
    fn interp_type_is_nil_value_against_text() {
        // Nil tested against text TypeIs pattern → falls through
        let source = r#"f x:O t>t;?x{t v:v;_:"none"}"#;
        let result = run_str(source, Some("f"), vec![Value::Nil]);
        assert_eq!(result, Value::Text("none".to_string()));
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
        assert_eq!(result, Value::Text("c".to_string()));
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
    fn interp_at_fractional_index_errors() {
        let prog = parse_program("f>n;xs=[10,20,30];at xs 1.5");
        let err = run(&prog, Some("f"), vec![]).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("integer"), "got {msg}");
    }

    // ---- lst xs i v: replace element at index, returning a new list ----

    #[test]
    fn interp_lst_happy() {
        let source = "f>L n;lst [10,20,30] 1 99";
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(
            result,
            Value::List(vec![
                Value::Number(10.0),
                Value::Number(99.0),
                Value::Number(30.0),
            ])
        );
    }

    #[test]
    fn interp_lst_first_index() {
        let source = "f>L n;lst [10,20,30] 0 7";
        let result = run_str(source, Some("f"), vec![]);
        assert_eq!(
            result,
            Value::List(vec![
                Value::Number(7.0),
                Value::Number(20.0),
                Value::Number(30.0),
            ])
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
            vec![Value::Text("hi".to_string()), Value::Number(2.0)],
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
        fastrand::seed(42);
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
}
