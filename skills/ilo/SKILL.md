---
name: ilo
description: "Write, run, debug, and explain programs in ilo, a token-optimised programming language for AI agents. Use when the user asks to write ilo code, mentions .@ or .ilo files, asks about ilo syntax, wants to create token-optimised programs, or wants to convert code from other languages to ilo."
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

## Bootstrap pointer

This file is a thin bootstrap. The rich, version-matched ilo skill content is served by the installed ilo binary, not embedded here. To discover it:

1. List available skills: `ilo skill list`
2. Get a specific skill: `ilo skill get <name>`
3. JSON envelope: `ilo skill list --json`

Every skill subcommand accepts `--json` (short alias `-j`, ILO-442). The envelope is `{schemaVersion: 1, ...}`, matching the rest of ilo's CLI JSON contract. `-j` works on every CLI subcommand — `ilo check -j`, `ilo spec -j ai`, `ilo run -j`, etc.

## Available skills

Twelve task-focused skills cover the surface. Load only the slices the current task needs (typical: 1-2 modules):

- `ilo-language` writing or reviewing .@ source: syntax, types, guards, match, pipes, records, Results.
- `ilo-language-records` writing ilo code with record types: declarations, construction, field access, destructuring, update syntax, safe navigation. Load alongside `ilo-language` when your code uses `type`.
- `ilo-builtins-core` core builtins: type coercions (`len str num trm`), list ops, HOFs, map ops.
- `ilo-builtins-math` math builtins: arithmetic, trig, constants (`pi tau e`), random, statistics.
- `ilo-builtins-io` I/O builtins: file read/write, HTTP, JSON, path ops, env, time, process.
- `ilo-builtins-text` text builtins: manipulation, regex, formatting (`fmt fmt2`), CSV/TSV, date parsing (`dtfmt`, `dtparse`, `dtparse-rel` for relative phrases), calendar arithmetic (`add-mo`, `last-dom`, `next-business-day`, `day-of-week`).
- `ilo-errors` reading ILO-XXXX codes: lex / parse / type / runtime classes with one-line cause + fix.
- `ilo-tools` declaring and using external tools: MCP servers and HTTP providers.
- `ilo-engines` picking an execution backend: tree, VM, JIT, AOT.
- `ilo-agent` integrating ilo into an agent loop: discovery, running, output contract.
- `ilo-examples` finding a runnable pattern: curated index of `examples/*.@` by task shape.
- `ilo-edit-loop` recovering from failures: the repair cycle, JSON diagnostics, common fixes.

The content lives in `skills/ilo/<name>.md`. The installed binary serves the same files via `include_str!`, so the bundled copy and the served copy cannot drift.

## Quick reference - things agents miss

**Text concatenation - three builtins, three jobs.** Pick by shape, not by habit:

- `+ a b` - two-arg text/number concat (also list concat). `+ "hi " name` -> `"hi alice"`.
- `fmt "x={} y={}" x y` - template formatting (variadic; `{}` placeholders filled left-to-right, count must equal arg count, no list splat). `{name}` slots auto-desugar to a lookup of the binding `name`.
- `cat xs sep` - join a list of text with a separator. `cat ["a" "b" "c"] ","` -> `"a,b,c"`. NOT two-string concat; reach for `+` for that.
- **Labelled args (ILO-71):** any callable with declared parameter names accepts `label:value` form. `dtfmt epoch:e fmt:"%Y"` ≡ `dtfmt e "%Y"`. Order is free; mix positional + labelled freely (positional fill from left, labels fill remaining slots by name). Works in postfix and paren form. Unknown label → `ILO-P019` with known-params list.

**HTTP custom headers.** Every verb in the cluster (`get pst put pat del hed opt`) accepts an optional trailing `M t t` headers map. The two-arg `get url headers` and three-arg `pst url body headers` forms are real, not workarounds. Build the map with `mmap` + `mset`:

```
hs=mset (mset mmap "Authorization" tok) "Accept" "application/json"
r=get! "https://api.example.com/v1/users" hs
```

