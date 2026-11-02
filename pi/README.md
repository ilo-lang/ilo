# pi-ilo-lang

A Pi extension for the [ilo programming language](https://github.com/ilo-lang/ilo).

Gives the pi coding agent execution tools, focused compiler guidance, and the ilo skill so it can write ilo code and run it without shelling out.

## Install

```bash
pi install npm:pi-ilo-lang
```

The extension calls the `ilo` binary on `PATH`. Install it first with the [project install script](https://github.com/ilo-lang/ilo#install) or via `npm i -g ilo-lang`.

## Tools

### `ilo_check`

Verify an ilo program WITHOUT running it. Pass either inline source (`code`) or a file path (`file`).

```
ilo_check({ code: "f x:n>n;xyzz" })
```

Returns every diagnostic (code, location, message, suggestion) and, where the compiler can prove the fix, a structured `fix_plan` that `ilo_fix` can apply. Prefer this over running a program to find out whether it compiles.

### `ilo_fix`

Apply the compiler's mechanical, type-proven repairs to an ilo program and return the corrected source. Nothing is written to disk — you decide whether to apply it.

```
ilo_fix({ code: "f x:n>n;xyzz" })
```

Options: `max_iters` (default 10), `safe_only` (only apply repairs the verifier's types prove safe). Returns `{fixed_source, applied, remaining_fixable, preview[]}`. A diagnostic without a fix_plan is left in place and reported.

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

Requires a compiler build with `ilo lsp`. The extension's `ilo_check` and `ilo_fix` tools cover the most common verification and repair paths directly, so most agents do not need to register the MCP server at all.

For the full MCP surface (hover, completion, go-to-definition, inspect), register the compiler manually:

```bash
pi mcp add ilo -- /absolute/path/to/ilo lsp
pi mcp list
```

Set `exposure` to `direct` in that server's `mcp.json` entry. Reload Pi after changing configuration. Despite the subcommand name, this transport is MCP, not standard LSP. `ilo serv` remains a custom execution protocol used by `ilo_repl`; do not configure it as MCP.

## Skill

Ships the ilo skill at `pi/skills/ilo/SKILL.md`. Pi loads it automatically so the agent knows the syntax before reaching for the tools. The skill carries *knowledge*; the hooks below carry *invariants* — anything the agent must always/never do lives in code, not prose.

## Hooks

Beyond tools, the extension enforces ilo workflow invariants through Pi events (see the cross-harness comment block in `extensions/ilo.ts` for the Claude Code / Codex equivalents):

- **Validate-on-edit** (`tool_result`): after `edit`/`write`/`bash` results touching `.ilo` files, runs `ilo check` and appends diagnostics to the result the model sees. Clean files stay silent.
- **Run-failure enrichment** (`tool_result` on `ilo_run` errors): attaches verifier diagnostics for file runs, or verifier diagnostics for inline code. Diagnostics are rendered as compact locations, codes, messages, and suggestions; raw stderr remains in tool details.
- **Shell redirect** (`tool_call`): blocks `bash` invocations of the `ilo` binary with a reason pointing at `ilo_run`/`ilo_repl`.
- **Settle gate** (`agent_before_settle`): if any `.ilo` file was edited without a subsequent clean check, forces one more model pass naming the files (at most once per user request, avoiding endless retries).

All hooks are fail-open: timeouts and errors never break a turn.

## Configuration

- `ILO_BIN`: absolute path to the compiler used for execution, guidance, and validation. Otherwise resolve in order: bundled node_modules binary, `target/debug/ilo` (this repo's build), `PATH`. The checkout build wins over `PATH` so an older `ilo` earlier on PATH (which may lack newer subcommands like `ilo fix`) does not silently break the tools.
- `ILO_NO_HOOKS=1`: disable hooks (execution and validation tools keep working).

## Integration test

Build the compiler, install the package's host peer dependencies, then run `node pi/tests/integration.mjs` from the repository root. Tests use a temporary workspace and Pi's SDK tool pipeline (including hooks and MCP), without model API calls or changes to your Pi settings.

## Licence

MIT.
