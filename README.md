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
| Rust (native) | 499ns | 209ns | n/a | 10.0us | 1.6us | 168ns | 104ns | n/a | 485ns | 11.0us | 159.1us |
| Go | 327ns | 3.9us | 107ns | 5.7us | 797ns | 535ns | 589ns | 89ns | 126ns | 19.7us | 193.8us |
| C# (.NET) | 5.8us | 2.3us | 560ns | 30.3us | 7.6us | 248ns | 1.4us | 607ns | 895ns | 21.8us | 221.9us |
| Kotlin (JVM) | 488ns | 2.2us | 299ns | 7.8us | 1.0us | 184ns | 1.2us | 166ns | 204ns | 17.6us | 179.6us |
| LuaJIT | 305ns | 855ns | 150ns | 10.2us | 3.4us | 772ns | 1.5us | 71ns | 211ns | 14.5us | 45.0us |
| Node/V8 | 484ns | 426ns | 389ns | 5.3us | 1.0us | 399ns | 627ns | 104ns | 230ns | 12.3us | 270.3us |
| TypeScript | 438ns | 373ns | 239ns | 5.2us | 1.0us | 378ns | 422ns | 73ns | 161ns | 12.0us | 274.7us |
| ilo AOT | 3.8us | 4.2us | 1.8us | 47.4us | 7.9us | 768ns | n/a | 775ns | 991ns | n/a | n/a |
| ilo JIT | 5.2us | 759ns | 619ns | 40.7us | 6.1us | 524ns | 10.6us | 404ns | 494ns | 16.3us | 207.9us |
| ilo VM | 11.8us | 2.8us | 3.0us | 28.5us | 52.6us | 5.7us | 2.3us | 1.2us | 6.8us | 16.5us | 162.3us |
| ilo Interpreter | 92.5us | 15.8us | 54.5us | 1.3ms | 987.8us | 133.8us | 76.7us | 10.5us | 146.7us | 31.9us | 950.1us |
| Lua | 4.1us | 5.2us | 7.6us | 50.0us | 30.2us | 2.8us | 3.7us | 925ns | 4.2us | 14.9us | 52.2us |
| Ruby | 21.4us | 5.1us | 8.7us | 18.2us | 43.4us | 2.8us | 3.1us | 2.0us | 6.2us | 18.4us | 266.3us |
| PHP | 6.5us | 1.3us | 4.0us | 8.2us | 30.1us | 4.3us | 993ns | 735ns | 6.7us | 14.5us | 176.6us |
| Python 3 | 28.3us | 2.2us | 8.6us | 28.3us | 71.0us | 5.7us | 2.2us | 2.7us | 10.2us | 20.1us | 2.2us |
| PyPy 3 | 771ns | 849ns | 436ns | 21.3us | 4.3us | 1.0us | 525ns | 269ns | 459ns | 23.0us | 723ns |

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
