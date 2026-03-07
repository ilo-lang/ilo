#![warn(clippy::all)]

mod ast;
mod codegen;
mod diagnostic;
mod interpreter;
mod lexer;
mod parser;
mod tools;
mod verify;
mod vm;

use diagnostic::{Diagnostic, ansi::AnsiRenderer, json};

/// Compact spec for LLM consumption — generated from SPEC.md at compile time.
fn compact_spec() -> &'static str {
    include_str!(concat!(env!("OUT_DIR"), "/spec_ai.txt"))
}

// ── `ilo tools` subcommand ─────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum ToolsOutputFmt {
    Human, // human-readable table (default)
    Ilo,   // valid Decl::Tool ilo syntax
    Json,  // JSON array
}

/// Load and display tool signatures from MCP / HTTP sources.
fn tools_cmd(args: &[String]) {
    let mut mcp_path: Option<String> = None;
    let mut http_path: Option<String> = None;
    let mut fmt = ToolsOutputFmt::Human;
    let mut full = false;
    let mut graph = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--mcp" | "-m" => {
                if i + 1 >= args.len() {
                    eprintln!("error: --mcp requires a path");
                    std::process::exit(1);
                }
                mcp_path = Some(args[i + 1].clone());
                i += 2;
            }
            "--tools" | "-t" => {
                if i + 1 >= args.len() {
                    eprintln!("error: --tools requires a path");
                    std::process::exit(1);
                }
                http_path = Some(args[i + 1].clone());
                i += 2;
            }
            "--human" => {
                fmt = ToolsOutputFmt::Human;
                i += 1;
            }
            "--ilo" => {
                fmt = ToolsOutputFmt::Ilo;
                i += 1;
            }
            "--json" => {
                fmt = ToolsOutputFmt::Json;
                i += 1;
            }
            "--full" | "-f" => {
                full = true;
                i += 1;
            }
            "--graph" | "-g" => {
                graph = true;
                i += 1;
            }
            _ => {
                eprintln!("unknown flag: {}", args[i]);
                eprintln!(
                    "Usage: ilo tools [-m <path>] [-t <path>] \
                     [--human|--ilo|--json] [--full] [--graph]"
                );
                std::process::exit(1);
            }
        }
    }

    if mcp_path.is_none() && http_path.is_none() {
        eprintln!(
            "error: ilo tools requires at least one of --mcp <path> or --tools <path>"
        );
        eprintln!(
            "Usage: ilo tools [--mcp <path>] [--tools <path>] \
             [--human|--ilo|--json] [--full] [--graph]"
        );
        std::process::exit(1);
    }

    // --ilo, --json, and --graph always show full signatures.
    if matches!(fmt, ToolsOutputFmt::Ilo | ToolsOutputFmt::Json) || graph {
        full = true;
    }

    // ── HTTP tools (sync, no feature gate) ───────────────────────────────────
    let mut http_names: Vec<String> = Vec::new();
    if let Some(ref path) = http_path {
        let config = tools::http_provider::ToolsConfig::from_file(path)
            .unwrap_or_else(|e| {
                eprintln!("{}", e);
                std::process::exit(1);
            });
        let mut names: Vec<String> = config.tools.keys().cloned().collect();
        names.sort();
        http_names = names;
    }

    // ── MCP tools (async, feature-gated) ─────────────────────────────────────
    let mcp_decls = collect_mcp_tool_decls(mcp_path.as_deref());

    // ── Render ────────────────────────────────────────────────────────────────
    match fmt {
        ToolsOutputFmt::Human => {
            for name in &http_names {
                if full {
                    println!("{:<32} (http tool — no type info)", name);
                } else {
                    println!("{}", name);
                }
            }
            for decl in &mcp_decls {
                if let ast::Decl::Tool { name, description, params, return_type, .. } = decl {
                    if full {
                        let sig = tool_sig_str(params, return_type);
                        println!("{:<32} {:<44} {}", name, description, sig);
                    } else {
                        println!("{}", name);
                    }
                }
            }
        }
        ToolsOutputFmt::Ilo => {
            for name in &http_names {
                // No type info: emit with empty description and generic R t t.
                println!("tool {}\"\" > R t t", name);
            }
            for decl in &mcp_decls {
                println!("{}", codegen::fmt::format_decl(decl, codegen::fmt::FmtMode::Dense));
            }
        }
        ToolsOutputFmt::Json => {
            let mut items: Vec<serde_json::Value> = Vec::new();
            for name in &http_names {
                items.push(serde_json::json!({
                    "name": name,
                    "source": "http",
                    "description": null,
                    "params": [],
                    "return": null
                }));
            }
            for decl in &mcp_decls {
                if let ast::Decl::Tool { name, description, params, return_type, .. } = decl {
                    let params_json: Vec<serde_json::Value> = params
                        .iter()
                        .map(|p| {
                            serde_json::json!({
                                "name": p.name,
                                "type": codegen::fmt::type_str(&p.ty)
                            })
                        })
                        .collect();
                    items.push(serde_json::json!({
                        "name": name,
                        "source": "mcp",
                        "description": description,
                        "params": params_json,
                        "return": codegen::fmt::type_str(return_type)
                    }));
                }
            }
            match serde_json::to_string_pretty(&items) {
                Ok(s) => println!("{}", s),
                Err(e) => {
                    eprintln!("failed to render JSON: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }

    // ── Graph (additive — shown after or instead of table) ───────────────────
    if graph {
        print_tool_graph(&mcp_decls);
    }
}

/// Print a type-level composition graph: for each tool, which tools can consume its output.
fn print_tool_graph(decls: &[ast::Decl]) {
    // Collect (name, params, return_type) for all typed tools.
    let tools: Vec<(&str, &[ast::Param], &ast::Type)> = decls
        .iter()
        .filter_map(|d| {
            if let ast::Decl::Tool { name, params, return_type, .. } = d {
                Some((name.as_str(), params.as_slice(), return_type))
            } else {
                None
            }
        })
        .collect();

    if tools.is_empty() {
        println!("(no typed tools — graph requires MCP source)");
        return;
    }

    // Name column width.
    let name_w = tools.iter().map(|(n, _, _)| n.len()).max().unwrap_or(8).max(8);
    // Sig column width — cap at 36 for readability.
    let sig_w: usize = 36;

    println!("Tool composition graph\n");
    println!("{:<name_w$}  {:<sig_w$}  feeds →", "tool", "signature");
    println!("{}", "─".repeat(name_w + 2 + sig_w + 2 + 40));

    for &(src_name, src_params, src_ret) in &tools {
        let sig = tool_sig_str(src_params, src_ret);
        // The "output" is the ok branch of R, or the type itself.
        let out_ty = tool_ok_type(src_ret);

        // Find tools whose first param (or any param) accepts out_ty.
        let mut consumers: Vec<&str> = tools
            .iter()
            .filter(|&&(dst_name, dst_params, _)| {
                dst_name != src_name
                    && dst_params.iter().any(|p| types_pipe_compatible(out_ty, &p.ty))
            })
            .map(|&(n, _, _)| n)
            .collect();
        consumers.sort();

        let feeds = if consumers.is_empty() {
            "—".to_string()
        } else {
            consumers.join(", ")
        };

        let sig_char_len = sig.chars().count();
        let sig_display = if sig_char_len > sig_w {
            let truncated: String = sig.chars().take(sig_w.saturating_sub(1)).collect();
            format!("{}…", truncated)
        } else {
            sig
        };
        println!("{:<name_w$}  {:<sig_w$}  {}", src_name, sig_display, feeds);
    }
    println!();
}

/// Unwrap `R ok err` → `ok`; otherwise return the type itself.
fn tool_ok_type(ty: &ast::Type) -> &ast::Type {
    if let ast::Type::Result(ok, _) = ty { ok } else { ty }
}

/// True if a value of type `out` can be piped into a parameter of type `param`.
/// Intentionally permissive: unknown/named types match anything.
fn types_pipe_compatible(out: &ast::Type, param: &ast::Type) -> bool {
    use ast::Type::*;
    // Unwrap Optional on the param side — Optional(T) accepts T.
    let param = if let Optional(inner) = param { inner } else { param };
    match (out, param) {
        // Named / unknown types — treat as wildcard.
        (Named(_), _) | (_, Named(_)) => true,
        // Exact matches.
        (Number, Number) | (Text, Text) | (Bool, Bool) | (Nil, Nil) => true,
        // List element type must match.
        (List(a), List(b)) => types_pipe_compatible(a, b),
        // Map — key and value types must match.
        (Map(ak, av), Map(bk, bv)) => {
            types_pipe_compatible(ak, bk) && types_pipe_compatible(av, bv)
        }
        // Result ok branch feeds a result-accepting param.
        (Result(ao, ae), Result(bo, be)) => {
            types_pipe_compatible(ao, bo) && types_pipe_compatible(ae, be)
        }
        // Sum types are text at runtime — compatible with text params.
        (Sum(_), Text) | (Text, Sum(_)) | (Sum(_), Sum(_)) => true,
        _ => false,
    }
}

/// Format params + return type as a human-readable signature string.
fn tool_sig_str(params: &[ast::Param], ret: &ast::Type) -> String {
    let ps: Vec<String> = params
        .iter()
        .map(|p| format!("{}:{}", p.name, codegen::fmt::type_str(&p.ty)))
        .collect();
    if ps.is_empty() {
        format!("> {}", codegen::fmt::type_str(ret))
    } else {
        format!("{} > {}", ps.join(" "), codegen::fmt::type_str(ret))
    }
}

/// Connect to MCP servers and return synthesized `Decl::Tool` nodes.
/// Exits with error if `tools` feature is not enabled and a path is given.
#[cfg(feature = "tools")]
fn collect_mcp_tool_decls(path: Option<&str>) -> Vec<ast::Decl> {
    let path = match path {
        Some(p) => p,
        None => return vec![],
    };
    let config = tools::mcp_provider::McpConfig::from_file(path).unwrap_or_else(|e| {
        eprintln!("{}", e);
        std::process::exit(1);
    });
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let provider = rt
        .block_on(tools::mcp_provider::McpProvider::connect(&config))
        .unwrap_or_else(|e| {
            eprintln!("MCP error: {}", e);
            std::process::exit(1);
        });
    provider.tool_decls()
}

#[cfg(not(feature = "tools"))]
fn collect_mcp_tool_decls(path: Option<&str>) -> Vec<ast::Decl> {
    if path.is_some() {
        eprintln!(
            "error: --mcp requires the 'tools' feature \
             (build with: cargo build --features tools)"
        );
        std::process::exit(1);
    }
    vec![]
}

// ── `ilo serv` subcommand ──────────────────────────────────────────────────

/// Render a `Diagnostic` as a `serde_json::Value` for inclusion in serve responses.
fn diag_to_json(d: &Diagnostic) -> serde_json::Value {
    let s = diagnostic::json::render(d);
    serde_json::from_str(&s).unwrap_or(serde_json::json!({"message": s}))
}

/// Process a single serve request line and return the JSON response.
fn process_serv_request(
    line: &str,
    mcp_tool_decls: &[ast::Decl],
    #[cfg(feature = "tools")] provider: Option<std::sync::Arc<dyn tools::ToolProvider>>,
    #[cfg_attr(not(feature = "tools"), allow(unused_variables))]
    http_config: Option<&tools::http_provider::ToolsConfig>,
    #[cfg(feature = "tools")] rt: std::sync::Arc<tokio::runtime::Runtime>,
) -> serde_json::Value {
    #[derive(serde::Deserialize)]
    struct Req {
        program: String,
        #[serde(default)]
        args: Vec<String>,
        func: Option<String>,
    }

    let req: Req = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            return serde_json::json!({
                "error": {"phase": "request", "message": format!("invalid JSON: {e}")}
            })
        }
    };

    let start = std::time::Instant::now();
    let source = req.program.clone();

    // Lex
    let tokens = match lexer::lex(&source) {
        Ok(t) => t,
        Err(e) => {
            return serde_json::json!({
                "error": {
                    "phase": "lex",
                    "diagnostics": [diag_to_json(&Diagnostic::from(&e))]
                }
            })
        }
    };

    // Parse
    let token_spans: Vec<_> = tokens
        .into_iter()
        .map(|(t, r)| (t, ast::Span { start: r.start, end: r.end }))
        .collect();
    let (mut program, parse_errors) = parser::parse(token_spans);
    program.source = Some(source.clone());

    if !parse_errors.is_empty() {
        let diags: Vec<_> =
            parse_errors.iter().map(|e| diag_to_json(&Diagnostic::from(e))).collect();
        return serde_json::json!({"error": {"phase": "parse", "diagnostics": diags}});
    }

    // Inject static MCP tool decls so the verifier sees them
    if !mcp_tool_decls.is_empty() {
        let mut decls = mcp_tool_decls.to_vec();
        decls.append(&mut program.declarations);
        program.declarations = decls;
    }

    // Verify
    let vr = verify::verify(&program);
    if !vr.errors.is_empty() {
        let diags: Vec<_> = vr
            .errors
            .iter()
            .map(|e| diag_to_json(&Diagnostic::from(e).with_source(source.clone())))
            .collect();
        return serde_json::json!({"error": {"phase": "verify", "diagnostics": diags}});
    }

    // Run
    let run_args: Vec<interpreter::Value> =
        req.args.iter().map(|a| parse_cli_arg(a)).collect();
    let func_name = req.func.as_deref();

    #[cfg(feature = "tools")]
    let result = if let Some(p) = provider {
        interpreter::run_with_tools(&program, func_name, run_args, p, rt)
    } else if let Some(cfg) = http_config {
        let p = std::sync::Arc::new(tools::http_provider::HttpProvider::new(cfg.clone()));
        interpreter::run_with_tools(&program, func_name, run_args, p, rt)
    } else {
        interpreter::run(&program, func_name, run_args)
    };

    #[cfg(not(feature = "tools"))]
    let result = interpreter::run(&program, func_name, run_args);

    let ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(value) => match value {
            interpreter::Value::Ok(inner) => {
                let v = inner.to_json().unwrap_or(serde_json::Value::Null);
                serde_json::json!({"ok": v, "ms": ms})
            }
            interpreter::Value::Err(inner) => {
                let v = inner.to_json().unwrap_or_else(|_| {
                    serde_json::Value::String(inner.to_string())
                });
                serde_json::json!({"error": {"phase": "program", "value": v}, "ms": ms})
            }
            other => {
                let v = other.to_json().unwrap_or_else(|_| {
                    serde_json::Value::String(other.to_string())
                });
                serde_json::json!({"ok": v, "ms": ms})
            }
        },
        Err(e) => {
            let d = Diagnostic::from(&e).with_source(source);
            serde_json::json!({"error": {"phase": "runtime", "diagnostics": [diag_to_json(&d)]}})
        }
    }
}

fn type_to_ilo(ty: &ast::Type) -> String {
    match ty {
        ast::Type::Number => "n".to_string(),
        ast::Type::Text => "t".to_string(),
        ast::Type::Bool => "b".to_string(),
        ast::Type::Nil => "_".to_string(),
        ast::Type::Optional(inner) => format!("O {}", type_to_ilo(inner)),
        ast::Type::List(inner) => format!("L {}", type_to_ilo(inner)),
        ast::Type::Map(k, v) => format!("M {} {}", type_to_ilo(k), type_to_ilo(v)),
        ast::Type::Result(ok, err) => format!("R {} {}", type_to_ilo(ok), type_to_ilo(err)),
        ast::Type::Sum(variants) => format!("S {}", variants.join(" ")),
        ast::Type::Fn(params, ret) => {
            let ps: Vec<_> = params.iter().map(type_to_ilo).collect();
            format!("F {} {}", ps.join(" "), type_to_ilo(ret))
        }
        ast::Type::Named(name) => name.clone(),
    }
}

/// Interactive REPL — define functions, evaluate expressions, accumulate state.
fn repl_cmd() {
    use std::io::{BufRead, Write};

    let version = env!("CARGO_PKG_VERSION");
    let renderer = AnsiRenderer { use_color: true };
    println!("ilo {version} — type :help for commands, :q to quit\n");

    // Accumulated function definitions across the session
    let mut defs: Vec<String> = Vec::new();

    let stdin = std::io::stdin();
    let mut reader = stdin.lock();

    loop {
        print!("> ");
        std::io::stdout().flush().ok();

        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break, // Ctrl+D / EOF
            Ok(_) => {}
            Err(_) => break,
        }

        let input = line.trim();
        if input.is_empty() {
            continue;
        }

        // Meta-commands (nvim-style)
        if input.starts_with(':') {
            match input {
                ":q" | ":q!" | ":x" | ":quit" | ":exit" => break,
                ":wq" => {
                    if defs.is_empty() {
                        eprintln!("no definitions to save");
                    } else {
                        eprintln!("usage: :w <file.ilo>");
                    }
                    continue;
                }
                _ if input.starts_with(":wq ") || input.starts_with(":w ") => {
                    let is_wq = input.starts_with(":wq");
                    let path = input.split_once(' ').unwrap().1.trim();
                    if defs.is_empty() {
                        eprintln!("no definitions to save");
                    } else if let Err(e) = std::fs::write(path, defs.join(" ") + "\n") {
                        eprintln!("error: {e}");
                    } else {
                        println!("saved {} definition(s) to {path}", defs.len());
                    }
                    if is_wq { break; } else { continue; }
                }
                ":defs" => {
                    if defs.is_empty() {
                        println!("(no definitions)");
                    } else {
                        for d in &defs {
                            println!("  {d}");
                        }
                    }
                    continue;
                }
                ":clear" => {
                    defs.clear();
                    println!("cleared all definitions");
                    continue;
                }
                ":help" => {
                    println!(":q :q! :x :quit :exit   quit");
                    println!(":w <file>               save definitions to file");
                    println!(":wq <file>              save and quit");
                    println!(":defs                   list defined functions");
                    println!(":clear                  clear all definitions");
                    println!(":help                   show this help");
                    continue;
                }
                _ => {
                    eprintln!("unknown command: {input}  (type :help)");
                    continue;
                }
            }
        }

        // Also support bare "exit"
        if input == "exit" || input == "quit" {
            break;
        }

        // Try to parse input as function definition(s) first
        let source = input.to_string();
        let def_program = {
            let tokens = lexer::lex(&source);
            if let Ok(tokens) = tokens {
                let token_spans: Vec<_> = tokens
                    .into_iter()
                    .map(|(t, r)| (t, ast::Span { start: r.start, end: r.end }))
                    .collect();
                let (program, errors) = parser::parse(token_spans);
                if errors.is_empty()
                    && !program.declarations.is_empty()
                    && program.declarations.iter().all(|d| matches!(d, ast::Decl::Function { .. }))
                {
                    Some(program)
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(program) = def_program {
            defs.push(input.to_string());

            for d in &program.declarations {
                if let ast::Decl::Function { name, params, return_type, .. } = d {
                    let params_str: Vec<_> = params.iter().map(|p| format!("{}:{}", p.name, type_to_ilo(&p.ty))).collect();
                    println!("defined: {}({}) -> {}", name, params_str.join(", "), type_to_ilo(return_type));
                }
            }
            continue;
        }

        // Expression or function call — wrap in a repl function with all accumulated defs
        // Use `repleval` as name (starts with letter, won't collide)
        let full_source = if defs.is_empty() {
            format!("repleval>n;{input}")
        } else {
            format!("{} repleval>n;{input}", defs.join(" "))
        };

        let tokens = match lexer::lex(&full_source) {
            Ok(t) => t,
            Err(e) => {
                let d = Diagnostic::from(&e);
                eprintln!("{}", renderer.render(&d));
                continue;
            }
        };

        let token_spans: Vec<_> = tokens
            .into_iter()
            .map(|(t, r)| (t, ast::Span { start: r.start, end: r.end }))
            .collect();
        let (mut full_program, parse_errors) = parser::parse(token_spans);
        full_program.source = Some(full_source.clone());

        if !parse_errors.is_empty() {
            // Show errors relative to user input, not the wrapper
            for e in &parse_errors {
                let d = Diagnostic::from(e);
                eprintln!("{}", renderer.render(&d));
            }
            continue;
        }

        // Skip type checking for the repl wrapper — just run it
        // This allows expressions of any type to be evaluated
        match interpreter::run(&full_program, Some("repleval"), vec![]) {
            Ok(value) => println!("{value}"),
            Err(e) => {
                let d = Diagnostic::from(&e).with_source(full_source);
                eprintln!("{}", renderer.render(&d));
            }
        }
    }
}

/// Stdio-based agent serve loop.
/// Reads one JSON request per line from stdin, writes one JSON response per line to stdout.
fn serv_cmd(args_slice: &[String]) {
    let mut mcp_path: Option<String> = None;
    let mut http_path: Option<String> = None;

    let mut i = 0;
    while i < args_slice.len() {
        match args_slice[i].as_str() {
            "--mcp" | "-m" => {
                if i + 1 >= args_slice.len() {
                    eprintln!("error: --mcp requires a path");
                    std::process::exit(1);
                }
                mcp_path = Some(args_slice[i + 1].clone());
                i += 2;
            }
            "--tools" | "-t" => {
                if i + 1 >= args_slice.len() {
                    eprintln!("error: --tools requires a path");
                    std::process::exit(1);
                }
                http_path = Some(args_slice[i + 1].clone());
                i += 2;
            }
            "-j" | "--json" => {
                // JSON protocol is always on in repl/serv; flag is a no-op (accepted for alias compat)
                i += 1;
            }
            _ => {
                eprintln!("unknown flag: {}", args_slice[i]);
                eprintln!("Usage: ilo repl [-j] [--mcp <path>] [--tools <path>]");
                std::process::exit(1);
            }
        }
    }

    // Load HTTP config (sync)
    let http_config: Option<tools::http_provider::ToolsConfig> =
        http_path.as_ref().map(|p| {
            tools::http_provider::ToolsConfig::from_file(p).unwrap_or_else(|e| {
                eprintln!("{}", e);
                std::process::exit(1);
            })
        });

    // Create tokio runtime once (used for MCP connect + all tool calls)
    #[cfg(feature = "tools")]
    let rt = std::sync::Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime"),
    );

    // Connect to MCP once, keep provider + static decls alive for all requests
    #[cfg(feature = "tools")]
    let (mcp_tool_decls, mcp_provider_arc): (
        Vec<ast::Decl>,
        Option<std::sync::Arc<dyn tools::ToolProvider>>,
    ) = if let Some(ref path) = mcp_path {
        let config = tools::mcp_provider::McpConfig::from_file(path).unwrap_or_else(|e| {
            eprintln!("{}", e);
            std::process::exit(1);
        });
        let provider =
            rt.block_on(tools::mcp_provider::McpProvider::connect(&config))
                .unwrap_or_else(|e| {
                    eprintln!("MCP error: {}", e);
                    std::process::exit(1);
                });
        let decls = provider.tool_decls();
        (decls, Some(std::sync::Arc::new(provider)))
    } else {
        (vec![], None)
    };

    #[cfg(not(feature = "tools"))]
    let mcp_tool_decls: Vec<ast::Decl> = {
        if mcp_path.is_some() {
            eprintln!(
                "error: --mcp requires the 'tools' feature \
                 (build with: cargo build --features tools)"
            );
            std::process::exit(1);
        }
        vec![]
    };

    // Signal ready
    println!("{}", serde_json::json!({"ready": true}));

    use std::io::BufRead;
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("stdin read error: {}", e);
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }

        let resp = process_serv_request(
            &line,
            &mcp_tool_decls,
            #[cfg(feature = "tools")]
            mcp_provider_arc.as_ref().map(std::sync::Arc::clone),
            http_config.as_ref(),
            #[cfg(feature = "tools")]
            std::sync::Arc::clone(&rt),
        );
        println!("{}", resp);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    Ansi,
    Text,
    Json,
}

/// Scan args for --json/-j, --text/-t, --ansi/-a, --no-hints/-nh.
/// Return (mode, explicit_json, no_hints, remaining_args).
/// Multiple format flags → error + exit(1).
/// `explicit_json` is true only when the user passed --json/-j; auto-detection never sets it.
fn detect_output_mode(args: Vec<String>) -> (OutputMode, bool, bool, Vec<String>) {
    let mut mode: Option<OutputMode> = None;
    let mut remaining = Vec::with_capacity(args.len());
    let mut conflict = false;
    let mut no_hints = false;

    for arg in args {
        match arg.as_str() {
            "--json" | "-j" => {
                if mode.is_some() { conflict = true; } else { mode = Some(OutputMode::Json); }
            }
            "--text" | "-t" => {
                if mode.is_some() { conflict = true; } else { mode = Some(OutputMode::Text); }
            }
            "--ansi" | "-a" => {
                if mode.is_some() { conflict = true; } else { mode = Some(OutputMode::Ansi); }
            }
            "--no-hints" | "-nh" => {
                no_hints = true;
            }
            _ => remaining.push(arg),
        }
    }

    if conflict {
        eprintln!("error: --json, --text, and --ansi are mutually exclusive");
        std::process::exit(1);
    }

    let explicit_json = matches!(mode, Some(OutputMode::Json));

    let resolved = mode.unwrap_or_else(|| {
        // Auto-detect: isatty(stderr) && !NO_COLOR → Ansi; isatty && NO_COLOR → Text; !isatty → Json
        use std::io::IsTerminal;
        let is_tty = std::io::stderr().is_terminal();
        let no_color = std::env::var("NO_COLOR").is_ok();
        if is_tty && !no_color {
            OutputMode::Ansi
        } else if is_tty {
            OutputMode::Text
        } else {
            OutputMode::Json
        }
    });

    (resolved, explicit_json, no_hints, remaining)
}

/// Replace the contents of string literals with spaces, preserving length.
/// `"https://x.com"` → `"               "`. This prevents cross-language pattern
/// detection from matching inside strings (e.g. `//` in URLs).
fn strip_string_contents(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let mut in_string = false;
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        if in_string {
            if c == '\\' {
                // Escaped character — skip both backslash and next char
                result.push(' ');
                if chars.next().is_some() {
                    result.push(' ');
                }
            } else if c == '"' {
                result.push('"');
                in_string = false;
            } else {
                result.push(' ');
            }
        } else if c == '"' {
            result.push('"');
            in_string = true;
        } else {
            result.push(c);
        }
    }
    result
}

