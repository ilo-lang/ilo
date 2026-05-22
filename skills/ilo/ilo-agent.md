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

**Auto-echo suppression.** An entry-fn ending in a bare `prnt` call, a tail loop with no early return, or — when the body has an unconditional top-level `prnt` — a wrapped string-literal tail `~"text"` / `^"text"` (status sentinel) does NOT auto-echo its return value. The collision-avoidance rules let you write `m>R t t;prnt "report";~"ok"` and get clean `report\n` on stdout instead of `report\nok\n`. A no-prnt function returning `~"ok"` (e.g. `addtask`) still emits `ok` — the wrapped literal IS the output. `~v` where `v` is a binding or call always auto-echoes; only string LITERAL sentinels are dropped.

## Testing

`ilo test <path>` runs `-- run: <fn> <args>` / `-- out: <expected>` (and `-- err:` for `^reason` shapes) annotations embedded in `.ilo` files. Same format the in-tree integration harness uses, surfaced as a user-facing command so end-user programs and test suites can assert behaviour the same way examples do.

```
ilo test program.ilo          single file
ilo test tests/               walk a directory recursively
ilo test program.ilo --engine all   run every engine, tag PASS/FAIL with [vm]/[jit]
```

Exit 0 on all-pass, 1 on any failure. Default engine is `vm`; `--engine jit` / `--engine all` widen the matrix. `-- engine-skip: vm jit` annotations in the file source skip the listed engines for that file. Same `-- run:` / `-- out:` / `-- err:` format every example in the repo uses, so an agent writing tests can copy from any nearby example file.

## Serv mode

`ilo serv [--mcp m.json] [--tools http.json]` is a long-lived JSON request/response loop on stdin/stdout. Send `{"program":"fn p:n>n;*p 2","func":"fn","args":[21]}`, get `{"ok": 42}` or `{"error":{...}}`. Cuts process-spawn overhead to zero.

## AST depth cap

Parser nesting is capped at 256 by default — guards `ilo serv` and any other context that compiles untrusted source against `((((...((1+1))))...))` DoS payloads that would otherwise blow the parser stack. Hand-written ilo rarely exceeds depth 10. Override with `--max-ast-depth N` on `ilo`, `ilo run`, `ilo check`, `ilo build`, or `ilo serv` when a real program needs more. Hitting the cap surfaces as `ILO-P103`.

## Runtime + output caps

`ilo run`: wall-clock 60 s (`ILO-R016`), stdout ~100 MB (`ILO-R017`). Override: `--max-runtime SECS` / `--max-output-bytes BYTES` (0 disables). Hitting either = missing loop increment or no base case.

## Packages

GitHub-based registry; no central server.

```
ilo add ilo-lang/ilo-example-package        -- shallow-clone into ~/.ilo/pkgs/, write ilo.lock
ilo add owner/repo@v1.2                     -- pin to tag / branch / SHA
ilo update                                  -- re-fetch all locked packages
```

After `ilo add`, import with `use`:

```
use "ilo-lang/ilo-example-package"               -- all public symbols from index.ilo
use "ilo-lang/ilo-example-package" [greet clamp] -- selective import

main>_
  prnt greet "agent"      -- hello, agent
  prnt str clamp 15 0 10  -- 10
```

`ilo.lock` records slug, SHA, and URL — commit it to source control. A path whose first component has no `.` is always a package reference; use `"./local.ilo"` for local files.

## Branching

Failures / repair: `ilo-edit-loop`. Runnable patterns: `ilo-examples`. Tools: `ilo-tools`. Engine pick: `ilo-engines`.
