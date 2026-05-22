//! `ilo trace` — run a program and emit one JSON line per statement.
//!
//! Each line has the schema:
//! ```json
//! {"schemaVersion":1,"kind":"stmt","line":7,"stmt":"a = +x y","bindings":{"x":3,"y":4,"a":7},"result":7}
//! ```
//!
//! With `--depth expr`, additional sub-expression events are emitted:
//! ```json
//! {"schemaVersion":1,"kind":"expr","line":7,"expr":"+x y","refs":["x","y"],"result":7}
//! ```
//!
//! With `--watch <name>`, only events whose bindings/refs include `<name>` are emitted.
//!
//! Touch points: ILO-72, ILO-344.

use super::args::{TraceArgs, TraceDepth};
use crate::ast;
use crate::interpreter::{ExprTraceEvent, TraceEvent, Value, run_with_trace, run_with_trace_opts};
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

    let watch = t.watch.clone();
    let depth = t.depth;

    // Build stmt callback (always active).
    let watch_stmt = watch.clone();
    let on_stmt = move |ev: TraceEvent| emit_stmt_event(ev, &watch_stmt);

    // Run with trace hook — each event is serialised to one stdout JSON line.
    let result = if depth == TraceDepth::Expr {
        let on_expr = move |ev: ExprTraceEvent| emit_expr_event(ev, &watch);
        run_with_trace_opts(&program, func_name, call_args, on_stmt, Some(on_expr))
    } else {
        run_with_trace(&program, func_name, call_args, on_stmt)
    };

    match result {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("ilo trace: runtime error [{}]: {}", e.code, e.message);
            1
        }
    }
}

/// Serialise a single statement `TraceEvent` as a JSON line to stdout.
/// If `watch` is non-empty, only emit if any watched name appears in bindings.
fn emit_stmt_event(ev: TraceEvent, watch: &[String]) {
    // Build the bindings object (deduplicated: innermost wins).
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

    // Watch filter: skip if none of the watched names appear in bindings.
    if !watch.is_empty() && !watch.iter().any(|w| bindings.contains_key(w.as_str())) {
        return;
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

/// Serialise a single expression `ExprTraceEvent` as a JSON line to stdout.
/// If `watch` is non-empty, only emit if any watched name appears in refs.
fn emit_expr_event(ev: ExprTraceEvent, watch: &[String]) {
    // Watch filter: skip if none of the watched names appear in refs.
    if !watch.is_empty() && !watch.iter().any(|w| ev.refs.iter().any(|r| r == w)) {
        return;
    }

    let result_json = ev.result.to_json().unwrap_or(serde_json::Value::Null);
    let refs_json: Vec<serde_json::Value> =
        ev.refs.iter().map(|r| serde_json::Value::String(r.clone())).collect();

    let event = serde_json::json!({
        "schemaVersion": 1,
        "kind": "expr",
        "line": ev.line,
        "expr": ev.expr,
        "refs": refs_json,
        "result": result_json,
    });

    println!("{event}");
}
