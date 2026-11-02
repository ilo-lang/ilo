# MCP Implementation Status

## Overview
Implemented MCP (Model Context Protocol) adapter for the ilo compiler under the `ilo lsp` subcommand. This adapter provides language intelligence tools to AI agents via standard MCP.

## Architecture
```
ilo (binary)
├── core language service (src/lang_svc/)
│   ├── check, diagnostic_json, inferred_effects, inspect
│   ├── fix (repair loop)
│   └── builtins, imports, semantic
└── MCP adapter (src/cli/lsp.rs) → `ilo lsp`
```

## Implementation Details

### MCP Adapter (`src/cli/lsp.rs`)
- **Protocol:** MCP (Model Context Protocol) over stdio
- **Serialization:** Newline-delimited JSON-RPC
- **Tools:**
  - `ilo_validate` - Validate source, return diagnostics
  - `ilo_fix` - Apply mechanical repairs (max 10 iterations, preview support)
  - `ilo_inspect` - Semantic analysis (overview, symbol, callers, dependencies)

### Design Decisions

#### Single `ilo lsp` Command
Despite the name `lsp`, this implements MCP, not the standard LSP protocol. This was chosen to:
- Maintain compatibility with existing documentation references
- Provide a single entry point for language intelligence
- Allow future expansion to standard LSP if needed

#### Tool Routing
Tools are routed by name in the `call` function, making it easy to add new tools.

#### Helper Functions
Extraction helpers (`extract_source`, `extract_path`, etc.) reduce boilerplate and provide consistent error messages.

## Test Coverage

- Tool definitions validation
- Required parameter validation (source, operation)
- Protocol-level tests (initialize envelope, unknown method error)
- 8 unit tests total

## Files Modified

- `src/cli/lsp.rs` - MCP adapter implementation (new)
- `src/cli/mod.rs` - Added `pub mod lsp;`
- `src/cli/args.rs` - Added `Cmd::Lsp(LspArgs)` variant
- `src/main.rs` - Added dispatch for `Cmd::Lsp`
- `README.md` - Documentation for `ilo lsp`
- `pi/README.md` - Documentation for MCP integration

## Integration

### Pi Integration
Add to `~/.pi/agent/mcp.json`:
```json
{
  "mcpServers": {
    "ilo": {
      "command": "/absolute/path/to/ilo",
      "args": ["lsp"],
      "exposure": "direct"
    }
  }
}
```

## Future Enhancements

- Add `ilo_hover` tool using `semantic.rs` hover support
- Add `ilo_definition` tool for jump-to-definition
- Add `ilo_references` tool for symbol lookup

## Notes

This implementation is intentionally minimal and focused on the core MCP pattern (tools over stdio). It does not implement full LSP features like hover, completion, or semantic highlighting. Those would require a separate standard LSP adapter if needed.
