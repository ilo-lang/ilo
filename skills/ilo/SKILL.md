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

Eight task-focused skills cover the surface. Load only the slices the current task needs (typical: 1-2 modules):

- `ilo-language` writing or reviewing .ilo source: syntax, types, guards, match, pipes, records, Results.
- `ilo-builtins` calling builtin functions: list, text, IO, HTTP, JSON, map, math, time, HOFs.
- `ilo-errors` reading ILO-XXXX codes: lex / parse / type / runtime classes with one-line cause + fix.
- `ilo-tools` declaring and using external tools: MCP servers and HTTP providers.
- `ilo-engines` picking an execution backend: tree, VM, JIT, AOT.
- `ilo-agent` integrating ilo into an agent loop: discovery, running, output contract.
- `ilo-examples` finding a runnable pattern: curated index of `examples/*.ilo` by task shape.
- `ilo-edit-loop` recovering from failures: the repair cycle, JSON diagnostics, common fixes.

The content lives in `skills/ilo/<name>.md`. The installed binary serves the same files via `include_str!`, so the bundled copy and the served copy cannot drift.

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
