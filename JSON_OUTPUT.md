# JSON output across the ilo CLI

Every ilo subcommand that produces machine-readable output supports a
`--json` (or `-j`) flag. JSON outputs are pure JSON on `stdout`; any prose
goes to `stderr`. Plain-text output is the default and is unchanged.

Every JSON envelope carries `"schemaVersion": 1` as the canonical contract
field so agents can route on the contract and the schema can evolve without
breaking older consumers. Six long-standing outputs were brought into the
convention in 0.12.1: `ilo run`, `ilo graph`, `ilo --ast`, `ilo serv`,
`ilo tools --json`, and the newly-added `ilo spec --json` mode. For
five of those six the change is strictly additive - the existing
envelopes were JSON objects with key sets disjoint from `schemaVersion`,
so old consumers that never read the new field keep working unchanged.

The one observable break in 0.12.1 is `ilo tools --json`: its legacy
shape was a bare JSON array, so wrapping it in an envelope changes the
top-level type. Indexing consumers should read `.tools[0]` instead of
`[0]`. The bare-array shape pre-dated the schema-version convention
and was the last hold-out; bringing it into line means every CLI
`--json` envelope now answers the same routing question the same way.

Outputs added in 0.13 (`version`, `compile` / `build`, `explain`,
`skill list`/`get`/`path`/`show`) have always carried `schemaVersion`.

## Audit table

| Command                     | `--json` support     | Schema versioned?                | Agent equivalent           |
| --------------------------- | -------------------- | -------------------------------- | -------------------------- |
| `ilo run <file> [args]`     | yes (success + err)  | yes (0.12.1+)                    |                            |
| `ilo <file> [args]`         | yes (success + err)  | yes (0.12.1+)                    |                            |
| `ilo check <file>`          | yes (diagnostics)    | per-diagnostic                   |                            |
| `ilo build <file> -o out`   | yes                  | yes                              |                            |
| `ilo compile <file> -o out` | yes                  | yes                              |                            |
| `ilo graph <file>`          | yes (always JSON)    | yes (0.12.1+)                    |                            |
| `ilo --ast <file>`          | yes (always JSON)    | yes (0.12.1+)                    |                            |
| `ilo tools --json ...`      | yes                  | yes (0.12.1+, breaking wrap)     |                            |
| `ilo serv`                  | yes (JSONL stdio)    | yes (0.12.1+, every line)        |                            |
| `ilo explain <code>`        | yes                  | yes                              |                            |
| `ilo skill list`            | yes                  | yes                              |                            |
| `ilo skill get <name>`      | yes                  | yes                              |                            |
| `ilo skill path <name>`     | yes                  | yes                              |                            |
| `ilo skill show <name>`     | yes                  | yes                              |                            |
| `ilo version`               | yes                  | yes                              |                            |
| `ilo spec [lang\|ai]`       | yes (wraps prose)    | yes (0.12.1+)                    | `ilo spec ai` (plain text) |
| `ilo repl`                  | no (interactive)     | n/a                              | `ilo serv` (JSONL stdio)   |

`spec` emits markdown for humans by default and `ai.txt` for LLMs; in
`--json` mode it wraps the prose so the contract matches every other
emitter. `repl` is interactive and stays out of the JSON contract - the
JSONL-over-stdio equivalent for agents is `ilo serv`.

## Schemas

### `ilo run` and bare-file run

Success:

```json
{ "schemaVersion": 1, "ok": <value-as-json> }
```

Failure (`Value::Err` returned from the entry function):

```json
{ "schemaVersion": 1, "error": { "phase": "program", "value": <value-as-json> } }
```

Lex / parse / type errors are emitted as one diagnostic JSON object per
error to `stdout`; see `reference/diagnostics.md`. The `schemaVersion`
field was added in 0.12.1; it is strictly additive and old consumers
that read `ok` / `error` keep working.

### `ilo check`

One diagnostic JSON object per error / warning to **stderr** (NDJSON — one
JSON object per line). Exit code is `0` on a clean check, `1` if any error
fired.

Each object has the shape:

```json
{
  "severity": "error" | "warning",
  "code": "ILO-XXXX",
  "message": "<human-readable summary>",
  "suggestion": "<prose hint>",
  "labels": [{ "start": 0, "end": 3, "line": 1, "col": 1, "primary": true, "message": "" }],
  "notes": ["in function 'f'"],
  "fix_plan": {
    "path": "src/foo.ilo",
    "edits": [
      { "line_range": [1, 1], "before": "xyzz", "after": "x" }
    ]
  }
}
```

`fix_plan` is **optional**; it is present for diagnostics where the fix is a
mechanical text replacement an agent can apply without re-parsing prose.
`path` within `fix_plan` is absent for inline code (`ilo check '<code>'`).

Codes that currently emit a `fix_plan`:

| Code | Condition | Edit |
|------|-----------|------|
| ILO-T004 | undefined variable with `"did you mean 'X'?"` hint | rename span to X |
| ILO-T003 | undefined type with `"did you mean 'X'?"` hint | rename span to X |
| ILO-T032 | bare `fmt`/`fmt2` result discarded | prepend `prnt ` before the call |
| ILO-L002 | underscore identifier | replace with hyphenated form |

