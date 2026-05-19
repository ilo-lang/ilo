---
name: ilo-tools
description: Use this when declaring or using MCP tools in ilo programs. Covers the `tool` keyword, HTTP and MCP providers, and runtime tool-call handling.
---

# ilo tools

External tools (HTTP, MCP) called as typed prefix-call functions. Declared at file top; verifier checks call sites against signature.

## Declaration

```
tool name "description" (arg1:t arg2:n) > R _ t timeout:30,retry:2
```

`name` is the call-site binding (fn ident rules). `"description"` is prose passed to the provider for routing. `(args)` parens required. Return usually `R _ t` (opaque JSON) or specific. Suffix `timeout:N` seconds, `retry:N`.

## Call

`r=name! arg1 arg2` like any function. `!` auto-unwraps; dispatcher resolves to a provider at runtime.

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
ilo tools --mcp m.json [--ilo|--json|--full|--graph]
```

`--ilo` emits paste-ready `tool` decls. `--json` is structured. `--graph` is a type-level composition graph.

## Failures

Network/transport errors become `^"..."` Results; propagate with `!` or match. Common: timeout (default 60s), non-2xx (`^"status N"`), MCP server crashed (`^"server gone"`), shape mismatch (`^"type N: ..."`). Retries apply transparently before the error surfaces.
