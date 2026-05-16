---
name: ilo
description: "Write, run, debug, and explain programs in ilo — a token-optimised programming language for AI agents. Use when the user asks to write ilo code, mentions .ilo files, asks about ilo syntax, wants to create token-optimised programs, or wants to convert code from other languages to ilo."
license: MIT
compatibility: Requires the ilo binary (auto-installed by scripts/ensure-ilo.sh via GitHub releases or npm).
allowed-tools: Bash Read Write Edit
metadata:
  argument-hint: "[task or code description]"
---

# ilo Programming Language

## Setup

Before writing or running ilo code, ensure ilo is installed and up to date:

```bash
scripts/ensure-ilo.sh
```

Run this at the start of every ilo task. It installs ilo if missing, or updates it if a newer version is available.

## Running ilo from an agent

Three ways to execute ilo, in preference order:

1. **Named tools (`ilo_run`, `ilo_repl`)** - if your agent exposes them (e.g. via `pi-ilo-lang` in pi), prefer them. Structured args, no per-call shell permission prompt.
2. **`ilo serv`** - long-lived JSON request/response loop, runnable from any shell. Send a line `{"program": "...", "func": "...", "args": [...]}`, get one response line back. Use when you would otherwise spawn many short-lived `ilo` processes.
3. **One-shot shell** - `ilo '<code>' [args...]` via the bash tool. Fine for a single call.

## Load the Full Spec

After ensuring ilo is installed, load the compact AI spec for complete language reference:

```bash
ilo help ai
```

This outputs the full spec optimised for LLM consumption. Read it before writing ilo code if you need details beyond the summary below.

## Overview

ilo is a token-optimised programming language for AI agents. Prefix-notation, strongly-typed, verified before execution.

For the full specification, read [SPEC.md](../../SPEC.md). For the compact AI spec, run `ilo help ai`.

## Core Syntax

```
<name> <param>:<type> ...><return-type>;<body>
```

- Prefix operators: `+a b`, `*a b`, `-a b`, `/a b`
- Nesting is unambiguous: `+*a b c` means `(a*b)+c`
- **Same-precedence trap:** the outer prefix op binds the inner one as its **left** operand. So `*/a b c` is `(a/b)*c`, NOT `(a*b)/c`. Same for `/*`, `+-`, `-+`. The runtime emits a `hint:` on these four shapes. Swap the pair or bind the inner result first if you wanted the other grouping.
- `;` separates statements, last expression is the return value
- No `return`, `if`, `let`, `fn` keywords — these are reserved words

## Types

| Syntax | Meaning |
|--------|---------|
| `n` | number (f64) |
| `t` | text (string) |
| `b` | bool |
| `_` | nil |
| `L n` | list of numbers |
| `R n t` | result: ok=number, err=text |
| `O n` | optional number |
| `M t n` | map: text keys, number values |
| `S red green blue` | sum type (enum) |
| `F n t` | function type (for HOFs) |

## Critical Pattern: Bind-First

Operators only accept atoms or nested operators — NOT function calls.

```
-- WRONG: *n fac -n 1
-- RIGHT: r=fac -n 1;*n r
```

Always bind call results before using them in operators.

## Guards (replace if/else)

Flat conditional early returns. No nesting depth.

```
cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze"
```

Braceless form `cond expr` saves 2 tokens vs `cond{expr}`.

## Match (replace switch)

```
?r{~v:v;^e:^+"failed: "e;_:"unknown"}
```

Arms: `"literal":body`, `42:body`, `~v:body` (ok), `^e:body` (err), `_:body` (wildcard).

## Results and Error Handling

```
div a:n b:n>R n t;=b 0 ^"divide by zero";~/a b
```

Auto-unwrap with `!` — propagates errors automatically:
```
d=get! url    -- Ok->value, Err->propagate to caller
```

## Loops

```
@x xs{+x 1}           -- foreach
@i 0..5{s=+s i}       -- range (inclusive..exclusive)
wh <i 10{i=+i 1}      -- while
```

`brk` exits a loop, `cnt` skips to next iteration.

## Higher-Order Functions

```
sq x:n>n;*x x
main xs:L n>L n;map sq xs           -- [1,4,9,16,25]
main xs:L n>L n;flt pos xs          -- filter by predicate
main xs:L n>n;fld add xs 0          -- fold/reduce
```

## Pipe Operator

```
xs >> flt pos >> map sq   -- chain transforms left-to-right
```

