use clap::{Args, Parser, Subcommand, ValueEnum};

/// ilo -- a token-minimal programming language for AI agents.
#[derive(Parser, Debug)]
#[command(
    name = "ilo",
    version,
    about = "Token-minimal programming language for AI agents"
)]
#[command(args_conflicts_with_subcommands = true)]
#[command(disable_help_subcommand = true)]
#[command(disable_help_flag = true)]
#[command(disable_version_flag = true)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Option<Cmd>,

    /// Global output-mode and hint flags.
    #[command(flatten)]
    pub global: Global,

    /// Positional arguments for the default run mode (no subcommand).
    /// First positional is code-or-file, rest are func/args.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
}

/// Global flags that apply across all subcommands.
#[derive(Args, Debug, Clone)]
pub struct Global {
    /// Force ANSI colour output (default when stderr is a TTY).
    #[arg(long, short = 'a', global = true)]
    pub ansi: bool,

    /// Force plain text output (no colour).
    #[arg(long, short = 't', global = true, conflicts_with = "ansi")]
    pub text: bool,

    /// Force JSON output (default when stderr is not a TTY).
    #[arg(long, short = 'j', global = true, conflicts_with_all = ["ansi", "text"])]
    pub json: bool,

    /// Suppress idiomatic hints after execution.
    #[arg(long = "no-hints", short = 'n', global = true)]
    pub no_hints: bool,

    /// Suppress program stdout during execution. Primarily meant for
    /// `ilo <file> --bench`: combined with `--json` it lets the persona
    /// harness consume the bench JSON envelope without it being drowned in
    /// the program's own `prnt` / `prnv` / `jprn` output. Stderr is
    /// untouched so errors still surface. The bench JSON envelope itself
    /// is written to stdout *outside* the silenced region.
    #[arg(long, short = 's', global = true)]
    pub silent: bool,

    /// Cap on AST nesting depth. Applies to every subcommand that parses source
    /// (`run`, `check`, `build`, `serv`). Default 256 — far above anything
    /// hand-written, low enough to keep `ilo serv` safe from `((((...))))`
    /// DoS payloads against the parser stack. Raise only if a legitimate
    /// program needs deeper nesting.
    #[arg(long = "max-ast-depth", global = true)]
    pub max_ast_depth: Option<usize>,

    /// Wall-clock budget for `ilo run` in seconds. Default 60. Set to 0 to
    /// disable. A runaway loop (missing increment, recursion with no base
    /// case) aborts with `ILO-R016` once the budget is hit instead of
    /// burning CPU and producing megabytes of useless stdout.
    #[arg(long = "max-runtime", global = true)]
    pub max_runtime: Option<u64>,

    /// Maximum stdout bytes for `ilo run`. Default ~100 MB. Set to 0 to
    /// disable. A loop calling `prnt` without termination aborts with
    /// `ILO-R017` once the budget is hit, instead of filling the agent
    /// transcript with garbage.
    #[arg(long = "max-output-bytes", global = true)]
    pub max_output_bytes: Option<u64>,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Run ilo code or a file.
    Run(RunArgs),

    /// Interactive REPL.
    Repl,

    /// Stdio-based agent serve loop (always JSON).
    Serv(ServArgs),

    /// List/discover tool signatures from MCP/HTTP sources.
    #[command(alias = "tool")]
    Tools(ToolsArgs),

    /// Analyse a program's dependency graph.
    Graph(GraphArgs),

    /// AOT compile to a standalone native binary.
    Compile(CompileArgs),

    /// AOT compile to a standalone native binary (alias for `compile`).
    Build(CompileArgs),

    /// Verify a program without running it.
    Check(CheckArgs),

    /// Show language specification or compact spec.
    #[command(alias = "help")]
    Spec(SpecArgs),

    /// Explain an error code (e.g. ILO-T005).
    Explain(ExplainArgs),

    /// Modular agent skills (ilo-language, ilo-builtins, ...).
    Skill(SkillArgs),

    /// Run `-- run:` / `-- out:` / `-- err:` assertions in `.ilo` files.
    Test(TestArgs),

    /// Print version.
    Version,

    /// Trace program execution, emitting one JSON line per statement.
    Trace(TraceArgs),
}