**`jpth` is dot-path, not JSONPath.** `jpth body "user.addresses.0.city"` works. Leading `$`, `*`, and `[...]` are rejected with a diagnostic - don't reach for JSONPath wildcards. Numeric list indices are bare numbers in the path, segment-separated by `.`.

## Reserved names (ILO-P011)

Every builtin name, builtin alias, and control-flow keyword is reserved. Using any as a binding triggers ILO-P011 at parse time. Use 4+ character descriptive names (`item`, `rows`, `accum`, `total`, `count`, `index`, `result`) to stay clear of this class of error permanently.

**Lexer keywords** (reserved tokens, never identifiers): `fn` `def` `let` `var` `const` `if` `return` `true` `false` `nil` `type` `tool` `use`

**1-char**: `e` (Euler's number — math constant)

**2-char**: `at` `ct` `hd` `rd` `tl` `wr` `wh` (while-loop keyword)

**3-char builtins**: `abs` `avg` `cap` `cat` `cel` `chr` `cos` `det` `dot` `env` `exp` `fft` `fld` `flt` `fmt` `frq` `get` `grp` `has` `inv` `log` `lsd` `lst` `lwr` `map` `max` `min` `mod` `now` `ord` `pi` `pow` `pst` `rdb` `rdl` `rev` `rgx` `rnd` `rou` `run` `sin` `slc` `spl` `srt` `sum` `tan` `tau` `trm` `unq` `upr` `wra` `wrl` `zip`

**3-char control-flow**: `brk` (break) `cnt` (continue) `ret` (return). `ret` from inside any loop body (`@x xs`, `@i a..b`, `wh`) returns from the enclosing fn directly - no sentinel-flag pattern needed.

**4-char+**: `acos` `asin` `atan` `argmax` `argmin` `argsort` `basename` `chars` `chunks` `clamp` `cprod` `cumsum` `dirname` `dot` `drop` `dtfmt` `dtparse` `dtparse-rel` `enumerate` `flat` `flatmap` `fmod` `fmt2` `fsize` `glob` `ifft` `inv` `isdir` `isfile` `jdmp` `jkeys` `jpar` `jpth` `len` `mapr` `matmul` `matvec` `mdel` `median` `mget` `mhas` `mkeys` `mmap` `mpairs` `mset` `mtime` `mvals` `padl` `padr` `partition` `pathjoin` `prnt` `prod` `quantile` `rand-bytes` `range` `rdb` `rdin` `rdinl` `rdjl` `rdl` `rndn` `rsrt` `setdiff` `setinter` `setunion` `sleep` `solve` `sqrt` `stdev` `take` `transpose` `uniqby` `variance` `walk` `where` `window` `wra` `wrl`
**4-char+**: `acos` `asin` `atan` `argmax` `argmin` `argsort` `basename` `chars` `chunks` `clamp` `cprod` `cumsum` `dirname` `dot` `drop` `dtfmt` `dtparse` `dtparse-rel` `enumerate` `flat` `flatmap` `fmod` `fmt2` `fsize` `glob` `ifft` `inv` `isdir` `isfile` `jdmp` `jkeys` `jpar` `jpth` `len` `mapr` `matmul` `matvec` `mdel` `median` `mget` `mhas` `mkeys` `mmap` `mpairs` `mset` `mtime` `mvals` `padl` `padr` `partition` `pathjoin` `prnt` `prod` `quantile` `rand-bytes` `range` `rdb` `rdin` `rdinl` `rdjl` `rdl` `rndn` `rsrt` `setdiff` `setinter` `setunion` `sleep` `solve` `sqrt` `stdev` `take` `transpose` `uniqby` `variance` `walk` `window` `wra` `wrl`
**4-char+**: `acos` `asin` `atan` `argmax` `argmin` `argsort` `basename` `chars` `chunks` `clamp` `cprod` `cumsum` `dirname` `dot` `drop` `dtfmt` `dtparse` `dtparse-rel` `enumerate` `flat` `flatmap` `fmod` `fmt2` `fsize` `glob` `ifft` `inv` `isdir` `isfile` `jdmp` `jkeys` `jpar` `jpth` `len` `mapr` `matmul` `mdel` `median` `mget` `mhas` `mkeys` `mmap` `mpairs` `mset` `mtime` `mvals` `padl` `padr` `partition` `pathjoin` `prnt` `prod` `quantile` `range` `rdb` `rdin` `rdinl` `rdjl` `rdl` `rndn` `rsrt` `seed` `setdiff` `setinter` `setunion` `sleep` `solve` `sqrt` `stdev` `take` `transpose` `uniqby` `variance` `walk` `window` `wra` `wrl`

**Builtin aliases** (long-form names that resolve to short builtins, also reserved): `length`→`len`, `head`→`hd`, `tail`→`tl`, `reverse`→`rev`, `sort`→`srt`, `slice`→`slc`, `unique`→`unq`, `filter`→`flt`, `fold`→`fld`, `flatten`→`flat`, `concat`→`cat`, `contains`→`has`, `group`→`grp`, `average`→`avg`, `print`→`prnt`, `trim`→`trm`, `split`→`spl`, `format`→`fmt`, `regex`→`rgx`, `read`→`rd`, `readlines`→`rdl`, `write`→`wr`, `writelines`→`wrl`, `ceil`→`cel`, `round`→`rou`, `rand`/`random`→`rnd`, `rng`→`range`, `string`→`str`, `number`→`num`.

### One-liner rename suggestions

Agents reach for these names constantly — pick the listed alternative and move on:

- `fn` (keyword) → `f`, `fv`, `func`, or a descriptive name like `callback` / `predicate`
- `def` (keyword) → `d`, `defn`, `defv`
- `let` / `var` / `const` (keywords) → `l`/`lv`/`letv`, `v`/`value`, `c`/`k`/`constv`
- `if` / `return` (keywords) → `cond`/`flag`/`iff`, `result`/`out`/`ret_val`
- `e` (math constant) → `event`, `evt`, `entry`, `elem`, `err`
- `env` (builtin) → `ev`, `envir`, `environ`, `envv`
- `log` (math builtin) → `lg`, `logger`, `entry`, `record`
- `now` (time builtin) → `tnow`, `current`, `nowts`, `timestamp`
- `iter` (not reserved today, but agents often pair with builtins — prefer descriptive) → `i`, `idx`, `step`, `cursor`
- `chars` (builtin) → `cs`, `glyphs`, `letters`
- `head` / `tail` / `length` (aliases) → `hd`/`tl`/`len` directly, or `first`/`rest`/`size`
- `filter` / `sort` / `concat` / `fold` (aliases) → `flt`/`srt`/`cat`/`fld` directly, or `keep`/`sorted`/`joined`/`reduced`
- `fld` (fold builtin) → `field`, `folder`, `record`
- `cnt` / `brk` (loop control) → `count`/`index`, `brake`/`stop`

When in doubt: pick a 4+ char descriptive name. The token cost of an extra character is dwarfed by the cost of an ILO-P011 retry round-trip.

## Streaming stdin (ILO-70)

For unbounded input streams (e.g. `tail -f`, a producer that never closes stdin), use `for-line` instead of `rdinl`:

```
-- rdinl buffers ALL of stdin before returning — blocks on infinite/slow producers.
-- for-line "stdin" yields one line at a time as the pipe delivers it.
main>n;n=0;@line (for-line "stdin"){prnt line;n=+ n 1};n
```

`for-line` takes exactly one arg: the text `"stdin"`. It is iterable directly with `@binding` foreach. On WASM it returns Err. Tree + VM engines only (Cranelift JIT follow-up). Keep using `rdinl` when you need random access to all lines (e.g. to sort them).

## Date/time builtins

`now` and `now-ms` are not the whole surface. Full set: `now` (Unix seconds), `now-ms` (Unix ms), `dtfmt ts fmt` (timestamp to text), `dtparse s fmt` (text to timestamp, `R n t`), `dtparse-rel s now` (relative phrase like `"in 3 days"` / `"last monday"` anchored at `now`, `R n t`). Reach for `dtparse-rel` before hand-rolling phrase parsers. Details in `ilo skill get ilo-builtins-text`.
**4-char+**: `acos` `asin` `atan` `argmax` `argmin` `argsort` `basename` `chars` `chunks` `clamp` `cprod` `cumsum` `dirname` `dot` `drop` `dtfmt` `dtparse` `dtparse-rel` `enumerate` `flat` `flatmap` `fmod` `fmt2` `fsize` `glob` `ifft` `inv` `isdir` `isfile` `jdmp` `jkeys` `jpar` `jpth` `len` `mapr` `matmul` `matvec` `mdel` `median` `mget` `mhas` `mkeys` `mmap` `mpairs` `mset` `mtime` `mvals` `padl` `padr` `partition` `pathjoin` `prnt` `prod` `quantile` `rand-bytes` `range` `rdb` `rdin` `rdinl` `rdjl` `rdl` `rndn` `rsrt` `setdiff` `setinter` `setunion` `sleep` `solve` `sqrt` `stdev` `take` `transpose` `uniqby` `variance` `walk` `window` `wra` `wrl`
**4-char+**: `acos` `asin` `atan` `argmax` `argmin` `argsort` `basename` `chars` `chunks` `clamp` `cprod` `cumsum` `dirname` `dot` `drop` `dtfmt` `dtparse` `dtparse-rel` `enumerate` `flat` `flatmap` `fmod` `fmt2` `fsize` `glob` `ifft` `inv` `isdir` `isfile` `jdmp` `jkeys` `jpar` `jpth` `len` `mapr` `matmul` `matvec` `mdel` `median` `mget` `mhas` `mkeys` `mmap` `mpairs` `mset` `mtime` `mvals` `padl` `padr` `partition` `pathjoin` `prnt` `prod` `quantile` `range` `rdb` `rdin` `rdinl` `rdjl` `rdl` `rndn` `rsrt` `setdiff` `setinter` `setunion` `sleep` `solve` `sqrt` `stdev` `take` `transpose` `uniqby` `variance` `walk` `window` `wra` `wrl`
**4-char+**: `acos` `asin` `atan` `argmax` `argmin` `argsort` `basename` `chars` `chunks` `clamp` `cprod` `cumsum` `dirname` `dot` `drop` `dtfmt` `dtparse` `dtparse-rel` `enumerate` `flat` `flatmap` `fmod` `fmt2` `fsize` `glob` `ifft` `inv` `isdir` `isfile` `jdmp` `jkeys` `jpar` `jpth` `len` `mapr` `matmul` `mdel` `median` `mget` `mhas` `mkeys` `mmap` `mpairs` `mset` `mtime` `mvals` `padl` `padr` `partition` `pathjoin` `prnt` `prod` `quantile` `range` `rdb` `rdin` `rdinl` `rdjl` `rdl` `rndn` `rsrt` `setdiff` `setinter` `setunion` `sleep` `solve` `sqrt` `stdev` `take` `transpose` `tz-offset` `uniqby` `variance` `walk` `window` `wra` `wrl`

## Common pitfalls

**No tuple type.** `zip xs ys` returns `L (L n)` — a list of two-element lists, not a list of tuples. Destructure pairs with `at pair 0` / `at pair 1`, never `pair.0` / `pair.1` where `pair` is unbound. ILO-T004 on `tup.0` / `pair.0` carries a hint naming the exact `at <name> <N>` call to write.

```
-- DON'T: zs=zip xs ys; +tup.0 tup.1   -- tup is unbound; ilo has no tuple type
-- DO:    zs=zip xs ys; map (pair:L n>n;+at pair 0 at pair 1) zs
```

(`pair.0` itself is valid sugar for list indexing once `pair` is bound to an `L T` parameter; the diagnostic only fires when the identifier is unbound.)

## Compatibility note

ilo has no borrow checker, no lifetime annotations, no ownership rules. Values are RC-managed; the type checker enforces shape only. There is no `&`, no `&mut`, no `'a`. If an agent is reaching for lifetime-style reasoning in ilo, it has the wrong mental model.

## Quick start

If the binary is available, the fastest path is:

```bash
ilo skill list --json | jq
ilo skill get ilo-language
ilo skill get ilo-edit-loop
```

That gives the modular reference, the repair loop, and the discovery contract in three commands.
