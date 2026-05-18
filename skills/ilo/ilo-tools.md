---
name: ilo-tools
description: Use this when declaring or using MCP tools in ilo programs. Covers the `tool` keyword, HTTP and MCP providers, and runtime tool-call handling.
---

# ilo tools

ilo programs can call external tools (HTTP endpoints, MCP servers) as typed prefix-call functions. Tools are declared at the top of the file; the verifier checks call sites against the declared signature.

## Declaration

```
tool name "description" (arg1:t arg2:n) > R _ t timeout:30,retry:2
```

- `name` - the binding used at call sites (same ident rules as functions).
- `"description"` - prose passed to the tool provider (MCP server, HTTP host) for routing.
- `(args)` - typed parameter list. Parens are required here (different from fn decls).
- `> return-type` - typically `R _ t` for an opaque JSON payload, or a more specific type.
- Suffix options: `timeout:N` (seconds), `retry:N` (retries on failure).

## Use at call sites

A declared tool calls like any other function:

```
r=name! arg1 arg2
```

Auto-unwrap with `!`. The tool dispatcher resolves the binding to a provider lookup at runtime.

## HTTP tool provider

Pass a JSON config with `--tools path.json`:

```json
{
  "tools": [
    {
      "name": "weather",
      "url": "https://api.example.com/weather",
      "method": "POST",
      "headers": {"Authorization": "Bearer {env:API_KEY}"}
    }
  ]
}
```

Args are serialised as JSON body for POST, query string for GET. The response body is the tool's return value.

## MCP server provider

Pass an MCP config with `--mcp path.json`:

```json
{
  "servers": {
    "myserver": {
      "command": "/usr/local/bin/my-mcp-server",
      "args": ["--config", "x.toml"],
      "env": {"FOO": "bar"}
    }
  }
}
```

Tool names on the MCP server appear as bindings. Run `ilo tools --mcp path.json` to list discovered tools and their inferred ilo signatures.

## Discovery

```
ilo tools --mcp m.json                Human-readable list
ilo tools --mcp m.json --ilo          Emit valid `tool` decls
ilo tools --mcp m.json --json         Structured JSON
ilo tools --tools http.json --full    Full signatures
ilo tools --mcp m.json --graph        Type-level composition graph
```

The `--ilo` output is paste-ready into a `.ilo` file.

## Runtime flow

1. Verifier matches each call site against the declared `tool` signature.
2. Dispatcher routes to the configured provider (HTTP or MCP).
3. Response is parsed into the declared return type.
4. Network/transport errors become `^"..."` Result errors; the program either auto-propagates with `!` or matches.

## Failure handling

```
r=?name! arg
?r{~v:use v;^e:^+"weather: "e}
```

Common failures:
- timeout (default 60s, configurable per-tool)
- non-2xx HTTP status -> `^"status N"`
- MCP server crashed -> `^"server gone"`
- response shape mismatch -> `^"type N: ..."`

Retries are applied transparently before the error surfaces.
