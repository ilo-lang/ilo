---
name: ilo-examples
description: Use this when looking for a runnable pattern for the kind of task you are doing. Curated index of `examples/*.ilo` grouped by what each one demonstrates.
---

# ilo examples index

Pointers into `examples/`. Every entry is a real program the engine harness runs on every push. Pick the closest match, `rd examples/<name>.ilo` for the working pattern.

## Data shaping

- `flt-basics.ilo` filter a list. `map-ops.ilo` map with builtin ref.
- `inline-lambda.ilo` lambda inline to HOF. `flatmap.ilo` 1-to-many.
- `sort-by-key.ilo` sort with key projection. `cumsum.ilo` running totals.
- `chunks.ilo` window. `03-data-transform.ilo` end-to-end parse/transform/emit.

## JSON

- `json.ilo` round-trip parse and print. `jpar-stream.ilo` line-by-line.
- `wr-json.ilo` write a JSON file. `jpth-jsonpath-diagnostic.ilo` path query.

## HTTP

- `get-many.ilo` parallel GETs. `mget-bang.ilo` `!` on failure.
- `mget-default.ilo` fall back on HTTP failure.

## Filesystem

- `fs-builtins.ilo` `rd` / `wr` / `lsd`. `csv-tsv-writer.ilo` emit CSV/TSV.

## Error handling

- `results.ilo` returning `R t e`. `result-match.ilo` `~v` / `^e` arms.
- `bang-propagation-result.ilo` `!` auto-unwrap in `R`-fn.
- `bangbang-panic-unwrap.ilo` `!!` panic-unwrap. `guards.ilo` early returns.

## Match

- `match.ilo` core shape. `match-block.ilo` multi-stmt arms.
- `match-in-loop.ilo` inside foreach. `match-types.ilo` sum-type tags.

## Lambdas

- `inline-lambda.ilo` direct HOF. `inline-lambda-typevar.ilo` type var.
- `inline-lambda-capture.ilo` capture (tree only). `closure-bind.ilo` bind.

## Workflow

- `01-simple-function.ilo` smallest program. `02-with-dependencies.ilo` multi-fn.
- `04-tool-interaction.ilo` MCP call. `05-workflow.ilo` agent end-to-end.

## Header

`-- run: <fn>` + `-- out: <expected>` comments are parsed by `tests/examples_engines.rs` and run cross-engine. Same shape gives free regression coverage.
