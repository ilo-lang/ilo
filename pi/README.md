# pi-ilo-lang

A Pi extension for the [ilo programming language](https://github.com/ilo-lang/ilo).

Gives the pi coding agent execution tools, focused compiler guidance, and the ilo skill so it can write ilo code and run it without shelling out.

## Install

```bash
pi install npm:pi-ilo-lang
```

The extension calls the `ilo` binary on `PATH`. Install it first with the [project install script](https://github.com/ilo-lang/ilo#install) or via `npm i -g ilo-lang`.

## Tools

### `ilo_run`

Run a program. Pass either inline source (`code`) or a file path (`file`). Optionally invoke a specific function with `func` and pass `args`.

```
ilo_run({ code: "double n:int >int;*n 2", func: "double", args: ["21"] })
```

Returns stdout, stderr, and the exit code.

### `ilo_repl`

Hold an interactive `ilo serv` session. The session auto-closes after 5 minutes of inactivity.

```
ilo_repl({ command: "start" })
ilo_repl({ command: "send", program: "double n:int >int;*n 2", func: "double", args: ["21"] })
ilo_repl({ command: "stop" })
```

`send` writes one JSON request to the running session and returns the matching response line, per the `ilo serv` (execution) protocol.

### `ilo_help`

Read one compiler-matched topic: `ilo-language`, `ilo-edit-loop`, `ilo-builtins`, or `ilo-errors`. Start with language and edit-loop guidance before writing unfamiliar syntax.

```
ilo_help({ topic: "ilo-language" })
```

## Language intelligence (MCP)

Requires a compiler build with `ilo lsp`. The extension registers that subcommand with Pi, exposing `ilo_validate`, `ilo_inspect`, and `ilo_fix` directly to the agent. Despite the subcommand name, this transport is MCP, not standard LSP. `ilo serv` remains a custom execution protocol used by `ilo_repl`; do not configure it as MCP.

For an explicit global configuration, independent of the extension:

```bash
pi mcp add ilo -- /absolute/path/to/ilo lsp
pi mcp list
```

Set `exposure` to `direct` in that server's `mcp.json` entry. Reload Pi after changing configuration. A file-configured `ilo` server overrides the extension registration.

## Skill

Ships the ilo skill at `pi/skills/ilo/SKILL.md`. Pi loads it automatically so the agent knows the syntax before reaching for the tools. The skill carries *knowledge*; the hooks below carry *invariants* — anything the agent must always/never do lives in code, not prose.

## Hooks

Beyond tools, the extension enforces ilo workflow invariants through Pi events (see the cross-harness comment block in `extensions/ilo.ts` for the Claude Code / Codex equivalents):

- **Validate-on-edit** (`tool_result`): after `edit`/`write`/`bash` results touching `.ilo` files, runs `ilo check` and appends diagnostics to the result the model sees. Clean files stay silent.
- **Run-failure enrichment** (`tool_result` on `ilo_run` errors): attaches verifier diagnostics for file runs, or verifier diagnostics for inline code. Diagnostics are rendered as compact locations, codes, messages, and suggestions; raw stderr remains in tool details.
- **Shell redirect** (`tool_call`): blocks `bash` invocations of the `ilo` binary with a reason pointing at `ilo_run`/`ilo_repl`.
- **Settle gate** (`agent_before_settle`): if any `.ilo` file was edited without a subsequent clean check, forces one more model pass naming the files (at most once per user request, avoiding endless retries).
- **ilo lsp bridge**: auto-registers the `ilo` binary with arguments `["lsp"] as a session MCP server when discoverable, so `ilo_validate`/`ilo_inspect`/`ilo_fix` appear alongside the execution tools.

All hooks are fail-open: timeouts and errors never break a turn.

## Configuration

- `ILO_BIN`: absolute path to the compiler used for execution, guidance, validation, and the MCP bridge. Otherwise resolve the bundled binary, PATH, then the checkout's `target/debug/ilo`.
- `ILO_NO_HOOKS=1`: disable hooks and automatic MCP registration (execution and guidance tools keep working).

## Integration test

Build the compiler, install the package's host peer dependencies, then run `node pi/tests/integration.mjs` from the repository root. Tests use a temporary workspace and Pi's SDK tool pipeline (including hooks and MCP), without model API calls or changes to your Pi settings.

## Licence

MIT.