### `ilo build` / `ilo compile --json`

Success:

```json
{
  "schemaVersion": 1,
  "ok": true,
  "output": "path/to/binary",
  "entry": "main",
  "bench": false,
  "sizeBytes": 9487416,
  "durationMs": 72
}
```

Failure:

```json
{
  "schemaVersion": 1,
  "ok": false,
  "error": { "phase": "aot-compile", "message": "..." }
}
```

Lex / parse / verify errors still emit per-diagnostic JSON to `stderr`
and the command exits `1` without a top-level envelope.

### `ilo explain <code> --json`

Found:

```json
{
  "schemaVersion": 1,
  "code": "ILO-T001",
  "short": "duplicate type definition",
  "long": "## ILO-T001 ..."
}
```

Unknown code:

```json
{
  "schemaVersion": 1,
  "error": {
    "code": "unknown-error-code",
    "message": "unknown error code: ILO-XXX",
    "input": "ILO-XXX"
  }
}
```

### `ilo skill list --json`

```json
{
  "schemaVersion": 1,
  "skills": [
    { "name": "ilo-language", "description": "...", "path": "skills/ilo/ilo-language.md" },
    ...
  ]
}
```

### `ilo skill get <name> --json` and `ilo skill show <name> --json`

```json
{
  "schemaVersion": 1,
  "name": "ilo-language",
  "description": "...",
  "path": "skills/ilo/ilo-language.md",
  "content": "..."
}
```

`get` and `show` produce the same JSON envelope: the human-mode prose
header in `show` makes no sense as JSON.

### `ilo skill path <name> --json`

```json
{
  "schemaVersion": 1,
  "name": "ilo-language",
  "path": "skills/ilo/ilo-language.md"
}
```

### Unknown-skill error (any skill subcommand, `--json`)

```json
{
  "schemaVersion": 1,
  "error": {
    "code": "unknown-skill",
    "message": "unknown skill 'foo'",
    "name": "foo"
  }
}
```

Exits `1`.

### `ilo version --json`

```json
{
  "schemaVersion": 1,
  "name": "ilo",
  "version": "0.11.8",
  "features": ["cranelift"]
}
```

`features` lists the compiled-in optional features (currently
`cranelift`, `llvm`, `tools`). Agents can route on `features` to detect
whether a JIT or MCP-tools capability is available without probing with
a sample program.

### `ilo --ast <file>` (always JSON)

Emits the parsed `ast::Program` as pretty JSON with `"schemaVersion": 1`
flattened in next to `declarations`:

```json
{
  "schemaVersion": 1,
  "declarations": [ ... ]
}
```

The `declarations` shape is the `serde::Serialize` projection of the
AST and is considered internal: agents should treat it as opaque and
stable within a minor version, but not across major version bumps.

### `ilo graph <file>` (always JSON unless `--dot`)

Emits one of `graph::ProgramGraph`, `graph::FnQuery`,
`graph::ReverseQuery`, `graph::BudgetQuery`, or `graph::SubgraphQuery`
depending on flags, each with `"schemaVersion": 1` flattened in at the
top level. See `crate::graph` for the per-query projection.

### `ilo serv`

JSON-Lines stdio: one request object per line on `stdin`, one response
object per line on `stdout`. Every response carries
`"schemaVersion": 1` at the top level - including the initial
`{"schemaVersion": 1, "ready": true}` handshake, every `{"ok": ..., "ms": ...}`
success, and every `{"error": {"phase": ..., ...}}` failure across the
`request`, `lex`, `parse`, `verify`, `runtime`, and `program` phases.
The phase schema and the request shape are documented separately on the
agent-loop page.

### `ilo tools --json`

```json
{
  "schemaVersion": 1,
  "tools": [
    {
      "name": "tool-name",
      "source": "mcp" | "http",
      "description": "...",
      "params": [{ "name": "x", "type": "n" }],
      "return": "t"
    },
    ...
  ]
}
```

The previous (pre-0.12.1) shape was a bare JSON array. Indexing
consumers should read `.tools[0]` instead of `[0]`.

### `ilo spec --json [lang|ai]`

```json
{
  "schemaVersion": 1,
  "format": "markdown" | "ai-txt",
  "content": "...the prose..."
}
```

`format` discriminates between the human-facing markdown spec (`lang`)
and the LLM-facing compact `ai.txt` (`ai`). `content` is opaque text:
agents should not assume a structured shape inside it. Without `--json`
the command still emits the raw text bodies as before, so existing
shell pipelines that piped the markdown into a reader are unchanged.

## Conventions

1. `--json` (or `-j`) is the canonical flag. The pre-existing `--output
   json` form on bare-arg mode still works as a legacy alias.
2. Plain-text output is unchanged. Adding `--json` is strictly additive.
3. When `--json` is set, prose lines (e.g. `Compiled: out`) move to
   `stderr` so `stdout` is pure JSON.
4. Every JSON envelope carries `"schemaVersion": 1`. Breaking changes to
   a versioned envelope bump the major version.

A CI test (`tests/json_output_contracts.rs`) exercises each command
with `--json` on a known input and asserts the output parses and
contains the documented top-level keys, locking the contracts.
