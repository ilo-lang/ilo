# The closed-loop benchmark is open — bring your language

ilo's **total tokens from intent to working code** benchmark is now a
reproducible harness, and it is deliberately not ilo-only.

## What it measures

Per task, per language, per model:

- `generation_tokens` and `input_tokens` per attempt — split by
  provider-cache hit vs miss
- `attempts_to_success` and `success_rate` under a fixed retry cap
- wall-clock time
- **$/successful task** from per-provider price tables

Across two arms: **warm** (stable prefixes; provider prefix caching active)
and **cold** (cache-busting nonce per attempt). The split exists because
cached steady-state and cold-start economics produce different winners, and
most published claims quietly assume one while advertising the other.

## How to add your language

Any CLI that runs `LANG file.ext` and exits non-zero on error works:

```sh
python3 scripts/closed-loop-bench.py \
    --model dsflash \
    --cache warm --cache cold \
    --lang2-name yourlang --lang2-bin /path/to/yourlang --lang2-ext .yl \
    --lang2-docs /path/to/your-lang-spec.md
```

Models: Anthropic (`--model haiku|sonnet`) or any OpenAI-compatible endpoint
(`MODELS` registry in the script; DeepSeek flash is wired in). Pass
`--lang2-docs` so your language's spec rides in its system prompt at parity
with ilo's curated spec — comparing languages without doc parity measures
documentation, not language.

## The ask

1. Run your language against the same `bench/closed-loop/tasks.json` set.
2. Publish your numbers — including the rows where you lose — with the
   harness's JSON output attached.
3. Link it from your README next to your headline claim.

Vendored comparisons and one-way benchmarks got this field contradictory
SOTA claims (LoCoMo scores spanning 64–94 for competing products; token
savings ranging 8.5%–65% for the same tool depending on who measured).
A shared harness with cold/warm arms and published loss rows is the cheapest
fix. ilo ships its own results first, red rows included — see
[`bench/measurements.md`](../bench/measurements.md).

Status, caveats, and known limits of the harness are documented in
`bench/measurements.md`. PRs against `scripts/closed-loop-bench.py` and
`bench/closed-loop/tasks.json` welcome — especially task-set additions from
languages whose strengths the current 5 tasks don't exercise.