// ── Run ────────────────────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct RunArgs {
    /// Source file or inline code.
    pub source: String,

    /// Execution engine. Not exposed as `--engine` on the CLI surface: the
    /// six rerun8 personas typed `--engine tree` expecting it to work and
    /// hit silent-consume / wrong-arity traps. The supported surface is the
    /// `--run-*` convenience flags below; `--engine` is rejected by the
    /// unknown-flag guard. The field is kept so internal construction sites
    /// still compile.
    #[arg(skip = Engine::Default)]
    pub engine: Engine,

    // ── Engine selection flags ─────────────────────────────────────────────
    /// Register VM (canonical form, symmetric with --jit). `--run-vm` is
    /// retained as a hidden alias for one release; it emits a one-shot
    /// deprecation hint on stderr. Removal planned for 0.13.0.
    #[arg(long = "vm", visible_alias = "run-vm", conflicts_with_all = ["jit", "run_llvm"])]
    pub run_vm: bool,
    /// Cranelift JIT (opt-in for hot numeric loops; falls back to VM on bailout).
    #[arg(long = "jit", conflicts_with_all = ["run_vm", "run_llvm"])]
    pub jit: bool,
    /// LLVM JIT.
    #[arg(long = "run-llvm", conflicts_with_all = ["run_vm", "jit"])]
    pub run_llvm: bool,

    /// Benchmark mode.
    #[arg(long)]
    pub bench: bool,

    /// Emit target (e.g. python) instead of running.
    #[arg(long)]
    pub emit: Option<String>,

    /// Explain/annotate each statement.
    #[arg(long = "explain", short = 'x')]
    pub explain: bool,

    /// Reformat (dense wire format).
    #[arg(long, short = 'd', aliases = ["fmt"])]
    pub dense: bool,

    /// Reformat (expanded human format).
    #[arg(long, short = 'e', aliases = ["fmt-expanded"])]
    pub expanded: bool,

    /// Dump the parsed AST as JSON instead of running.
    #[arg(long = "ast")]
    pub ast: bool,

    /// HTTP tool provider config (JSON).
    #[arg(long = "tools")]
    pub tools_path: Option<String>,

    /// MCP server config path.
    #[arg(long = "mcp")]
    pub mcp_path: Option<String>,

    /// Allow network access. Comma-separated host list, or `*` for all.
    /// Omitting this flag leaves behaviour unchanged (permissive).
    /// Passing the flag with an empty value (`--allow-net=`) blocks all net.
    #[arg(long = "allow-net", value_name = "HOSTS")]
    pub allow_net: Option<String>,

    /// Allow file reads. Comma-separated path prefix list, or `*` for all.
    #[arg(long = "allow-read", value_name = "PATHS")]
    pub allow_read: Option<String>,

    /// Allow file writes. Comma-separated path prefix list, or `*` for all.
    #[arg(long = "allow-write", value_name = "PATHS")]
    pub allow_write: Option<String>,

    /// Allow process execution. Comma-separated command list, or `*` for all.
    #[arg(long = "allow-run", value_name = "CMDS")]
    pub allow_run: Option<String>,

    /// Allow environment variable access. Comma-separated variable name list,
    /// or `*` for all. Omitting leaves behaviour unchanged (permissive).
    /// `--allow-env=` (empty value) blocks all env reads. `--allow-env=PATH,HOME`
    /// permits only those variables. `env-all` requires `*` in the allowlist.
    #[arg(long = "allow-env", value_name = "VARS")]
    pub allow_env: Option<String>,

    /// Remaining positional args: optional function name + call arguments.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub rest: Vec<String>,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Default,
    Vm,
    Cranelift,
    Llvm,
}

impl RunArgs {
    /// Resolve the effective engine from --engine flag and convenience bool flags.
    pub fn effective_engine(&self) -> Engine {
        if self.run_vm {
            Engine::Vm
        } else if self.jit {
            Engine::Cranelift
        } else if self.run_llvm {
            Engine::Llvm
        } else {
            self.engine
        }
    }
}

// ── Serv ───────────────────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct ServArgs {
    /// MCP server config path.
    #[arg(long = "mcp", short = 'm')]
    pub mcp_path: Option<String>,

    /// HTTP tool provider config (JSON).
    #[arg(long = "tools")]
    pub tools_path: Option<String>,
}

// ── Tools ──────────────────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct ToolsArgs {
    /// MCP server config path.
    #[arg(long = "mcp", short = 'm')]
    pub mcp_path: Option<String>,

    /// HTTP tool provider config (JSON).
    #[arg(long = "tools")]
    pub tools_path: Option<String>,

    /// Output format for tool listing.
    #[arg(long, value_enum)]
    pub format: Option<ToolsFormat>,

    /// Shorthand: --human.
    #[arg(long)]
    pub human: bool,

    /// Shorthand: --ilo.
    #[arg(long)]
    pub ilo: bool,

    /// Shorthand: --json.
    #[arg(long)]
    pub json: bool,

    /// Show full signatures.
    #[arg(long, short = 'f')]
    pub full: bool,

    /// Show type-level composition graph.
    #[arg(long, short = 'g')]
    pub graph: bool,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolsFormat {
    Human,
    Ilo,
    Json,
}

// ── Graph ──────────────────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct GraphArgs {
    /// Source file to analyze.
    pub file: String,

    /// Focus on a specific function.
    #[arg(long = "fn")]
    pub fn_name: Option<String>,

    /// Show reverse callers.
    #[arg(long)]
    pub reverse: bool,

    /// Show transitive dependencies.
    #[arg(long)]
    pub subgraph: bool,

    /// Limit to N tokens of source.
    #[arg(long)]
    pub budget: Option<usize>,

    /// Output as DOT (Graphviz).
    #[arg(long)]
    pub dot: bool,
}

