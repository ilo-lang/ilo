//! `ilo trace` — run a program and emit one JSON line per statement (default)
//! or per sub-expression (with `--depth expr`).
//!
//! Each line has the schema:
//! ```json
//! {"schemaVersion":1,"kind":"stmt","line":7,"stmt":"a = +x y","bindings":{"x":3,"y":4,"a":7},"result":7}
//! {"schemaVersion":1,"kind":"expr","line":7,"expr":"+x y","refs":["x","y"],"result":7}
//! ```
//!
//! Touch points: ILO-72 (tree-walker), ILO-343 (VM stmt path), ILO-344 (--depth, --watch).
//!
//! ## Engine selection
//!
//! Default (`--depth statement`): tries the VM path first via
//! `crate::vm::run_with_trace`, which fires one stmt event per `OP_STMT`
//! boundary. Falls back to the tree-walker if VM compile fails.
//!
//! `--depth expr`: routes through the tree-walker
//! `interpreter::run_with_trace_opts`, which installs both `TRACE_HOOK` and
//! `EXPR_TRACE_HOOK`. The VM does not currently emit per-expression events;
//! pinning expr-depth to the tree-walker is the simplest correct mapping.

use super::args::TraceArgs;
use super::args::TraceDepth;
use crate::ast;
use crate::interpreter::{ExprTraceEvent, TraceEvent, Value};
use crate::lexer;
use crate::parser;

/// Entry point for `ilo trace <file.ilo> [func] [args...]`.
/// Returns the process exit code (0 = success).
pub fn run(t: TraceArgs) -> i32 {
    trace_run(t)
}

#[inline(never)]
fn trace_run(t: TraceArgs) -> i32 {
    let source_arg = &t.source;

    // Read source from file.
    let source = match std::fs::read_to_string(source_arg) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ilo trace: cannot read '{}': {}", source_arg, e);
            return 1;
        }
    };

    // Lex.
    let tokens = match lexer::lex(&source) {
        Ok(ts) => ts,
        Err(e) => {
            eprintln!("ilo trace: lex error: [{}] {}", e.code, e.snippet);
            return 1;
        }
    };

    let token_spans: Vec<(lexer::Token, ast::Span)> = tokens
        .into_iter()
        .map(|(tok, r)| {
            (
                tok,
                ast::Span {
                    start: r.start,
                    end: r.end,
                },
            )
        })
        .collect();

    // Parse.
    let (mut program, parse_errors) = parser::parse(token_spans);
    for e in &parse_errors {
        eprintln!("ilo trace: parse error: {}", e.message);
    }
    if !parse_errors.is_empty() {
        return 1;
    }

    ast::resolve_aliases(&mut program);
    ast::desugar_dot_var_index(&mut program);
    program.source = Some(source.clone());

    // Resolve entry function and args.
    let func_name: Option<&str> = t.func.as_deref();

    // Convert string args to ilo Values (best-effort: try number, else text).
    let call_args: Vec<Value> = t
        .rest
        .iter()
        .map(|s| {
            if let Ok(n) = s.parse::<f64>() {
                Value::Number(n)
            } else {
                Value::Text(std::sync::Arc::new(s.clone()))
            }
        })
        .collect();

    // --watch filter for stmt events: only emit events whose bindings touch
    // one of the named vars. The expr-event filter checks `refs` instead.
    let watch_stmt = t.watch.clone();
    let emit_stmt = move |ev: TraceEvent| {
        if !watch_stmt.is_empty()
            && !ev
                .bindings
                .iter()
                .any(|(n, _)| watch_stmt.iter().any(|w| w == n))
        {
            return;
        }
        emit_stmt_event(ev);
    };

    match t.depth {
        TraceDepth::Statement => trace_run_stmt(program, source, func_name, call_args, emit_stmt),
        TraceDepth::Expr => {
            let watch_expr = t.watch.clone();
            let emit_expr = move |ev: ExprTraceEvent| {
                if !watch_expr.is_empty()
                    && !ev.refs.iter().any(|n| watch_expr.iter().any(|w| w == n))
                {
                    return;
                }
                emit_expr_event(ev);
            };
            trace_run_expr(program, func_name, call_args, emit_stmt, emit_expr)
        }
    }
}

/// Default depth: stmt events only. Tries VM first (fast path), falls back
/// to tree-walker if VM compile fails.
fn trace_run_stmt<F>(
    program: ast::Program,
    source: String,
    func_name: Option<&str>,
    call_args: Vec<Value>,
    emit_stmt: F,
) -> i32
where
    F: FnMut(TraceEvent) + 'static,
{
    match crate::vm::compile(&program) {
        Ok(compiled) => {
            let result = crate::vm::run_with_trace(
                &compiled,
                func_name,
                call_args,
                Some(source.clone()),
                emit_stmt,
            );
            match result {
                Ok(_) => 0,
                Err(e) => {
                    eprintln!("ilo trace: runtime error: {:?}", e.error);
                    1
                }
            }
        }
        Err(_compile_err) => {
            let result =
                crate::interpreter::run_with_trace(&program, func_name, call_args, emit_stmt);
            match result {
                Ok(_) => 0,
                Err(e) => {
                    eprintln!("ilo trace: runtime error [{}]: {}", e.code, e.message);
                    1
                }
            }
        }
    }
}

/// `--depth expr`: route through tree-walker which has full per-expression
/// trace support via `EXPR_TRACE_HOOK`. The VM does not currently emit
/// per-expression events.
fn trace_run_expr<F, G>(
    program: ast::Program,
    func_name: Option<&str>,
    call_args: Vec<Value>,
    emit_stmt: F,
    emit_expr: G,
) -> i32
where
    F: FnMut(TraceEvent) + 'static,
    G: FnMut(ExprTraceEvent) + 'static,
{
    let result = crate::interpreter::run_with_trace_opts(
        &program,
        func_name,
        call_args,
        emit_stmt,
        Some(emit_expr),
    );
    match result {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("ilo trace: runtime error [{}]: {}", e.code, e.message);
            1
        }
    }
}

/// Serialise a `TraceEvent` (statement-level) as a JSON line on stdout.
fn emit_stmt_event(ev: TraceEvent) {
    let mut bindings = serde_json::Map::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (name, val) in ev.bindings.iter().rev() {
        if seen.contains(name) {
            continue;
        }
        seen.insert(name.clone());
        let json_val = val.to_json().unwrap_or(serde_json::Value::Null);
        bindings.insert(name.clone(), json_val);
    }

    let result_json = ev.result.to_json().unwrap_or(serde_json::Value::Null);

    let event = serde_json::json!({
        "schemaVersion": 1,
        "kind": "stmt",
        "line": ev.line,
        "stmt": ev.stmt,
        "bindings": serde_json::Value::Object(bindings),
        "result": result_json,
    });

    println!("{event}");
}

/// Serialise an `ExprTraceEvent` (per-sub-expression) as a JSON line on stdout.
fn emit_expr_event(ev: ExprTraceEvent) {
    let refs_json: Vec<serde_json::Value> = ev
        .refs
        .iter()
        .map(|s| serde_json::Value::String(s.clone()))
        .collect();
    let result_json = ev.result.to_json().unwrap_or(serde_json::Value::Null);

    let event = serde_json::json!({
        "schemaVersion": 1,
        "kind": "expr",
        "line": ev.line,
        "expr": ev.expr,
        "refs": serde_json::Value::Array(refs_json),
        "result": result_json,
    });

    println!("{event}");
}
