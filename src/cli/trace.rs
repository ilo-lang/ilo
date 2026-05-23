//! `ilo trace` — run a program and emit one JSON line per statement.
//!
//! Each line has the schema:
//! ```json
//! {"schemaVersion":1,"line":7,"stmt":"a = +x y","bindings":{"x":3,"y":4,"a":7},"result":7}
//! ```
//!
//! Touch points: ILO-72 (tree-walker), ILO-343 (VM path).
//!
//! ## Engine selection
//!
//! `ilo trace` now tries the VM path first:
//! 1. Compile to bytecode via `crate::vm::compile`.
//! 2. Run via `crate::vm::run_with_trace`, which uses the same `TRACE_HOOK`
//!    thread-local and fires one `TraceEvent` per `OP_STMT` boundary.
//!
//! If compilation fails (e.g. uncompilable construct) it falls back to the
//! tree-walker's `interpreter::run_with_trace` so existing behaviour is
//! preserved. The JIT path is not wired here — JIT trace is tracked in a
//! follow-up ticket.

use super::args::TraceArgs;
use crate::ast;
use crate::interpreter::{TraceEvent, Value};
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

    // --watch filter: only emit events whose bindings touch one of the named vars.
    let watch = t.watch.clone();
    let emit = move |ev: TraceEvent| {
        if !watch.is_empty() && !ev.bindings.iter().any(|(n, _)| watch.iter().any(|w| w == n)) {
            return;
        }
        emit_event(ev);
    };

    // ── VM path (ILO-343) ─────────────────────────────────────────────────────
    // Try to compile to bytecode and run via the VM's OP_STMT trace path.
    // Falls back to the tree-walker if compilation fails.
    match crate::vm::compile(&program) {
        Ok(compiled) => {
            let result = crate::vm::run_with_trace(
                &compiled,
                func_name,
                call_args,
                Some(source.clone()),
                emit,
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
            // ── Tree-walker fallback ─────────────────────────────────────────
            // Use the original ILO-72 tree-walker path.
            let result = crate::interpreter::run_with_trace(&program, func_name, call_args, emit);
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
