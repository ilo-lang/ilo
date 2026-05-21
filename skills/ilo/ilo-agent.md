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
ilo file.@                         auto-pick main
ilo file.@ func a b                call named fn
ilo 'f x:n>n;+x 1' 5              inline source
ilo --jit file.@ --bench main     JIT + bench
```

First positional dispatches to a fn when it has ident shape. Otherwise (paths, numbers, sigils, negatives) routes to `main`. Unknown `--flag` shapes are rejected, not consumed.

## Top-level output contract

- `~v` (Ok)   -> stdout = inner value, exit 0
- `^e` (Err)  -> stderr = `^e`, exit 1
- Non-Result  -> stdout = value, exit 0
- `--json`    -> wraps as `{"ok": v}` / `{"error": ...}`

AOT-compiled binaries (`ilo compile`) follow the same contract byte-for-byte.

## Serv mode

`ilo serv [--mcp m.json] [--tools http.json]` is a long-lived JSON request/response loop on stdin/stdout. Send `{"program":"fn p:n>n;*p 2","func":"fn","args":[21]}`, get `{"ok": 42}` or `{"error":{...}}`. Cuts process-spawn overhead to zero.

## Agent-natural surface (experimental, compat/agent-natural branch)

On this branch and in 0.13.0-natural mode, lead with the agent-natural forms in generated code:

- `if cond { a } else { b }` for value-producing conditionals; `if cond { body }` for the no-else statement form.
- `for x in xs { body }` and `for i in a..b { body }` for loops; `while cond { body }` for while.
- Match arms accept brace-block bodies: `?r{~v:{d=*v 2;+d 1};^e:body}`.

The prefix/`?h`/`@`/`wh` forms still parse for backwards compatibility. See `SPEC-AGENT-NATURAL.md` for the falsification criterion this experiment is being measured against.

## Branching

Failures / repair: `ilo-edit-loop`. Runnable patterns: `ilo-examples`. Tools: `ilo-tools`. Engine pick: `ilo-engines`.
