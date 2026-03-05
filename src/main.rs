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
            println!("{}", serde_json::to_string_pretty(&items).unwrap());
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
        let line = line.expect("stdin read error");
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

/// Scan args for --json/-j, --text/-t, --ansi/-a. Return (mode, explicit_json, remaining_args).
/// Multiple format flags → error + exit(1).
/// `explicit_json` is true only when the user passed --json/-j; auto-detection never sets it.
fn detect_output_mode(args: Vec<String>) -> (OutputMode, bool, Vec<String>) {
    let mut mode: Option<OutputMode> = None;
    let mut remaining = Vec::with_capacity(args.len());
    let mut conflict = false;

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

    (resolved, explicit_json, remaining)
}

/// Scan source for common cross-language patterns and emit a single warning if found.
/// Non-fatal — program still attempts to run.
fn warn_cross_language_syntax(source: &str, mode: OutputMode) {
    let patterns: &[(&str, &str)] = &[
        ("&&", "'&&' — ilo uses '&' for AND"),
        ("||", "'||' — ilo uses '|' for OR"),
        ("->", "'->' — ilo uses '>' for return type separator"),
        ("==", "'==' — ilo uses '=' for equality comparison"),
        ("//", "'//' — ilo uses '--' for comments"),
    ];

    let details: Vec<&str> = patterns
        .iter()
        .filter(|(pat, _)| source.contains(*pat))
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

fn report_diagnostic(d: &Diagnostic, mode: OutputMode) {
    let s = match mode {
        OutputMode::Ansi => AnsiRenderer { use_color: true }.render(d),
        OutputMode::Text => AnsiRenderer { use_color: false }.render(d),
        // JSON mode: one object per line (NDJSON) so multiple errors are parseable.
        OutputMode::Json => format!("{}\n", json::render(d)),
    };
    eprint!("{}", s);
}

fn main() {
    let raw_args: Vec<String> = std::env::args().collect();

    // `ilo tools` is handled before output-mode detection so that --json/--ilo
    // can be used as *tool format* flags without conflicting with error format flags.
    if matches!(raw_args.get(1).map(|s| s.as_str()), Some("tools") | Some("tool")) {
        tools_cmd(&raw_args[2..]);
        std::process::exit(0);
    }

    if matches!(raw_args.get(1).map(|s| s.as_str()), Some("serv") | Some("repl")) {
        let is_serv = raw_args.get(1).map(|s| s.as_str()) == Some("serv");
        let rest: Vec<String> = if is_serv {
            // `ilo serv [args]` is an alias for `ilo repl -j [args]`
            let mut v = vec!["-j".to_string()];
            v.extend_from_slice(&raw_args[2..]);
            v
        } else {
            raw_args[2..].to_vec()
        };
        serv_cmd(&rest);
        std::process::exit(0);
    }

    let (mode, explicit_json, args) = detect_output_mode(raw_args);

    if args.len() < 2 {
        eprintln!("Usage: ilo <file-or-code> [args... | --run func args... | --bench func args... | --emit python]");
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
            println!("  ilo help lang                     Show language specification");
            println!("  ilo help ai | ilo -ai             Compact spec for LLM consumption");
            println!("  ilo --explain ILO-T005            Explain an error code\n");
            println!("Output format (errors):");
            println!("  --ansi / -a   Force ANSI colour output (default when stderr is a TTY)");
            println!("  --text / -t   Force plain text output (no colour)");
            println!("  --json / -j   Force JSON output (default when stderr is not a TTY)");
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
