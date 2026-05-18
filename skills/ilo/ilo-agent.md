---
name: ilo-agent
description: Use this when integrating ilo into an agent loop. Covers skill discovery, running programs, reading JSON diagnostics, and the repair loop.
---

# ilo for agents

ilo is designed for agent loops: dense source, verified before run, structured diagnostics. This module covers the workflow, not the language. For syntax load `ilo-language`; for builtins load `ilo-builtins`.

## Skill discovery

```
ilo skill list                  name + description per skill
ilo skill get <name>            full content
ilo skill path <name>           filesystem path
ilo skill show <name>           content with header
```

Six skills: `ilo-language`, `ilo-builtins`, `ilo-errors`, `ilo-tools`, `ilo-engines`, `ilo-agent`. Load only what the task needs; typical load is 1-2 modules.

`ilo -ai` emits the full concatenated spec for back-compat. Prefer modular loading.

## Running

```
ilo file.ilo                       auto-pick main
ilo file.ilo func a b              call named fn
ilo 'f x:n>n;+x 1' 5               inline source
ilo --jit file.ilo --bench main    JIT + bench
```

First positional dispatches to a fn when it has ident shape. Otherwise (paths, numbers, sigils, negatives) routes to `main`. Unknown `--flag` shapes are rejected, not consumed.

## Verification

```
ilo check file.ilo --json
```

Type-checks and runs the verifier without executing. Output is the JSON diagnostic stream; exit 0 means valid. Use this in the agent loop to validate before any side-effecting `ilo run`.

## Diagnostic format

When stderr isn't a TTY (or `--json`), every error is one JSON line:

```json
{"code":"ILO-T004","message":"expected n, got t",
 "span":{"file":"x.ilo","line":3,"col":12,"len":5},"hint":"..."}
```

Route on `code` (`ILO-L*` lex, `ILO-P*` parse, `ILO-T*` type, `ILO-R*` runtime). Edit at `span`. Pass `code` to `ilo --explain` for the long form. For full code list load `ilo-errors`.

## Repair loop

1. Generate ilo source.
2. `ilo check file.ilo --json`; if exit 0, go to 4.
3. Read first diagnostic, route on `code`, load `ilo-errors` if needed, apply fix at `span`. Back to 2.
4. `ilo run file.ilo`, capture stdout. `^e` is a runtime error; otherwise consume the value.

Bound retries (3 per code). On persistent failure, dump source + diagnostics and escalate.

## Top-level output contract

- `~v` (Ok)   -> stdout = inner value, exit 0
- `^e` (Err)  -> stderr = `^e`, exit 1
- Non-Result  -> stdout = value, exit 0
- `--json`    -> wraps in `{"ok": v}` / `{"error": ...}`

AOT-compiled binaries (`ilo compile`) follow the same contract byte-for-byte. Pipe stdout to consumer; check exit code for branching.

## Serv mode

```
ilo serv [--mcp m.json] [--tools http.json]
```

Long-lived JSON request/response loop on stdin/stdout. Send one line:

```json
{"program": "fn p:n>n;*p 2", "func": "fn", "args": [21]}
```

Get one line back: `{"ok": 42}` or `{"error": {...}}`. Use this when you'd otherwise spawn many short-lived `ilo` processes; cuts startup overhead to zero.

## Tools

If the task involves MCP servers or HTTP tool providers, load `ilo-tools`. If picking a non-default execution engine matters, load `ilo-engines`.
