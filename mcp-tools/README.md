# ilo MCP Tools

Ten ilo functions exposed as MCP tools across four families. Each tool is
an ilo function with a typed signature; the input schema is generated from
the AST via `ilo --ast`.

## Families

| File | Family | Tools |
|---|---|---|
| `demo.ilo` | Demo | `tri(n)`, `slug(text)`, `stats-summary(numbers_json)` |
| `text.ilo` | Text | `count-matches(pat, s)`, `headline(s)` |
| `numbers.ilo` | Numbers | `percent(part, whole)`, `between(x, lo, hi)` |
| `gates.ilo` | Decision gates | `route-priority(subject)`, `within-budget(requested, limit)`, `severity(value)` |

## Running

```bash
# All 10 tools via the MCP server (stdio transport)
python3 scripts/ilo-mcp-server.py --dir mcp-tools --ilo ./target/release/ilo

# Or via streamable-HTTP
python3 scripts/ilo-mcp-server.py --dir mcp-tools --http --port 8391

# Schema cost report
python3 scripts/ilo-mcp-server.py --dir mcp-tools --stats
```

## Adding a tool

1. Add a `pub fn` with typed parameters to a `.ilo` file in this directory.
2. The function name becomes the MCP tool name (`file_stem:fn_name`).
3. The input schema is generated from the ilo type sigils.
4. Add a `-- description` header comment (becomes the tool description).
5. Run `ilo check` to verify.