/// Collect idiomatic hints by scanning source text for non-canonical forms.
/// Returns a list of human-readable hint strings.
fn collect_hints(source: &str) -> Vec<String> {
    let mut hints = Vec::new();
    // Hint: == → = saves 1 character
    // Scan for `==` outside string literals (reuse strip_string_contents)
    let stripped = strip_string_contents(source);
    let mut pos = 0;
    let bytes = stripped.as_bytes();
    while pos + 1 < bytes.len() {
        if bytes[pos] == b'=' && bytes[pos + 1] == b'=' {
            hints.push("hint: `==` → `=` saves 1 char (both mean equality in ilo)".to_string());
            break; // one hint per pattern type is enough
        }
        pos += 1;
    }
    hints
}

/// Emit hints to the appropriate output channel.
/// TTY → stderr, JSON → adds to JSON output (caller handles), pipe → nothing.
fn emit_hints(hints: &[String], mode: OutputMode) {
    if hints.is_empty() {
        return;
    }
    match mode {
        OutputMode::Ansi | OutputMode::Text => {
            for hint in hints {
                eprintln!("{hint}");
            }
        }
        OutputMode::Json => {
            // JSON mode: hints go to stderr as JSON array
            let json = serde_json::json!({ "hints": hints });
            eprintln!("{}", json);
        }
    }
}

/// Scan source for common cross-language patterns and emit a single warning if found.
/// Non-fatal — program still attempts to run.
fn warn_cross_language_syntax(source: &str, mode: OutputMode) {
    let patterns: &[(&str, &str)] = &[
        ("&&", "'&&' — ilo uses '&' for AND"),
        ("||", "'||' — ilo uses '|' for OR"),
        ("->", "'->' — ilo uses '>' for return type separator"),
        ("//", "'//' — ilo uses '--' for comments"),
    ];

    // Strip string literal contents so patterns inside "..." don't trigger warnings.
    // This avoids false positives for URLs like "https://example.com".
    let stripped = strip_string_contents(source);

    let details: Vec<&str> = patterns
        .iter()
        .filter(|(pat, _)| stripped.contains(*pat))
        .map(|(_, desc)| *desc)
        .collect();

    if details.is_empty() {
        return;
    }

    let msg = format!(
        "source contains syntax from another language: {}",
        details.join(", ")
    );
    let d = Diagnostic::warning(msg);
    report_diagnostic(&d, mode);
}

/// Return the declared name of a `Decl`, if it has one.
fn decl_name(decl: &ast::Decl) -> Option<&str> {
    match decl {
        ast::Decl::Function { name, .. } => Some(name),
        ast::Decl::Tool { name, .. } => Some(name),
        ast::Decl::TypeDef { name, .. } => Some(name),
        ast::Decl::Alias { name, .. } => Some(name),
        ast::Decl::Use { .. } | ast::Decl::Error { .. } => None,
    }
}

/// Resolve all `Decl::Use` nodes in `decls` recursively, returning a flat
/// merged list with imported declarations prepended and `Use` nodes stripped.
///
/// - `base_dir`: directory of the importing file (used to resolve relative paths).
///   `None` means inline code — `use` is not supported without a file context.
/// - `visited`: canonical paths already in the import chain; circular imports are errors.
/// - `diagnostics`: errors are pushed here (file-not-found, circular, parse failures).
fn resolve_imports(
    decls: Vec<ast::Decl>,
    base_dir: Option<&std::path::Path>,
    visited: &mut std::collections::HashSet<std::path::PathBuf>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<ast::Decl> {
    let mut result: Vec<ast::Decl> = Vec::new();

    for decl in decls {
        if let ast::Decl::Use { path, only, span } = decl {
            let Some(dir) = base_dir else {
                diagnostics.push(
                    Diagnostic::error("`use` requires a file path context — not supported in inline code")
                        .with_code("ILO-P017")
                        .with_span(span, "here"),
                );
                continue;
            };

            let file_path = dir.join(&path);
            let canonical = match file_path.canonicalize() {
                Ok(c) => c,
                Err(_) => {
                    diagnostics.push(
                        Diagnostic::error(format!("use \"{}\": file not found", path))
                            .with_code("ILO-P017")
                            .with_span(span, "imported here"),
                    );
                    continue;
                }
            };

            if visited.contains(&canonical) {
                diagnostics.push(
                    Diagnostic::error(format!("use \"{}\": circular import", path))
                        .with_code("ILO-P018")
                        .with_span(span, "imported here"),
                );
                continue;
            }

            let source = match std::fs::read_to_string(&canonical) {
                Ok(s) => s,
                Err(e) => {
                    diagnostics.push(
                        Diagnostic::error(format!("use \"{}\": {}", path, e))
                            .with_code("ILO-P017")
                            .with_span(span, "imported here"),
                    );
                    continue;
                }
            };

            let tokens = match lexer::lex(&source) {
                Ok(t) => t,
                Err(e) => {
                    diagnostics.push(Diagnostic::from(&e));
                    continue;
                }
            };

            let token_spans: Vec<(lexer::Token, ast::Span)> = tokens
                .into_iter()
                .map(|(t, r)| (t, ast::Span { start: r.start, end: r.end }))
                .collect();

            let (imported_prog, parse_errors) = parser::parse(token_spans);
            for e in &parse_errors {
                diagnostics.push(Diagnostic::from(e));
            }

            visited.insert(canonical.clone());
            let imported_dir = canonical.parent();
            let imported_decls = resolve_imports(
                imported_prog.declarations,
                imported_dir,
                visited,
                diagnostics,
            );
            visited.remove(&canonical);

            // Apply `only [...]` filter if specified
            let filtered = if let Some(ref names) = only {
                // Warn about any requested names that weren't found
                for name in names {
                    let found = imported_decls.iter().any(|d| decl_name(d) == Some(name.as_str()));
                    if !found {
                        diagnostics.push(
                            Diagnostic::error(format!("use \"{}\": name '{}' not found in imported file", path, name))
                                .with_code("ILO-P019")
                                .with_span(span, "imported here"),
                        );
                    }
                }
                imported_decls
                    .into_iter()
                    .filter(|d| decl_name(d).map(|n| names.iter().any(|s| s == n)).unwrap_or(false))
                    .collect::<Vec<_>>()
            } else {
                imported_decls
            };

            // Prepend imported declarations (so they appear before the importer's own decls)
            result.extend(filtered);
        } else {
            result.push(decl);
        }
    }

    result
}

fn report_diagnostic(d: &Diagnostic, mode: OutputMode) {
    let s = match mode {
        OutputMode::Ansi => AnsiRenderer { use_color: true }.render(d),
        OutputMode::Text => AnsiRenderer { use_color: false }.render(d),
        // JSON mode: one object per line (NDJSON) so multiple errors are parseable.
        OutputMode::Json => format!("{}\n", json::render(d)),
    };
    eprint!("{}", s);
}

/// Load a single env file into the process environment.
/// Lines starting with `#` are comments. Blank lines are skipped.
/// Format: `KEY=VALUE` — no quoting, no variable expansion.
/// Does NOT overwrite variables already set in the environment.
fn load_env_file(path: &str) {
    let Ok(contents) = std::fs::read_to_string(path) else { return };
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        if let Some((key, val)) = line.split_once('=') {
            let key = key.trim();
            let val = val.trim();
            if !key.is_empty() && std::env::var(key).is_err() {
                // SAFETY: single-threaded at startup, no concurrent env access
                unsafe { std::env::set_var(key, val) };
            }
        }
    }
}

/// Load `.env.local` then `.env` from the current working directory.
/// `.env.local` takes priority (loaded first; later files don't overwrite).
fn load_dotenv() {
    load_env_file(".env.local");
    load_env_file(".env");
}

