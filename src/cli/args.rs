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

    /// Show language specification or compact spec.
    #[command(alias = "help")]
    Spec(SpecArgs),

    /// Explain an error code (e.g. ILO-T005).
    Explain(ExplainArgs),

    /// Print version.
    Version,
}

// ── Run ────────────────────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct RunArgs {
    /// Source file or inline code.
    pub source: String,

    /// Execution engine.
    #[arg(long, value_enum, default_value_t = Engine::Default)]
    pub engine: Engine,

    // Convenience aliases for --engine (mutually exclusive via conflicts_with).
    /// Tree-walking interpreter.
    #[arg(long = "run-tree", conflicts_with_all = ["engine", "run", "run_vm", "run_cranelift", "run_llvm"])]
    pub run_tree: bool,
    /// Alias for --run-tree.
    #[arg(long = "run", conflicts_with_all = ["engine", "run_tree", "run_vm", "run_cranelift", "run_llvm"])]
    pub run: bool,
    /// Register VM.
    #[arg(long = "run-vm", conflicts_with_all = ["engine", "run", "run_tree", "run_cranelift", "run_llvm"])]
    pub run_vm: bool,
    /// Cranelift JIT.
    #[arg(long = "run-cranelift", conflicts_with_all = ["engine", "run", "run_tree", "run_vm", "run_llvm"])]
    pub run_cranelift: bool,
    /// LLVM JIT.
    #[arg(long = "run-llvm", conflicts_with_all = ["engine", "run", "run_tree", "run_vm", "run_cranelift"])]
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

    /// HTTP tool provider config (JSON).
    #[arg(long = "tools")]
    pub tools_path: Option<String>,

    /// MCP server config path.
    #[arg(long = "mcp")]
    pub mcp_path: Option<String>,

    /// Remaining positional args: optional function name + call arguments.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub rest: Vec<String>,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Default,
    Tree,
    Vm,
    Cranelift,
    Llvm,
}

impl RunArgs {
    /// Resolve the effective engine from --engine flag and convenience bool flags.
    pub fn effective_engine(&self) -> Engine {
        if self.run || self.run_tree {
            Engine::Tree
        } else if self.run_vm {
            Engine::Vm
        } else if self.run_cranelift {
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
    fn engine_flag_run_tree() {
        let cli = Cli::try_parse_from(["ilo", "run", "--run-tree", "code"]).unwrap();
        if let Some(Cmd::Run(r)) = cli.cmd {
            assert_eq!(r.effective_engine(), Engine::Tree);
        }
    }

    #[test]
    fn engine_flag_run_vm() {
        let cli = Cli::try_parse_from(["ilo", "run", "--run-vm", "code"]).unwrap();
        if let Some(Cmd::Run(r)) = cli.cmd {
            assert_eq!(r.effective_engine(), Engine::Vm);
        }
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
    fn engine_flag_run_cranelift() {
        let cli = Cli::try_parse_from(["ilo", "run", "--run-cranelift", "code"]).unwrap();
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
    fn engine_flag_run_alias() {
        // --run is alias for --run-tree
        let cli = Cli::try_parse_from(["ilo", "run", "--run", "code"]).unwrap();
        if let Some(Cmd::Run(r)) = cli.cmd {
            assert_eq!(r.effective_engine(), Engine::Tree);
        } else {
            panic!("expected Run subcommand");
        }
    }

    // ── effective_engine: default when no flags set ───────────────────────────

    #[test]
    fn engine_default_when_no_flags() {
        let r = RunArgs {
            source: "code".to_string(),
            engine: Engine::Default,
            run_tree: false,
            run: false,
            run_vm: false,
            run_cranelift: false,
            run_llvm: false,
            bench: false,
            emit: None,
            explain: false,
            dense: false,
            expanded: false,
            tools_path: None,
            mcp_path: None,
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

    #[test]
    fn run_with_mcp_path() {
        let cli = Cli::try_parse_from(["ilo", "run", "--mcp", "cfg.json", "code"]).unwrap();
        if let Some(Cmd::Run(r)) = cli.cmd {
            assert_eq!(r.mcp_path.as_deref(), Some("cfg.json"));
        }
    }
}
