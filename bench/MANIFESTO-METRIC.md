# Closed-loop metric

Intent → green is the only cost that counts:

```
spec + generation + context + error feedback + retries
```

A closed-loop cell is one task × language arm × model. The schema is
[`metric-schema.json`](metric-schema.json). The gate is
[`scripts/validate-closed-loop-results.py`](../scripts/validate-closed-loop-results.py)
(stdlib only, no API key).

## What is not a density claim

`generation_tokens` is the provider output-token sum. On a thinking model
that sum includes thinking. Quoting it as emitted-code density, or as an
intent→green win, is not a measurement.

A cell is honest when at least one of these is a non-null integer:

- `thinking_tokens` — provider thinking tokens (a lower bound when
  `thinking_unknown_attempts > 0`)
- `generated_chars` — length of emitted code across attempts

Both is the target. A present-but-null `thinking_tokens` does not count.
If any attempt left thinking unknown, `code_tokens` must be null: unknown
thinking is not code.

## Harness status

`scripts/closed-loop-bench.py` writes an honest cell. `thinking_tokens` is
null when no billed attempt reported a split, and a partial sum is a lower
bound. `generated_chars` is the length of emitted program text.
`code_tokens` is null when any billed attempt left thinking unknown.
`generation_tokens` stays the provider output sum. It is not a density
claim, and the markdown table does not lead with it.

The same cell records the context arm (`--context`, default `curated`)
and the language arm (`lang_arm`). `--python` and `--bash` are the
comparator shorthands. Without `--lang2-docs` the comparator is
`fair_docs=false` and is not a fair bakeoff against ilo skills.
`task_class` (`artefact` | `ops` | `sanity`) marks the row. An ops task,
or a bash win on wall time, is not an ilo manifesto loss.

`--dry-run` and `--emit-fixture` need no API key. The synthetic fixture
is `bench/fixtures/closed-loop-harness-shape.json`.

## Providers

A live run uses DeepSeek when `DEEPSEEK_API_KEY` is set, unless
`--provider anthropic` or an Anthropic model (`haiku`, `sonnet`, `both`)
is requested. DeepSeek is OpenAI-compatible Chat Completions. The origin
is `DEEPSEEK_BASE_URL` (default `https://api.deepseek.com`;
`DEEPSEEK_API_BASE` is the same setting). The harness posts
`{base}/chat/completions`. It does not call
`https://api.deepseek.com/anthropic`.

```bash
python3 scripts/closed-loop-bench.py --dry-run --provider deepseek --model deepseek-chat
python3 scripts/closed-loop-bench.py --provider deepseek --model deepseek-chat --task simple-function
python3 scripts/closed-loop-bench.py --provider deepseek --model deepseek-reasoner --task simple-function
```

With the key set and no `--provider` or `--model`, the live path is
`deepseek-chat` (one model). `--model both --provider deepseek` runs
`deepseek-chat` and `deepseek-reasoner`.

`deepseek-chat` and `deepseek-reasoner` are CLI names. The request sends
`deepseek-flash` with `thinking.type` `disabled` or `enabled`. DeepSeek
discontinued those two model ids on 2026-07-24. `--model deepseek-flash`
and `--model deepseek-v4-pro` are sent as those ids. Each cell records
`provider`, `model` (the CLI key), and `model_id` (the id on the wire).

`generation_tokens` is `usage.completion_tokens`. `thinking_tokens` is
`usage.completion_tokens_details.reasoning_tokens` when that value is a
non-negative integer, including 0. A missing or non-integer split is
null. `reasoning_content` is not program text and is not a token count.
`generated_chars` is the length of `message.content` after fence
stripping. Do not quote `generation_tokens` as density.

New result files belong at `bench/closed-loop-*.json` and must pass the
validator. The CI job `Closed-loop metric gate` runs
[`scripts/check-closed-loop-metrics.sh`](../scripts/check-closed-loop-metrics.sh)
on every pull request and on `main` / `next`. It does not read
`ANTHROPIC_API_KEY` and does not call a model.

## Historical results

`closed-loop-2026-08-03.json` was never committed. An inventory of that
local file recorded 5 of 5 cells with neither honest column. The current
harness does not write that shape. The quarantined fixture keeps the
failure so the gate still rejects it.

That shape is quarantined, not republished as a result:

- [`historical/closed-loop-2026-08-03.undivided-shape.json`](historical/closed-loop-2026-08-03.undivided-shape.json)
  is a hand-built shape fixture. Counts are placeholders (zeros), not the
  unpublished run.
- `bench/historical/` is outside the `bench/closed-loop-*.json` publish
  glob, so new results cannot hide beside it.
- The gate asserts this fixture still fails. `--allow-historical-fail`
  prints that failure and still rejects every non-historical file.

Putting a pre-repair dump at `bench/closed-loop-2026-08-03.json` fails CI
on purpose. Move it under `bench/historical/` or regenerate it from a
harness that emits an honest column.
