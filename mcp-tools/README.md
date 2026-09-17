# ilo MCP Tools

Eighteen ilo functions exposed as MCP tools across six families. Each tool
is an ilo function with a typed signature; the input schema and the
output schema are generated from the AST via `ilo --ast` (the output
schema wraps the declared return type in MCP's structured-output
envelope).

## Families

| File | Family | Tools |
|---|---|---|
| `demo.ilo` | Demo | `tri(n)`, `slug(text)`, `stats-summary(numbers_json)` |
| `text.ilo` | Text | `count-matches(pat, s)`, `headline(s)` |
| `numbers.ilo` | Numbers | `percent(part, whole)`, `between(x, lo, hi)` |
| `gates.ilo` | Decision gates | `route-priority(subject)`, `within-budget(requested, limit)`, `severity(value)` |
| `quality.ilo` | Validation gates | `clampinto(x, lo, hi)`, `in-range(x, lo, hi)`, `not-blank(s)`, `length-ok(s, max)` |
| `lists.ilo` | List utilities | `sum-list(xs)`, `max-list(xs)`, `mean-list(xs)`, `take-first(xs, k)` |

## Running

```bash
# All 18 tools via the MCP server (stdio transport)
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
6. Run the wire tests: `python3 scripts/test-mcp-server.py`.

## Result contract (server v0.3)

- `tools/list` entries carry `outputSchema`:
  `{"type": "object", "properties": {"result": <from return type>}, "required": ["result"]}`.
- Successful `tools/call` results carry
  `structuredContent: {"result": <value>}` — JSON parse first, then
  scalar coercion per the declared type (number/boolean/string).
- Failed calls carry `isError: true` plus
  `structuredContent: {"error": {"code": "ILO-…", "message": …}}` using
  ilo's stable diagnostic codes (`ILO-R600` and friends), so clients
  branch on code, not prose.
