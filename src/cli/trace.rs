//! `ilo trace` — run a program and emit one JSON line per statement.
//!
//! Each line has the schema:
//! ```json
//! {"schemaVersion":1,"line":7,"stmt":"a = +x y","bindings":{"x":3,"y":4,"a":7},"result":7}
//! ```
//!
//! Touch points: ILO-72.

use super::args::TraceArgs;
use crate::ast;
use crate::interpreter::{TraceEvent, Value, run_with_trace};
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

    // Run with trace hook — each event is serialised to one stdout JSON line.
    let result = run_with_trace(&program, func_name, call_args, emit_event);

    match result {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("ilo trace: runtime error [{}]: {}", e.code, e.message);
            1
        }
    }
}

/// Serialise a single `TraceEvent` as a JSON line to stdout.
fn emit_event(ev: TraceEvent) {
    // Build the bindings object.
    let mut bindings = serde_json::Map::new();
    // Deduplicate: if a name appears multiple times (shadowed), take the last
    // (innermost) binding, which is what the interpreter sees.
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
        "line": ev.line,
        "stmt": ev.stmt,
        "bindings": serde_json::Value::Object(bindings),
        "result": result_json,
    });

    println!("{event}");
}
