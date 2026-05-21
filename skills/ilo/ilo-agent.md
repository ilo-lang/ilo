---
name: ilo-agent
description: Use this when integrating ilo into an agent loop. Covers skill discovery, running programs, and the output contract.
---

# ilo for agents

Workflow surface. For repair cycle + diagnostic shape load `ilo-edit-loop`. Syntax: `ilo-language`. Builtins: `ilo-builtins`.

## Skill discovery

```
ilo skill list                  name + description per skill
ilo skill get <name>            full content
ilo skill path <name>           filesystem path
ilo skill show <name>           content with header
```

Eight skills: `ilo-language`, `ilo-builtins`, `ilo-errors`, `ilo-tools`, `ilo-engines`, `ilo-agent`, `ilo-examples`, `ilo-edit-loop`. Load only what the task needs.

Every skill subcommand accepts `--json`. `ilo skill list --json` returns `{schemaVersion, skills: [{name, description, path}]}`. `ilo -ai` emits the full concatenated spec for back-compat.

## Running

```
ilo file.ilo                       auto-pick main
ilo file.ilo func a b              call named fn
ilo 'f x:n>n;+x 1' 5               inline source
ilo --jit file.ilo --bench main    JIT + bench
ilo file.ilo --bench main --json   bench output as NDJSON
ilo file.ilo --bench main --json --silent  suppress program stdout
```

`--silent` / `-s` mutes program-level `prnt` (and `prnv` / `jprn` / JIT prints) for the run. Paired with `--bench --json` it gives agent harnesses (e.g. persona cost rollup) a clean JSON stream on stdout instead of 10k+ lines of benchmarked output. Stderr is never silenced.

First positional dispatches to a fn when it has ident shape. Otherwise (paths, numbers, sigils, negatives) routes to `main`. Unknown `--flag` shapes are rejected, not consumed.

## Top-level output contract

- `~v` (Ok)   -> stdout = inner value, exit 0
- `^e` (Err)  -> stderr = `^e`, exit 1
- Non-Result  -> stdout = value, exit 0
- `--json`    -> wraps as `{"ok": v}` / `{"error": ...}`

AOT-compiled binaries (`ilo compile`) follow the same contract byte-for-byte.

## Serv mode

`ilo serv [--mcp m.json] [--tools http.json]` is a long-lived JSON request/response loop on stdin/stdout. Send `{"program":"fn p:n>n;*p 2","func":"fn","args":[21]}`, get `{"ok": 42}` or `{"error":{...}}`. Cuts process-spawn overhead to zero.

## AST depth cap

Parser nesting is capped at 256 by default — guards `ilo serv` and any other context that compiles untrusted source against `((((...((1+1))))...))` DoS payloads that would otherwise blow the parser stack. Hand-written ilo rarely exceeds depth 10. Override with `--max-ast-depth N` on `ilo`, `ilo run`, `ilo check`, `ilo build`, or `ilo serv` when a real program needs more. Hitting the cap surfaces as `ILO-P103`.

## Branching

Failures / repair: `ilo-edit-loop`. Runnable patterns: `ilo-examples`. Tools: `ilo-tools`. Engine pick: `ilo-engines`.
