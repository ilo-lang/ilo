# JSON output across the ilo CLI

Every ilo subcommand that produces machine-readable output supports a
`--json` (or `-j`) flag. JSON outputs are pure JSON on `stdout`; any prose
goes to `stderr`. Plain-text output is the default and is unchanged.

Where it makes sense, new JSON envelopes start with `"schemaVersion": 1`
so agents can route on the contract and the schema can evolve without
breaking older consumers. Two long-standing outputs predate this
convention and keep their original shape:

- `ilo run` (success / error envelopes for program return values)
- `ilo graph` (always JSON)
- `ilo --ast` (always JSON, the AST)
- `ilo serv` (one JSON object per line, request/response)
- `ilo tools --json`

These are documented below as-is. New outputs added in 0.13 (`version`,
`compile` / `build`, `explain`, `skill list`/`get`/`path`/`show`) all
carry `schemaVersion`.

## Audit table

| Command                     | `--json` support     | Schema versioned? |
| --------------------------- | -------------------- | ----------------- |
| `ilo run <file> [args]`     | yes (success + err)  | no (legacy shape) |
| `ilo <file> [args]`         | yes (success + err)  | no (legacy shape) |
| `ilo check <file>`          | yes (diagnostics)    | per-diagnostic    |
| `ilo build <file> -o out`   | yes                  | yes               |
| `ilo compile <file> -o out` | yes                  | yes               |
| `ilo graph <file>`          | yes (always JSON)    | no (legacy shape) |
| `ilo --ast <file>`          | yes (always JSON)    | no (legacy shape) |
| `ilo tools --json ...`      | yes                  | no (legacy shape) |
| `ilo serv`                  | yes (JSONL stdio)    | no (legacy shape) |
| `ilo explain <code>`        | yes                  | yes               |
| `ilo skill list`            | yes                  | yes               |
| `ilo skill get <name>`      | yes                  | yes               |
| `ilo skill path <name>`     | yes                  | yes               |
| `ilo skill show <name>`     | yes                  | yes               |
| `ilo version`               | yes                  | yes               |
| `ilo spec [lang\|ai]`       | no (markdown / text) | n/a               |
| `ilo repl`                  | no (interactive)     | n/a               |

`spec` and `repl` are intentionally not JSON: `spec` emits markdown for
humans and `ai.txt` for LLMs, and `repl` is interactive.

## Schemas

### `ilo run` and bare-file run (legacy shape)

Success:

```json
{ "ok": <value-as-json> }
```

Failure (`Value::Err` returned from the entry function):

```json
{ "error": { "phase": "program", "value": <value-as-json> } }
```

Lex / parse / type errors are emitted as one diagnostic JSON object per
error to `stdout`; see `reference/diagnostics.md`.

### `ilo check`

One diagnostic JSON object per error / warning to `stdout`. Exit code is
`0` on a clean check, `1` if any error fired. See
`reference/diagnostics.md` for the diagnostic schema.

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

Emits the parsed `ast::Program` as pretty JSON. Schema is the
`serde::Serialize` projection of the AST and is considered internal:
agents should treat it as opaque and stable within a minor version, but
not across major version bumps.

### `ilo graph <file>` (always JSON unless `--dot`)

Emits one of `graph::ProgramGraph`, `graph::FnQuery`,
`graph::ReverseQuery`, or `graph::BudgetQuery` depending on flags. See
`crate::graph` for the projection.

### `ilo serv`

JSON-Lines stdio: one request object per line on `stdin`, one response
object per line on `stdout`. The schema is documented separately on the
agent-loop page.

### `ilo tools --json`

```json
[
  {
    "name": "tool-name",
    "source": "mcp" | "http",
    "description": "...",
    "params": [{ "name": "x", "type": "n" }],
    "return": "t"
  },
  ...
]
```

## Conventions

1. `--json` (or `-j`) is the canonical flag. The pre-existing `--output
   json` form on bare-arg mode still works as a legacy alias.
2. Plain-text output is unchanged. Adding `--json` is strictly additive.
3. When `--json` is set, prose lines (e.g. `Compiled: out`) move to
   `stderr` so `stdout` is pure JSON.
4. New JSON envelopes start with `"schemaVersion": 1`. Breaking changes
   to a versioned envelope bump the major version.

A CI test (`tests/json_output_contracts.rs`) exercises each command
with `--json` on a known input and asserts the output parses and
contains the documented top-level keys, locking the contracts.
