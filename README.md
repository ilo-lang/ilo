# ilo

*Token-optimised programming language for AI agents, named from [Toki Pona](https://sona.pona.la/wiki/ilo) for "tool".*

[![CI](https://github.com/ilo-lang/ilo/actions/workflows/rust.yml/badge.svg)](https://github.com/ilo-lang/ilo/actions/workflows/rust.yml)  [![codecov](https://codecov.io/gh/ilo-lang/ilo/branch/main/graph/badge.svg)](https://codecov.io/gh/ilo-lang/ilo)  [![crates.io](https://img.shields.io/crates/v/ilo)](https://crates.io/crates/ilo)  [![npm](https://img.shields.io/npm/v/ilo-lang)](https://www.npmjs.com/package/ilo-lang)  [![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

AI agents pay three costs per program: generation tokens, error feedback, retries. ilo cuts all three: 0.33× the tokens and 0.22× the characters of Python, type-verified before execution.

## Why ilo makes agents more efficient

| | Python | ilo | saving |
|---|---|---|---|
| Token count | ~30 | ~10 | 67% |
| Characters | ~90 | ~20 | 78% |

- **Shorter programs**: 0.33× tokens, 0.22× characters vs Python
- **Verified first**: type errors caught before execution; agent gets `ILO-T004` not a stack trace
- **Compact error codes**: one token, not a paragraph; agents correct faster, fewer retries
- **Prefix notation**: eliminates parentheses; token-optimal form for AI generation

## Install

### Claude Code (CLI, recommended)
```bash
/plugin marketplace add ilo-lang/ilo   # add the marketplace (once)
/plugin install ilo-lang/ilo           # install the plugin + teach the agent ilo
```

### Claude Cowork
Browse Plugins → Add marketplace from GitHub → `ilo-lang/ilo` → install. Binary auto-installs via npm.

Can also be installed by telling Claude to run:
```bash
npm i -g ilo-lang
```

> **Note:** Cowork uses the npm/WASM build. HTTP builtins (`get`, `$`, `post`) are not yet supported; use the native binary for network access.

### Other agents (Codex, Opencode, Kilocode, etc.)
Copy `skills/ilo/` into your agent's skills directory. For the native binary (recommended):
```bash
curl -fsSL https://raw.githubusercontent.com/ilo-lang/ilo/main/install.sh | sh
```
Or via npm (WASM, no HTTP builtins):
```bash
npm i -g ilo-lang
```

### npm / npx (any platform with Node 20+)
```bash
npx ilo-lang 'dbl x:n>n;*x 2' 5   # run on-demand (no install needed)
npm i -g ilo-lang                   # or install globally
```
> **Note:** npm/WASM runs interpreter mode only. HTTP builtins (`get`, `$`, `post`) are not available; use the native binary for network access.

### macOS / Linux
```bash
curl -fsSL https://raw.githubusercontent.com/ilo-lang/ilo/main/install.sh | sh
```

### Windows (PowerShell)
```powershell
Invoke-WebRequest -Uri https://github.com/ilo-lang/ilo/releases/latest/download/ilo-x86_64-pc-windows-msvc.exe -OutFile ilo.exe
```

### From source

**crates.io:**
```bash
cargo install ilo
```

**Git:**
```bash
cargo install --git https://github.com/ilo-lang/ilo
```

## What it looks like

Python:
```python
def total(price: float, quantity: int, rate: float) -> float:
    sub = price * quantity
    tax = sub * rate
    return sub + tax
```

ilo:
```
tot p:n q:n r:n>n;s=*p q;t=*s r;+s t
```

0.33× the tokens, 0.22× the characters. Same semantics.

Real-world data pipeline: fetch JSON, parse, filter, sum:
```
fetch url:t>R ? t;r=($!url);rdb! r "json"
proc rows:L ?>n;clean=flt pos rows;sum clean
pos x:?>b;>x 0
```

Three functions, no boilerplate. `$!` auto-unwraps HTTP. `rdb!` auto-unwraps parse. `>>` chains transforms.

## Teaching agents

### Agent Skills (zero friction)

ilo ships as an [Agent Skill](https://agentskills.io). Install the plugin and the agent learns ilo automatically; no manual context loading needed.

| Surface | How to use ilo |
|---------|---------------|
| **Claude Code** (CLI) | Add marketplace then install (see [Install](#install)) |
| **Claude Cowork** (web) | Browse Plugins → install ilo (binary auto-installs via npm) |
| **Claude API / Console** | Run `ilo -ai`, paste output into your system prompt |

**Other agents** (Codex, Cursor, Copilot, etc.): copy `skills/ilo/` into your agent's skills directory. Any tool supporting the [Agent Skills standard](https://agentskills.io) will pick it up.

> **Note:** Sandboxed agents (Codex etc.) may lack filesystem/network access. Pre-install ilo in the container and use context loading instead.

### Context loading

```bash
ilo -ai              # compact spec for LLM consumption
ilo help lang        # full spec
```

### Fine-tuning

Future exploration: training on ilo programs and error feedback loops may improve agent performance. Not yet tested or available.

## Running ilo

```bash
ilo 'tot p:n q:n r:n>n;s=*p q;t=*s r;+s t' 10 20 30   # → 6200
ilo program.ilo 10 20 30                                 # from file
```

First arg is code or a file path (auto-detected). Remaining args are passed to the first function. Name a function to select it in multi-function programs:

```bash
ilo 'dbl x:n>n;s=*x 2;+s 0 tot p:n q:n r:n>n;s=*p q;t=*s r;+s t' tot 10 20 30
```

**Higher-order functions**: `map`, `flt`, `fld` take a function name as first arg:
```bash
ilo 'sq x:n>n;*x x main xs:L n>L n;map sq xs' main 1,2,3,4,5   # → [1, 4, 9, 16, 25]
ilo 'pos x:n>b;>x 0 main xs:L n>L n;flt pos xs' main -3,-1,0,2,4  # → [2, 4]
ilo 'add a:n b:n>n;+a b main xs:L n>n;fld add xs 0' main 1,2,3,4,5  # → 15
```

**Pipe `>>`**: pass result of left as last arg to right:
```bash
ilo 'sq x:n>n;*x x pos x:n>b;>x 0 main xs:L n>L n;xs >> flt pos >> map sq' main -3,-1,0,2,4
# → [4, 16]
```

**Pass list arguments** with commas:
```bash
ilo 'f xs:L n>n;len xs' 1,2,3         # → 3
ilo 'f xs:L t>t;xs.0' 'a,b,c'         # → a
```

**Interactive REPL:**
```bash
ilo repl                     # start interactive session
```
Define functions, evaluate expressions, accumulate state. nvim-style commands: `:q` `:w file.ilo` `:defs` `:clear` `:help`.

**Help & language spec:**
```bash
ilo help                     # usage and examples
ilo help lang                # full language specification
ilo -ai                      # compact spec for LLM consumption
```

**Static verification**: all programs verified before execution. Reports all errors at once with stable codes:

```bash
ilo 'f x:n>n;*y 2' 5
# error[ILO-T004]: undefined variable 'y'
#   = note: in function 'f'

ilo 'f x:t>n;*x 2' hello
# error[ILO-T009]: '*' expects n and n, got t and n
#   = note: in function 'f'
```

```bash
ilo --explain ILO-T004              # explain an error code
ilo 'f x:n>n;*x 2' --explain       # explain what the code does
```

## Language features

**Prefix and infix notation:**
```
+*a b c            # (a * b) + c      saves 4 chars, 1 token
>=*+a b c 100      # ((a + b) * c) >= 100   saves 7 chars, 3 tokens
```

Infix also works: `a + b`, `x * y + 1`. Across 25 expression patterns: **22% fewer tokens, 42% fewer characters** with prefix vs infix. See the [prefix-vs-infix benchmark](research/explorations/prefix-vs-infix/).

**Auto-unwrap `!`** eliminates Result matching boilerplate:
```bash
# Without !: 12 tokens
ilo 'inner x:n>R n t;~x outer x:n>R n t;r=inner x;?r{~v:~v;^e:^e}' 42

# With !: 1 token
ilo 'inner x:n>R n t;~x outer x:n>R n t;~(inner! x)' 42
# → 42
```

**HTTP GET**: `get url` or `$url` (terse alias), returns `R t t`:
```bash
ilo 'f url:t>R t t;$url' "http://httpbin.org/get"       # → ~{ ... }
ilo 'f url:t>R t t;~($!url)' "http://httpbin.org/get"   # auto-unwrap
```

**Environment variables**: `env key` returns `R t t`:
```bash
ilo 'f k:t>R t t;env k' "HOME"    # → ~"/Users/dan"
ilo 'f k:t>R t t;env! k' "HOME"   # auto-unwrap
```

**File I/O**: format auto-detected from extension:
```bash
ilo 'f p:t>R ? t;rd p' data.csv    # → Ok([[row1col1 …] …])
ilo 'f p:t>R ? t;rd p' data.json   # → Ok(parsed JSON)
ilo 'f p:t>R ? t;rd p' notes.txt   # → Ok("raw text")
ilo 'f p:t>R ? t;rd p "json"' data.csv   # force format
ilo 'f s:t>R ? t;rdb s "csv"' "a,b\n1,2" # parse buffer
```

**Data scripting:**
```bash
ilo 'f s:t>t;trm s' "  hello  "        # → "hello"
ilo 'f xs:L t>L t;unq xs' a,b,a,c,b   # → ["a" "b" "c"]
ilo 'f>t;fmt "{} + {} = {}" 1 2 3'     # → "1 + 2 = 3"
```

**Aggregation & reshape:**
```bash
ilo 'f xs:L n>n;sum xs' 1,2,3,4,5      # → 15
ilo 'f xs:L n>n;avg xs' 2,4,6          # → 4
ilo 'f s:t>L t;rgx "\d+" s' "abc 123"  # → ["123"]
```

**Imports:**
```bash
# math.ilo: dbl n:n>n;*n 2
# main.ilo: use "math.ilo"  run n:n>n;dbl n
ilo main.ilo run 5           # → 10
```

**Environment files**: `.env` and `.env.local` loaded automatically. `.env.local` takes priority; existing env vars are not overwritten:
```bash
echo 'ANTHROPIC_API_KEY=sk-...' > .env
ilo 'f k:t>R t t;env! k' ANTHROPIC_API_KEY
```

**Output formats:**
```bash
ilo 'code' -a               # ANSI colour (default for TTY)
ilo 'code' -t               # plain text
ilo 'code' -j               # JSON (default for piped output)
```

**Formatter:**
```bash
ilo 'code' --dense / -d     # dense wire format (agents)
ilo 'code' --expanded / -e  # expanded human format
```

**Other modes:**
```bash
ilo 'code' --emit python     # transpile to Python
ilo program.ilo --bench tot 10 20 30  # benchmark
```

**Run tests:**
```bash
cargo test
```

## For integrators

**Tool declarations**: external calls wired to HTTP endpoints via a JSON config:

```bash
ilo program.ilo --tools tools.json args...
```

`tools.json`:
```json
{
  "tools": {
    "get-user": {
      "url": "https://api.example.com/get-user",
      "method": "POST",
      "timeout_secs": 5,
      "retries": 2
    }
  }
}
```

ilo serialises call args as `{"args": [...]}` and deserialises the JSON response back to ilo values.

**MCP servers**: connect any MCP server to give ilo access to its tools:

```bash
ilo program.ilo --mcp mcp.json args...
```

`mcp.json` uses Claude Desktop format. MCP tools are injected as `tool` declarations before verification, so types are checked end-to-end.

```json
{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
    }
  }
}
```

See `examples/mcp.json` for a working example.

**Backends:**
```bash
ilo 'code' args              # default: Cranelift JIT → interpreter fallback
ilo 'code' --run-interp ...  # tree-walking interpreter
ilo 'code' --run-vm ...      # register VM (bytecode compiled)
ilo 'code' --run-cranelift . # Cranelift JIT
ilo 'code' --run-jit ...     # custom ARM64 JIT (macOS Apple Silicon only)
```

## Documentation

| Document | Purpose |
|----------|---------|
| [SPEC.md](SPEC.md) | Language specification |
| [examples/](examples/) | Runnable example programs (also `cargo test` regression suite) |
| [MANIFESTO.md](MANIFESTO.md) | Design rationale |
| [research/JOURNEY.md](research/JOURNEY.md) | Design journey: syntax variants, benchmarks, research index |
| [skills/ilo/](skills/ilo/) | Agent Skill (for AI coding agents) |
| [research/TODO.md](research/TODO.md) | Planned work |
| [research/OPEN.md](research/OPEN.md) | Open design questions |

## Principles

1. **Token-conservative**: every choice evaluated against total token cost: generation, retries, error feedback, context loading.
2. **Constrained**: small vocabulary, closed world, one way to do things. Fewer valid next-tokens = fewer wrong choices = fewer retries.
3. **Self-contained**: each unit carries its own context: deps, types, rules.
4. **Language-agnostic**: structural tokens (`@`, `>`, `?`, `^`, `~`, `!`, `$`) over English words.
5. **Graph-native**: programs express relationships navigable as a graph, not just linear text.

Guards instead of if/else: flat statements that return early and chain vertically. No nesting depth, no closing braces. Match instead of switch: no fall-through.

See [MANIFESTO.md](MANIFESTO.md) for full rationale.

## Design journey

We explored 9 syntax variants before settling on the current design. See [research/JOURNEY.md](research/JOURNEY.md) for the full comparison table, key findings, and all research documents.

## Community

- [r/ilolang](https://www.reddit.com/r/ilolang/) — discussion, feedback, and updates

```
  _  _          _
 (_)| | ___    | |  __ _  _ __    __ _
 | || |/ _ \   | | / _` || '_ \  / _` |
 | || | (_) |  | || (_| || | | || (_| |
 |_||_|\___/   |_| \__,_||_| |_| \__, |
                                   |___/
```