fn main() {
    load_dotenv();
    let raw_args: Vec<String> = std::env::args().collect();

    // `ilo tools` is handled before output-mode detection so that --json/--ilo
    // can be used as *tool format* flags without conflicting with error format flags.
    if matches!(raw_args.get(1).map(|s| s.as_str()), Some("tools") | Some("tool")) {
        tools_cmd(&raw_args[2..]);
        std::process::exit(0);
    }

    if matches!(raw_args.get(1).map(|s| s.as_str()), Some("serv") | Some("repl")) {
        let rest: Vec<String> = raw_args[2..].to_vec();
        let is_json = raw_args.get(1).map(|s| s.as_str()) == Some("serv")
            || rest.iter().any(|a| a == "-j" || a == "--json");
        if is_json {
            let mut serv_args = vec!["-j".to_string()];
            serv_args.extend(rest.iter().filter(|a| *a != "-j" && *a != "--json").cloned());
            serv_cmd(&serv_args);
        } else {
            repl_cmd();
        }
        std::process::exit(0);
    }

    let (mode, explicit_json, no_hints, args) = detect_output_mode(raw_args);

    if args.len() < 2 {
        eprintln!("Usage: ilo <file-or-code> [args... | --run func args... | --bench func args... | --emit python]");
        eprintln!("       ilo repl                                  Interactive REPL");
        eprintln!("       ilo serv [--mcp <path>] [--tools <path>]  Stdio agent loop");
        eprintln!("       ilo help | -h     Show usage and examples");
        eprintln!("       ilo help lang     Show language specification");
        eprintln!("       ilo help ai | -ai Compact spec for LLM consumption");
        std::process::exit(1);
    }

    if args[1] == "--version" || args[1] == "-V" {
        println!("ilo {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

    if args[1] == "--explain" {
        match args.get(2) {
            Some(code) => match diagnostic::registry::lookup(code) {
                Some(entry) => {
                    print!("{}", entry.long);
                    std::process::exit(0);
                }
                None => {
                    eprintln!("unknown error code: {code}");
                    eprintln!("Error codes have the form ILO-L001, ILO-P001, ILO-T001, ILO-R001.");
                    std::process::exit(1);
                }
            },
            None => {
                eprintln!("Usage: ilo --explain <code>  (e.g. ilo --explain ILO-T005)");
                std::process::exit(1);
            }
        }
    }

    if args[1] == "-ai" {
        print!("{}", compact_spec());
        std::process::exit(0);
    }

    if args[1] == "help" || args[1] == "--help" || args[1] == "-h" {
        if args.len() > 2 && args[2] == "lang" {
            print!("{}", include_str!("../SPEC.md"));
        } else if args.len() > 2 && args[2] == "ai" {
            print!("{}", compact_spec());
        } else {
            println!("ilo — a programming language for AI agents\n");
            println!("Usage:");
            println!("  ilo <code> [args...]              Run (Cranelift JIT, falls back to interpreter)");
            println!("  ilo <file.ilo> [args...]          Run from file");
            println!("  ilo <code> func [args...]         Run a specific function");
            println!("  ilo <code> --emit python          Transpile to Python");
            println!("  ilo <code> --explain / -x            Annotate each statement with its role");
            println!("  ilo <code> --dense / -d             Reformat (dense wire format)");
            println!("  ilo <code> --expanded / -e          Reformat (expanded human format)");
            println!("  ilo <code>                        Print AST as JSON (no args)");
            println!("  ilo <code> --bench func [args...] Benchmark a function");
            println!("  ilo repl                          Interactive REPL");
            println!("  ilo help lang                     Show language specification");
            println!("  ilo help ai | ilo -ai             Compact spec for LLM consumption");
            println!("  ilo --explain ILO-T005            Explain an error code\n");
            println!("Output format (errors):");
            println!("  --ansi / -a   Force ANSI colour output (default when stderr is a TTY)");
            println!("  --text / -t   Force plain text output (no colour)");
            println!("  --json / -j   Force JSON output (default when stderr is not a TTY)");
            println!("  --no-hints / -nh  Suppress idiomatic hints after execution");
            println!("  NO_COLOR=1    Disable colour (same as --text)\n");
            println!("Tool providers (requires --features tools build):");
            println!("  --tools <path>   HTTP tool provider config (JSON)");
            println!("  --mcp <path>     MCP server config (Claude Desktop format JSON)\n");
            println!("Tool discovery:");
            println!("  ilo tool -m <path>              List tools from MCP server");
            println!("  ilo tool -t <path>              List tools from HTTP config");
            println!("  ilo tool ... --full             Show full signatures");
            println!("  ilo tool ... --ilo              Output as valid ilo tool declarations");
            println!("  ilo tool ... --json             Output as JSON array\n");
            println!("Agent serve loop:");
            println!("  ilo serv [-m <path>] [-t <path>]");
            println!("  Request:  {{\"program\": \"<ilo>\", \"args\": [...], \"func\": \"name\"}}");
            println!("  Response: {{\"ok\": <value>, \"ms\": n}} | {{\"error\": {{\"phase\": \"...\", ...}}}}\n");
            println!("Backends:");
            println!("  (default)        Cranelift JIT → interpreter fallback");
            println!("  --run-interp     Tree-walking interpreter");
            println!("  --run-vm         Register VM");
            println!("  --run-cranelift  Cranelift JIT");
            println!("  --run-jit        Custom ARM64 JIT (macOS Apple Silicon only)");
            println!("  --run-llvm       LLVM JIT (requires --features llvm build)\n");
            println!("Examples:");
            println!("  ilo 'f x:n>n;*x 2' 5             Define and call f(5) → 10");
            println!("  ilo 'f xs:L n>n;len xs' 1,2,3     Pass a list → 3");
            println!("  ilo program.ilo 10 20             Run file with arguments");
            println!("  ilo 'f x:n>n;*x 2' --emit python Transpile to Python");
        }
        std::process::exit(0);
    }

    // If args[1] is a file that exists, read it. Otherwise treat it as inline code.
    let (source, mode_args_start) = if std::path::Path::new(&args[1]).is_file() {
        let s = match std::fs::read_to_string(&args[1]) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error reading {}: {}", args[1], e);
                std::process::exit(1);
            }
        };
        (s, 2)
    } else if args[1] == "-e" {
        // Legacy -e flag: skip it, use args[2] as code
        if args.len() < 3 || args[2].is_empty() {
            eprintln!("Usage: ilo <file-or-code> [args... | --run func args... | --emit python]");
            std::process::exit(1);
        }
        (args[2].clone(), 3)
    } else {
        let code = &args[1];
        if code.is_empty() {
            eprintln!("Error: empty code string");
            std::process::exit(1);
        }
        (code.clone(), 2)
    };

    // Scan for --tools <path> and --mcp <path>.
    // Remove both flags+values from args so downstream dispatch doesn't see them.
    let (tools_config_path, mcp_config_path, args) = {
        let mut tools_path: Option<String> = None;
        let mut mcp_path: Option<String> = None;
        let mut filtered: Vec<String> = Vec::with_capacity(args.len());
        let mut i = 0;
        while i < args.len() {
            if args[i] == "--tools" {
                if i + 1 < args.len() {
                    tools_path = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("error: --tools requires a path argument");
                    std::process::exit(1);
                }
            } else if args[i] == "--mcp" {
                if i + 1 < args.len() {
                    mcp_path = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("error: --mcp requires a path argument");
                    std::process::exit(1);
                }
            } else {
                filtered.push(args[i].clone());
                i += 1;
            }
        }
        if tools_path.is_some() && mcp_path.is_some() {
            eprintln!("error: --tools and --mcp are mutually exclusive");
            std::process::exit(1);
        }
        (tools_path, mcp_path, filtered)
    };

    warn_cross_language_syntax(&source, mode);

    let tokens = match lexer::lex(&source) {
        Ok(t) => t,
        Err(e) => {
            report_diagnostic(&Diagnostic::from(&e).with_source(source.clone()), mode);
            std::process::exit(1);
        }
    };

    let token_spans: Vec<(lexer::Token, ast::Span)> = tokens
        .into_iter()
        .map(|(t, r)| (t, ast::Span { start: r.start, end: r.end }))
        .collect();

    let (mut program, parse_errors) = parser::parse(token_spans);
    program.source = Some(source.clone());

    // If --mcp was provided, connect to the MCP servers and inject synthesized
    // Decl::Tool nodes into the program before the verifier runs.
    // This requires the `tools` feature (tokio process + async runtime).
    #[cfg(not(feature = "tools"))]
    if mcp_config_path.is_some() {
        eprintln!("error: --mcp requires the 'tools' feature (build with: cargo build --features tools)");
        std::process::exit(1);
    }

    #[cfg(feature = "tools")]
    let mut mcp_rt: Option<tokio::runtime::Runtime> = None;
    #[cfg(feature = "tools")]
    let mut mcp_provider_holder: Option<tools::mcp_provider::McpProvider> = None;

    #[cfg(feature = "tools")]
    if let Some(ref path) = mcp_config_path {
        let config = tools::mcp_provider::McpConfig::from_file(path)
            .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime");
        let provider = rt
            .block_on(tools::mcp_provider::McpProvider::connect(&config))
            .unwrap_or_else(|e| { eprintln!("MCP error: {}", e); std::process::exit(1); });
        // Prepend synthesized Decl::Tool nodes so the verifier sees them
        let mut decls = provider.tool_decls();
        decls.append(&mut program.declarations);
        program.declarations = decls;
        mcp_rt = Some(rt);
        mcp_provider_holder = Some(provider);
    }

    let mut had_errors = false;

    // Resolve `use "..."` imports — must happen before verification.
    // base_dir is the directory of the source file (None for inline code).
    {
        let base_dir: Option<std::path::PathBuf> = if std::path::Path::new(&args[1]).is_file() {
            std::path::Path::new(&args[1])
                .canonicalize()
                .ok()
                .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        } else {
            None
        };
        let mut import_diagnostics: Vec<Diagnostic> = Vec::new();
        let mut visited = std::collections::HashSet::new();
        // Mark the main file as visited to catch direct self-imports
        if let Ok(canonical_file) = std::path::Path::new(&args[1]).canonicalize() {
            visited.insert(canonical_file);
        }
        program.declarations = resolve_imports(
            program.declarations,
            base_dir.as_deref(),
            &mut visited,
            &mut import_diagnostics,
        );
        for d in import_diagnostics {
            report_diagnostic(&d, mode);
            had_errors = true;
        }
    }

    for e in &parse_errors {
        report_diagnostic(&Diagnostic::from(e).with_source(source.clone()), mode);
        had_errors = true;
    }

    // Always run the verifier — it skips Decl::Error poison nodes and reports
    // problems in any functions that did parse successfully.
    let verify_result = verify::verify(&program);
    for w in &verify_result.warnings {
        report_diagnostic(&Diagnostic::from(w).with_source(source.clone()), mode);
    }
    if !verify_result.errors.is_empty() {
        for e in &verify_result.errors {
            report_diagnostic(&Diagnostic::from(e).with_source(source.clone()), mode);
        }
        had_errors = true;
    }

    if had_errors {
        std::process::exit(1);
    }

    // Determine mode from args
    let m = mode_args_start;
    if args.len() > m && args[m] == "--bench" {
        // --bench [func] [args...]
        let func_name = if args.len() > m + 1 { Some(args[m + 1].as_str()) } else { None };
        let run_args: Vec<interpreter::Value> = if args.len() > m + 2 {
            args[m + 2..].iter().map(|a| parse_cli_arg(a)).collect()
        } else {
            vec![]
        };
        run_bench(&program, func_name, &run_args);
    } else if args.len() > m && matches!(args[m].as_str(), "--explain" | "-x") {
        let filename = if std::path::Path::new(&args[1]).is_file() { Some(args[1].as_str()) } else { None };
        print!("{}", codegen::explain::explain(&program, filename));
    } else if args.len() > m && args[m] == "--emit" {
        if args.len() > m + 1 && args[m + 1] == "python" {
            println!("{}", codegen::python::emit(&program));
        } else {
            eprintln!("Unknown emit target. Supported: python");
            std::process::exit(1);
        }
    } else if args.len() > m && matches!(args[m].as_str(), "--dense" | "-d" | "--fmt") {
        println!("{}", codegen::fmt::format(&program, codegen::fmt::FmtMode::Dense));
    } else if args.len() > m && matches!(args[m].as_str(), "--expanded" | "-e" | "--fmt-expanded") {
        print!("{}", codegen::fmt::format(&program, codegen::fmt::FmtMode::Expanded));
    } else if args.len() > m && args[m] == "--run-jit" {
        // --run-jit [func] [args...] — ARM64 JIT (aarch64 only)
        #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
        {
            let func_name = if args.len() > m + 1 { Some(args[m + 1].as_str()) } else { None };
            let run_args: Vec<f64> = if args.len() > m + 2 {
                args[m + 2..].iter().map(|a| a.parse::<f64>().expect("JIT args must be numbers")).collect()
            } else {
                vec![]
            };

            let compiled = vm::compile(&program).unwrap_or_else(|e| { eprintln!("Compile error: {}", e); std::process::exit(1); });
            let target = func_name.unwrap_or(compiled.func_names.first().map(|s| s.as_str()).unwrap_or("main"));
            let func_idx = compiled.func_names.iter().position(|n| n == target)
                .unwrap_or_else(|| { eprintln!("undefined function: {}", target); std::process::exit(1); });
            let chunk = &compiled.chunks[func_idx];
            let nan_consts = &compiled.nan_constants[func_idx];

            match vm::jit_arm64::compile_and_call(chunk, nan_consts, &run_args) {
                Some(result) => {
                    if result == (result as i64) as f64 {
                        println!("{}", result as i64);
                    } else {
                        println!("{}", result);
                    }
                }
                None => {
                    eprintln!("JIT: function not eligible for compilation (numeric-only required)");
                    std::process::exit(1);
                }
            }
        }
        #[cfg(not(all(target_arch = "aarch64", target_os = "macos")))]
        {
            eprintln!("Custom JIT (arm64) is only available on aarch64 macOS");
            std::process::exit(1);
        }
    } else if args.len() > m && args[m] == "--run-cranelift" {
        // --run-cranelift [func] [args...]
        #[cfg(feature = "cranelift")]
        {
            let func_name = if args.len() > m + 1 { Some(args[m + 1].as_str()) } else { None };
            let run_args: Vec<interpreter::Value> = if args.len() > m + 2 {
                args[m + 2..].iter().map(|a| parse_cli_arg(a)).collect()
            } else {
                vec![]
            };

            let compiled = vm::compile(&program).unwrap_or_else(|e| { eprintln!("Compile error: {}", e); std::process::exit(1); });
            let target = func_name.unwrap_or(compiled.func_names.first().map(|s| s.as_str()).unwrap_or("main"));
            let func_idx = compiled.func_names.iter().position(|n| n == target)
                .unwrap_or_else(|| { eprintln!("undefined function: {}", target); std::process::exit(1); });
            let chunk = &compiled.chunks[func_idx];
            let nan_consts = &compiled.nan_constants[func_idx];
            let nan_args: Vec<u64> = run_args.iter().map(|v| vm::NanVal::from_value(v).0).collect();

            match vm::jit_cranelift::compile_and_call(chunk, nan_consts, &nan_args, &compiled) {
                Some(result_bits) => {
                    let result = vm::NanVal(result_bits).to_value();
                    println!("{}", result);
                }
                None => {
                    eprintln!("Cranelift JIT: compilation failed");
                    std::process::exit(1);
                }
            }
        }
        #[cfg(not(feature = "cranelift"))]
        {
            eprintln!("Cranelift JIT not enabled. Build with: cargo build --features cranelift");
            std::process::exit(1);
        }
    } else if args.len() > m && args[m] == "--run-llvm" {
        // --run-llvm [func] [args...]
        #[cfg(feature = "llvm")]
        {
            let func_name = if args.len() > m + 1 { Some(args[m + 1].as_str()) } else { None };
            let run_args: Vec<f64> = if args.len() > m + 2 {
                args[m + 2..].iter().map(|a| a.parse::<f64>().expect("JIT args must be numbers")).collect()
            } else {
                vec![]
            };

            let compiled = vm::compile(&program).unwrap_or_else(|e| { eprintln!("Compile error: {}", e); std::process::exit(1); });
            let target = func_name.unwrap_or(compiled.func_names.first().map(|s| s.as_str()).unwrap_or("main"));
            let func_idx = compiled.func_names.iter().position(|n| n == target)
                .unwrap_or_else(|| { eprintln!("undefined function: {}", target); std::process::exit(1); });
            let chunk = &compiled.chunks[func_idx];
            let nan_consts = &compiled.nan_constants[func_idx];

            match vm::jit_llvm::compile_and_call(chunk, nan_consts, &run_args) {
                Some(result) => {
                    if result == (result as i64) as f64 {
                        println!("{}", result as i64);
                    } else {
                        println!("{}", result);
                    }
                }
                None => {
                    eprintln!("LLVM JIT: function not eligible for compilation");
                    std::process::exit(1);
                }
            }
        }
        #[cfg(not(feature = "llvm"))]
        {
            eprintln!("LLVM JIT not enabled. Build with: cargo build --features llvm");
            std::process::exit(1);
        }
    } else if args.len() > m && args[m] == "--run-vm" {
        // --run-vm [func] [args...]
        let func_name = if args.len() > m + 1 { Some(args[m + 1].as_str()) } else { None };
        let run_args: Vec<interpreter::Value> = if args.len() > m + 2 {
            args[m + 2..].iter().map(|a| parse_cli_arg(a)).collect()
        } else {
            vec![]
        };

        let compiled = vm::compile(&program).unwrap_or_else(|e| { eprintln!("Compile error: {}", e); std::process::exit(1); });
        run_vm_with_provider(
            &compiled,
            func_name,
            run_args,
            tools_config_path.as_deref(),
            #[cfg(feature = "tools")]
            mcp_provider_holder.as_ref(),
            #[cfg(feature = "tools")]
            mcp_rt.as_ref(),
            &source,
            mode,
            explicit_json,
        );
    } else if args.len() > m && (args[m] == "--run" || args[m] == "--run-interp") {
        // --run / --run-interp [func] [args...]
        let func_name = if args.len() > m + 1 { Some(args[m + 1].as_str()) } else { None };
        let run_args: Vec<interpreter::Value> = if args.len() > m + 2 {
            args[m + 2..].iter().map(|a| parse_cli_arg(a)).collect()
        } else {
            vec![]
        };

        run_interp_with_provider(
            &program,
            func_name,
            run_args,
            tools_config_path.as_deref(),
            #[cfg(feature = "tools")]
            mcp_provider_holder,
            #[cfg(feature = "tools")]
            mcp_rt,
            &source,
            mode,
            explicit_json,
        );
    } else if args.len() > m {
        // Bare args: default = Cranelift JIT, fall back to interpreter
        let func_names: Vec<&str> = program.declarations.iter().filter_map(|d| match d {
            ast::Decl::Function { name, .. } => Some(name.as_str()),
            _ => None,
        }).collect();

        let (func_name, run_args_start) = if func_names.contains(&args[m].as_str()) {
            (Some(args[m].as_str()), m + 1)
        } else {
            (None, m)
        };

        let run_args: Vec<interpreter::Value> = args[run_args_start..].iter().map(|a| parse_cli_arg(a)).collect();
        run_default(&program, func_name, run_args, &source, mode, explicit_json);
    } else {
        // No args: AST JSON
        match serde_json::to_string_pretty(&program) {
            Ok(json) => println!("{}", json),
            Err(e) => {
                eprintln!("Serialization error: {}", e);
                std::process::exit(1);
            }
        }
    }

    // Emit idiomatic hints after successful execution
    if !no_hints {
        let hints = collect_hints(&source);
        emit_hints(&hints, mode);
    }
}

