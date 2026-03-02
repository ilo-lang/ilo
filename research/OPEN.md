# Open Questions

Unresolved design questions and lessons from syntax exploration. For design rationale, see [MANIFESTO.md](MANIFESTO.md). For syntax variants, see [research/explorations/](research/explorations/).

## Lessons From Syntax Experiments

### What saves tokens

Positional arguments are the single biggest token saver. `reserve(items:items)` → `reserve items` eliminates parens, colons, and repeated names. Most call sites become `verb arg arg`.

Implicit last-result matching saves both tokens and variable names. `x=call(...);match x{err e:...}` → `call arg;?{^e:...}` — no intermediate binding needed.

Single-char operators (`?`/`^`/`~`/`@`/`>`/`!`) replace keywords (`match`/`err`/`ok`/`for`/`->`) but save fewer tokens than expected — the tokenizer already encodes common English words as single tokens. The savings are mainly in characters.

### What doesn't save tokens

Short variable names (`ord` instead of `order`, `dc` instead of `discount`) save characters but not tokens. Common English words are already single tokens in cl100k_base. Unusual abbreviations sometimes split into multiple tokens, costing more. This is why idea8 and idea9 have nearly identical token counts (285 vs 287) despite idea9 being 114 chars shorter.

### Key tradeoff: tokens vs characters

Tokens and characters optimise differently. idea4-ast-bytecode is 0.67x tokens but 0.33x chars. idea8-ultra-dense is 0.33x tokens and 0.25x chars. The best formats score well on both, but the techniques that help each metric are different.

### Spec quality matters for generation

LLM generation accuracy depends heavily on spec clarity. Adding operator examples (showing `<`, `>`, `/` usage) and explicit comparison operator docs raised scores from 8/10 to 10/10. The spec is part of the prompt — it needs to be unambiguous.

## Execution Model

**Option A: Graph engine (verify → execute)**
The program is a graph of nodes (functions, types, tools). The runtime validates new nodes and executes by traversing edges. No compile step — each node is verified and live immediately.

**Option B: Tool orchestration engine**
The runtime is a workflow engine. ilo programs are DAGs of tool calls. The runtime executes the DAG, calling real external services.

**Option C: Transpilation**
ilo verifies the program then compiles to Python/JS/WASM for execution. Verification in ilo, execution in a mature runtime.

## Graph Loading Problem

"Agent gets the world upfront" has a cost: the world must be loaded into context. 500 tools and 200 types = thousands of tokens of spec before the agent writes a line.

**Option 1: Full graph** — load everything. Only works for small projects.

**Option 2: Subgraph by task** — something decides which slice is relevant. Question: who decides?

**Option 3: Query on demand** — agent starts with nothing, queries the runtime for what it needs. Total context cost: 2 tool signatures instead of 500.

**Option 4: Progressive disclosure** — load tool names first (cheap), load full signatures on demand.

## ilo as a Typed Shell

Not just a language — a **typed shell** for agents. Like bash discovers executables on `$PATH`, ilo discovers typed tools from configured sources and lets agents compose them with verified types and error handling.

The runtime's job: discover → present → verify → execute.

### What bash got right

Bash commands are mini programs. Each one is self-contained, has a universal interface (stdin/stdout/exit code), is discoverable on `$PATH`, and composes with any other command via `|`. This model has lasted 50 years because it works.

ilo functions follow the same shape:

| Bash | ilo |
|------|-----|
| Self-contained binary | Self-contained function with declared deps |
| stdin/stdout/stderr | Typed params → typed result (ok/err) |
| `$PATH` discovery | Tool graph registration |
| `cmd1 \| cmd2` | Sequential binding + `?` matching |
| Exit codes | Typed error variants |

The Unix philosophy maps directly: do one thing well (small units), expect output to become input (composable), don't require interactive input (agent-friendly).

### What bash got wrong for agents

- **No types** — everything is text. `jq` output looks the same as an error message.
- **Silent failures** — `curl` can fail and the pipeline continues with empty input.
- **Text parsing tax** — agents must generate `grep`, `awk`, `sed` patterns to extract structured data from text streams.
- **Quoting hell** — escaping rules are a token tax that causes retry loops.

### Where ilo already uses implicit composition

ilo's `?` operator works like an implicit pipe — the result of the previous call flows directly into the match without a variable binding:

