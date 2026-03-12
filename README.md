# ilo

*A programming language AI agents write, not humans. Named from [Toki Pona](https://sona.pona.la/wiki/ilo) for "tool".*

[![CI](https://github.com/ilo-lang/ilo/actions/workflows/rust.yml/badge.svg)](https://github.com/ilo-lang/ilo/actions/workflows/rust.yml)  [![codecov](https://codecov.io/gh/ilo-lang/ilo/branch/main/graph/badge.svg)](https://codecov.io/gh/ilo-lang/ilo)  [![crates.io](https://img.shields.io/crates/v/ilo)](https://crates.io/crates/ilo)  [![npm](https://img.shields.io/npm/v/ilo-lang)](https://www.npmjs.com/package/ilo-lang)  [![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

```
Python                                    ilo
─────                                     ───
def total(price, quantity, rate):          tot p:n q:n r:n>n;s=*p q;t=*s r;+s t
    sub = price * quantity
    tax = sub * rate
    return sub + tax

4 lines, 30 tokens, 90 chars              1 line, 10 tokens, 20 chars
```

**0.33× the tokens. 0.22× the characters. Same semantics. Type-verified before execution.**

## Why

AI agents pay three costs per program: generation tokens, error feedback, retries. ilo cuts all three:

- **Shorter programs** - prefix notation eliminates parentheses; positional args eliminate boilerplate
- **Verified first** - type errors caught before execution; agents get `ILO-T004` not a stack trace
- **Compact errors** - one token, not a paragraph; agents correct faster, fewer retries

## Install

<details open>
<summary>macOS / Linux</summary>

```bash
curl -fsSL https://raw.githubusercontent.com/ilo-lang/ilo/main/install.sh | sh
```

</details>

<details>
<summary>Windows (PowerShell)</summary>

```powershell
Invoke-WebRequest -Uri https://github.com/ilo-lang/ilo/releases/latest/download/ilo-x86_64-pc-windows-msvc.exe -OutFile ilo.exe
```

</details>

<details>
<summary>npm (any platform with Node 20+)</summary>

```bash
npm i -g ilo-lang

# or run without installing
npx ilo-lang 'dbl x:n>n;*x 2' 5
```

> WASM mode: interpreter only. HTTP builtins (`get`, `$`, `post`) require the native binary.

</details>

<details>
<summary>Rust</summary>

```bash
cargo install ilo
```

</details>

<details>
<summary>Agent-specific install</summary>

| Agent | Install |
|-------|---------|
| **Claude Code** | `/plugin marketplace add ilo-lang/ilo` then `/plugin install ilo-lang/ilo` |
| **Claude Cowork** | Browse Plugins → Add marketplace → `ilo-lang/ilo` → install |
| **Other agents** | Copy `skills/ilo/` into your agent's skills directory |

</details>

**[All install methods →](https://ilo-lang.ai/docs/installation/)**

## Quick start

```bash
# Inline
ilo 'dbl x:n>n;*x 2' 5                    # → 10

# From file
ilo program.ilo functionName arg1 arg2
```

**[Tutorial: Write your first program →](https://ilo-lang.ai/docs/first-program/)**

## What it looks like

**Guards** - flat, no nesting:
```
cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze"
```

**Pipes** - left-to-right composition:
```
run x:n>n;x>>dbl>>inc
```

**Data pipeline** - fetch, parse, filter, sum:
```
fetch url:t>R ? t;r=($!url);rdb! r "json"
proc rows:L ?>n;clean=flt pos rows;sum clean
pos x:?>b;>x 0
```

**Auto-unwrap `!`** - eliminates Result matching:
```bash
ilo 'inner x:n>R n t;~x  outer x:n>R n t;~(inner! x)' 42  # → 42
```

## Teaching agents

ilo ships as an [Agent Skill](https://agentskills.io). Install the plugin and the agent learns ilo automatically.

For manual context loading:
```bash
ilo -ai              # compact spec for LLM system prompts
ilo help lang        # full spec
```

## Key docs

| | |
|---|---|
| **[Introduction](https://ilo-lang.ai/docs/introduction/)** | What ilo is and why |
| **[Installation](https://ilo-lang.ai/docs/installation/)** | All install methods |
| **[Tutorial](https://ilo-lang.ai/docs/first-program/)** | Write your first program |
| **[Types & Functions](https://ilo-lang.ai/docs/guide/types-and-functions/)** | Core language guide |
| **[Prefix Notation](https://ilo-lang.ai/docs/guide/prefix-notation/)** | Why prefix saves tokens |
| **[Guards](https://ilo-lang.ai/docs/guide/guards/)** | Pattern matching without if/else |
| **[Pipes](https://ilo-lang.ai/docs/guide/pipes/)** | Function composition |
| **[Collections](https://ilo-lang.ai/docs/guide/collections/)** | Lists and higher-order functions |
| **[Error Handling](https://ilo-lang.ai/docs/guide/error-handling/)** | Result types and auto-unwrap |
| **[Data & I/O](https://ilo-lang.ai/docs/guide/data-io/)** | HTTP, files, JSON, env |
| **[MCP Integration](https://ilo-lang.ai/docs/integrations/mcp/)** | Connect MCP servers |
| **[CLI Reference](https://ilo-lang.ai/docs/reference/cli/)** | Flags, REPL, output modes |
| **[Builtins](https://ilo-lang.ai/docs/reference/builtins/)** | All built-in functions |
| **[Error Codes](https://ilo-lang.ai/docs/reference/error-codes/)** | ILO-XXXX reference |
| **[SPEC.md](SPEC.md)** | Full language specification |
| **[examples/](examples/)** | Runnable examples (also test suite) |

## Benchmarks

Per-call time (ns) across 8 micro-benchmarks. Lower is better. [Full results →](https://ilo-lang.ai/docs/reference/benchmarks/)

| Language | numeric | string | record | mixed | guards | recurse | file | api |
|----------|--------:|--------:|--------:|--------:|--------:|--------:|--------:|--------:|
| Rust (native) | 212ns | 162ns | 16ns | 16.6us | 2.0us | 162ns | 25.4us | n/a |
| Go | 2.9us | 6.7us | 1.5us | 31.1us | 3.1us | 1.7us | 112.0us | 1.1ms |
| C# (.NET) | 19.6us | 2.4us | 450ns | 61.0us | 28.7us | 520ns | 35.2us | 534.6us |
| Kotlin (JVM) | 12.2us | 78.7us | 11.3us | 107.3us | 17.8us | 5.4us | n/a | n/a |
| LuaJIT | 3.5us | 3.7us | 3.2us | 71.1us | 10.7us | 7.5us | 103.2us | n/a |
| Node/V8 | 15.1us | 3.6us | 6.1us | 15.7us | 14.1us | 5.7us | 25.7us | 980.0us |
| TypeScript | 3.0us | 2.1us | 2.4us | 9.6us | 4.0us | 4.1us | 16.4us | 826.1us |
| ilo AOT | 6.6us | 14.0us | 1.8us | 11.2us | 20.1us | 1.2us | n/a | n/a |
| ilo JIT | 4.1us | 3.2us | 678ns | 40.9us | 120.5us | 5.3us | n/a | n/a |
| ilo VM | 14.2us | 5.1us | 3.4us | 42.5us | 53.0us | 5.1us | 16.5us | 157.5us |
| ilo Interpreter | 91.6us | 16.1us | 56.2us | 1.4ms | 941.7us | 131.5us | 31.6us | 3.3ms |
| Lua | 6.0us | 6.3us | 10.5us | 48.2us | 31.7us | 3.4us | 18.3us | n/a |
| Ruby | 22.2us | 5.3us | 8.0us | 25.3us | 38.9us | 3.2us | 19.8us | 504.8us |
| PHP | 7.5us | 1.3us | 4.3us | 8.9us | 26.5us | 4.4us | 15.2us | 274.6us |
| Python 3 | 28.2us | 2.3us | 8.8us | 28.7us | 63.1us | 6.0us | 22.4us | 2.2us |
| PyPy 3 | 63.9us | 76.8us | 125.7us | 684.1us | 470.1us | 78.3us | 24.8us | 31.1us |

*10 iterations, Darwin arm64, 2026-03-12*

## Community

- **[ilo-lang.ai](https://ilo-lang.ai)** - docs, playground, and examples
- **[r/ilolang](https://www.reddit.com/r/ilolang/)** - discussion and updates
- **[hello@ilo-lang.ai](mailto:hello@ilo-lang.ai)** - get in touch

## Principles

1. **Token-conservative** - every choice evaluated against total token cost
2. **Constrained** - small vocabulary, one way to do things, fewer wrong choices
3. **Verified** - types checked before execution, all errors reported at once
4. **Language-agnostic** - structural tokens (`@`, `>`, `?`, `^`, `~`, `!`, `$`) over English words

See the [manifesto](https://ilo-lang.ai/docs/manifesto/) for full rationale.
