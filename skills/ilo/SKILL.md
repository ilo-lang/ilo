---
name: ilo
description: "Write, run, debug, and explain programs in ilo, a token-optimised programming language for AI agents. Use when the user asks to write ilo code, mentions .ilo files, asks about ilo syntax, wants to create token-optimised programs, or wants to convert code from other languages to ilo."
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

Every skill subcommand accepts `--json`. The envelope is `{schemaVersion: 1, ...}`, matching the rest of ilo's CLI JSON contract.

## Available skills

Twelve task-focused skills cover the surface. Load only the slices the current task needs (typical: 1-2 modules):

- `ilo-language` writing or reviewing .ilo source: syntax, types, guards, match, pipes, Results, loops, lambdas.
- `ilo-language-records` writing ilo code with record types: declarations, construction, field access, destructuring, update syntax, safe navigation. Load alongside `ilo-language` when your code uses `type`.
- `ilo-builtins-core` core builtins: type coercions (`len str num trm`), list ops, HOFs, map ops.
- `ilo-builtins-math` math builtins: arithmetic, trig, constants (`pi tau e`), random, statistics.
- `ilo-builtins-io` I/O builtins: file read/write, HTTP, JSON, path ops, env, time, process.
- `ilo-builtins-text` text builtins: manipulation, regex, formatting (`fmt fmt2`), CSV/TSV, date parsing (`dtfmt`, `dtparse`, `dtparse-rel` for relative phrases).
- `ilo-errors` reading ILO-XXXX codes: lex / parse / type / runtime classes with one-line cause + fix.
- `ilo-tools` declaring and using external tools: MCP servers and HTTP providers.
- `ilo-engines` picking an execution backend: tree, VM, JIT, AOT.
- `ilo-agent` integrating ilo into an agent loop: discovery, running, output contract.
- `ilo-examples` finding a runnable pattern: curated index of `examples/*.ilo` by task shape.
- `ilo-edit-loop` recovering from failures: the repair cycle, JSON diagnostics, common fixes.

The content lives in `skills/ilo/<name>.md`. The installed binary serves the same files via `include_str!`, so the bundled copy and the served copy cannot drift.

## Reserved names (ILO-P011)

Every builtin name and control-flow keyword is reserved. Using any as a binding triggers ILO-P011. Use 4+ character descriptive names (`item`, `rows`, `accum`, `total`, `count`, `index`, `result`) to stay clear of this class of error permanently.

**1-char**: `e` (Euler's number — math constant)

**2-char**: `at` `ct` `hd` `rd` `tl` `wr` `wh` (while-loop keyword)

**3-char builtins**: `abs` `avg` `cap` `cat` `cel` `chr` `cos` `det` `dot` `env` `exp` `fft` `fld` `flt` `fmt` `frq` `get` `grp` `has` `inv` `log` `lsd` `lst` `lwr` `map` `max` `min` `mod` `now` `ord` `pi` `pow` `pst` `rdb` `rdl` `rev` `rgx` `rnd` `rou` `run` `sin` `slc` `spl` `srt` `sum` `tan` `tau` `trm` `unq` `upr` `wra` `wrl` `zip`

**3-char control-flow**: `brk` (break) `cnt` (continue) `ret` (return)

**4-char+**: `acos` `asin` `atan` `argmax` `argmin` `argsort` `basename` `chars` `chunks` `clamp` `cprod` `cumsum` `dirname` `dot` `drop` `dtfmt` `dtparse` `dtparse-rel` `enumerate` `flat` `flatmap` `fmod` `fmt2` `fsize` `glob` `ifft` `inv` `isdir` `isfile` `jdmp` `jkeys` `jpar` `jpth` `len` `mapr` `matmul` `matvec` `mdel` `median` `mget` `mhas` `mkeys` `mmap` `mpairs` `mset` `mtime` `mvals` `padl` `padr` `partition` `pathjoin` `prnt` `prod` `quantile` `range` `rdb` `rdin` `rdinl` `rdjl` `rdl` `rndn` `rsrt` `setdiff` `setinter` `setunion` `sleep` `solve` `sqrt` `stdev` `take` `transpose` `uniqby` `variance` `walk` `window` `wra` `wrl`

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
