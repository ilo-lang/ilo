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

| Language | numeric | string | record | mixed | guards | recurse | foreach | while | pipe | file | api |
|----------|--------:|--------:|--------:|--------:|--------:|--------:|--------:|--------:|--------:|--------:|--------:|
| Rust (native) | 277ns | 3.4us | n/a | 9.0us | 1.6us | 19.7us | 46ns | n/a | 349ns | 10.3us | 147.9us |
| Go | 430ns | 18.1us | 59ns | 6.0us | 484ns | 23.4us | 179ns | 30ns | 57ns | 19.7us | 187.5us |
| C# (.NET) | 5.7us | 14.9us | 449ns | 30.1us | 6.9us | 23.1us | 1.1us | 492ns | 702ns | 23.2us | 227.8us |
| Kotlin (JVM) | 513ns | 7.3us | 285ns | 7.7us | 1.1us | 17.0us | 1.2us | 171ns | 225ns | 18.6us | 183.5us |
| LuaJIT | 306ns | 20.7us | 144ns | 9.8us | 3.7us | 29.6us | 699ns | 55ns | 85ns | 14.8us | 46.9us |
| Node/V8 | 480ns | 1.3us | 351ns | 5.3us | 1.1us | 46.3us | 482ns | 79ns | 217ns | 12.8us | 264.0us |
| TypeScript | 454ns | 1.2us | 223ns | 5.3us | 1.1us | 45.9us | 434ns | 71ns | 167ns | 12.8us | 264.9us |
| ilo AOT | 3.7us | 12.8us | 883ns | 33.9us | 4.7us | 38.3us | n/a | 527ns | 690ns | n/a | n/a |
| ilo JIT | 3.5us | 5.0us | 664ns | 29.1us | 3.6us | 36.3us | 3.3us | 357ns | 416ns | 16.4us | 176.4us |
| ilo VM | 11.3us | 10.4us | 3.3us | 28.5us | 37.2us | 504.9us | 2.8us | 1.2us | 6.7us | 17.1us | 164.4us |
| ilo Interpreter | 94.3us | 62.0us | 55.5us | 1.4ms | 959.1us | 16.4ms | 79.8us | 10.5us | 149.5us | 32.4us | 179.0us |
| Lua | 4.1us | 39.2us | 7.6us | 48.7us | 27.7us | 330.0us | 3.9us | 945ns | 4.3us | 15.8us | 54.0us |
| Ruby | 20.5us | 25.6us | 8.7us | 18.2us | 37.8us | 376.7us | 3.2us | 2.2us | 6.0us | 18.9us | 260.2us |
| PHP | 6.6us | 4.1us | 4.0us | 8.2us | 26.1us | 562.6us | 1.0us | 679ns | 6.7us | 15.1us | 168.8us |
| Python 3 | 29.7us | 12.2us | 8.7us | 28.3us | 64.6us | 745.6us | 2.1us | 2.7us | 10.3us | 19.9us | 13.2us |
| PyPy 3 | 1.3us | 2.0us | 445ns | 20.4us | 4.4us | 109.8us | 565ns | 276ns | 452ns | 22.9us | 2.0us |

*10000 iterations, Darwin arm64, 2026-03-13*

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