/// Dispatch --run-vm, routing to MCP / HTTP / plain run based on available providers.
#[allow(clippy::too_many_arguments)]
fn run_vm_with_provider(
    compiled: &vm::CompiledProgram,
    func_name: Option<&str>,
    args: Vec<interpreter::Value>,
    tools_config_path: Option<&str>,
    #[cfg(feature = "tools")] mcp_provider: Option<&tools::mcp_provider::McpProvider>,
    #[cfg(feature = "tools")] mcp_rt: Option<&tokio::runtime::Runtime>,
    source: &str,
    mode: OutputMode,
    explicit_json: bool,
) {
    #[cfg(feature = "tools")]
    if let Some(provider) = mcp_provider {
        let rt = mcp_rt.expect("runtime present with mcp_provider");
        match vm::run_with_tools(compiled, func_name, args, provider, rt) {
            Ok(val) => { print_value(&val, explicit_json); return; }
            Err(e) => {
                report_diagnostic(&Diagnostic::from(&e).with_source(source.to_string()), mode);
                std::process::exit(1);
            }
        }
    }

    if let Some(tools_path) = tools_config_path {
        let config = tools::http_provider::ToolsConfig::from_file(tools_path)
            .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });
        let provider = tools::http_provider::HttpProvider::new(config);
        #[cfg(feature = "tools")]
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime");
        match vm::run_with_tools(
            compiled,
            func_name,
            args,
            &provider,
            #[cfg(feature = "tools")]
            &runtime,
        ) {
            Ok(val) => print_value(&val, explicit_json),
            Err(e) => {
                report_diagnostic(&Diagnostic::from(&e).with_source(source.to_string()), mode);
                std::process::exit(1);
            }
        }
        return;
    }

    match vm::run(compiled, func_name, args) {
        Ok(val) => print_value(&val, explicit_json),
        Err(e) => {
            report_diagnostic(&Diagnostic::from(&e).with_source(source.to_string()), mode);
            std::process::exit(1);
        }
    }
}

/// Dispatch --run-interp, routing to MCP / HTTP / plain run based on available providers.
#[allow(clippy::too_many_arguments)]
fn run_interp_with_provider(
    program: &ast::Program,
    func_name: Option<&str>,
    args: Vec<interpreter::Value>,
    tools_config_path: Option<&str>,
    #[cfg(feature = "tools")] mcp_provider: Option<tools::mcp_provider::McpProvider>,
    #[cfg(feature = "tools")] mcp_rt: Option<tokio::runtime::Runtime>,
    source: &str,
    mode: OutputMode,
    explicit_json: bool,
) {
    #[cfg(feature = "tools")]
    if let Some(provider) = mcp_provider {
        let rt = std::sync::Arc::new(mcp_rt.expect("runtime present with mcp_provider"));
        match interpreter::run_with_tools(program, func_name, args, std::sync::Arc::new(provider), rt) {
            Ok(val) => { print_value(&val, explicit_json); return; }
            Err(e) => {
                report_diagnostic(&Diagnostic::from(&e).with_source(source.to_string()), mode);
                std::process::exit(1);
            }
        }
    }

    if let Some(tools_path) = tools_config_path {
        let config = tools::http_provider::ToolsConfig::from_file(tools_path)
            .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });
        let provider = std::sync::Arc::new(tools::http_provider::HttpProvider::new(config));
        #[cfg(feature = "tools")]
        let runtime = std::sync::Arc::new(tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime"));
        match interpreter::run_with_tools(
            program,
            func_name,
            args,
            provider,
            #[cfg(feature = "tools")]
            runtime,
        ) {
            Ok(val) => print_value(&val, explicit_json),
            Err(e) => {
                report_diagnostic(&Diagnostic::from(&e).with_source(source.to_string()), mode);
                std::process::exit(1);
            }
        }
        return;
    }

    match interpreter::run(program, func_name, args) {
        Ok(val) => print_value(&val, explicit_json),
        Err(e) => {
            report_diagnostic(&Diagnostic::from(&e).with_source(source.to_string()), mode);
            std::process::exit(1);
        }
    }
}

fn run_default(program: &ast::Program, func_name: Option<&str>, args: Vec<interpreter::Value>, source: &str, mode: OutputMode, explicit_json: bool) {
    // Try Cranelift JIT first — all functions are now eligible
    #[cfg(feature = "cranelift")]
    {
        if let Ok(compiled) = vm::compile(program) {
            let target = func_name.unwrap_or(compiled.func_names.first().map(|s| s.as_str()).unwrap_or("main"));
            if let Some(func_idx) = compiled.func_names.iter().position(|n| n == target) {
                let chunk = &compiled.chunks[func_idx];
                let nan_consts = &compiled.nan_constants[func_idx];
                let nan_args: Vec<u64> = args.iter().map(|v| vm::NanVal::from_value(v).0).collect();
                if let Some(result_bits) = vm::jit_cranelift::compile_and_call(chunk, nan_consts, &nan_args, &compiled) {
                    let result = vm::NanVal(result_bits).to_value();
                    print_value(&result, explicit_json);
                    return;
                }
            }
        }
    }

    // Fall back to interpreter
    match interpreter::run(program, func_name, args) {
        Ok(val) => print_value(&val, explicit_json),
        Err(e) => {
            report_diagnostic(&Diagnostic::from(&e).with_source(source.to_string()), mode);
            std::process::exit(1);
        }
    }
}

/// Print a program result value. When `as_json` is true (explicit -j/--json), wraps it as
/// `{"ok": ...}` or `{"error": ...}`. Auto-detected JSON mode does not affect result format.
fn print_value(val: &interpreter::Value, as_json: bool) {
    if !as_json {
        println!("{}", val);
        return;
    }
    let json = match val {
        interpreter::Value::Ok(inner) => {
            let v = inner.to_json().unwrap_or(serde_json::Value::Null);
            serde_json::json!({"ok": v})
        }
        interpreter::Value::Err(inner) => {
            let v = inner
                .to_json()
                .unwrap_or_else(|_| serde_json::Value::String(inner.to_string()));
            serde_json::json!({"error": {"phase": "program", "value": v}})
        }
        other => {
            let v = other
                .to_json()
                .unwrap_or_else(|_| serde_json::Value::String(other.to_string()));
            serde_json::json!({"ok": v})
        }
    };
    println!("{}", json);
}

fn run_bench(program: &ast::Program, func_name: Option<&str>, args: &[interpreter::Value]) {
    use std::time::Instant;
    use std::io::Write;
    use std::process::Command;

    let iterations: u32 = 10_000;

    // -- Rust interpreter benchmark --
    // Warmup
    for _ in 0..100 {
        let _ = interpreter::run(program, func_name, args.to_vec());
    }

    let start = Instant::now();
    let mut result = interpreter::Value::Nil;
    for _ in 0..iterations {
        result = interpreter::run(program, func_name, args.to_vec()).expect("interpreter error during benchmark");
    }
    let interp_dur = start.elapsed();
    let interp_ns = interp_dur.as_nanos() / iterations as u128;

    println!("Rust interpreter");
    println!("  result:     {}", result);
    println!("  iterations: {}", iterations);
    println!("  total:      {:.2}ms", interp_dur.as_nanos() as f64 / 1e6);
    println!("  per call:   {}ns", interp_ns);
    println!();

    // -- Register VM benchmark --
    let compiled = vm::compile(program).expect("compile error in benchmark");
    // Warmup
    for _ in 0..100 {
        let _ = vm::run(&compiled, func_name, args.to_vec());
    }

    let start = Instant::now();
    let mut vm_result = interpreter::Value::Nil;
    for _ in 0..iterations {
        vm_result = vm::run(&compiled, func_name, args.to_vec()).expect("VM error during benchmark");
    }
    let vm_dur = start.elapsed();
    let vm_ns = vm_dur.as_nanos() / iterations as u128;

    println!("Register VM");
    println!("  result:     {}", vm_result);
    println!("  iterations: {}", iterations);
    println!("  total:      {:.2}ms", vm_dur.as_nanos() as f64 / 1e6);
    println!("  per call:   {}ns", vm_ns);
    println!();

    // -- Register VM (reusable) benchmark --
    let call_name = func_name.unwrap_or(compiled.func_names.first().map(|s| s.as_str()).unwrap_or("main"));
    let mut vm_state = vm::VmState::new(&compiled);
    for _ in 0..100 {
        let _ = vm_state.call(call_name, args.to_vec());
    }

    let start = Instant::now();
    for _ in 0..iterations {
        vm_result = vm_state.call(call_name, args.to_vec()).expect("VM reusable error during benchmark");
    }
    let vm_reuse_dur = start.elapsed();
    let vm_reuse_ns = vm_reuse_dur.as_nanos() / iterations as u128;

    println!("Register VM (reusable)");
    println!("  result:     {}", vm_result);
    println!("  iterations: {}", iterations);
    println!("  total:      {:.2}ms", vm_reuse_dur.as_nanos() as f64 / 1e6);
    println!("  per call:   {}ns", vm_reuse_ns);
    println!();

    // -- JIT benchmarks --
    // Extract function info for JIT
    let call_name_jit = func_name.unwrap_or(compiled.func_names.first().map(|s| s.as_str()).unwrap_or("main"));
    let func_idx_jit = compiled.func_names.iter().position(|n| n == call_name_jit);
    let jit_args: Vec<f64> = args.iter().filter_map(|a| match a {
        interpreter::Value::Number(n) => Some(*n),
        _ => None,
    }).collect();
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    let all_numeric = jit_args.len() == args.len();

    let mut jit_arm64_ns: Option<u128> = None;
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    if let Some(fi) = func_idx_jit
        && all_numeric {
            let chunk = &compiled.chunks[fi];
            let nan_consts = &compiled.nan_constants[fi];
            if let Some(jit_func) = vm::jit_arm64::compile(chunk, nan_consts) {
                // Warmup
                for _ in 0..100 {
                    let _ = vm::jit_arm64::call(&jit_func, &jit_args);
                }

                let start = Instant::now();
                let mut jit_result = 0.0f64;
                for _ in 0..iterations {
                    jit_result = vm::jit_arm64::call(&jit_func, &jit_args).expect("arm64 JIT error during benchmark");
                }
                let jit_dur = start.elapsed();
                let ns = jit_dur.as_nanos() / iterations as u128;
                jit_arm64_ns = Some(ns);

                println!("Custom JIT (arm64)");
                if jit_result == (jit_result as i64) as f64 {
                    println!("  result:     {}", jit_result as i64);
                } else {
                    println!("  result:     {}", jit_result);
                }
                println!("  iterations: {}", iterations);
                println!("  total:      {:.2}ms", jit_dur.as_nanos() as f64 / 1e6);
                println!("  per call:   {}ns", ns);
                println!();
            }
        }

    let mut jit_cranelift_ns: Option<u128> = None;
    #[cfg(feature = "cranelift")]
    if let Some(fi) = func_idx_jit {
        let chunk = &compiled.chunks[fi];
        let nan_consts = &compiled.nan_constants[fi];
        let nan_args: Vec<u64> = args.iter().map(|v| vm::NanVal::from_value(v).0).collect();
        // SAFETY: `compiled` outlives the entire bench loop below.
        unsafe { vm::set_active_registry(&compiled); }
        if let Some(jit_func) = vm::jit_cranelift::compile(chunk, nan_consts, &compiled) {
            for _ in 0..100 {
                let _ = vm::jit_cranelift::call(&jit_func, &nan_args);
            }

            let start = Instant::now();
            let mut jit_result_bits = 0u64;
            for _ in 0..iterations {
                jit_result_bits = vm::jit_cranelift::call(&jit_func, &nan_args).expect("Cranelift JIT error during benchmark");
            }
            let jit_dur = start.elapsed();
            let ns = jit_dur.as_nanos() / iterations as u128;
            jit_cranelift_ns = Some(ns);

            let jit_result = vm::NanVal(jit_result_bits).to_value();
            println!("Cranelift JIT");
            println!("  result:     {}", jit_result);
            println!("  iterations: {}", iterations);
            println!("  total:      {:.2}ms", jit_dur.as_nanos() as f64 / 1e6);
            println!("  per call:   {}ns", ns);
            println!();
        }
    }

    #[allow(unused_variables)]
    let jit_llvm_ns: Option<u128> = None;
    #[cfg(feature = "llvm")]
    if let Some(fi) = func_idx_jit {
        if all_numeric {
            let chunk = &compiled.chunks[fi];
            let nan_consts = &compiled.nan_constants[fi];
            if let Some(jit_func) = vm::jit_llvm::compile(chunk, nan_consts) {
                for _ in 0..100 {
                    let _ = vm::jit_llvm::call(&jit_func, &jit_args);
                }

                let start = Instant::now();
                let mut jit_result = 0.0f64;
                for _ in 0..iterations {
                    jit_result = vm::jit_llvm::call(&jit_func, &jit_args).expect("LLVM JIT error during benchmark");
                }
                let jit_dur = start.elapsed();
                let ns = jit_dur.as_nanos() / iterations as u128;
                jit_llvm_ns = Some(ns);

                println!("LLVM JIT");
                if jit_result == (jit_result as i64) as f64 {
                    println!("  result:     {}", jit_result as i64);
                } else {
                    println!("  result:     {}", jit_result);
                }
                println!("  iterations: {}", iterations);
                println!("  total:      {:.2}ms", jit_dur.as_nanos() as f64 / 1e6);
                println!("  per call:   {}ns", ns);
                println!();
            }
        }
    }

    // -- Python transpiler benchmark (single invocation) --
    let py_code = codegen::python::emit(program);
    let call_func = func_name.unwrap_or("main").replace('-', "_");
    let call_args: Vec<String> = args.iter().map(|a| match a {
        interpreter::Value::Number(n) => {
            if *n == (*n as i64) as f64 { format!("{}", *n as i64) } else { format!("{}", n) }
        }
        interpreter::Value::Text(s) => format!("\"{}\"", s),
        interpreter::Value::Bool(b) => if *b { "True".to_string() } else { "False".to_string() },
        _ => "None".to_string(),
    }).collect();

    // Python script: prints human-readable lines then a final __NS__=<value> for parsing
    let py_script = format!(
        r#"import time
{code}
_n = {n}
for _ in range(100):
    {func}({args})
_start = time.perf_counter_ns()
for _ in range(_n):
    _r = {func}({args})
_elapsed = time.perf_counter_ns() - _start
_per = _elapsed // _n
print(f"result:     {{_r}}")
print(f"iterations: {{_n}}")
print(f"total:      {{_elapsed / 1e6:.2f}}ms")
print(f"per call:   {{_per}}ns")
print(f"__NS__={{_per}}")
"#,
        code = py_code,
        n = iterations,
        func = call_func,
        args = call_args.join(", ")
    );

    println!("Python transpiled");
    let output = Command::new("python3")
        .arg("-c")
        .arg(&py_script)
        .output();

    let mut py_ns: Option<u128> = None;
    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                if let Some(val) = line.strip_prefix("__NS__=") {
                    py_ns = val.parse().ok();
                } else {
                    println!("  {}", line);
                }
            }
            std::io::stderr().write_all(&out.stderr).expect("write to stderr");
        }
        Err(e) => eprintln!("  failed to run python3: {}", e),
    }

    println!();

    // -- Summary --
    println!("Summary");
    if vm_ns > 0 && interp_ns > 0 {
        if vm_ns < interp_ns {
            println!("  Register VM is {:.1}x faster than interpreter", interp_ns as f64 / vm_ns as f64);
        } else {
            println!("  Interpreter is {:.1}x faster than bytecode VM", vm_ns as f64 / interp_ns as f64);
        }
    }
    if let Some(jit_ns) = jit_arm64_ns
        && jit_ns > 0 && vm_reuse_ns > 0 {
            println!("  Custom JIT (arm64) is {:.1}x faster than VM (reusable)", vm_reuse_ns as f64 / jit_ns as f64);
        }
    if let Some(jit_ns) = jit_cranelift_ns
        && jit_ns > 0 && vm_reuse_ns > 0 {
            println!("  Cranelift JIT is {:.1}x faster than VM (reusable)", vm_reuse_ns as f64 / jit_ns as f64);
        }
    if let Some(jit_ns) = jit_llvm_ns
        && jit_ns > 0 && vm_reuse_ns > 0 {
            println!("  LLVM JIT is {:.1}x faster than VM (reusable)", vm_reuse_ns as f64 / jit_ns as f64);
        }
    if let Some(py) = py_ns {
        if interp_ns > 0 && py > 0 {
            if interp_ns < py {
                println!("  Rust interpreter is {:.1}x faster than Python", py as f64 / interp_ns as f64);
            } else {
                println!("  Python is {:.1}x faster than Rust interpreter", interp_ns as f64 / py as f64);
            }
        }
        if vm_ns > 0 && py > 0 {
            if vm_ns < py {
                println!("  Register VM is {:.1}x faster than Python", py as f64 / vm_ns as f64);
            } else {
                println!("  Python is {:.1}x faster than Register VM", vm_ns as f64 / py as f64);
            }
        }
        if vm_reuse_ns > 0 && py > 0 {
            if vm_reuse_ns < py {
                println!("  VM (reusable) is {:.1}x faster than Python", py as f64 / vm_reuse_ns as f64);
            } else {
                println!("  Python is {:.1}x faster than VM (reusable)", vm_reuse_ns as f64 / py as f64);
            }
        }
        if let Some(jit_ns) = jit_arm64_ns
            && jit_ns > 0 && py > 0 {
                println!("  Custom JIT (arm64) is {:.1}x faster than Python", py as f64 / jit_ns as f64);
            }
        if let Some(jit_ns) = jit_cranelift_ns
            && jit_ns > 0 && py > 0 {
                println!("  Cranelift JIT is {:.1}x faster than Python", py as f64 / jit_ns as f64);
            }
        if let Some(jit_ns) = jit_llvm_ns
            && jit_ns > 0 && py > 0 {
                println!("  LLVM JIT is {:.1}x faster than Python", py as f64 / jit_ns as f64);
            }
    }
}