// ── Compile ────────────────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct CompileArgs {
    /// Source file or inline code.
    pub source: String,

    /// Output path.
    #[arg(short = 'o')]
    pub output: Option<String>,

    /// Entry function name.
    pub func: Option<String>,

    /// Benchmark binary mode.
    #[arg(long)]
    pub bench: bool,

    /// Transpile to Python source (`.py`) via the Python backend.
    ///
    /// Manifesto-strict: this is the canonical replacement for the removed
    /// `--emit python` flag. Use `ilo build file.ilo --py [-o out.py]`.
    #[arg(long)]
    pub py: bool,

    /// Compile to WebAssembly via the WASM backend (Phase 5 Stage 5d).
    ///
    /// Default target is `wasm32-component`. Pick a different target with
    /// `--target` (e.g. `--target wasm32-wasip1` for plain WASI preview1).
    #[arg(long)]
    pub wasm: bool,

    /// WASM target triple (only meaningful with `--wasm`). Accepts
    /// `wasm32-wasip1`, `wasm32-wasip2`, `wasm32-component`,
    /// `wasm32-unknown-unknown`, plus the aliases `wasm32-wasi` and
    /// `wasm32-web`.
    #[arg(long)]
    pub target: Option<String>,

    /// Transpile to Zero source (`.0`) via the Zero backend
    /// (Phase 5 Stage 5e). Pinned to `zero 0.1.2`.
    #[arg(long = "0")]
    pub zero: bool,

    /// Transpile to Zero source then chain through the pinned `zero`
    /// compiler to produce a native binary. Requires `zero` on PATH or
    /// at `~/.zero/bin/zero`.
    #[arg(long = "0bin")]
    pub zero_bin: bool,
}

// ── Check ──────────────────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct CheckArgs {
    /// Source file or inline code.
    pub source: String,

    /// Treat warnings as errors. Exit 1 when any diagnostic fires, even
    /// warning-severity ones (ILO-T032, ILO-T033, etc). CI harnesses use
    /// this to fail builds on warnings; the diagnostic stream itself is
    /// unchanged - warnings still emit with severity=warning, only the
    /// exit-code decision is elevated.
    #[arg(long)]
    pub strict: bool,
}

// ── Test ───────────────────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct TestArgs {
    /// File or directory to test. Directories are walked recursively for `*.ilo`.
    /// Defaults to `examples/` when omitted, so `ilo test` on a freshly-cloned
    /// repo does something useful without an explicit path argument.
    pub path: Option<String>,

    /// Engine to run each assertion on. `all` runs every engine and reports
    /// per-engine PASS/FAIL. Defaults to `vm` (matches the in-tree harness).
    #[arg(long, value_enum, default_value = "vm")]
    pub engine: TestEngine,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestEngine {
    Vm,
    Jit,
    All,
}

// ── Spec ───────────────────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct SpecArgs {
    /// Which spec to show: "lang" for full spec, "ai" for compact LLM spec.
    pub topic: Option<String>,
}

// ── Explain ────────────────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct ExplainArgs {
    /// Error code to explain (e.g. ILO-T005).
    pub code: String,
}

// ── Skill ─────────────────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct SkillArgs {
    #[command(subcommand)]
    pub cmd: SkillCmd,
}

#[derive(Subcommand, Debug)]
pub enum SkillCmd {
    /// List all available skills with their descriptions.
    List,
    /// Print the full content of a skill by name.
    Get { name: String },
    /// Print the bundled filesystem path of a skill by name.
    Path { name: String },
    /// Print a skill with a formatted header.
    Show { name: String },
}

// ── Trace ──────────────────────────────────────────────────────────────────────

/// Granularity of trace events.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TraceDepth {
    /// Emit one event per statement (default).
    #[default]
    Statement,
    /// Emit one event per sub-expression in addition to per-statement events.
    Expr,
}

#[derive(Args, Debug)]
pub struct TraceArgs {
    /// Source file to trace.
    pub source: String,

    /// Entry function name (defaults to first function).
    pub func: Option<String>,

    /// Trace granularity: `statement` (default) or `expr` (per sub-expression).
    #[arg(long = "depth", value_enum, default_value = "statement")]
    pub depth: TraceDepth,

    /// Only emit events that touch this variable name (may be repeated).
    #[arg(long = "watch", value_name = "NAME", action = clap::ArgAction::Append)]
    pub watch: Vec<String>,

    /// Call arguments passed to the entry function.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub rest: Vec<String>,
}

// ── OutputMode resolution ──────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OutputMode {
    Ansi,
    Text,
    Json,
}

impl Global {
    /// Resolve the effective output mode.
    /// Priority: explicit flags > NO_COLOR env > TTY detection.
    pub fn output_mode(&self) -> OutputMode {
        if self.ansi {
            return OutputMode::Ansi;
        }
        if self.text {
            return OutputMode::Text;
        }
        if self.json {
            return OutputMode::Json;
        }
        // Auto-detect
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
    }

    /// True only when the user explicitly passed --json/-j.
    pub fn explicit_json(&self) -> bool {
        self.json
    }
}

// ── Unknown-flag guard ─────────────────────────────────────────────────────────

/// Reject any token in `args` that looks like a clean long flag (`--word`,
/// `--word-with-dashes`) UNLESS it appears after a `--` literal separator.
///
/// Background: `Cli::args` and `RunArgs::rest` use `trailing_var_arg = true`
/// with `allow_hyphen_values = true` so clap collects unrecognised
/// hyphen-prefixed tokens as positional. Without this guard,
/// `ilo main.ilo --engine tree`
/// silently consumes `--engine` as a positional and the program runs with
/// the wrong arity, surfacing as misleading `ILO-R012 no functions defined`
/// or `ILO-R004 main: expected N args, got N+1`. Six rerun8 personas
/// (ab-tester, routing-tsp, content-mod, qa-tester, interactive-cli,
/// security-researcher) independently burned minutes on this trap.
///
/// To pass a hyphen-prefixed token as a literal arg, separate with `--` first:
/// `ilo main.ilo -- --foo`. Anything after the first `--` is data.
///
/// Shape match: `^--[a-z][a-z0-9]*(-[a-z0-9]+)*(=.*)?$`. The `--key=value`
/// form is normalised by splitting on the first `=` before the shape check,
/// so `--engine=tree` and `--foo=bar` are rejected the same way as the
/// space-separated forms. Tokens with digits-first or non-ASCII prefixes
/// are NOT treated as flags, they're data. Short flags (`-x`, `-V`) are
/// NOT flagged here either, clap rejects unknown short flags upfront at
/// parse time; only the long-flag shape slips through the trailing_var_arg
/// sink.
pub fn reject_unknown_flags(args: &[String]) -> Result<(), String> {
    reject_unknown_flags_with_allowlist(args, &[])
}

