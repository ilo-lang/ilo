# pi-ilo-lang

A Pi extension for the [ilo programming language](https://github.com/ilo-lang/ilo).

Gives the pi coding agent two tools and the ilo skill so it can write ilo code and run it without shelling out.

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

`send` writes one JSON request to the running session and returns the matching response line, per the `ilo serv` protocol.

## Skill

Ships the ilo skill at `skills/ilo/SKILL.md`. Pi loads it automatically so the agent knows the syntax before reaching for the tools.

## Configuration

- `ILO_BIN` env var: absolute path to a specific `ilo` binary. Useful when running a development build.

## Licence

MIT.