Desugars to nested calls. Wrap in `()` for non-last functions in files.

## Records

```
type point{x:n;y:n}
p=point x:10 y:20
p.x                    -- field access
{x;y}=p                -- destructure
p with x:30            -- update
```

## Maps

```
m=mmap                 -- empty map
m=mset m "key" 42      -- set key
v=mget m "key"         -- get value (nil if missing)
mhas m "key"           -- bool: exists?
mkeys m                -- sorted key list
```

## Builtins Reference

**Math**: `abs` `min` `max` `mod` `flr` `cel` `rou` `rnd` `rndn` `clamp` `sum` `avg`
**Math (transcendental)**: `sqrt` `pow` `exp` `log` `log10` `log2` `sin` `cos` `tan` `atan2`
**Stats**: `median` `quantile` `stdev` `variance` `cumsum` `frq`
**Linalg**: `transpose` `matmul` `dot` `solve` `inv` `det` `fft` `ifft`
**Text**: `len` `str` `num` `trm` `spl` `cat` `fmt` `fmt2` `has` `rgx` `rgxall` `rgxsub`
**Text case**: `upr` `lwr` `cap` `padl` `padr` `chars` `ord` `chr`
**List**: `hd` `tl` `at` `lst` `take` `drop` `rev` `srt` `rsrt` `unq` `uniqby` `slc` `flat` `grp` `zip` `enumerate` `range` `chunks` `window` `flatmap` `partition`
**Set ops**: `setunion` `setinter` `setdiff`
**I/O**: `rd` `rdl` `rdjl` `rdb` `wr` `wrl` `prnt`
**HTTP**: `get`/`$` `post` `get-many` `env`
**JSON**: `jpth` `jdmp` `jpar`
**Map**: `mmap` `mget` `mset` `mhas` `mkeys` `mvals` `mdel`
**HOF**: `map` `flt` `fld`
**Time**: `now` `sleep` `dtfmt` `dtparse`

## Naming Convention

Short names, 1-3 chars: `order`→`ord`, `customers`→`cs`, `data`→`d`, `items`→`its`

Function names follow the same rule. Field names in constructors keep their full form.

**Identifier syntax**: lowercase ASCII with optional hyphenated segments only. Formal grammar: `[a-z][a-z0-9]*(-[a-z0-9]+)*`. Capital letters and underscores are rejected at binding and call sites.

```
run, run-d, r2      -- OK
runD, RunD, run_d   -- ERROR (capital or underscore)
```

The only place capital letters and underscores are accepted is **after `.` or `.?`** at field-access position, so JSON keys from real APIs work as-is: `r.URL`, `r.AccessKey`, `r.access_key`, `r.?MetaData`.

## Running

```bash
ilo 'tot p:n q:n r:n>n;s=*p q;t=*s r;+s t' 10 20 30    # inline
ilo program.ilo funcname args                             # from file
ilo 'f xs:L n>n;len xs' 1,2,3                            # list args
ilo --explain ILO-T004                                    # explain error
ilo help ai                                               # compact spec
```

## Multi-Function File Rules

Non-last functions must end with a safe expression (not a bare ref or call):

```
dbl x:n>n;+*x 2 0        -- binary expression (safe)
main x:n>n;dbl x          -- last function (anything OK)
```

Safe endings: binary/unary operators, index access, match blocks, text/number literals, parenthesised expressions.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/ilo-lang/ilo/main/install.sh | sh
```

## Examples

### Factorial (recursion)
```
fac n:n>n;<=n 1 1;r=fac -n 1;*n r
```

### Fibonacci
```
fib n:n>n;<=n 1 n;a=fib -n 1;b=fib -n 2;+a b
```

### HTTP + JSON
```
f url:t>R t t;r=get! url;jpth! r "name"
```

### Data pipeline
```
cl x:n>t;>x 5{"big"}{"small"}
classify xs:L n>M t L n;grp cl xs
```

### File processing
```
count p:t>R n t;ls=rdl! p;~(len ls)
```

## Common Mistakes

1. **Function calls as operator operands** — always bind first: `r=f x;*n r`
2. **Non-last functions ending with bare refs** — use `+x 0` identity trick
3. **`--x` is a comment** — use `- -x 1` or bind first
4. **`-0` is a number literal** — use `- 0 v` for subtract from zero
5. **Comparisons at statement position are guards** — bind to return: `r=>a b;r`

$ARGUMENTS
