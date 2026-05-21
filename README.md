# ilo

*A programming language AI agents write, not humans. Named from [Toki Pona](https://sona.pona.la/wiki/ilo) for "tool".*

> Experimental language. Expect breaking changes.

[![CI](https://github.com/ilo-lang/ilo/actions/workflows/rust.yml/badge.svg)](https://github.com/ilo-lang/ilo/actions/workflows/rust.yml)  [![codecov](https://codecov.io/gh/ilo-lang/ilo/graph/badge.svg?token=4W1SWFLXNW)](https://codecov.io/gh/ilo-lang/ilo)  [![crates.io](https://img.shields.io/crates/v/ilo)](https://crates.io/crates/ilo)  [![npm](https://img.shields.io/npm/v/ilo-lang)](https://www.npmjs.com/package/ilo-lang)  [![skills.sh](https://skills.sh/badge/ilo-lang/ilo)](https://skills.sh/ilo-lang/ilo)  [![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

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

```sh
curl -fsSL https://ilo-lang.ai/install.sh | sh
```

The installer downloads the release binary plus its `checksums-sha256.txt` from the matching GitHub release and refuses to install if the SHA-256 doesn't match. Source: [`scripts/install/install.sh`](./scripts/install/install.sh).

</details>

<details>
<summary>Windows (PowerShell)</summary>

```powershell
iwr -useb https://ilo-lang.ai/install.ps1 | iex
```

Same checksum verification as the Unix installer. Source: [`scripts/install/install.ps1`](./scripts/install/install.ps1).

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
<summary>Agent skills (Claude Code / AI agents)</summary>

```bash
npx skills add ilo-lang/ilo
```

Uses the [skills](https://www.npmjs.com/package/skills) npm package (396K+ installs). Installs the ilo Agent Skill into Claude Code or any compatible agent.

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

# Verb form (cargo / go / zero style; bare positional still works)
ilo run program.ilo arg1 arg2             # run
ilo check program.ilo                     # verify only - exit 0 if clean
ilo build program.ilo -o ./bin            # AOT compile
```

**[Tutorial: Write your first program →](https://ilo-lang.ai/docs/first-program/)**

## Editor support

Syntax highlighting, snippets, and `--` comment handling for `.ilo` files ships in [`extensions/vscode/`](./extensions/vscode/). Install into Cursor with `cd extensions/vscode && npm run install:cursor`. VS Code marketplace publish is tracked separately.

## Versioning

CalVer. Releases are `YY.M` (e.g. `26.5`), patches within a month are `YY.M.P` (e.g. `26.5.1`). The version string carries recency: an agent loading `ilo spec --json ai` knows which spec applies from the version alone, no changelog lookup needed. Token-conservative (manifesto principle 1) vs semver's `0.12.1`.

### Branches

| Branch | Purpose                     | Tags                                       |
|--------|-----------------------------|--------------------------------------------|
| `main` | Current release line, patches | `26.5`, `26.5.1`, `26.5.1-dev.N`         |
| `next` | Next release                  | `26.6-dev.N`, then clean `26.6` on cut    |

Universal scheme: `-dev.N` is the only in-flight marker on either branch. Clean (no suffix) means soaked, ready, published. No RC tag.

### Release flow (next release)

1. Work lands on `next`
2. Iterations tagged `26.6-dev.1`, `26.6-dev.2`, …
3. When soaked: merge `next` → `main`, cut clean `26.6` tag on `main`
4. `next` continues for `26.7-dev.1`

### Patch flow (current release fix)

1. Fix lands on `main` (direct or short-lived fix branch)
2. Iterations tagged `26.5.1-dev.1`, `26.5.1-dev.2`, …
3. When soaked: cut clean `26.5.1` tag on `main`
4. Merge `main` → `next` to carry the fix forward (auto via `sync-next.yml`)

### Migration from 0.x

Last semver release: `0.12.1`. First CalVer release cuts on the next breaking change as `26.X`. Hard cut, no `0.13` bridge. The file version pragma ships with the CalVer cut and is optional — existing 0.x files have no pragma and verify without a diagnostic.

### File version pragma

Optional. ilo source files may declare the minimum required runtime with a top-of-file sigil:

```
^26.5
-- rest of file
```

- Sigil-led (principle 4), ~3 tokens (principle 1)
- `^` carries "at minimum" semantics — agents read it correctly without a spec lookup
- First-class syntax, not a magic comment
- Position: first line, no leading whitespace

Verifier behaviour:

| Case                                                    | Result                              |
|---------------------------------------------------------|-------------------------------------|
| Pragma absent                                           | Assume latest installed runtime, no diagnostic |
| File targets older than runtime, breaking change between| Fail with migration pointer         |
| File targets newer than runtime                         | Fail asking to upgrade              |

Tooling: `ilo --version-of <file>` reads the pragma (returns nothing when absent); the formatter canonicalises position when present, never inserts one.

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

**Batteries included** - the standard library covers what real programs need without leaving ilo:

- **Math**: `sqrt`, `pow`, `exp`, `log`/`log10`/`log2`, `sin`/`cos`/`tan`, `atan2`, `clamp`, `rou`
- **Stats**: `median`, `quantile`, `stdev`, `variance`, `cumsum`, `frq`
- **Linear algebra**: `transpose`, `matmul`, `dot`, `solve`, `inv`, `det`, `fft`/`ifft`
- **Lists**: `at`, `take`/`drop`, `zip`, `enumerate`, `range`, `chunks`, `window`, `flatmap`, `partition`, `setunion`/`setinter`/`setdiff`
- **Text**: `upr`/`lwr`/`cap`, `padl`/`padr`, `chars`, `ord`/`chr`, `rgxall`, `rgxsub`
- **Time**: `now`, `sleep`, `dtfmt`/`dtparse` (strftime, UTC)
- **JSONL**: `rdjl` for line-by-line parsing
- **Concurrent HTTP**: `get-many` (up to 10 parallel)

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
| Rust (native) | 496ns | 3.7us | n/a | 9.9us | 1.8us | 18.8us | 44ns | n/a | 254ns | 9.3us | 159.0us |
| Go | 705ns | 17.2us | 107ns | 6.2us | 738ns | 21.5us | 162ns | 30ns | 31ns | 19.0us | 200.0us |
| C# (.NET) | 6.2us | 14.6us | 554ns | 29.9us | 6.8us | 24.7us | 1.2us | 417ns | 610ns | 22.7us | 228.8us |
| Kotlin (JVM) | 492ns | 8.2us | 265ns | 7.8us | 1.0us | 17.4us | 1.2us | 168ns | 226ns | 18.6us | 183.4us |
| LuaJIT | 513ns | 19.0us | 149ns | 9.8us | 3.5us | 28.7us | 676ns | 48ns | 85ns | 14.7us | 63.2us |
| Node/V8 | 569ns | 1.3us | 354ns | 5.3us | 1.0us | 46.7us | 480ns | 81ns | 191ns | 12.4us | 284.5us |
| TypeScript | 427ns | 1.2us | 236ns | 5.4us | 1.0us | 45.9us | 421ns | 75ns | 167ns | 12.7us | 310.2us |
| ilo AOT | 1.8us | 12.9us | 1.1us | 33.0us | 5.3us | 37.3us | 1.1us | 148ns | 126ns | n/a | n/a |
| ilo Cranelift | 1.3us | 5.0us | 504ns | 26.2us | 3.2us | 37.2us | 935ns | 150ns | 125ns | 16.5us | 1.7ms |
| ilo VM | 11.3us | 10.5us | 3.1us | 31.6us | 53.5us | 528.3us | 2.4us | 1.2us | 7.0us | 16.9us | 171.1us |
| Lua | 4.0us | 39.8us | 7.5us | 49.2us | 27.0us | 339.1us | 3.8us | 967ns | 4.3us | 16.1us | 54.3us |
| Ruby | 19.8us | 26.0us | 8.7us | 18.4us | 38.5us | 373.0us | 3.2us | 2.3us | 6.3us | 19.6us | 269.2us |
| PHP | 6.4us | 4.0us | 4.0us | 8.2us | 25.8us | 552.9us | 1.1us | 767ns | 6.7us | 15.3us | 173.9us |
| Python 3 | 28.1us | 12.0us | 8.5us | 28.0us | 63.3us | 717.2us | 2.1us | 2.7us | 11.0us | 20.9us | 12.2us |
| PyPy 3 | 1.2us | 2.0us | 445ns | 20.5us | 4.3us | 107.1us | 605ns | 286ns | 470ns | 24.7us | 2.0us |

*10000 iterations, Darwin arm64, 2026-03-14*

## Community

- **[ilo-lang.ai](https://ilo-lang.ai)** - docs, playground, and examples
- **[hello@ilo-lang.ai](mailto:hello@ilo-lang.ai)** - get in touch

## Principles

1. **Token-conservative** - every choice evaluated against total token cost
2. **Constrained** - small vocabulary, one way to do things, fewer wrong choices
3. **Self-contained** - each function declares its deps; no globals, no ambient state
4. **Language-agnostic** - structural tokens (`@`, `>`, `?`, `^`, `~`, `!`, `$`) over English words
5. **Graph-reducible** - load only the relevant subgraph per task

See the [manifesto](https://ilo-lang.ai/docs/manifesto/) for full rationale.

## Author

Built by [Daniel John Morris](https://danieljohnmorris.com). ilo is the language I wanted while building with LLMs and AI agents.
