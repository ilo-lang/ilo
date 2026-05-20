---
name: ilo-examples
description: Use this when looking for a runnable pattern for the kind of task you are doing. Curated index of `examples/*.@` grouped by what each one demonstrates.
---

# ilo examples index

Pointers into `examples/`. Every entry runs cross-engine on push. `rd examples/<name>.@` for the working pattern.

## Data shaping

- `flt-basics.@` filter a list. `map-ops.@` map with builtin ref.
- `inline-lambda.@` lambda inline to HOF. `flatmap.@` 1-to-many.
- `sort-by-key.@` sort with key projection. `cumsum.@` running totals.
- `chunks.@` window. `03-data-transform.@` end-to-end parse/transform/emit.

## JSON

- `json.@` round-trip parse and print. `jpar-stream.@` line-by-line.
- `wr-json.@` write a JSON file. `jpth-jsonpath-diagnostic.@` path query.

## HTTP

- `get-many.@` parallel GETs. `mget-bang.@` `!` on failure.
- `mget-default.@` fall back on HTTP failure.

## Filesystem

- `fs-builtins.@` `rd` / `wr` / `lsd`. `csv-tsv-writer.@` emit CSV/TSV.

## Error handling

- `results.@` returning `R t e`. `result-match.@` `~v` / `^e` arms.
- `bang-propagation-result.@` `!` auto-unwrap in `R`-fn.
- `bangbang-panic-unwrap.@` `!!` panic-unwrap. `guards.@` early returns.

## Match

- `match.@` core shape. `match-block.@` multi-stmt arms.
- `match-in-loop.@` inside foreach. `match-types.@` sum-type tags.

## Lambdas

- `inline-lambda.@` direct HOF. `inline-lambda-typevar.@` type var.
- `inline-lambda-capture.@` capture. `closure-bind.@` bind.

## Workflow

- `01-simple-function.@` smallest. `02-with-dependencies.@` multi-fn.
- `04-tool-interaction.@` MCP. `05-workflow.@` end-to-end.

## Header

`-- run: <fn>` + `-- out: <expected>` parsed by `tests/examples_engines.rs`, run cross-engine.
