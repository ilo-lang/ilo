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

## Repair shape hint (exp-03)

`--repair-shape-hint` is off by default. With the flag, an ilo repair
turn whose stderr contains `ILO-P003` — or the paren-header pair
expected `` `>``, got `` `(` `` — appends a fixed gloss inside
`make_repair_prompt`:

```
SHAPE: function headers are `name params>ret;body` — never `name()`.
Zero-arg entry: `main>_;…` not `main()>_;…`. Example: `tri n:n>n;+n 1` then `main>_;prnt (tri 10)`.
Call sites may use `(…)`; headers must not.
```

The initial prompt stays the baseline text. The flag is a retry cut
(manifesto principle 6: structured compiler→agent surface). It does not
change `generation_tokens` into a density claim. `--dry-run` prints
`repair_shape_hint: on` or `off` and needs no API key. Each result cell
and the JSON header record `repair_shape_hint`.

```bash
python3 scripts/closed-loop-bench.py --provider deepseek --model deepseek-chat \
  --context curated --retry-cap 2 --dry-run
python3 scripts/closed-loop-bench.py --provider deepseek --model deepseek-chat \
  --context curated --retry-cap 2 --repair-shape-hint --dry-run
```

## Header recovery (exp-04)

`--header-recovery` is off by default. With the flag, extracted ilo text is
rewritten once after fence stripping and before `ilo`:

| Rule | Rewrite |
|------|---------|
| R4 | leading `f` / `fn` on a header line (`>`, `(`, or `:`) |
| R3 | `name():…>` → `name>` |
| R1 | `name()>` → `name>` at line start |
| R6 | `name() { … }` → `name>_;` and drop the matching `}` |
| R2 | a whole line `name()` / `name();` → `name>_;` |
| R5 | a whole line `main` → `main>_` |

Call-site `(…)` is left as written. The programme ilo ran is
`attempt_trace[].code`. The trace also keeps `code_raw`,
`code_pre_recovery`, `header_recovery_rules`, a sha256 of the preimage
when a rule fired, and `p003_pre_recovery` (raw emit matched an R1, R2,
R3, or R5 header, including after a leading `f`/`fn`). `code_chars` and
`generated_chars` stay the pre-rewrite lengths. A green first invoke
after the rewrite does not spend another LLM turn. The flag is a retry
cut (manifesto principle 6). It does not turn `generation_tokens` into a
density claim. `--dry-run` prints `header_recovery: on` or `off` and
needs no API key. Each result cell and the JSON header record
`header_recovery`.

```bash
python3 scripts/closed-loop-bench.py --provider deepseek --model deepseek-chat \
  --context curated --retry-cap 2 --repair-shape-hint --dry-run
python3 scripts/closed-loop-bench.py --provider deepseek --model deepseek-chat \
  --context curated --retry-cap 2 --repair-shape-hint --header-recovery --dry-run
```

## Meta stdout fold (exp-05)

`--fold-meta-stdout` is off by default. With the flag, ilo text is folded
once after header recovery (if that flag is also on) and before `ilo`.
It is not a header rule.

| Step | Effect |
|------|--------|
| M1 | a bare number glued under `-- out:` joins that comment when later programme source follows (`-- out: 5` / `0` / `safe-div …` → `-- out: 5\n0`) |
| Judge | stdout lines that are `-- out:`, `-- run:`, or `-- err:` are ignored |

A blank line ends the M1 continuation. A number that is the whole
programme is left alone. A trailing `0` inside a function body is a
real expression and stays, so `55` then `0` against expected `55` is
still partial. The programme ilo ran is `attempt_trace[].code`. When
the flag is on, the trace also records `meta_recovery_rules`.
`code_chars` and `generated_chars` stay the pre-fold lengths. A green
first invoke after the fold does not spend another LLM turn. The flag
is a retry cut (manifesto principle 6): do not fail a correct programme
because meta noise sat in the source or on stdout. It does not turn
`generation_tokens` into a density claim. `--dry-run` prints
`fold_meta_stdout: on` or `off` and needs no API key. Each result cell
and the JSON header record `fold_meta_stdout`.

```bash
python3 scripts/closed-loop-bench.py --provider deepseek --model deepseek-chat \
  --context curated --retry-cap 2 --repair-shape-hint --header-recovery --dry-run
python3 scripts/closed-loop-bench.py --provider deepseek --model deepseek-chat \
  --context curated --retry-cap 2 --repair-shape-hint --header-recovery \
  --fold-meta-stdout --dry-run
```

## Constrained decode (exp-11)

`--constrain` defaults to `none`. DeepSeek hosted Chat Completions cannot
attach logit or grammar masks. `--constrain reject-retry` is the feasible
substitute: after fence-stripping, and before header, meta, or soft-edge
recovery, the harness runs `ilo check` on the raw emit. A failing check
does not run the programme. The repair signal is the next user message,
on the same channel as stderr repair, and the reject counts toward
`--retry-cap`. Cells record `constrain_mode`, `constrain_reject_count`,
and `constrain_reject_codes`. Soft-edge recovery stays off unless that
flag is passed. `local-mask` is a placeholder for a future local guided
sampler and is not a DeepSeek mask. This does not add an `ilo constrain`
CLI. `--dry-run` prints the mode and needs no API key.

```bash
python3 scripts/closed-loop-bench.py --dry-run --constrain none
python3 scripts/closed-loop-bench.py --dry-run --constrain reject-retry
```

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