```
get-user uid;?{^e:handle-error;~data:use-data}
```

This is equivalent to `get-user uid | match` in a hypothetical typed bash. No intermediate variable needed for single-use results.

Explicit binding is only needed when a value is referenced more than once or later:

```
rid=reserve items;charge pid amt;?{^e:release rid;...}
```

Here `rid` must be named because it's used in the error-compensation branch. Bash handles this with `tee` or temp files, which is worse.

### The sweet spot

ilo sits between bash and traditional languages:

- **Bash**: implicit pipes, no types, no verification, text everywhere
- **Traditional languages**: explicit everything, types, verbose, lots of ceremony
- **ilo**: implicit where safe (`?` matching), explicit where needed (multi-use values), types verified before execution

The composition model is Unix pipes with a type checker. Programs should feel like shell scripts — sequences of tool calls with branching — not like class hierarchies or module systems.

### Format parsing is a tool concern

ilo doesn't parse JSON, XML, HTML, or YAML. Tools do that. ilo composes typed tool results.

`curl -s url | pup 'h1 text{}'` is ~8 tokens to fetch and extract. It works because the parsing complexity lives in `pup`, not in bash. ilo follows the same pattern:

```
fetch url "h1";?{~t:t;^e:^e}
```

Curl-level conciseness, but verified. The agent can't call a tool that doesn't exist, return types flow into the next expression, and error handling is structural (`^e`) not textual (`|| echo "failed"`).

Compare the alternative — Python `requests` + `try/except` + `json.loads` + key checking — that's ~80 tokens for what ilo does in ~15. The token savings come from tools doing the heavy lifting while ilo provides the verified composition layer.

This means ilo needs exactly one data format at the tool boundary: **JSON ↔ Value mapping**, guided by the tool's declared return type. The tool declaration *is* the schema. MCP's `inputSchema`/`outputSchema` align directly — ilo records map to JSON Schema objects, ilo types map to JSON Schema primitives.

When the shape is unknown or too complex, the tool declares `>t` and the agent gets raw JSON as text — passable to another tool without parsing. When the shape is known, the tool declares `>R profile t` and the runtime maps the JSON response to a typed record, failing with a structured error if it doesn't match.

No new types needed: `_` handles null, `t` is the escape hatch for untyped data, records handle known shapes, `R ok err` handles fallible tools.

## The "Essential Packages" Principle

Every mainstream language has packages so universally installed they're effectively part of the language: Python's `requests`, Node's `lodash`/`express`, Ruby's `rails`/`nokogiri`, Go's `gorilla/mux`, Rust's `serde`/`tokio`. .NET absorbed `Newtonsoft.Json` into `System.Text.Json` after years of universal dependency.

The gap between what language designers thought was core and what developers actually install reveals what the language got wrong — or more charitably, what real-world usage demanded that the designers didn't anticipate.

**This matters for ilo because agents cannot install packages.** There is no `pip install` or `npm install`. Whatever an ilo program needs must be either a builtin or a declared tool. The "essential packages" of other languages tell us exactly what builtins ilo needs from day one.

Cross-language patterns that emerge:

| Capability | Python | JS/TS | Rust | Go | Ruby |
|-----------|--------|-------|------|-----|------|
| HTTP client | `requests` | `axios` | `reqwest` | stdlib | `httparty`/`faraday` |
| Data validation | `pydantic` | `zod` | `serde` | struct tags | Rails validators |
| Env vars from file | `python-dotenv` | `dotenv` | `dotenvy` | `godotenv` | `dotenv` |
| Date/time | stdlib (bad) | `dayjs`/`date-fns` | `chrono` | stdlib | `activesupport` |
| UUID generation | `uuid` | `uuid` | `uuid` | `google/uuid` | `securerandom` |
| CLI parsing | `click`/`typer` | `commander` | `clap` | `cobra` | `optparse` |
| Structured logging | `structlog` | `pino`/`winston` | `tracing` | `zap`/`zerolog` | `logger` |
| Testing | `pytest` | `jest`/`vitest` | built-in | `testify` | `rspec` |