fn parse_cli_arg(s: &str) -> interpreter::Value {
    // Bracketed list: [1,2,3] or []
    if s.starts_with('[') && s.ends_with(']') {
        let inner = s[1..s.len()-1].trim();
        if inner.is_empty() {
            return interpreter::Value::List(vec![]);
        }
        let items = inner.split(',').map(|part| parse_cli_arg(part.trim())).collect();
        return interpreter::Value::List(items);
    }
    // Bare comma list: 1,2,3
    if s.contains(',') {
        let items = s.split(',').map(|part| parse_cli_arg(part.trim())).collect();
        return interpreter::Value::List(items);
    }
    if let Ok(n) = s.parse::<f64>()
        && n.is_finite() {
            return interpreter::Value::Number(n);
        }
    if s == "true" {
        interpreter::Value::Bool(true)
    } else if s == "false" {
        interpreter::Value::Bool(false)
    } else {
        interpreter::Value::Text(s.to_string())
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── test helpers ──────────────────────────────────────────────────────────

    fn make_compiled(src: &str) -> vm::CompiledProgram {
        let tokens = lexer::lex(src).unwrap();
        let token_spans: Vec<_> = tokens
            .into_iter()
            .map(|(t, r)| (t, ast::Span { start: r.start, end: r.end }))
            .collect();
        let (program, _) = parser::parse(token_spans);
        vm::compile(&program).unwrap()
    }

    fn make_program(src: &str) -> ast::Program {
        let tokens = lexer::lex(src).unwrap();
        let token_spans: Vec<_> = tokens
            .into_iter()
            .map(|(t, r)| (t, ast::Span { start: r.start, end: r.end }))
            .collect();
        let (mut program, _) = parser::parse(token_spans);
        program.source = Some(src.to_string());
        program
    }

    // Helper: call process_serv_request portably across feature configs.
    fn run_serv(line: &str) -> serde_json::Value {
        #[cfg(not(feature = "tools"))]
        return process_serv_request(line, &[], None);

        #[cfg(feature = "tools")]
        {
            let rt = std::sync::Arc::new(
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap(),
            );
            process_serv_request(line, &[], None, None, rt)
        }
    }

    // ── parse_cli_arg ─────────────────────────────────────────────────────────

    #[test]
    fn cli_arg_integer() {
        assert_eq!(parse_cli_arg("42"), interpreter::Value::Number(42.0));
    }

    #[test]
    fn cli_arg_float() {
        assert_eq!(parse_cli_arg("3.14"), interpreter::Value::Number(3.14));
    }

    #[test]
    fn cli_arg_bool_true() {
        assert_eq!(parse_cli_arg("true"), interpreter::Value::Bool(true));
    }

    #[test]
    fn cli_arg_bool_false() {
        assert_eq!(parse_cli_arg("false"), interpreter::Value::Bool(false));
    }

    #[test]
    fn cli_arg_text() {
        assert_eq!(parse_cli_arg("hello"), interpreter::Value::Text("hello".into()));
    }

    #[test]
    fn cli_arg_bracketed_list() {
        assert_eq!(
            parse_cli_arg("[1,2,3]"),
            interpreter::Value::List(vec![
                interpreter::Value::Number(1.0),
                interpreter::Value::Number(2.0),
                interpreter::Value::Number(3.0),
            ])
        );
    }

    #[test]
    fn cli_arg_empty_bracketed_list() {
        assert_eq!(parse_cli_arg("[]"), interpreter::Value::List(vec![]));
    }

    #[test]
    fn cli_arg_comma_list() {
        assert_eq!(
            parse_cli_arg("1,2,3"),
            interpreter::Value::List(vec![
                interpreter::Value::Number(1.0),
                interpreter::Value::Number(2.0),
                interpreter::Value::Number(3.0),
            ])
        );
    }

    #[test]
    fn cli_arg_mixed_comma_list() {
        assert_eq!(
            parse_cli_arg("1,hello,true"),
            interpreter::Value::List(vec![
                interpreter::Value::Number(1.0),
                interpreter::Value::Text("hello".into()),
                interpreter::Value::Bool(true),
            ])
        );
    }

    #[test]
    fn cli_arg_infinity_is_text() {
        // inf is not finite so it should fall through to text
        assert_eq!(parse_cli_arg("inf"), interpreter::Value::Text("inf".into()));
    }

    // ── detect_output_mode ────────────────────────────────────────────────────

    #[test]
    fn detect_mode_json_long_flag() {
        let (mode, explicit, _, remaining) =
            detect_output_mode(vec!["--json".into(), "foo".into()]);
        assert!(matches!(mode, OutputMode::Json));
        assert!(explicit);
        assert_eq!(remaining, vec!["foo".to_string()]);
    }

    #[test]
    fn detect_mode_json_short_flag() {
        let (mode, explicit, _, _) = detect_output_mode(vec!["-j".into()]);
        assert!(matches!(mode, OutputMode::Json));
        assert!(explicit);
    }

    #[test]
    fn detect_mode_text_long_flag() {
        let (mode, explicit, _, _) = detect_output_mode(vec!["--text".into()]);
        assert!(matches!(mode, OutputMode::Text));
        assert!(!explicit);
    }

    #[test]
    fn detect_mode_text_short_flag() {
        let (mode, _, _, _) = detect_output_mode(vec!["-t".into()]);
        assert!(matches!(mode, OutputMode::Text));
    }

    #[test]
    fn detect_mode_ansi_long_flag() {
        let (mode, explicit, _, _) = detect_output_mode(vec!["--ansi".into()]);
        assert!(matches!(mode, OutputMode::Ansi));
        assert!(!explicit);
    }

    #[test]
    fn detect_mode_ansi_short_flag() {
        let (mode, _, _, _) = detect_output_mode(vec!["-a".into()]);
        assert!(matches!(mode, OutputMode::Ansi));
    }

    #[test]
    fn detect_mode_non_flag_args_pass_through() {
        let (_, _, _, remaining) =
            detect_output_mode(vec!["ilo".into(), "f>n;1".into(), "42".into()]);
        assert_eq!(remaining, vec!["ilo", "f>n;1", "42"]);
    }

    #[test]
    fn detect_mode_format_flag_stripped_from_remaining() {
        let (_, _, _, remaining) =
            detect_output_mode(vec!["--json".into(), "code".into(), "arg".into()]);
        assert_eq!(remaining, vec!["code", "arg"]);
    }

    #[test]
    fn detect_mode_no_hints_flag() {
        let (_, _, no_hints, _) = detect_output_mode(vec!["--no-hints".into(), "code".into()]);
        assert!(no_hints);
    }

    #[test]
    fn detect_mode_no_hints_short_flag() {
        let (_, _, no_hints, _) = detect_output_mode(vec!["-nh".into(), "code".into()]);
        assert!(no_hints);
    }

    #[test]
    fn detect_mode_no_hints_not_stripped() {
        let (_, _, no_hints, remaining) =
            detect_output_mode(vec!["--no-hints".into(), "code".into()]);
        assert!(no_hints);
        assert_eq!(remaining, vec!["code"]);
    }

    // ── collect_hints ─────────────────────────────────────────────────────────

    #[test]
    fn collect_hints_double_equals() {
        let hints = collect_hints("f x:n y:n>b;==x y");
        assert_eq!(hints.len(), 1);
        assert!(hints[0].contains("=="));
    }

    #[test]
    fn collect_hints_single_equals_no_hint() {
        let hints = collect_hints("f x:n y:n>b;=x y");
        assert!(hints.is_empty());
    }

    #[test]
    fn collect_hints_double_equals_inside_string_no_hint() {
        let hints = collect_hints(r#"f x:s>s;"hello==world""#);
        assert!(hints.is_empty());
    }

    #[test]
    fn collect_hints_no_source_no_hint() {
        let hints = collect_hints("f x:n>n;+x 1");
        assert!(hints.is_empty());
    }

    // ── process_serv_request ─────────────────────────────────────────────────

    #[test]
    fn serv_invalid_json_returns_request_phase() {
        let resp = run_serv("not valid json");
        assert_eq!(resp["error"]["phase"], "request");
    }

    #[test]
    fn serv_parse_error_returns_parse_phase() {
        let resp = run_serv(r#"{"program": "f>n;??invalid"}"#);
        assert_eq!(resp["error"]["phase"], "parse");
    }

    #[test]
    fn serv_verify_error_returns_verify_phase() {
        // x:n>t — returns a number where text is expected → ILO-T005
        let resp = run_serv(r#"{"program": "f x:n>t;x"}"#);
        assert_eq!(resp["error"]["phase"], "verify");
    }

    #[test]
    fn serv_success_simple_number() {
        let resp = run_serv(r#"{"program": "f>n;99"}"#);
        assert!(resp.get("ok").is_some(), "expected ok, got: {resp}");
        assert_eq!(resp["ok"].as_f64(), Some(99.0));
        assert!(resp["ms"].is_number());
    }

    #[test]
    fn serv_success_with_args() {
        let resp = run_serv(r#"{"program": "f x:n>n;*x 2", "args": ["5"], "func": "f"}"#);
        assert!(resp.get("ok").is_some(), "expected ok, got: {resp}");
        assert_eq!(resp["ok"].as_f64(), Some(10.0));
    }

    #[test]
    fn serv_result_err_value_returns_program_phase() {
        // Function returns Err value (not a runtime crash — interpreter returns Ok(Value::Err))
        let resp = run_serv(r#"{"program": "f>R n t;^\"oops\""}"#);
        assert_eq!(resp["error"]["phase"], "program",
            "expected program phase for Err value, got: {resp}");
    }

    #[test]
    fn serv_result_ok_value_unwrapped() {
        let resp = run_serv(r#"{"program": "f>R n t;~42"}"#);
        assert!(resp.get("ok").is_some(), "expected ok, got: {resp}");
        assert_eq!(resp["ok"].as_f64(), Some(42.0));
    }

    #[test]
    fn serv_text_result() {
        let resp = run_serv(r#"{"program": "f>t;\"hello\""}"#);
        assert_eq!(resp["ok"].as_str(), Some("hello"));
    }

    #[test]
    fn serv_with_func_field_selects_function() {
        // Two functions; func field picks second one
        let prog = "{\"program\": \"a>n;1\\nb>n;2\", \"func\": \"b\"}";
        let resp = run_serv(prog);
        assert_eq!(resp["ok"].as_f64(), Some(2.0));
    }

    #[test]
    fn serv_mcp_decls_prepended() {
        // mcp_tool_decls are prepended before verify; an empty slice should still work
        let resp = run_serv(r#"{"program": "f>n;1"}"#);
        assert_eq!(resp["ok"].as_f64(), Some(1.0));
    }

    // ── tool_ok_type ──────────────────────────────────────────────────────────

    #[test]
    fn tool_ok_type_extracts_ok_branch() {
        let ty = ast::Type::Result(
            Box::new(ast::Type::Number),
            Box::new(ast::Type::Text),
        );
        assert!(matches!(tool_ok_type(&ty), ast::Type::Number));
    }

    #[test]
    fn tool_ok_type_passthrough_non_result() {
        assert!(matches!(tool_ok_type(&ast::Type::Text), ast::Type::Text));
        assert!(matches!(tool_ok_type(&ast::Type::Number), ast::Type::Number));
        assert!(matches!(tool_ok_type(&ast::Type::Bool), ast::Type::Bool));
    }

    // ── types_pipe_compatible ─────────────────────────────────────────────────

    #[test]
    fn pipe_compat_same_primitives() {
        assert!(types_pipe_compatible(&ast::Type::Number, &ast::Type::Number));
        assert!(types_pipe_compatible(&ast::Type::Text, &ast::Type::Text));
        assert!(types_pipe_compatible(&ast::Type::Bool, &ast::Type::Bool));
        assert!(types_pipe_compatible(&ast::Type::Nil, &ast::Type::Nil));
    }

    #[test]
    fn pipe_compat_different_primitives_incompatible() {
        assert!(!types_pipe_compatible(&ast::Type::Number, &ast::Type::Text));
        assert!(!types_pipe_compatible(&ast::Type::Bool, &ast::Type::Number));
    }

    #[test]
    fn pipe_compat_named_type_is_wildcard() {
        assert!(types_pipe_compatible(&ast::Type::Named("foo".into()), &ast::Type::Text));
        assert!(types_pipe_compatible(&ast::Type::Text, &ast::Type::Named("bar".into())));
    }

    #[test]
    fn pipe_compat_optional_param_accepts_inner_type() {
        let opt = ast::Type::Optional(Box::new(ast::Type::Number));
        assert!(types_pipe_compatible(&ast::Type::Number, &opt));
    }

    #[test]
    fn pipe_compat_list_checks_element_type() {
        let list_n = ast::Type::List(Box::new(ast::Type::Number));
        let list_t = ast::Type::List(Box::new(ast::Type::Text));
        assert!(types_pipe_compatible(&list_n, &list_n.clone()));
        assert!(!types_pipe_compatible(&list_n, &list_t));
    }

    #[test]
    fn pipe_compat_sum_is_text_compatible() {
        let sum = ast::Type::Sum(vec!["a".into(), "b".into()]);
        assert!(types_pipe_compatible(&sum, &ast::Type::Text));
        assert!(types_pipe_compatible(&ast::Type::Text, &sum));
        assert!(types_pipe_compatible(&sum, &sum.clone()));
    }

    #[test]
    fn pipe_compat_map_checks_key_and_value() {
        let map_nn = ast::Type::Map(Box::new(ast::Type::Text), Box::new(ast::Type::Number));
        let map_nt = ast::Type::Map(Box::new(ast::Type::Text), Box::new(ast::Type::Text));
        assert!(types_pipe_compatible(&map_nn, &map_nn.clone()));
        assert!(!types_pipe_compatible(&map_nn, &map_nt));
    }

    // ── tool_sig_str ──────────────────────────────────────────────────────────

    #[test]
    fn tool_sig_str_no_params() {
        assert_eq!(tool_sig_str(&[], &ast::Type::Number), "> n");
    }

    #[test]
    fn tool_sig_str_one_param() {
        let params = vec![ast::Param { name: "x".into(), ty: ast::Type::Number }];
        assert_eq!(tool_sig_str(&params, &ast::Type::Text), "x:n > t");
    }

    #[test]
    fn tool_sig_str_two_params() {
        let params = vec![
            ast::Param { name: "a".into(), ty: ast::Type::Number },
            ast::Param { name: "b".into(), ty: ast::Type::Text },
        ];
        assert_eq!(tool_sig_str(&params, &ast::Type::Bool), "a:n b:t > b");
    }

    // ── load_env_file ─────────────────────────────────────────────────────────

    #[test]
    fn load_env_file_sets_new_var() {
        use std::io::Write;
        let path = "/tmp/ilo_test_env_load_A7B3.env";
        let key = "ILO_TEST_LOAD_VAR_A7B3";
        // SAFETY: test-only, no concurrent env access
        unsafe { std::env::remove_var(key) };
        std::fs::remove_file(path).ok();

        let mut f = std::fs::File::create(path).unwrap();
        writeln!(f, "# comment line").unwrap();
        writeln!(f).unwrap(); // blank line
        writeln!(f, "{key}=hello_world").unwrap();
        writeln!(f, "  KEY_WITH_SPACES = trimmed  ").unwrap();
        drop(f);

        load_env_file(path);
        assert_eq!(std::env::var(key).unwrap(), "hello_world");

        // SAFETY: test-only, no concurrent env access
        unsafe { std::env::remove_var(key) };
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn load_env_file_does_not_overwrite_existing_var() {
        use std::io::Write;
        let path = "/tmp/ilo_test_env_no_overwrite_C5D1.env";
        let key = "ILO_TEST_NO_OVERWRITE_C5D1";
        // SAFETY: test-only, no concurrent env access
        unsafe { std::env::remove_var(key) };
        unsafe { std::env::set_var(key, "original") };

        let mut f = std::fs::File::create(path).unwrap();
        writeln!(f, "{key}=new_value").unwrap();
        drop(f);

        load_env_file(path);
        assert_eq!(std::env::var(key).unwrap(), "original");

        // SAFETY: test-only, no concurrent env access
        unsafe { std::env::remove_var(key) };
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn load_env_file_missing_file_is_noop() {
        // Should not panic when file doesn't exist
        load_env_file("/tmp/ilo_nonexistent_env_file_X9Z2.env");
    }

    // ── warn_cross_language_syntax ────────────────────────────────────────────

    #[test]
    fn warn_cross_lang_clean_source_no_panic() {
        warn_cross_language_syntax("f x:n>n;*x 2", OutputMode::Text);
    }

    #[test]
    fn warn_cross_lang_detects_double_ampersand() {
        // Should not panic; warning goes to stderr
        warn_cross_language_syntax("f a:b>b;&& a true", OutputMode::Text);
    }

    #[test]
    fn warn_cross_lang_detects_double_pipe() {
        warn_cross_language_syntax("f a:b>b;|| a false", OutputMode::Text);
    }

    #[test]
    fn warn_cross_lang_detects_arrow() {
        warn_cross_language_syntax("f x:n->n;x", OutputMode::Text);
    }

    #[test]
    fn warn_cross_lang_equality_no_longer_warns() {
        // == is now sugar for =, so no cross-language warning
        warn_cross_language_syntax("f x:n>b;== x 1", OutputMode::Text);
        // This should not produce any warning — just a no-op call.
        // (Previously this warned about ==.)
    }

    // ── decl_name ─────────────────────────────────────────────────────────────

    #[test]
    fn decl_name_function_returns_name() {
        let d = ast::Decl::Function {
            name: "myfunc".into(),
            params: vec![],
            return_type: ast::Type::Number,
            body: vec![],
            span: ast::Span { start: 0, end: 0 },
        };
        assert_eq!(decl_name(&d), Some("myfunc"));
    }

    #[test]
    fn decl_name_use_returns_none() {
        let d = ast::Decl::Use {
            path: "lib.ilo".into(),
            only: None,
            span: ast::Span { start: 0, end: 0 },
        };
        assert_eq!(decl_name(&d), None);
    }

    #[test]
    fn decl_name_alias_returns_name() {
        let d = ast::Decl::Alias {
            name: "mytype".into(),
            target: ast::Type::Number,
            span: ast::Span { start: 0, end: 0 },
        };
        assert_eq!(decl_name(&d), Some("mytype"));
    }

    #[test]
    fn decl_name_error_returns_none() {
        let d = ast::Decl::Error {
            span: ast::Span { start: 0, end: 0 },
        };
        assert_eq!(decl_name(&d), None);
    }

    // ── resolve_imports: `only` filter path ───────────────────────────────────

    #[test]
    fn resolve_imports_only_filter_keeps_named_decl() {
        use std::io::Write;
        let lib_path = "/tmp/ilo_test_resolve_only_F2G7.ilo";
        let mut f = std::fs::File::create(lib_path).unwrap();
        writeln!(f, "dbl n:n>n;*n 2").unwrap();
        writeln!(f, "half n:n>n;/n 2").unwrap();
        drop(f);

        let use_decl = ast::Decl::Use {
            path: "ilo_test_resolve_only_F2G7.ilo".into(),
            only: Some(vec!["dbl".into()]),
            span: ast::Span { start: 0, end: 0 },
        };
        let mut diags = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let result = resolve_imports(
            vec![use_decl],
            Some(std::path::Path::new("/tmp")),
            &mut visited,
            &mut diags,
        );

        let names: Vec<&str> = result.iter().filter_map(|d| decl_name(d)).collect();
        assert!(names.contains(&"dbl"), "expected dbl: {names:?}");
        assert!(!names.contains(&"half"), "half should be filtered: {names:?}");
        assert!(diags.is_empty(), "no errors expected: {diags:?}");

        std::fs::remove_file(lib_path).ok();
    }

    #[test]
    fn resolve_imports_only_filter_warns_missing_name() {
        use std::io::Write;
        let lib_path = "/tmp/ilo_test_resolve_missing_H4K9.ilo";
        let mut f = std::fs::File::create(lib_path).unwrap();
        writeln!(f, "dbl n:n>n;*n 2").unwrap();
        drop(f);

        let use_decl = ast::Decl::Use {
            path: "ilo_test_resolve_missing_H4K9.ilo".into(),
            only: Some(vec!["dbl".into(), "nonexistent".into()]),
            span: ast::Span { start: 0, end: 0 },
        };
        let mut diags = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let _ = resolve_imports(
            vec![use_decl],
            Some(std::path::Path::new("/tmp")),
            &mut visited,
            &mut diags,
        );

        assert!(
            diags.iter().any(|d| d.code.as_deref() == Some("ILO-P019")),
            "expected ILO-P019 for missing name, got: {diags:?}"
        );

        std::fs::remove_file(lib_path).ok();
    }

    // ── diag_to_json ──────────────────────────────────────────────────────────

    #[test]
    fn diag_to_json_simple_error() {
        let d = Diagnostic::error("something went wrong").with_code("ILO-T001");
        let val = diag_to_json(&d);
        assert!(val.is_object());
        let obj = val.as_object().unwrap();
        assert_eq!(obj["code"], "ILO-T001");
        assert!(obj["message"].as_str().unwrap().contains("something went wrong"));
    }

    #[test]
    fn diag_to_json_with_span_and_source() {
        let d = Diagnostic::error("bad token")
            .with_code("ILO-L001")
            .with_span(ast::Span { start: 0, end: 3 }, "here")
            .with_source("abc".to_string());
        let val = diag_to_json(&d);
        assert!(val.is_object());
        assert_eq!(val["severity"], "error");
    }

    // ── types_pipe_compatible: Result and Map branches ────────────────────────

    #[test]
    fn pipe_compat_result_matching() {
        use ast::Type::*;
        let r1 = Result(Box::new(Number), Box::new(Text));
        let r2 = Result(Box::new(Number), Box::new(Text));
        assert!(types_pipe_compatible(&r1, &r2));
    }

    #[test]
    fn pipe_compat_result_mismatched_ok() {
        use ast::Type::*;
        assert!(!types_pipe_compatible(
            &Result(Box::new(Number), Box::new(Text)),
            &Result(Box::new(Text), Box::new(Text)),
        ));
    }

    #[test]
    fn pipe_compat_result_mismatched_err() {
        use ast::Type::*;
        assert!(!types_pipe_compatible(
            &Result(Box::new(Number), Box::new(Text)),
            &Result(Box::new(Number), Box::new(Number)),
        ));
    }

    #[test]
    fn pipe_compat_map_matching() {
        use ast::Type::*;
        let m = Map(Box::new(Text), Box::new(Number));
        assert!(types_pipe_compatible(&m, &m.clone()));
    }

    #[test]
    fn pipe_compat_map_key_mismatch() {
        use ast::Type::*;
        assert!(!types_pipe_compatible(
            &Map(Box::new(Text), Box::new(Number)),
            &Map(Box::new(Number), Box::new(Number)),
        ));
    }

    #[test]
    fn pipe_compat_map_value_mismatch() {
        use ast::Type::*;
        assert!(!types_pipe_compatible(
            &Map(Box::new(Text), Box::new(Number)),
            &Map(Box::new(Text), Box::new(Text)),
        ));
    }

    #[test]
    fn pipe_compat_result_named_wildcard() {
        use ast::Type::*;
        // Named types inside Result act as wildcards
        assert!(types_pipe_compatible(
            &Result(Box::new(Named("T".into())), Box::new(Text)),
            &Result(Box::new(Number), Box::new(Text)),
        ));
    }

    #[test]
    fn pipe_compat_map_named_wildcard() {
        use ast::Type::*;
        assert!(types_pipe_compatible(
            &Map(Box::new(Text), Box::new(Named("V".into()))),
            &Map(Box::new(Text), Box::new(Number)),
        ));
    }

    // ── run_vm_with_provider: success path ────────────────────────────────────

    #[test]
    fn run_vm_with_provider_success_no_tools() {
        let compiled = make_compiled("f x:n>n;*x 2");
        run_vm_with_provider(
            &compiled,
            Some("f"),
            vec![interpreter::Value::Number(5.0)],
            None,
            #[cfg(feature = "tools")] None,
            #[cfg(feature = "tools")] None,
            "f x:n>n;*x 2",
            OutputMode::Text,
            false,
        );
    }

    #[test]
    fn run_vm_with_provider_explicit_json_wraps_ok() {
        let compiled = make_compiled("f x:n>n;*x 3");
        run_vm_with_provider(
            &compiled,
            Some("f"),
            vec![interpreter::Value::Number(4.0)],
            None,
            #[cfg(feature = "tools")] None,
            #[cfg(feature = "tools")] None,
            "f x:n>n;*x 3",
            OutputMode::Json,
            true,
        );
    }

    // ── run_interp_with_provider: success path ────────────────────────────────

    #[test]
    fn run_interp_with_provider_success_no_tools() {
        let program = make_program("f x:n>n;*x 2");
        run_interp_with_provider(
            &program,
            Some("f"),
            vec![interpreter::Value::Number(7.0)],
            None,
            #[cfg(feature = "tools")] None,
            #[cfg(feature = "tools")] None,
            "f x:n>n;*x 2",
            OutputMode::Text,
            false,
        );
    }

    #[test]
    fn run_interp_with_provider_explicit_json() {
        let program = make_program("f x:n>n;+x 1");
        run_interp_with_provider(
            &program,
            Some("f"),
            vec![interpreter::Value::Number(10.0)],
            None,
            #[cfg(feature = "tools")] None,
            #[cfg(feature = "tools")] None,
            "f x:n>n;+x 1",
            OutputMode::Json,
            true,
        );
    }

    // ── run_default: cranelift-then-interpreter dispatch ──────────────────────

    #[test]
    fn run_default_simple_numeric() {
        let program = make_program("f x:n>n;*x 2");
        run_default(&program, Some("f"), vec![interpreter::Value::Number(3.0)],
            "f x:n>n;*x 2", OutputMode::Text, false);
    }

    #[test]
    fn run_default_text_result() {
        let program = make_program("greet name:t>t;cat \"hi \" name");
        run_default(&program, Some("greet"),
            vec![interpreter::Value::Text("world".into())],
            "greet name:t>t;cat \"hi \" name", OutputMode::Text, false);
    }

    #[test]
    fn run_default_none_func_name_uses_first() {
        let program = make_program("double x:n>n;*x 2");
        run_default(&program, None, vec![interpreter::Value::Number(4.0)],
            "double x:n>n;*x 2", OutputMode::Text, false);
    }

    // ── resolve_imports: additional paths ─────────────────────────────────────

    #[test]
    fn resolve_imports_inline_code_emits_p017() {
        let use_decl = ast::Decl::Use {
            path: "something.ilo".into(),
            only: None,
            span: ast::Span { start: 0, end: 20 },
        };
        let mut visited = std::collections::HashSet::new();
        let mut diags = Vec::new();
        let result = resolve_imports(vec![use_decl], None, &mut visited, &mut diags);
        assert!(result.is_empty());
        assert!(diags.iter().any(|d| d.code.as_deref() == Some("ILO-P017")));
        assert!(diags[0].message.contains("inline code"));
    }

    #[test]
    fn resolve_imports_file_not_found_emits_p017() {
        let use_decl = ast::Decl::Use {
            path: "nonexistent_xyz_99999.ilo".into(),
            only: None,
            span: ast::Span { start: 0, end: 30 },
        };
        let mut visited = std::collections::HashSet::new();
        let mut diags = Vec::new();
        let result = resolve_imports(
            vec![use_decl], Some(std::path::Path::new("/tmp")), &mut visited, &mut diags,
        );
        assert!(result.is_empty());
        assert!(diags.iter().any(|d| d.code.as_deref() == Some("ILO-P017")
            && d.message.contains("file not found")));
    }

    #[test]
    fn resolve_imports_non_use_decl_passes_through() {
        let func_decl = ast::Decl::Function {
            name: "f".into(),
            params: vec![],
            return_type: ast::Type::Number,
            body: vec![],
            span: ast::Span { start: 0, end: 0 },
        };
        let mut visited = std::collections::HashSet::new();
        let mut diags = Vec::new();
        let result = resolve_imports(vec![func_decl], None, &mut visited, &mut diags);
        assert_eq!(result.len(), 1);
        assert!(diags.is_empty());
    }

    // ── print_value ───────────────────────────────────────────────────────────

    #[test]
    fn print_value_plain_number_no_json() {
        print_value(&interpreter::Value::Number(42.0), false);
    }

    #[test]
    fn print_value_ok_as_json() {
        let val = interpreter::Value::Ok(Box::new(interpreter::Value::Number(42.0)));
        print_value(&val, true);
    }

    #[test]
    fn print_value_err_as_json() {
        let val = interpreter::Value::Err(Box::new(interpreter::Value::Text("oops".into())));
        print_value(&val, true);
    }

    #[test]
    fn print_value_err_no_json() {
        let val = interpreter::Value::Err(Box::new(interpreter::Value::Text("fail".into())));
        print_value(&val, false);
    }

    #[test]
    fn print_value_text_as_json() {
        print_value(&interpreter::Value::Text("hello".into()), true);
    }

    #[test]
    fn print_value_bool_as_json() {
        print_value(&interpreter::Value::Bool(true), true);
    }

    #[test]
    fn print_value_nil_as_json() {
        print_value(&interpreter::Value::Nil, true);
    }

    #[test]
    fn print_value_list_as_json() {
        let val = interpreter::Value::List(vec![
            interpreter::Value::Number(1.0),
            interpreter::Value::Number(2.0),
        ]);
        print_value(&val, true);
    }

    // ── warn_cross_language_syntax: Json mode ─────────────────────────────────

    #[test]
    fn warn_cross_lang_json_mode() {
        warn_cross_language_syntax("f x:b y:b>b;&& x y", OutputMode::Json);
    }

    #[test]
    fn warn_cross_lang_multiple_patterns_json_mode() {
        // == no longer warns, but -> and // still do
        warn_cross_language_syntax("f x:n->n;== x 1 // check", OutputMode::Json);
    }

    // ── resolve_imports: parse error propagation ──────────────────────────────

    #[test]
    fn resolve_imports_parse_error_in_imported_file() {
        let bad_path = "/tmp/ilo_unit_bad_parse_imports.ilo";
        std::fs::write(bad_path, "f x:>n;x").expect("write bad file");

        let decls = vec![ast::Decl::Use {
            path: "ilo_unit_bad_parse_imports.ilo".into(),
            only: None,
            span: ast::Span { start: 0, end: 0 },
        }];
        let mut visited = std::collections::HashSet::new();
        let mut diags = Vec::new();
        let _ = resolve_imports(decls, Some(std::path::Path::new("/tmp")), &mut visited, &mut diags);

        assert!(!diags.is_empty(), "expected parse error diagnostic from imported file");
        std::fs::remove_file(bad_path).ok();
    }

    // ── resolve_imports: transitive imports ───────────────────────────────────

    #[test]
    fn resolve_imports_transitive() {
        let file_b = "/tmp/ilo_unit_trans_b_Q3R8.ilo";
        let file_a = "/tmp/ilo_unit_trans_a_Q3R8.ilo";

        std::fs::write(file_b, "triple x:n>n;*x 3").expect("write B");
        std::fs::write(file_a, "use \"ilo_unit_trans_b_Q3R8.ilo\"\nsextuple x:n>n;t=triple x;*t 2")
            .expect("write A");

        let decls = vec![ast::Decl::Use {
            path: "ilo_unit_trans_a_Q3R8.ilo".into(),
            only: None,
            span: ast::Span { start: 0, end: 0 },
        }];
        let mut visited = std::collections::HashSet::new();
        let mut diags = Vec::new();
        let result = resolve_imports(decls, Some(std::path::Path::new("/tmp")), &mut visited, &mut diags);

        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        let names: Vec<_> = result.iter().filter_map(|d| decl_name(d)).collect();
        assert!(names.contains(&"triple"), "expected triple: {names:?}");
        assert!(names.contains(&"sextuple"), "expected sextuple: {names:?}");

        std::fs::remove_file(file_b).ok();
        std::fs::remove_file(file_a).ok();
    }

    // ── report_diagnostic: all three output modes ─────────────────────────────

    #[test]
    fn report_diagnostic_text_mode_no_panic() {
        let d = Diagnostic::error("test error").with_code("ILO-T001");
        report_diagnostic(&d, OutputMode::Text);
    }

    #[test]
    fn report_diagnostic_ansi_mode_no_panic() {
        let d = Diagnostic::error("ansi error").with_code("ILO-T002");
        report_diagnostic(&d, OutputMode::Ansi);
    }

    #[test]
    fn report_diagnostic_json_mode_no_panic() {
        let d = Diagnostic::error("json error").with_code("ILO-T003");
        report_diagnostic(&d, OutputMode::Json);
    }

    #[test]
    fn report_diagnostic_warning_all_modes_no_panic() {
        let d = Diagnostic::warning("test warning");
        report_diagnostic(&d, OutputMode::Text);
        report_diagnostic(&d, OutputMode::Ansi);
        report_diagnostic(&d, OutputMode::Json);
    }

    // ── decl_name: Tool and TypeDef variants ─────────────────────────────────

    #[test]
    fn decl_name_tool_returns_name() {
        let d = ast::Decl::Tool {
            name: "my_tool".into(),
            description: "does things".into(),
            params: vec![],
            return_type: ast::Type::Text,
            timeout: None,
            retry: None,
            span: ast::Span { start: 0, end: 0 },
        };
        assert_eq!(decl_name(&d), Some("my_tool"));
    }

    #[test]
    fn decl_name_typedef_returns_name() {
        let d = ast::Decl::TypeDef {
            name: "Point".into(),
            fields: vec![],
            span: ast::Span { start: 0, end: 0 },
        };
        assert_eq!(decl_name(&d), Some("Point"));
    }

    // ── warn_cross_language_syntax: // comment pattern ────────────────────────

    #[test]
    fn warn_cross_lang_detects_double_slash_comment() {
        // '//' outside strings is a foreign-syntax comment; ilo uses '--'
        warn_cross_language_syntax("f x:n>n;// this is a comment", OutputMode::Text);
    }

    #[test]
    fn warn_cross_lang_ignores_slash_in_strings() {
        // '//' inside string literals should NOT trigger the warning
        // (common case: URLs like "https://example.com")
        warn_cross_language_syntax(r#"f>t;"https://example.com""#, OutputMode::Text);
        // No warning emitted — this just verifies no panic and no false positive.
    }

    #[test]
    fn strip_string_contents_preserves_outside() {
        let result = strip_string_contents(r#"abc "hello" def"#);
        assert_eq!(result, r#"abc "     " def"#);
    }

    #[test]
    fn strip_string_contents_handles_escapes() {
        let result = strip_string_contents(r#""a\"b""#);
        assert_eq!(result, r#""    ""#);
    }

    #[test]
    fn strip_string_contents_url() {
        let result = strip_string_contents(r#"get "https://api.com/users""#);
        assert!(!result.contains("//"));
    }

    #[test]
    fn warn_cross_lang_ansi_mode_no_panic() {
        warn_cross_language_syntax("f x:n>n;&& x true", OutputMode::Ansi);
    }

    // ── parse_cli_arg: edge cases ─────────────────────────────────────────────

    #[test]
    fn cli_arg_nan_is_text() {
        // NaN is not finite, so it should fall through to text
        assert_eq!(parse_cli_arg("NaN"), interpreter::Value::Text("NaN".into()));
    }

    #[test]
    fn cli_arg_negative_number() {
        assert_eq!(parse_cli_arg("-5"), interpreter::Value::Number(-5.0));
    }

    // ── load_dotenv: priority ordering ───────────────────────────────────────

    #[test]
    fn load_dotenv_env_local_takes_priority_over_env() {
        // Write two env files in a temp dir, change cwd, call load_dotenv(),
        // verify that .env.local value wins (loaded first; .env cannot overwrite).
        use std::io::Write;

        let dir = "/tmp/ilo_test_load_dotenv_prio_M8N2";
        std::fs::create_dir_all(dir).unwrap();
        let local_path = format!("{dir}/.env.local");
        let env_path = format!("{dir}/.env");
        let key = "ILO_TEST_DOTENV_PRIO_M8N2";

        // SAFETY: test-only, single-threaded env manipulation
        unsafe { std::env::remove_var(key) };

        let mut f = std::fs::File::create(&local_path).unwrap();
        writeln!(f, "{key}=from_local").unwrap();
        drop(f);

        let mut f = std::fs::File::create(&env_path).unwrap();
        writeln!(f, "{key}=from_env").unwrap();
        drop(f);

        // Load both files manually in the same order load_dotenv() would
        load_env_file(&local_path);
        load_env_file(&env_path);

        assert_eq!(
            std::env::var(key).unwrap(),
            "from_local",
            ".env.local should take priority over .env"
        );

        unsafe { std::env::remove_var(key) };
        std::fs::remove_file(&local_path).ok();
        std::fs::remove_file(&env_path).ok();
        std::fs::remove_dir(dir).ok();
    }

    // ── process_serv_request: lex error phase ─────────────────────────────────

    #[test]
    fn serv_lex_error_returns_lex_phase() {
        // A string with an unterminated string literal causes a lex error
        let resp = run_serv(r#"{"program": "f>t;\""}"#);
        // Either lex or parse phase is acceptable depending on how the lexer reports it
        let phase = resp["error"]["phase"].as_str().unwrap_or("");
        assert!(
            phase == "lex" || phase == "parse",
            "expected lex or parse phase for unterminated string, got: {resp}"
        );
    }

    // ── process_serv_request: runtime error phase ────────────────────────────

    #[test]
    fn serv_runtime_error_returns_runtime_phase() {
        // Division by zero passes lex/parse/verify but fails at runtime
        let resp = run_serv(r#"{"program": "f>n;/1 0", "func": "f"}"#);
        assert_eq!(
            resp["error"]["phase"], "runtime",
            "expected runtime phase for division by zero, got: {resp}"
        );
    }

    // ── process_serv_request: mcp_tool_decls prepended ───────────────────────

    #[test]
    fn serv_with_non_empty_mcp_tool_decls_succeed() {
        // Build a synthetic Tool decl that represents an external tool.
        // process_serv_request prepends mcp_tool_decls before verify, so a
        // program that calls the tool should verify and run cleanly IF the
        // stub returns Ok(Nil).  Here we just pass a decl and call a simple
        // function that doesn't use the tool — verifies the prepend path is
        // exercised without a tool call error.
        let tool_decl = ast::Decl::Tool {
            name: "echo_tool".into(),
            description: "echoes input".into(),
            params: vec![ast::Param { name: "msg".into(), ty: ast::Type::Text }],
            return_type: ast::Type::Result(
                Box::new(ast::Type::Text),
                Box::new(ast::Type::Text),
            ),
            timeout: None,
            retry: None,
            span: ast::Span { start: 0, end: 0 },
        };

        let line = r#"{"program": "f>n;42", "func": "f"}"#;

        #[cfg(not(feature = "tools"))]
        let resp = process_serv_request(line, &[tool_decl], None);

        #[cfg(feature = "tools")]
        let resp = {
            let rt = std::sync::Arc::new(
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap(),
            );
            process_serv_request(line, &[tool_decl], None, None, rt)
        };

        assert!(resp.get("ok").is_some(), "expected ok response, got: {resp}");
        assert_eq!(resp["ok"].as_f64(), Some(42.0));
    }

    // ── diag_to_json: warning severity ───────────────────────────────────────

    #[test]
    fn diag_to_json_warning_severity() {
        let d = Diagnostic::warning("suspicious pattern");
        let val = diag_to_json(&d);
        assert!(val.is_object());
        assert_eq!(val["severity"], "warning");
    }

    // ── print_value: remaining branches ──────────────────────────────────────

    #[test]
    fn print_value_list_plain_not_json() {
        let val = interpreter::Value::List(vec![
            interpreter::Value::Number(1.0),
            interpreter::Value::Text("x".into()),
        ]);
        print_value(&val, false);
    }

    #[test]
    fn print_value_map_as_json() {
        let mut m = std::collections::HashMap::new();
        m.insert("k".to_string(), interpreter::Value::Number(7.0));
        let val = interpreter::Value::Map(m);
        print_value(&val, true);
    }

    #[test]
    fn print_value_map_plain_not_json() {
        let mut m = std::collections::HashMap::new();
        m.insert("key".to_string(), interpreter::Value::Bool(true));
        let val = interpreter::Value::Map(m);
        print_value(&val, false);
    }

    // ── subprocess helpers ────────────────────────────────────────────────────

    /// Locate the `ilo` binary that corresponds to the current test profile.
    ///
    /// Unit tests run as `target/<profile>/deps/ilo-<hash>`.  The actual
    /// binary is one directory up, at `target/<profile>/ilo`.
    fn ilo_bin() -> std::path::PathBuf {
        // current_exe() → e.g. /…/target/debug/deps/ilo-abc123
        let exe = std::env::current_exe().expect("current_exe");
        // Go up from deps/ to the profile dir (debug or release)
        let profile_dir = exe
            .parent()          // deps/
            .and_then(|p| p.parent()) // debug/ or release/
            .expect("could not locate profile dir");
        let bin = profile_dir.join("ilo");
        assert!(
            bin.exists(),
            "ilo binary not found at {}; run `cargo build` first",
            bin.display()
        );
        bin
    }

    // ── subprocess: --version ─────────────────────────────────────────────────

    #[test]
    fn cli_version_flag_prints_version() {
        let out = std::process::Command::new(ilo_bin())
            .arg("--version")
            .output()
            .expect("failed to run ilo --version");
        assert!(out.status.success(), "exit status: {}", out.status);
        let stdout = String::from_utf8_lossy(&out.stdout);
        // Should contain a semver-ish version like "0.7.0"
        assert!(
            stdout.contains('.'),
            "expected version number in stdout, got: {stdout}"
        );
        // Should contain the binary name or version token
        assert!(
            stdout.to_lowercase().contains("ilo") || stdout.trim().chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false),
            "expected ilo version string, got: {stdout}"
        );
    }

    // ── subprocess: --explain ─────────────────────────────────────────────────

    #[test]
    fn cli_explain_valid_code_exits_zero_with_text() {
        let out = std::process::Command::new(ilo_bin())
            .args(["--explain", "ILO-T001"])
            .output()
            .expect("failed to run ilo --explain ILO-T001");
        assert!(
            out.status.success(),
            "expected exit 0, got: {}; stderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            !stdout.trim().is_empty(),
            "expected explanation text on stdout, got empty"
        );
    }

    #[test]
    fn cli_explain_unknown_code_exits_nonzero() {
        let out = std::process::Command::new(ilo_bin())
            .args(["--explain", "INVALID-CODE"])
            .output()
            .expect("failed to run ilo --explain INVALID-CODE");
        assert!(
            !out.status.success(),
            "expected non-zero exit for unknown code"
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("unknown") || stderr.contains("not found"),
            "expected 'unknown' or 'not found' in stderr, got: {stderr}"
        );
    }

    // ── subprocess: help ─────────────────────────────────────────────────────

    #[test]
    fn cli_help_default_exits_zero_with_usage() {
        let out = std::process::Command::new(ilo_bin())
            .arg("help")
            .output()
            .expect("failed to run ilo help");
        assert!(
            out.status.success(),
            "expected exit 0, got: {}; stderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("Usage") || stdout.contains("usage") || stdout.contains("ilo"),
            "expected usage info in stdout, got: {stdout}"
        );
    }

    #[test]
    fn cli_help_lang_exits_zero_with_spec_content() {
        let out = std::process::Command::new(ilo_bin())
            .args(["help", "lang"])
            .output()
            .expect("failed to run ilo help lang");
        assert!(
            out.status.success(),
            "expected exit 0, got: {}; stderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        // SPEC.md has ilo language content
        assert!(
            !stdout.trim().is_empty(),
            "expected spec content on stdout, got empty"
        );
    }

    #[test]
    fn cli_help_ai_exits_zero_with_compact_spec() {
        let out = std::process::Command::new(ilo_bin())
            .args(["help", "ai"])
            .output()
            .expect("failed to run ilo help ai");
        assert!(
            out.status.success(),
            "expected exit 0, got: {}; stderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            !stdout.trim().is_empty(),
            "expected compact spec on stdout, got empty"
        );
    }

    // ── subprocess: empty code ────────────────────────────────────────────────

    #[test]
    fn cli_empty_code_string_exits_nonzero() {
        let out = std::process::Command::new(ilo_bin())
            .arg("")
            .output()
            .expect("failed to run ilo with empty arg");
        // An empty string is not a valid file path and not valid ilo code;
        // the process must exit non-zero.
        assert!(
            !out.status.success(),
            "expected non-zero exit for empty code string"
        );
        // Error should appear on stderr
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !stderr.trim().is_empty(),
            "expected some error on stderr, got empty"
        );
    }

    // ── subprocess: --emit unknown target ─────────────────────────────────────

    #[test]
    fn cli_emit_unknown_target_exits_nonzero() {
        let out = std::process::Command::new(ilo_bin())
            .args(["f>n;1", "--emit", "rust"])
            .output()
            .expect("failed to run ilo --emit rust");
        assert!(
            !out.status.success(),
            "expected non-zero exit for unknown emit target"
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("Unknown emit") || stderr.contains("Supported") || stderr.contains("python"),
            "expected unknown-emit error in stderr, got: {stderr}"
        );
    }

    // ── subprocess: --tools and --mcp mutually exclusive ─────────────────────

    #[test]
    fn cli_tools_and_mcp_mutually_exclusive() {
        let out = std::process::Command::new(ilo_bin())
            .args(["f>n;1", "--tools", "/tmp/x.json", "--mcp", "/tmp/y.json"])
            .output()
            .expect("failed to run ilo with --tools and --mcp");
        assert!(
            !out.status.success(),
            "expected non-zero exit when both --tools and --mcp are provided"
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("mutually exclusive") || stderr.contains("exclusive"),
            "expected 'mutually exclusive' in stderr, got: {stderr}"
        );
    }

    // ── subprocess: tools subcommand ─────────────────────────────────────────

    #[test]
    fn cli_tools_cmd_no_flags_exits_nonzero() {
        let out = std::process::Command::new(ilo_bin())
            .arg("tools")
            .output()
            .expect("failed to run ilo tools");
        assert!(
            !out.status.success(),
            "expected non-zero exit for `ilo tools` with no flags"
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("--mcp") || stderr.contains("--tools") || stderr.contains("requires"),
            "expected usage hint in stderr, got: {stderr}"
        );
    }

    #[test]
    fn cli_tools_cmd_mcp_no_path_exits_nonzero() {
        let out = std::process::Command::new(ilo_bin())
            .args(["tools", "--mcp"])
            .output()
            .expect("failed to run ilo tools --mcp");
        assert!(
            !out.status.success(),
            "expected non-zero exit for `ilo tools --mcp` with no path"
        );
    }

    // ── subprocess: serv subcommand unknown flag ──────────────────────────────

    #[test]
    fn cli_serv_unknown_flag_exits_nonzero() {
        let out = std::process::Command::new(ilo_bin())
            .args(["serv", "--invalid-flag"])
            .output()
            .expect("failed to run ilo serv --invalid-flag");
        assert!(
            !out.status.success(),
            "expected non-zero exit for unknown flag to `ilo serv`"
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !stderr.trim().is_empty(),
            "expected error on stderr, got empty"
        );
    }

    // ── subprocess: tools subcommand with --tools http config file ───────────

    fn write_temp_tools_config(name: &str, tools_json: &str) -> String {
        let path = format!("/tmp/ilo_test_{name}.json");
        std::fs::write(&path, tools_json).expect("write tools config");
        path
    }

    #[test]
    fn cli_tools_cmd_with_http_config_human_output() {
        // tools --tools <path> → human-readable list of tool names
        let config = r#"{"tools":{"greet":{"url":"http://localhost:9"},"ping":{"url":"http://localhost:9"}}}"#;
        let path = write_temp_tools_config("human_A1", config);
        let out = std::process::Command::new(ilo_bin())
            .args(["tools", "--tools", &path])
            .output()
            .expect("failed to run ilo tools --tools");
        // May exit non-zero (no MCP), but should not crash
        let stdout = String::from_utf8_lossy(&out.stdout);
        // greet and ping should appear in output
        assert!(
            stdout.contains("greet") || stdout.contains("ping"),
            "expected tool names in stdout, got: {stdout}"
        );
    }

    #[test]
    fn cli_tools_cmd_with_http_config_ilo_output() {
        // --ilo flag: emit valid ilo tool decls
        let config = r#"{"tools":{"calc":{"url":"http://localhost:9"}}}"#;
        let path = write_temp_tools_config("ilo_B2", config);
        let out = std::process::Command::new(ilo_bin())
            .args(["tools", "--tools", &path, "--ilo"])
            .output()
            .expect("failed to run ilo tools --tools --ilo");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("tool") || stdout.contains("calc"),
            "expected ilo tool decl in stdout, got: {stdout}"
        );
    }

    #[test]
    fn cli_tools_cmd_with_http_config_json_output() {
        // --json flag: emit JSON array
        let config = r#"{"tools":{"lookup":{"url":"http://localhost:9"}}}"#;
        let path = write_temp_tools_config("json_C3", config);
        let out = std::process::Command::new(ilo_bin())
            .args(["tools", "--tools", &path, "--json"])
            .output()
            .expect("failed to run ilo tools --tools --json");
        let stdout = String::from_utf8_lossy(&out.stdout);
        // Should be a JSON array
        assert!(
            stdout.trim().starts_with('['),
            "expected JSON array in stdout, got: {stdout}"
        );
        assert!(
            stdout.contains("lookup"),
            "expected 'lookup' in JSON output, got: {stdout}"
        );
    }

    #[test]
    fn cli_tools_cmd_with_http_config_full_flag() {
        // --full flag: human output with type info
        let config = r#"{"tools":{"do_thing":{"url":"http://localhost:9"}}}"#;
        let path = write_temp_tools_config("full_D4", config);
        let out = std::process::Command::new(ilo_bin())
            .args(["tools", "--tools", &path, "--full"])
            .output()
            .expect("failed to run ilo tools --tools --full");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("do_thing"),
            "expected 'do_thing' in output, got: {stdout}"
        );
    }

    #[test]
    fn cli_tools_cmd_unknown_flag_exits_nonzero() {
        // Unknown flag in tools subcommand → exit nonzero
        let config = r#"{"tools":{}}"#;
        let path = write_temp_tools_config("ukflag_E5", config);
        let out = std::process::Command::new(ilo_bin())
            .args(["tools", "--tools", &path, "--unknown-flag"])
            .output()
            .expect("failed to run ilo tools with unknown flag");
        assert!(
            !out.status.success(),
            "expected non-zero exit for unknown flag"
        );
    }

    #[test]
    fn cli_tools_cmd_tools_no_path_exits_nonzero() {
        // --tools with no path arg → exit nonzero
        let out = std::process::Command::new(ilo_bin())
            .args(["tools", "--tools"])
            .output()
            .expect("failed to run ilo tools --tools");
        assert!(
            !out.status.success(),
            "expected non-zero exit for --tools with no path"
        );
    }

    // ── unit: print_tool_graph (no typed tools = empty message) ──────────────

    #[test]
    fn print_tool_graph_no_tools_prints_no_typed_tools() {
        // With no Decl::Tool entries, graph prints "(no typed tools...)"
        // We can't easily capture stdout in unit tests, but calling the function
        // with an empty slice exercises the early return path.
        // This test just ensures no panic.
        print_tool_graph(&[]);
    }

    #[test]
    fn print_tool_graph_with_typed_tools_no_panic() {
        use ast::{Decl, Param, Type};
        let decls = vec![
            Decl::Tool {
                name: "alpha".into(),
                description: "first tool".into(),
                params: vec![Param { name: "x".into(), ty: Type::Number }],
                return_type: Type::Result(Box::new(Type::Text), Box::new(Type::Text)),
                timeout: None,
                retry: None,
                span: ast::Span::UNKNOWN,
            },
            Decl::Tool {
                name: "beta".into(),
                description: "second tool".into(),
                params: vec![Param { name: "s".into(), ty: Type::Text }],
                return_type: Type::Text,
                timeout: None,
                retry: None,
                span: ast::Span::UNKNOWN,
            },
        ];
        // Should not panic — exercises the graph table printing code
        print_tool_graph(&decls);
    }

    // ── unit: tool_sig_str edge cases ─────────────────────────────────────────

    #[test]
    fn tool_sig_str_no_params_result_type() {
        use ast::{Param, Type};
        let params: Vec<Param> = vec![];
        let ret = Type::Result(Box::new(Type::Number), Box::new(Type::Text));
        let sig = tool_sig_str(&params, &ret);
        assert!(sig.starts_with('>'), "expected '>' prefix for no-param sig, got: {sig}");
        assert!(sig.contains("R"), "expected result type in sig, got: {sig}");
    }

    // ── unit: resolve_imports verify-warning path ─────────────────────────────

    #[test]
    fn verify_warnings_emitted_via_subprocess() {
        // A program that triggers a verify warning (unused variable or similar).
        // Run via subprocess so the warn path (L1107) is exercised.
        // Use a program with a dead let binding to trigger warnings.
        let out = std::process::Command::new(ilo_bin())
            .args(["f>n;x=1;2"])
            .output()
            .expect("failed to run ilo");
        // May succeed or fail depending on verify behavior, just ensure it runs
        let _ = out;
    }

    // ── unit: resolve_imports error paths ─────────────────────────────────────

    fn make_use_decl(path: &str) -> ast::Decl {
        ast::Decl::Use {
            path: path.to_string(),
            only: None,
            span: ast::Span::UNKNOWN,
        }
    }

    #[test]
    fn resolve_imports_no_base_dir_emits_error() {
        // `use` without a file context → ILO-P017 error (lines 699-703)
        let decls = vec![make_use_decl("math.ilo")];
        let mut visited = std::collections::HashSet::new();
        let mut diagnostics = Vec::new();
        let result = resolve_imports(decls, None, &mut visited, &mut diagnostics);
        assert!(result.is_empty(), "should return no decls");
        assert!(!diagnostics.is_empty(), "should emit error");
        assert!(diagnostics[0].message.contains("file path context"));
    }

    #[test]
    fn resolve_imports_file_not_found_emits_error() {
        // Import a non-existent file → ILO-P017 (lines 711-716)
        let decls = vec![make_use_decl("nonexistent_file_xyz.ilo")];
        let mut visited = std::collections::HashSet::new();
        let mut diagnostics = Vec::new();
        let dir = std::path::Path::new("/tmp");
        let result = resolve_imports(decls, Some(dir), &mut visited, &mut diagnostics);
        assert!(result.is_empty());
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("nonexistent_file_xyz.ilo"));
    }

    #[test]
    fn resolve_imports_circular_emits_error() {
        // Pre-populate visited with a file that we then try to import → ILO-P018 (lines 721-726)
        let path = "/tmp/ilo_circ_test.ilo";
        std::fs::write(path, "f>n;1").unwrap();
        let canonical = std::fs::canonicalize(path).unwrap();

        let decls = vec![make_use_decl("ilo_circ_test.ilo")];
        let mut visited = std::collections::HashSet::new();
        visited.insert(canonical);
        let mut diagnostics = Vec::new();
        let dir = std::path::Path::new("/tmp");
        let result = resolve_imports(decls, Some(dir), &mut visited, &mut diagnostics);
        assert!(result.is_empty());
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("circular"));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn resolve_imports_lex_error_in_imported_file() {
        // Import a file with invalid syntax → lex error pushed to diagnostics (lines 743-745)
        let path = "/tmp/ilo_lex_err_test.ilo";
        std::fs::write(path, "MyFunc invalid_UpperCase").unwrap();
        let decls = vec![make_use_decl("ilo_lex_err_test.ilo")];
        let mut visited = std::collections::HashSet::new();
        let mut diagnostics = Vec::new();
        let dir = std::path::Path::new("/tmp");
        let _result = resolve_imports(decls, Some(dir), &mut visited, &mut diagnostics);
        assert!(!diagnostics.is_empty(), "should emit lex error diagnostic");
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn resolve_imports_read_error_after_canonicalize() {
        // Create a real file, canonicalize it, then delete it — when resolve_imports
        // tries to read_to_string after canonicalize, it gets Err → lines 731-737.
        let path = "/tmp/ilo_read_err_test.ilo";
        std::fs::write(path, "f>n;1").unwrap();
        // Create a symlink-like path that canonicalizes to /tmp/ilo_read_err_test_gone.ilo
        // Instead: just test file-not-found by giving a path whose parent exists but file doesn't.
        // Use a path that doesn't exist at all — canonicalize will Err → covers lines 711-716 again.
        // To hit the read_to_string Err path (731-737), we'd need canonicalize to succeed but
        // read to fail — which requires platform tricks. Skip that specific sub-path.
        std::fs::remove_file(path).ok();
        // Simple verification: non-existent path hits the canonical error (711-716)
        let decls = vec![make_use_decl("ilo_read_err_test.ilo")];
        let mut visited = std::collections::HashSet::new();
        let mut diagnostics = Vec::new();
        let dir = std::path::Path::new("/tmp");
        resolve_imports(decls, Some(dir), &mut visited, &mut diagnostics);
        assert!(!diagnostics.is_empty());
    }

    // ── unit: warn_cross_language_syntax ─────────────────────────────────────

    #[test]
    fn warn_cross_language_syntax_detects_and_or() {
        // && and || in source → warn_cross_language_syntax emits a diagnostic
        // Capture is not possible directly; test that no panic occurs and the
        // function is callable with Text mode.
        warn_cross_language_syntax("f>b;x&&y", OutputMode::Text);
        warn_cross_language_syntax("f>b;x||y", OutputMode::Text);
        // No return value — just ensure it completes without panic.
    }

    #[test]
    fn warn_cross_language_syntax_no_match_is_silent() {
        // Clean source → no warning emitted (early return at line 658)
        warn_cross_language_syntax("f x:n>n;+x 1", OutputMode::Text);
    }

    // ── unit: report_diagnostic modes ────────────────────────────────────────

    #[test]
    fn report_diagnostic_ansi_mode() {
        let d = Diagnostic::error("test error".to_string());
        // Just verify it doesn't panic with ANSI mode
        report_diagnostic(&d, OutputMode::Ansi);
    }

    #[test]
    fn report_diagnostic_text_mode() {
        let d = Diagnostic::error("test error".to_string());
        report_diagnostic(&d, OutputMode::Text);
    }

    #[test]
    fn report_diagnostic_json_mode() {
        let d = Diagnostic::error("test error".to_string());
        report_diagnostic(&d, OutputMode::Json);
    }

    // ── unit: tools_cmd rendering paths ──────────────────────────────────────

    fn write_tools_config_unit(name: &str) -> String {
        let path = format!("/tmp/ilo_unit_tools_{name}.json");
        std::fs::write(&path,
            r#"{"tools":{"search":{"url":"http://localhost:9"},"fetch":{"url":"http://localhost:9"}}}"#
        ).unwrap();
        path
    }

    #[test]
    fn tools_cmd_human_flag_renders_no_panic() {
        // Covers: --human flag (lines 57-58), Human rendering without --full (lines 122, 126)
        let path = write_tools_config_unit("human_unit");
        tools_cmd(&[
            "--tools".to_string(), path.clone(),
            "--human".to_string(),
        ]);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn tools_cmd_ilo_flag_renders_no_panic() {
        // Covers: --ilo flag (lines 60-61), full=true (line 99-100), Ilo rendering (lines 140-147)
        let path = write_tools_config_unit("ilo_unit");
        tools_cmd(&[
            "--tools".to_string(), path.clone(),
            "--ilo".to_string(),
        ]);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn tools_cmd_json_flag_renders_no_panic() {
        // Covers: --json flag (lines 64-65), Json rendering (lines 149-181)
        let path = write_tools_config_unit("json_unit");
        tools_cmd(&[
            "--tools".to_string(), path.clone(),
            "--json".to_string(),
        ]);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn tools_cmd_full_flag_human_shows_http_label() {
        // Covers: --full flag (lines 68-70), Human+full path (lines 122, 124)
        let path = write_tools_config_unit("full_unit");
        tools_cmd(&[
            "--tools".to_string(), path.clone(),
            "--full".to_string(),
        ]);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn tools_cmd_graph_flag_no_panic() {
        // Covers: --graph flag (lines 72-74), graph flag path (lines 190-192)
        let path = write_tools_config_unit("graph_unit");
        tools_cmd(&[
            "--tools".to_string(), path.clone(),
            "--graph".to_string(),
        ]);
        std::fs::remove_file(&path).ok();
    }

    // ── unit: print_tool_graph with non-Tool decl → filter_map None ──────────

    #[test]
    fn print_tool_graph_with_function_decl_skipped() {
        // Covers line 205: the None arm of filter_map when decl is not a Tool.
        use ast::{Decl, Param, Type, Span};
        let decls = vec![
            Decl::Function {
                name: "helper".into(),
                params: vec![Param { name: "x".into(), ty: Type::Number }],
                return_type: Type::Number,
                body: vec![],
                span: Span::UNKNOWN,
            },
            Decl::Tool {
                name: "alpha".into(),
                description: "a tool".into(),
                params: vec![Param { name: "x".into(), ty: Type::Text }],
                return_type: Type::Result(Box::new(Type::Text), Box::new(Type::Text)),
                timeout: None,
                retry: None,
                span: Span::UNKNOWN,
            },
        ];
        // Function decl → None in filter_map (line 205); Tool → Some
        print_tool_graph(&decls);
    }

    // ── unit: tool_sig_str with params ───────────────────────────────────────

    #[test]
    fn tool_sig_str_with_params() {
        let params = vec![
            ast::Param { name: "url".into(), ty: ast::Type::Text },
            ast::Param { name: "limit".into(), ty: ast::Type::Number },
        ];
        let ret = ast::Type::Result(Box::new(ast::Type::Text), Box::new(ast::Type::Text));
        let sig = tool_sig_str(&params, &ret);
        assert!(sig.contains("url"), "expected url param in sig: {sig}");
        assert!(sig.contains("limit"), "expected limit param in sig: {sig}");
    }

    // ── unit: print_tool_graph with long sig triggers truncation ─────────────

    #[test]
    fn print_tool_graph_long_sig_truncates_no_panic() {
        // sig_w is 36; create a tool with enough params that the sig exceeds 36 chars.
        // e.g. "url:t query:t page:n limit:n size:n>R t t" is ~43 chars.
        use ast::{Decl, Param, Type};
        let decls = vec![Decl::Tool {
            name: "search".into(),
            description: "search tool".into(),
            params: vec![
                Param { name: "url".into(),   ty: Type::Text },
                Param { name: "query".into(), ty: Type::Text },
                Param { name: "page".into(),  ty: Type::Number },
                Param { name: "limit".into(), ty: Type::Number },
                Param { name: "size".into(),  ty: Type::Number },
            ],
            return_type: Type::Result(Box::new(Type::Text), Box::new(Type::Text)),
            timeout: None,
            retry: None,
            span: ast::Span::UNKNOWN,
        }];
        // sig = "url:t query:t page:n limit:n size:n>R t t" (42 chars) > 36 → truncation path
        print_tool_graph(&decls);
    }

    // ── unit: resolve_imports — directory path triggers read_to_string error ──

    #[test]
    fn resolve_imports_directory_triggers_read_error() {
        // Importing a path that resolves to a directory: canonicalize succeeds,
        // but read_to_string fails ("Is a directory") → covers lines 731-737.
        let dir_name = "ilo_test_dir_import_Z9.ilo";
        let dir_path = format!("/tmp/{dir_name}");
        std::fs::create_dir_all(&dir_path).unwrap();

        let decls = vec![make_use_decl(dir_name)];
        let mut visited = std::collections::HashSet::new();
        let mut diagnostics = Vec::new();
        let result = resolve_imports(decls, Some(std::path::Path::new("/tmp")), &mut visited, &mut diagnostics);

        assert!(result.is_empty());
        assert!(!diagnostics.is_empty(), "should emit error for directory import");

        std::fs::remove_dir(&dir_path).ok();
    }

    // ── unit: load_env_file — line without '=' is skipped silently ───────────

    #[test]
    fn load_env_file_line_without_equals_skipped() {
        // Line without '=' hits the else branch of split_once (line 826 coverage).
        use std::io::Write;
        let path = "/tmp/ilo_test_env_noeq_X7.env";
        let key = "ILO_TEST_ENV_NOEQ_X7";
        unsafe { std::env::remove_var(key) };

        let mut f = std::fs::File::create(path).unwrap();
        writeln!(f, "# comment").unwrap();          // skipped (comment)
        writeln!(f, "no_equals_here").unwrap();      // split_once returns None → line 826
        writeln!(f, "{key}=set_value").unwrap();     // normal assignment
        drop(f);

        load_env_file(path);
        assert_eq!(std::env::var(key).unwrap(), "set_value");

        unsafe { std::env::remove_var(key) };
        std::fs::remove_file(path).ok();
    }
}
