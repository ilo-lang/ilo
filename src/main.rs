#![warn(clippy::all)]

mod ast;
mod codegen;
mod diagnostic;
mod interpreter;
mod lexer;
mod parser;
mod verify;
mod vm;

use diagnostic::{Diagnostic, ansi::AnsiRenderer, json};

/// Compact spec for LLM consumption — generated from SPEC.md at compile time.
fn compact_spec() -> &'static str {
    include_str!(concat!(env!("OUT_DIR"), "/spec_ai.txt"))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    Ansi,
    Text,
    Json,
}

/// Scan args for --json/-j, --text/-t, --ansi/-a. Return (mode, remaining_args).
/// Multiple format flags → error + exit(1).
fn detect_output_mode(args: Vec<String>) -> (OutputMode, Vec<String>) {
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

    (resolved, remaining)
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
    let (mode, args) = detect_output_mode(raw_args);

    if args.len() < 2 {
        eprintln!("Usage: ilo <file-or-code> [args... | --run func args... | --bench func args... | --emit python]");
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
        print!("{}", codegen::explain::explain(&program));
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
        match vm::run(&compiled, func_name, run_args) {
            Ok(val) => println!("{}", val),
            Err(e) => {
                report_diagnostic(&Diagnostic::from(&e).with_source(source.clone()), mode);
                std::process::exit(1);
            }
        }
    } else if args.len() > m && (args[m] == "--run" || args[m] == "--run-interp") {
        // --run / --run-interp [func] [args...]
        let func_name = if args.len() > m + 1 { Some(args[m + 1].as_str()) } else { None };
        let run_args: Vec<interpreter::Value> = if args.len() > m + 2 {
            args[m + 2..].iter().map(|a| parse_cli_arg(a)).collect()
        } else {
            vec![]
        };

        match interpreter::run(&program, func_name, run_args) {
            Ok(val) => println!("{}", val),
            Err(e) => {
                report_diagnostic(&Diagnostic::from(&e).with_source(source.clone()), mode);
                std::process::exit(1);
            }
        }
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
        run_default(&program, func_name, run_args, &source, mode);
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

fn run_default(program: &ast::Program, func_name: Option<&str>, args: Vec<interpreter::Value>, source: &str, mode: OutputMode) {
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
                    println!("{}", result);
                    return;
                }
            }
        }
    }

    // Fall back to interpreter
    match interpreter::run(program, func_name, args) {
        Ok(val) => println!("{}", val),
        Err(e) => {
            report_diagnostic(&Diagnostic::from(&e).with_source(source.to_string()), mode);
            std::process::exit(1);
        }
    }
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
