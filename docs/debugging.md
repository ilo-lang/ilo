# Debugging ilo programs

## `ilo trace` — JSON-line value snapshots

`ilo trace <file.ilo> [func] [args...]` runs a program through the
tree-walking interpreter and emits one JSON line to stdout after every
statement executes.  This gives agents (and humans) a machine-readable
execution trace they can pipe into `jq`, store in a file, or compare
across runs.

```
ilo trace examples/trace-demo.ilo add 3 4
```

Output (one JSON object per line, pretty-printed here for readability):

```json
{"schemaVersion":1,"line":2,"stmt":"a = + x y","bindings":{"a":7,"x":3,"y":4},"result":7}
{"schemaVersion":1,"line":3,"stmt":"b = * a 2","bindings":{"a":7,"b":14,"x":3,"y":4},"result":14}
{"schemaVersion":1,"line":4,"stmt":"b","bindings":{"a":7,"b":14,"x":3,"y":4},"result":14}
```

### Event schema (schemaVersion 1)

| Field           | Type    | Description |
|-----------------|---------|-------------|
| `schemaVersion` | integer | Always `1`. Increment if the schema changes incompatibly. |
| `line`          | integer | 1-based source line number of the statement. |
| `stmt`          | string  | Source text of that line (trimmed). |
| `bindings`      | object  | All variable bindings visible in scope after the statement. |
| `result`        | any     | Value produced by the statement (`null` for statements with no value). For `let` assignments this is the assigned value; for expression statements it is the expression value. |

### Consuming traces with jq

```sh
# Show only lines where a binding changes to a number > 10
ilo trace prog.ilo | jq 'select(.result > 10)'

# Extract all result values in order
ilo trace prog.ilo | jq -r '.result'

# Show what bindings looked like at line 5
ilo trace prog.ilo | jq 'select(.line == 5) | .bindings'
```

### Notes

- `ilo trace` always uses the **tree-walking interpreter**.  The VM and
  JIT engines do not yet support tracing.
- Traces can be large for programs with loops.  Pipe through `head` or
  use `jq 'first(…)'` to limit output while iterating.
- Functions called by the entry function are also traced.  Each
  statement in every called function body emits its own event.
- The `bindings` object reflects the innermost scope at the point of
  the statement.  If the same name appears in multiple nested scopes,
  the innermost binding is shown.
- `FnRef` and `Closure` values cannot be serialised to JSON; they
  appear as `null` in `bindings` and `result`.

### v1 limitations / follow-ups

- Expression-level granularity (sub-statement snapshots) is not yet
  supported.
- Watchpoints and conditional breakpoints are not yet supported.
- DAP (Debug Adapter Protocol) integration is not yet supported.
- The VM / JIT trace path is not yet implemented.