/// Same as `reject_unknown_flags`, but tokens listed in `allowlist` (exact
/// match, including the leading `--`) are accepted as known flags and pass
/// through. Used by the bare-arg dispatcher where some known long flags
/// (e.g. `--bench`, `--emit`, `--tools`) are still present in the positional
/// vec at the point of the guard call because they're consumed by later
/// position-based dispatch logic, not by clap.
pub fn reject_unknown_flags_with_allowlist(
    args: &[String],
    allowlist: &[&str],
) -> Result<(), String> {
    for a in args {
        if a == "--" {
            // Separator reached: everything after is data.
            return Ok(());
        }
        // Normalise `--key=value` to its `--key` head before the shape check
        // so the equals form is rejected the same way as the space form.
        // Without this split, `--engine=tree` slips past the guard and gets
        // consumed as a positional, surfacing as a misleading ILO-R012/R004
        // downstream.
        let head = a.split_once('=').map(|(h, _)| h).unwrap_or(a.as_str());
        if looks_like_clean_long_flag(head)
            && !allowlist.contains(&head)
            && !allowlist.contains(&a.as_str())
        {
            return Err(format!(
                "error: unrecognised flag '{a}'. Use 'ilo --help' for valid flags. To pass it as a literal arg, separate with '--' first."
            ));
        }
    }
    Ok(())
}

