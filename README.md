# ilo

*ilo* — Toki Pona for "tool" ([sona.pona.la/wiki/ilo](https://sona.pona.la/wiki/ilo)). A programming language for AI agents.

Languages were designed for humans — visual parsing, readable syntax, spatial navigation. AI agents are not humans. They generate tokens. Every token costs latency, money, and context window. The only metric that matters is **total tokens from intent to working code**.

```
Total cost = spec loading + generation + context loading + error feedback + retries
```

## What It Looks Like

Python:
```python
def total(price: float, quantity: int, rate: float) -> float:
    sub = price * quantity
    tax = sub * rate
    return sub + tax
```

ilo (idea9 — ultra-dense-short):
```
tot p:n q:n r:n>n;s=*p q;t=*s r;+s t
```

0.33x the tokens, 0.22x the characters. Same semantics.

### Why prefix notation?

ilo uses prefix notation (`+a b` instead of `a + b`). Nesting eliminates parentheses entirely:

```
(a * b) + c       →  +*a b c        -- saves 4 chars, 1 token
((a + b) * c) >= 100  →  >=*+a b c 100  -- saves 7 chars, 3 tokens
```

Across 25 expression patterns: **22% fewer tokens, 42% fewer characters** vs infix. See the [prefix-vs-infix benchmark](research/explorations/prefix-vs-infix/).

## Principles

1. **Token-conservative** — every choice evaluated against total token cost across the full loop: generation, retries, error feedback, context loading.
2. **Constrained** — small vocabulary, closed world, one way to do things. Fewer valid next-tokens = fewer wrong choices = fewer retries.
3. **Self-contained** — each unit carries its own context: deps, types, rules. The spec travels with the program.
4. **Language-agnostic** — structural tokens (`@`, `>`, `?`, `^`, `~`, `!`, `$`) over English words.
5. **Graph-native** — programs express relationships (calls, depends-on, has-type). Navigable as a graph, not just readable as linear text.

**Guards instead of if/else** — flat statements that return early and chain vertically. No nesting depth, no closing braces to match. **Match instead of switch** — no fall-through, each arm is independent.

See [MANIFESTO.md](MANIFESTO.md) for the full rationale.

## Teaching a model to write ilo

Three paths, in order of friction:

**1. Context loading** — paste the spec into the system prompt. The compact form fits comfortably in any context window:

```bash
ilo help ai          # ~16-line ultra-compact spec for LLM consumption
ilo help lang        # full spec
```

Lowest friction. Works with any model today. Good for one-off agents and short sessions.

**2. Fine-tuning** — train on ilo programs and error feedback loops. Best for production agents that write a lot of ilo. Not yet available as a hosted service.

**3. Foundation model training** — ilo is public and MIT licensed. As usage grows, frontier models will encounter it in training data and learn it natively — the same path Python, SQL, and JSON took.

The compact spec (`ilo help ai`) is designed specifically for path 1: small enough to fit in a system prompt, dense enough to fully constrain generation.

## Design Journey

We explored 9 syntax variants before settling on the current design. The final syntax achieves 0.33x the tokens and 0.22x the characters of equivalent Python — with perfect 10/10 LLM generation accuracy.

See [research/JOURNEY.md](research/JOURNEY.md) for the full comparison table, key findings, and links to all research documents.

## Install

**One-liner (macOS / Linux):**
```bash
curl -fsSL https://raw.githubusercontent.com/ilo-lang/ilo/main/install.sh | sh
```

**Direct download (example: macOS Apple Silicon):**
```bash
curl -fsSL https://github.com/ilo-lang/ilo/releases/latest/download/ilo-aarch64-apple-darwin -o /usr/local/bin/ilo && chmod +x /usr/local/bin/ilo
```

**From source (developers):**
```bash
cargo install --git https://github.com/ilo-lang/ilo
```

## Running

**Run inline code:**
```bash
ilo 'tot p:n q:n r:n>n;s=*p q;t=*s r;+s t' 10 20 30
# → 6200
```

No flags needed. The first arg is code (or a file path — auto-detected). Remaining args are passed to the first function. To select a specific function in multi-function programs, name it:

```bash
ilo 'dbl x:n>n;s=*x 2;+s 0 tot p:n q:n r:n>n;s=*p q;t=*s r;+s t' tot 10 20 30
```

**Higher-order functions** — `map`, `flt`, `fld` take a function name as first arg:
```bash
# map: apply function to each element
ilo 'sq x:n>n;*x x main xs:L n>L n;map sq xs' main 1,2,3,4,5
# → [1, 4, 9, 16, 25]

# flt: filter list by predicate
ilo 'pos x:n>b;>x 0 main xs:L n>L n;flt pos xs' main -3,-1,0,2,4
# → [2, 4]

# fld: fold/reduce with accumulator
ilo 'add a:n b:n>n;+a b main xs:L n>n;fld add xs 0' main 1,2,3,4,5
# → 15
```

**Pipe `>>`** — pass result of left as last arg to right. Chains transforms without intermediate names:
```bash
# xs >> flt pos >> map sq  =  map sq (flt pos xs)
ilo 'sq x:n>n;*x x pos x:n>b;>x 0 main xs:L n>L n;xs >> flt pos >> map sq' main -3,-1,0,2,4
# → [4, 16]

# binding the result of a pipe chain:
# clean=xs >> flt pos >> map dbl
```

**Pass list arguments** with commas:
```bash
ilo 'f xs:L n>n;len xs' 1,2,3         # → 3
ilo 'f xs:L t>t;xs.0' 'a,b,c'         # → a
```

**Run from a file:**
```bash
ilo program.ilo 10 20 30
```

**Help & language spec:**
```bash
ilo help                     # usage and examples
ilo -h                       # same as ilo help
ilo help lang                # print the full language specification
ilo help ai                  # compact spec for LLM consumption (~16 lines)
ilo -ai                      # same as ilo help ai
```

**Backends:**

ilo programs can run interpreted or compiled. The default is JIT compilation via Cranelift — every program is verified before execution (all calls resolve, all types align), so the compiler can trust the code and generate efficient native machine code. Falls back to the interpreter for functions using strings, lists, or records (not yet JIT-eligible).

```bash
ilo 'code' args              # default: Cranelift JIT → interpreter fallback
ilo 'code' --run-interp ...  # tree-walking interpreter
ilo 'code' --run-vm ...      # register VM (bytecode compiled)
ilo 'code' --run-cranelift . # Cranelift JIT (compiled to native code)
ilo 'code' --run-jit ...     # custom ARM64 JIT (macOS Apple Silicon only)
```

**Static verification:**

All programs are verified before execution. The verifier checks function existence, arity, variable scope, type compatibility, record fields, and more — reporting all errors at once with stable error codes:

```bash
ilo 'f x:n>n;*y 2' 5
# error[ILO-T004]: undefined variable 'y'
#   = note: in function 'f'

ilo 'f x:t>n;*x 2' hello
# error[ILO-T009]: '*' expects n and n, got t and n
#   = note: in function 'f'
```

Use `--explain` to get a detailed explanation of any error code:

```bash
ilo --explain ILO-T004
```

This matches the manifesto: "verification before execution — all calls resolve, all types align, all dependencies exist."

**Auto-unwrap `!`** eliminates Result matching boilerplate:
```bash
# Without !: 12 tokens
ilo 'inner x:n>R n t;~x outer x:n>R n t;r=inner x;?r{~v:~v;^e:^e}' 42

# With !: 1 token
ilo 'inner x:n>R n t;~x outer x:n>R n t;~(inner! x)' 42
# → 42
```

**HTTP GET** — `get url` or `$url` (terse alias). Returns `R t t` (Ok=body, Err=error message):
```bash
# fetch a URL, get Ok/Err result
ilo 'f url:t>R t t;get url' "http://httpbin.org/get"
# → ~{ ... }

# $ is shorthand for get
ilo 'f url:t>R t t;$url' "http://httpbin.org/get"
# → ~{ ... }

# auto-unwrap with $! — 18 chars for a verified, error-handled HTTP call
ilo 'f url:t>R t t;~($!url)' "http://httpbin.org/get"
# → ~{ ... }
```

**Environment variables** — `env key` reads an env var, returns `R t t`:
```bash
ilo 'f k:t>R t t;env k' "HOME"
# → ~"/Users/dan"

ilo 'f k:t>R t t;env! k' "HOME"
# auto-unwrap: Ok→value, Err→propagate
```

**File I/O** — `rd`, `rdl`, `wr`, `wrl` read and write files; format is auto-detected from extension:
```bash
# rd: read file — auto-detects format from extension
ilo 'f p:t>R ? t;rd p' data.csv      # → Ok([[row1col1 row1col2 …] …])
ilo 'f p:t>R ? t;rd p' data.json     # → Ok(parsed JSON)
ilo 'f p:t>R ? t;rd p' notes.txt     # → Ok("raw text")

# rd with explicit format override
ilo 'f p:t>R ? t;rd p "json"' data.csv   # force JSON parse regardless of extension

# rdb: parse a string/buffer with explicit format (for HTTP responses, env vars, etc.)
ilo 'f s:t>R ? t;rdb s "csv"' "a,b\n1,2"   # → Ok([["a" "b"] ["1" "2"]])

# rdl: read as lines → L t
# wr / wrl: write string / write lines to file → R t t
```

**Data scripting** — string/list utilities:
```bash
ilo 'f s:t>t;trm s' "  hello  "        # → "hello"
ilo 'f xs:L t>L t;unq xs' a,b,a,c,b   # → ["a" "b" "c"]
ilo 'f>t;fmt "{} + {} = {}" 1 2 3'     # → "1 + 2 = 3"
```

**Aggregation & reshape** — `grp`, `flat`, `sum`, `avg`, `rgx` for data pipelines:
```bash
# grp: group list by key function → M t (L a)
ilo 'cl x:n>t;>x 5{"big"}{"small"} f xs:L n>M t L n;grp cl xs' f 1,8,3,9
# → {"small": [1, 3], "big": [8, 9]}

# sum / avg: numeric aggregation
ilo 'f xs:L n>n;sum xs' 1,2,3,4,5      # → 15
ilo 'f xs:L n>n;avg xs' 2,4,6          # → 4

# flat: flatten nested lists one level
# rgx: regex match/extract
ilo 'f s:t>L t;rgx "\d+" s' "abc 123 def 456"   # → ["123", "456"]
```

**Structured output** — `wr` with format arg writes CSV, TSV, or JSON:
```bash
# wr path data "csv" — writes list-of-lists as CSV with proper quoting
# wr path data "json" — writes any value as pretty JSON
```

**Imports** — split programs across files:
```bash
# math.ilo: dbl n:n>n;*n 2
# main.ilo: use "math.ilo"  run n:n>n;dbl n
ilo main.ilo run 5           # → 10
ilo main.ilo run 5           # scoped: use "math.ilo" [dbl]
```

**Environment files** — `.env` and `.env.local` are loaded automatically at startup. `KEY=VALUE` format, `#` comments supported. `.env.local` takes priority. Variables are not overwritten if already set in the process environment:
```bash
echo 'ANTHROPIC_API_KEY=sk-...' > .env
ilo 'f k:t>R t t;env! k' ANTHROPIC_API_KEY   # reads from .env
```

**Error output formats:**
```bash
ilo 'code' -a               # ANSI colour (default for TTY)
ilo 'code' -t               # plain text (no colour)
ilo 'code' -j               # JSON (default for piped output)
NO_COLOR=1 ilo 'code'       # disable colour
```

**Formatter:**

Newlines are for humans — agents don't need them. An entire ilo program can be one line:

```bash
ilo 'code' --dense / -d       # reformat (dense wire format)
ilo 'code' --expanded / -e    # reformat (expanded human format)
```

**Tool execution:**

Tool declarations (`tool get-user"..." uid:t>R profile t`) are external calls. Wire them to HTTP endpoints with a JSON config:

```bash
ilo program.ilo --tools tools.json args...
```

`tools.json` maps tool names to endpoints:

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

ilo serialises call args as `{"args": [...]}`, deserialises the JSON response back to ilo values.

**Other modes:**
```bash
ilo 'code' --emit python     # transpile to Python
ilo 'code'                   # no args/flags → print AST as JSON
ilo program.ilo --bench tot 10 20 30  # benchmark
```

**Run tests:**
```bash
cargo test
```

Tests cover: lexer, parser, interpreter, VM, verifier, codegen, diagnostic, formatter, CLI integration, and annotated example programs.

## Agent Skill

ilo ships as an [Agent Skill](https://agentskills.io) — a portable skill that teaches AI agents to write, run, and debug ilo programs. Works with Claude Code, Codex, Cursor, GitHub Copilot, and any tool that supports the Agent Skills standard.

**Claude Code plugin:**
```bash
/plugin install ilo-lang/ilo
```

**Manual:** copy `skills/ilo/` into your agent's skills directory (e.g. `~/.claude/skills/`, `~/.agents/skills/`, `.cursor/skills/`).

The skill auto-installs the ilo binary if it's not already present.

## Documentation

| Document | Purpose |
|----------|---------|
| [SPEC.md](SPEC.md) | Language specification |
| [examples/](examples/) | Runnable example programs (also `cargo test` regression suite) |
| [MANIFESTO.md](MANIFESTO.md) | Design rationale |
| [research/JOURNEY.md](research/JOURNEY.md) | Design journey — syntax variants, benchmarks, research index |
| [skills/ilo/](skills/ilo/) | Agent Skill (for AI coding agents) |
| [research/TODO.md](research/TODO.md) | Planned work |
| [research/OPEN.md](research/OPEN.md) | Open design questions |

## Community

- [r/ilolang](https://www.reddit.com/r/ilolang/) — discussion, feedback, and updates on Reddit

```
  _  _          _
 (_)| | ___    | |  __ _  _ __    __ _
 | || |/ _ \   | | / _` || '_ \  / _` |
 | || | (_) |  | || (_| || | | || (_| |
 |_||_|\___/   |_| \__,_||_| |_| \__, |
                                   |___/
```