**Key observations:**
- **HTTP client** is universally needed and universally underserved by stdlibs. ilo already has `get`/`$`; `post` is the obvious next builtin.
- **Env loading** (`dotenv`) exists in every ecosystem — proof that env var access is a core agent need, not a niche feature.
- **Data validation** (`pydantic`/`zod`/`serde`) is the most-installed category across languages. ilo's type system + tool declarations + verifier already serve this role — the type declaration IS the validation schema.
- **CLI parsing** is a human concern — agents don't need `--help` text or subcommands. ilo's positional function params already handle this.
- **Testing** is a human workflow concern. Agents don't write test suites; they generate correct programs verified before execution.

**The design heuristic:** if a capability requires a near-universal third-party package in 3+ mainstream languages, it belongs in ilo's builtin set (or as a declared tool with a standard name). If it's universal but human-facing (CLI parsing, testing, formatting), it doesn't.

See `research/essential-packages-analysis.md` for the full cross-language analysis.

## Syntax Questions (Resolved by Experiments)

These were open questions that the syntax experiments have now answered:

- **`let` keyword** — dropped entirely in idea7+. `x=expr` is unambiguous. Saves ~15 tokens per program.
- **`concat` operator** — `+` doubles as string concat in idea8+. One fewer keyword.
- **`for` syntax** — `@` in idea8+. Always produces a list. Statement-form iteration wasn't needed.
- **Named vs positional args** — positional wins for token efficiency. Named args at call sites were the biggest token cost in idea1.

## Still Open

### ~~Which syntax to build?~~ — Resolved: idea9

idea9-ultra-dense-short is the chosen syntax. See [/SPEC.md](../SPEC.md) for the canonical spec. The debugging concern (error messages pointing at dense code) is tracked in [TODO.md](../TODO.md) under Tooling.

### Hybrid approach?

Could the runtime accept multiple syntax levels — dense wire format for LLM generation, expanded form for human review — with lossless conversion between them? Same AST, different serialisations.

### Match exhaustiveness

Should the verifier require all patterns to be covered? The verifier exists but exhaustiveness checking is not yet implemented.

### Compensation patterns

The workflow examples show inline compensation (`charge pid amt;?{^e:release rid;^+"Payment failed"...}`). Should compensation be a first-class concept, or is inline error handling sufficient?

### Builtin naming: competing proposals across research files

Research files propose different names for the same operations. These need a single decision per capability. The table below shows proposals and emerging consensus:

| Capability | Proposals | Source files | Emerging consensus |
|-----------|-----------|-------------|-------------------|
| File read | `fread`, `rd`, `read` | Go/Rust/Lua-Elixir, Python/Universal-gaps, JS-TS | `fread` (3 files) |
| File write | `fwrite`, `wr`, `write` | Go/Rust/Lua-Elixir, Python/Universal-gaps, JS-TS | `fwrite` (3 files) |
| Regex match | `rgx`, `rx`, `mtc`, `matchall` | Python/Essential/Universal-gaps, Ruby-PHP, JS-TS, Go | `rgx` (3 files) |
| Regex all | `rga`, — | Python/Essential/Universal-gaps | `rga` (3 files) |
| Regex sub | `rgs`, `rxr`, `rep`, `sub` | Python/Essential/Universal-gaps, Ruby-PHP, Rust, Go | `rgs` (3 files) |
| String replace | `rpl`, `sub`, `rep` | Python/Essential/Universal-gaps, Go, Rust | `rpl` (3 files) |
| JSON parse | `jsn`, `jp`, `jparse` | Python/Universal-gaps, Essential, Go | `jsn` (2 files) |
| JSON dump | `ser`, `jdump` | Python/Universal-gaps, Go | `ser` (2 files) |
| Hash | `sha`, `hash` | Python/Essential, Go | undecided |
| HMAC | `hmac` | Python/Essential, Go | `hmac` (consensus) |
| Sleep | `slp`, `sleep` | Python/Universal-gaps, Go | `slp` (2 files) |

### File I/O: builtin vs tool

Early research files (ruby-php-research, swift-kotlin-research) say "file I/O is a tool concern, not a language concern." Later files (go-stdlib, rust-capabilities, python-stdlib, essential-packages) propose file I/O builtins (`fread`/`fwrite` or `rd`/`wr`). This represents an evolving position: as the design matured, file I/O moved from "tool" to "builtin." The earlier files have not been updated to reflect this shift.

The OPEN.md principle "format parsing is a tool concern" applies to format parsing (JSON, XML, HTML), not to raw file read/write. File I/O as a builtin is consistent with the principle — the builtin reads/writes bytes or text, while format-specific parsing remains a tool concern.