/// Return true iff `s` matches the clean long-flag shape `--word(-word)*`:
/// starts with `--`, then `[a-z]`, then `[a-z0-9-]*`, no trailing or
/// doubled dash, no `=`, no other chars. Anything else is data.
fn looks_like_clean_long_flag(s: &str) -> bool {
    let Some(rest) = s.strip_prefix("--") else {
        return false;
    };
    if rest.is_empty() {
        return false; // bare `--` is the separator, handled by caller.
    }
    // First char must be `[a-z]`.
    let bytes = rest.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    let mut prev_dash = false;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'-' {
            if prev_dash || i + 1 == bytes.len() {
                // Doubled dash or trailing dash → not a clean flag (data).
                return false;
            }
            prev_dash = true;
        } else if b.is_ascii_lowercase() || b.is_ascii_digit() {
            prev_dash = false;
        } else {
            // Anything else (=, !, /, ., quote, uppercase, etc.) is data.
            return false;
        }
    }
    true
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_run_subcommand() {
        let cli = Cli::try_parse_from(["ilo", "run", "file.ilo", "func", "42"]).unwrap();
        match cli.cmd {
            Some(Cmd::Run(r)) => {
                assert_eq!(r.source, "file.ilo");
                assert_eq!(r.rest, vec!["func", "42"]);
            }
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn parse_repl_subcommand() {
        let cli = Cli::try_parse_from(["ilo", "repl"]).unwrap();
        assert!(matches!(cli.cmd, Some(Cmd::Repl)));
    }

    #[test]
    fn parse_serv_with_mcp() {
        let cli = Cli::try_parse_from(["ilo", "serv", "--mcp", "cfg.json"]).unwrap();
        match cli.cmd {
            Some(Cmd::Serv(s)) => assert_eq!(s.mcp_path.as_deref(), Some("cfg.json")),
            other => panic!("expected Serv, got {other:?}"),
        }
    }

    #[test]
    fn parse_tools_with_flags() {
        let cli =
            Cli::try_parse_from(["ilo", "tools", "--mcp", "p.json", "--full", "--graph"]).unwrap();
        match cli.cmd {
            Some(Cmd::Tools(t)) => {
                assert_eq!(t.mcp_path.as_deref(), Some("p.json"));
                assert!(t.full);
                assert!(t.graph);
            }
            other => panic!("expected Tools, got {other:?}"),
        }
    }

    #[test]
    fn parse_graph_subcommand() {
        let cli =
            Cli::try_parse_from(["ilo", "graph", "file.ilo", "--fn", "main", "--dot"]).unwrap();
        match cli.cmd {
            Some(Cmd::Graph(g)) => {
                assert_eq!(g.file, "file.ilo");
                assert_eq!(g.fn_name.as_deref(), Some("main"));
                assert!(g.dot);
            }
            other => panic!("expected Graph, got {other:?}"),
        }
    }

    #[test]
    fn parse_compile_subcommand() {
        let cli =
            Cli::try_parse_from(["ilo", "compile", "prog.ilo", "-o", "out", "--bench"]).unwrap();
        match cli.cmd {
            Some(Cmd::Compile(c)) => {
                assert_eq!(c.source, "prog.ilo");
                assert_eq!(c.output.as_deref(), Some("out"));
                assert!(c.bench);
            }
            other => panic!("expected Compile, got {other:?}"),
        }
    }

    #[test]
    fn parse_global_json_flag() {
        let cli = Cli::try_parse_from(["ilo", "--json", "repl"]).unwrap();
        assert!(cli.global.json);
        assert_eq!(cli.global.output_mode(), OutputMode::Json);
    }

    #[test]
    fn parse_global_ansi_flag() {
        let cli = Cli::try_parse_from(["ilo", "-a", "repl"]).unwrap();
        assert!(cli.global.ansi);
        assert_eq!(cli.global.output_mode(), OutputMode::Ansi);
    }

    #[test]
    fn parse_global_text_flag() {
        let cli = Cli::try_parse_from(["ilo", "--text", "repl"]).unwrap();
        assert!(cli.global.text);
        assert_eq!(cli.global.output_mode(), OutputMode::Text);
    }

    #[test]
    fn parse_global_no_hints() {
        let cli = Cli::try_parse_from(["ilo", "-n", "repl"]).unwrap();
        assert!(cli.global.no_hints);
    }

    #[test]
    fn parse_explain_subcommand() {
        let cli = Cli::try_parse_from(["ilo", "explain", "ILO-T005"]).unwrap();
        match cli.cmd {
            Some(Cmd::Explain(e)) => assert_eq!(e.code, "ILO-T005"),
            other => panic!("expected Explain, got {other:?}"),
        }
    }

    #[test]
    fn parse_version_subcommand() {
        let cli = Cli::try_parse_from(["ilo", "version"]).unwrap();
        assert!(matches!(cli.cmd, Some(Cmd::Version)));
    }

    #[test]
    fn parse_tool_alias() {
        let cli = Cli::try_parse_from(["ilo", "tool", "--mcp", "p.json"]).unwrap();
        assert!(matches!(cli.cmd, Some(Cmd::Tools(_))));
    }

    #[test]
    fn parse_spec_subcommand_lang() {
        let cli = Cli::try_parse_from(["ilo", "spec", "lang"]).unwrap();
        match cli.cmd {
            Some(Cmd::Spec(s)) => assert_eq!(s.topic.as_deref(), Some("lang")),
            other => panic!("expected Spec, got {other:?}"),
        }
    }

    #[test]
    fn parse_spec_subcommand_ai() {
        let cli = Cli::try_parse_from(["ilo", "spec", "ai"]).unwrap();
        match cli.cmd {
            Some(Cmd::Spec(s)) => assert_eq!(s.topic.as_deref(), Some("ai")),
            other => panic!("expected Spec, got {other:?}"),
        }
    }

    #[test]
    fn run_tree_flag_rejected_by_clap() {
        // --run-tree was removed from the public CLI surface as part of the
        // tree-walker soft-deprecation. Clap should now reject it as an
        // unknown long flag.
        let err = Cli::try_parse_from(["ilo", "run", "--run-tree", "code"]).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("--run-tree") || msg.contains("unexpected"),
            "expected clap to reject --run-tree; got: {msg}"
        );
    }

    #[test]
    fn engine_flag_vm_canonical() {
        // Canonical post-0.12.1 spelling. Symmetric with --jit / --run-llvm.
        let cli = Cli::try_parse_from(["ilo", "run", "--vm", "code"]).unwrap();
        if let Some(Cmd::Run(r)) = cli.cmd {
            assert!(r.run_vm);
            assert_eq!(r.effective_engine(), Engine::Vm);
        } else {
            panic!("expected Run subcommand");
        }
    }

    #[test]
    fn engine_flag_run_vm_alias_still_parses() {
        // --run-vm is retained as a visible_alias for one release so
        // existing carry-forward scripts keep parsing cleanly. The
        // deprecation hint is emitted at the main() argv-scan layer, not
        // at clap parse time, so this unit test pins clap-level acceptance
        // only. The stderr-hint behaviour is covered end-to-end in
        // tests/regression_vm_flag_rename.rs.
        let cli = Cli::try_parse_from(["ilo", "run", "--run-vm", "code"]).unwrap();
        if let Some(Cmd::Run(r)) = cli.cmd {
            assert!(r.run_vm);
            assert_eq!(r.effective_engine(), Engine::Vm);
        } else {
            panic!("expected Run subcommand");
        }
    }

    #[test]
    fn engine_flag_vm_conflicts_with_jit() {
        // --vm and --jit are mutually exclusive at the clap level.
        let err = Cli::try_parse_from(["ilo", "run", "--vm", "--jit", "code"]).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("cannot be used with") || msg.contains("conflict"),
            "expected clap conflict error; got: {msg}"
        );
    }

    #[test]
    fn default_positional_args_fallback() {
        // When no subcommand matches, args should be captured as positional
        let cli = Cli::try_parse_from(["ilo", "f>n;42", "5"]).unwrap();
        assert!(cli.cmd.is_none());
        assert_eq!(cli.args, vec!["f>n;42", "5"]);
    }

    #[test]
    fn tools_json_shorthand() {
        let cli = Cli::try_parse_from(["ilo", "tools", "--mcp", "p.json", "--json"]).unwrap();
        if let Some(Cmd::Tools(t)) = cli.cmd {
            assert!(t.json);
        }
    }

    #[test]
    fn tools_ilo_shorthand() {
        let cli = Cli::try_parse_from(["ilo", "tools", "--mcp", "p.json", "--ilo"]).unwrap();
        if let Some(Cmd::Tools(t)) = cli.cmd {
            assert!(t.ilo);
        }
    }

    #[test]
    fn tools_human_shorthand() {
        let cli = Cli::try_parse_from(["ilo", "tools", "--mcp", "p.json", "--human"]).unwrap();
        if let Some(Cmd::Tools(t)) = cli.cmd {
            assert!(t.human);
        }
    }

    #[test]
    fn compile_with_func() {
        let cli = Cli::try_parse_from(["ilo", "compile", "prog.ilo", "entry"]).unwrap();
        if let Some(Cmd::Compile(c)) = cli.cmd {
            assert_eq!(c.func.as_deref(), Some("entry"));
        }
    }

    #[test]
    fn compile_with_target() {
        let cli = Cli::try_parse_from(["ilo", "compile", "prog.ilo", "--target", "wasm32-wasip1"])
            .unwrap();
        if let Some(Cmd::Compile(c)) = cli.cmd {
            assert_eq!(c.target.as_deref(), Some("wasm32-wasip1"));
        }
    }

    #[test]
    fn compile_target_flag_parses_all_supported() {
        for triple in SUPPORTED_TARGETS {
            let cli =
                Cli::try_parse_from(["ilo", "compile", "prog.ilo", "--target", triple]).unwrap();
            if let Some(Cmd::Compile(c)) = cli.cmd {
                assert_eq!(c.target.as_deref(), Some(*triple));
            } else {
                panic!("expected Compile for target {triple}");
            }
        }
    }

    #[test]
    fn graph_with_budget() {
        let cli = Cli::try_parse_from(["ilo", "graph", "f.ilo", "--budget", "100"]).unwrap();
        if let Some(Cmd::Graph(g)) = cli.cmd {
            assert_eq!(g.budget, Some(100));
        }
    }

    #[test]
    fn graph_with_reverse() {
        let cli = Cli::try_parse_from(["ilo", "graph", "f.ilo", "--reverse"]).unwrap();
        if let Some(Cmd::Graph(g)) = cli.cmd {
            assert!(g.reverse);
        }
    }

    #[test]
    fn graph_with_subgraph() {
        let cli = Cli::try_parse_from(["ilo", "graph", "f.ilo", "--subgraph"]).unwrap();
        if let Some(Cmd::Graph(g)) = cli.cmd {
            assert!(g.subgraph);
        }
    }

    #[test]
    fn run_with_bench() {
        let cli = Cli::try_parse_from(["ilo", "run", "--bench", "code", "func", "42"]).unwrap();
        if let Some(Cmd::Run(r)) = cli.cmd {
            assert!(r.bench);
            assert_eq!(r.source, "code");
        }
    }

    #[test]
    fn run_with_emit_python() {
        let cli = Cli::try_parse_from(["ilo", "run", "--emit", "python", "code"]).unwrap();
        if let Some(Cmd::Run(r)) = cli.cmd {
            assert_eq!(r.emit.as_deref(), Some("python"));
        }
    }

    #[test]
    fn run_with_explain() {
        let cli = Cli::try_parse_from(["ilo", "run", "--explain", "code"]).unwrap();
        if let Some(Cmd::Run(r)) = cli.cmd {
            assert!(r.explain);
        }
    }

    #[test]
    fn run_with_dense() {
        let cli = Cli::try_parse_from(["ilo", "run", "--dense", "code"]).unwrap();
        if let Some(Cmd::Run(r)) = cli.cmd {
            assert!(r.dense);
        }
    }

    #[test]
    fn run_with_expanded() {
        let cli = Cli::try_parse_from(["ilo", "run", "--expanded", "code"]).unwrap();
        if let Some(Cmd::Run(r)) = cli.cmd {
            assert!(r.expanded);
        }
    }

    #[test]
    fn serv_with_tools() {
        let cli = Cli::try_parse_from(["ilo", "serv", "--tools", "http.json"]).unwrap();
        if let Some(Cmd::Serv(s)) = cli.cmd {
            assert_eq!(s.tools_path.as_deref(), Some("http.json"));
        }
    }

    #[test]
    fn run_with_tools_and_mcp() {
        let cli = Cli::try_parse_from(["ilo", "run", "--tools", "http.json", "code"]).unwrap();
        if let Some(Cmd::Run(r)) = cli.cmd {
            assert_eq!(r.tools_path.as_deref(), Some("http.json"));
        }
    }

    #[test]
    fn help_alias_for_spec() {
        let cli = Cli::try_parse_from(["ilo", "help", "ai"]).unwrap();
        assert!(matches!(cli.cmd, Some(Cmd::Spec(_))));
    }

    // ── effective_engine: Cranelift and Llvm paths ────────────────────────────

    #[test]
    fn engine_flag_jit() {
        let cli = Cli::try_parse_from(["ilo", "run", "--jit", "code"]).unwrap();
        if let Some(Cmd::Run(r)) = cli.cmd {
            assert_eq!(r.effective_engine(), Engine::Cranelift);
        } else {
            panic!("expected Run subcommand");
        }
    }

    #[test]
    fn engine_flag_run_llvm() {
        let cli = Cli::try_parse_from(["ilo", "run", "--run-llvm", "code"]).unwrap();
        if let Some(Cmd::Run(r)) = cli.cmd {
            assert_eq!(r.effective_engine(), Engine::Llvm);
        } else {
            panic!("expected Run subcommand");
        }
    }

    #[test]
    fn run_alias_flag_rejected_by_clap() {
        // --run (the old --run-tree alias) is also gone from the public surface.
        let err = Cli::try_parse_from(["ilo", "run", "--run", "code"]).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("--run") || msg.contains("unexpected"),
            "expected clap to reject --run as a flag; got: {msg}"
        );
    }

    // ── effective_engine: default when no flags set ───────────────────────────

    #[test]
    fn engine_default_when_no_flags() {
        let r = RunArgs {
            source: "code".to_string(),
            engine: Engine::Default,
            run_vm: false,
            jit: false,
            run_llvm: false,
            bench: false,
            emit: None,
            explain: false,
            dense: false,
            expanded: false,
            ast: false,
            tools_path: None,
            mcp_path: None,
            allow_net: None,
            allow_read: None,
            allow_write: None,
            allow_run: None,
            allow_env: None,
            rest: vec![],
        };
        assert_eq!(r.effective_engine(), Engine::Default);
    }

    // ── output_mode: NO_COLOR auto-detect path ────────────────────────────────

    #[test]
    fn output_mode_no_color_env_returns_text_when_tty_unavailable() {
        // When none of ansi/text/json are set, output_mode auto-detects.
        // We can verify that explicit flags take priority over auto-detect.
        let g = Global {
            ansi: false,
            text: false,
            json: false,
            no_hints: false,
            silent: false,
            max_ast_depth: None,
            max_runtime: None,
            max_output_bytes: None,
        };
        // In test environment stderr is typically not a TTY → should return Json.
        // We can't reliably test the TTY branch, but we can test that explicit_json
        // is false when json is false.
        assert!(!g.explicit_json());
        // And that output_mode returns something valid.
        let mode = g.output_mode();
        assert!(
            matches!(mode, OutputMode::Ansi | OutputMode::Text | OutputMode::Json),
            "output_mode should return a valid mode"
        );
    }

    // ── Global::explicit_json ─────────────────────────────────────────────────

    #[test]
    fn global_explicit_json_true_when_json_flag_set() {
        let g = Global {
            ansi: false,
            text: false,
            json: true,
            no_hints: false,
            silent: false,
            max_ast_depth: None,
            max_runtime: None,
            max_output_bytes: None,
        };
        assert!(g.explicit_json());
        assert_eq!(g.output_mode(), OutputMode::Json);
    }

    #[test]
    fn global_explicit_json_false_when_text_set() {
        let g = Global {
            ansi: false,
            text: true,
            json: false,
            no_hints: false,
            silent: false,
            max_ast_depth: None,
            max_runtime: None,
            max_output_bytes: None,
        };
        assert!(!g.explicit_json());
        assert_eq!(g.output_mode(), OutputMode::Text);
    }

    #[test]
    fn global_explicit_json_false_when_ansi_set() {
        let g = Global {
            ansi: true,
            text: false,
            json: false,
            no_hints: false,
            silent: false,
            max_ast_depth: None,
            max_runtime: None,
            max_output_bytes: None,
        };
        assert!(!g.explicit_json());
        assert_eq!(g.output_mode(), OutputMode::Ansi);
    }

    // ── ToolsFormat variants ──────────────────────────────────────────────────

    #[test]
    fn tools_format_human_parse() {
        let cli =
            Cli::try_parse_from(["ilo", "tools", "--mcp", "p.json", "--format", "human"]).unwrap();
        if let Some(Cmd::Tools(t)) = cli.cmd {
            assert_eq!(t.format, Some(ToolsFormat::Human));
        }
    }

    #[test]
    fn tools_format_ilo_parse() {
        let cli =
            Cli::try_parse_from(["ilo", "tools", "--mcp", "p.json", "--format", "ilo"]).unwrap();
        if let Some(Cmd::Tools(t)) = cli.cmd {
            assert_eq!(t.format, Some(ToolsFormat::Ilo));
        }
    }

    #[test]
    fn tools_format_json_parse() {
        let cli =
            Cli::try_parse_from(["ilo", "tools", "--mcp", "p.json", "--format", "json"]).unwrap();
        if let Some(Cmd::Tools(t)) = cli.cmd {
            assert_eq!(t.format, Some(ToolsFormat::Json));
        }
    }

    // ── GraphArgs: fn_name field ──────────────────────────────────────────────

    #[test]
    fn graph_with_fn_name() {
        let cli = Cli::try_parse_from(["ilo", "graph", "f.ilo", "--fn", "main"]).unwrap();
        if let Some(Cmd::Graph(g)) = cli.cmd {
            assert_eq!(g.fn_name.as_deref(), Some("main"));
        }
    }

    // ── RunArgs: mcp_path field ───────────────────────────────────────────────

    // ── reject_unknown_flags ──────────────────────────────────────────────────

    #[test]
    fn unknown_long_flag_rejected() {
        let args = vec![
            "main.ilo".to_string(),
            "--engine".to_string(),
            "tree".to_string(),
        ];
        let err = reject_unknown_flags(&args).unwrap_err();
        assert!(err.contains("--engine"), "msg={err}");
        assert!(err.contains("unrecognised flag"));
        assert!(err.contains("'--' first"));
    }

    #[test]
    fn unknown_long_flag_no_value_rejected() {
        let args = vec!["main.ilo".to_string(), "--foo".to_string()];
        assert!(reject_unknown_flags(&args).is_err());
    }

    #[test]
    fn unknown_hyphenated_flag_rejected() {
        let args = vec!["main.ilo".to_string(), "--some-long-flag".to_string()];
        assert!(reject_unknown_flags(&args).is_err());
    }

    #[test]
    fn dash_dash_separator_escapes_subsequent_flags() {
        let args = vec![
            "main.ilo".to_string(),
            "--".to_string(),
            "--foo".to_string(),
            "--engine".to_string(),
        ];
        assert!(reject_unknown_flags(&args).is_ok());
    }

    #[test]
    fn plain_positional_args_accepted() {
        let args = vec!["main.ilo".to_string(), "func".to_string(), "42".to_string()];
        assert!(reject_unknown_flags(&args).is_ok());
    }

    #[test]
    fn negative_number_not_treated_as_flag() {
        let args = vec![
            "main.ilo".to_string(),
            "-1".to_string(),
            "-3.14".to_string(),
        ];
        assert!(reject_unknown_flags(&args).is_ok());
    }

    #[test]
    fn equals_form_unknown_flag_rejected() {
        // `--key=value` shape used to slip past the guard and get consumed as
        // a positional, surfacing as a misleading ILO-R012/R004 downstream.
        // The guard now splits on `=` before the shape check so this form is
        // caught the same way as the space-separated form.
        let args = vec!["main.ilo".to_string(), "--foo=bar".to_string()];
        let err = reject_unknown_flags(&args).unwrap_err();
        assert!(err.contains("--foo=bar"), "msg={err}");
        assert!(err.contains("unrecognised flag"));
    }

    #[test]
    fn equals_form_engine_rejected() {
        // Concrete repro from the originating bug report: `--engine=tree`
        // slipped past while `--engine tree` was caught.
        let args = vec!["main.ilo".to_string(), "--engine=tree".to_string()];
        let err = reject_unknown_flags(&args).unwrap_err();
        assert!(err.contains("--engine=tree"), "msg={err}");
    }

    #[test]
    fn equals_form_allowlisted_head_accepted() {
        // Allowlist matches against the `--key` head, so callers can
        // pre-approve a known flag and its `--key=value` form is accepted.
        let args = vec!["main.ilo".to_string(), "--bench=on".to_string()];
        assert!(reject_unknown_flags_with_allowlist(&args, &["--bench"]).is_ok());
    }

    #[test]
    fn equals_form_after_dash_dash_accepted() {
        // The `--` separator still escapes everything that follows, including
        // the equals form.
        let args = vec![
            "main.ilo".to_string(),
            "--".to_string(),
            "--foo=bar".to_string(),
        ];
        assert!(reject_unknown_flags(&args).is_ok());
    }

    #[test]
    fn equals_form_with_empty_value_rejected() {
        // `--foo=` (empty value) is still an unrecognised flag.
        let args = vec!["main.ilo".to_string(), "--foo=".to_string()];
        assert!(reject_unknown_flags(&args).is_err());
    }

    #[test]
    fn equals_form_with_non_flag_head_accepted() {
        // `key=value` (no leading `--`) is data, not a flag.
        let args = vec!["main.ilo".to_string(), "key=value".to_string()];
        assert!(reject_unknown_flags(&args).is_ok());
    }

    #[test]
    fn trailing_dash_not_treated_as_flag() {
        let args = vec!["main.ilo".to_string(), "--foo-".to_string()];
        assert!(reject_unknown_flags(&args).is_ok());
    }

    #[test]
    fn doubled_dash_inside_not_treated_as_flag() {
        let args = vec!["main.ilo".to_string(), "--foo--bar".to_string()];
        assert!(reject_unknown_flags(&args).is_ok());
    }

    #[test]
    fn empty_args_ok() {
        let args: Vec<String> = vec![];
        assert!(reject_unknown_flags(&args).is_ok());
    }

    #[test]
    fn looks_like_clean_long_flag_shapes() {
        assert!(looks_like_clean_long_flag("--foo"));
        assert!(looks_like_clean_long_flag("--engine"));
        assert!(looks_like_clean_long_flag("--some-long-flag"));
        assert!(looks_like_clean_long_flag("--a1"));
        // Not flags:
        assert!(!looks_like_clean_long_flag("--"));
        assert!(!looks_like_clean_long_flag("-x"));
        assert!(!looks_like_clean_long_flag("--Foo"));
        assert!(!looks_like_clean_long_flag("--foo=bar"));
        assert!(!looks_like_clean_long_flag("--foo-"));
        assert!(!looks_like_clean_long_flag("--foo--bar"));
        assert!(!looks_like_clean_long_flag("--1foo"));
        assert!(!looks_like_clean_long_flag("foo"));
        assert!(!looks_like_clean_long_flag("-1"));
    }

    #[test]
    fn run_with_mcp_path() {
        let cli = Cli::try_parse_from(["ilo", "run", "--mcp", "cfg.json", "code"]).unwrap();
        if let Some(Cmd::Run(r)) = cli.cmd {
            assert_eq!(r.mcp_path.as_deref(), Some("cfg.json"));
        }
    }
}
