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
ilo 'tot p:n q:n r:n>n;s=*p q;t=*s r;+s t' 10 20 30  # → 6200
ilo program.ilo 10 20 30                               # from file
```

All programs are type-verified before execution. See the [CLI Reference](https://github.com/ilo-lang/ilo/wiki/CLI-Reference) for REPL, HOFs, pipes, output flags, and more.

## Language features

**Prefix and infix notation** — the core token-saving device:
```
+*a b c            # (a * b) + c      saves 4 chars, 1 token
>=*+a b c 100      # ((a + b) * c) >= 100   saves 7 chars, 3 tokens
```

Infix also works: `a + b`, `x * y + 1`. Across 25 expression patterns: **22% fewer tokens, 42% fewer characters** with prefix vs infix. See the [prefix-vs-infix benchmark](https://github.com/ilo-lang/ilo/wiki/Prefix-vs-Infix-Benchmark).

**Auto-unwrap `!`** eliminates Result matching boilerplate:
```bash
# Without !: 12 tokens
ilo 'inner x:n>R n t;~x outer x:n>R n t;r=inner x;?r{~v:~v;^e:^e}' 42

# With !: 1 token
ilo 'inner x:n>R n t;~x outer x:n>R n t;~(inner! x)' 42
# → 42
```

For built-ins (HTTP, env, file I/O, data ops, imports) and CLI flags, see the [Tutorial](https://github.com/ilo-lang/ilo/wiki) and [SPEC.md](SPEC.md).

## Integrations

**Tool declarations** (`--tools tools.json`) — wire external HTTP endpoints as typed ilo functions.

**MCP servers** (`--mcp mcp.json`) — connect any MCP server; tools are type-checked end-to-end before execution.

See the [Integrations wiki page](https://github.com/ilo-lang/ilo/wiki/Integrations) for full config examples and backend options.

## Principles

1. **Token-conservative**: every choice evaluated against total token cost: generation, retries, error feedback, context loading.
2. **Constrained**: small vocabulary, closed world, one way to do things. Fewer valid next-tokens = fewer wrong choices = fewer retries.
3. **Self-contained**: each unit carries its own context: deps, types, rules.
4. **Language-agnostic**: structural tokens (`@`, `>`, `?`, `^`, `~`, `!`, `$`) over English words.
5. **Graph-native**: programs express relationships navigable as a graph, not just linear text.

Guards instead of if/else: flat statements that return early and chain vertically. No nesting depth, no closing braces. Match instead of switch: no fall-through.

See [MANIFESTO.md](MANIFESTO.md) for full rationale.

## Design journey

See [research/JOURNEY.md](research/JOURNEY.md) — 9 syntax variants, key findings, all research documents.

## Community

- [r/ilolang](https://www.reddit.com/r/ilolang/) — discussion, feedback, and updates

## Documentation

| Document | Purpose |
|----------|---------|
| [Tutorial](https://github.com/ilo-lang/ilo/wiki) | Step-by-step guide (10 lessons) |
| [SPEC.md](SPEC.md) | Language specification |
| [examples/](examples/) | Runnable example programs (also `cargo test` regression suite) |
| [MANIFESTO.md](MANIFESTO.md) | Design rationale |
| [research/JOURNEY.md](research/JOURNEY.md) | Design journey: syntax variants, benchmarks, research index |
| [skills/ilo/](skills/ilo/) | Agent Skill (for AI agents) |
| [research/TODO.md](research/TODO.md) | Planned work |
| [research/OPEN.md](research/OPEN.md) | Open design questions |

```
  _  _          _
 (_)| | ___    | |  __ _  _ __    __ _
 | || |/ _ \   | | / _` || '_ \  / _` |
 | || | (_) |  | || (_| || | | || (_| |
 |_||_|\___/   |_| \__,_||_| |_| \__, |
                                   |___/
```
