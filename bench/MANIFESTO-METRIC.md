# Closed-loop metric (intent → green)

The only cost that counts is total tokens from intent to working code:

```
spec + generation + context + error feedback + retries
```

`generation_tokens` is the provider output-token sum. It may include thinking. It is not a density claim, and it must not be headlined as one.

## Schema

[`metric-schema.json`](metric-schema.json) describes one task × language-arm × model cell.

An honest cell carries a numeric `thinking_tokens` or `generated_chars` (emitted source length). Both is the target. A null `thinking_tokens` does not count. `code_tokens` must be null when `thinking_unknown_attempts` > 0, so unknown thinking is not folded into code.

`scripts/validate-closed-loop-results.py` enforces that spine with the standard library. It does not call a model and does not read `ANTHROPIC_API_KEY`.

## Harness status

`scripts/closed-loop-bench.py` on main is still the **pre-repair** harness. This change does not alter it. A run still logs undivided `generation_tokens` only: no thinking split, no emitted-code characters, no context-arm label. Those columns land in later PRs (context modes, then honest columns, then the python and bash arms).

Until that repair lands, do not publish density or intent→green totals from this harness. A new `bench/closed-loop-*.json` file is rejected by the validator on purpose.

## Historical results

The local 2026-08-03 run is not in this repository (it was never committed). [`historical/`](historical/) quarantines a shape fixture with the same undivided cells: five cells, required base fields, `generation_tokens`, and neither honest column. Every number is zero and `measurement` is false. It is not that run. The fixture must fail validation.

New results live at the top of `bench/` (`bench/closed-loop-*.json`). That glob does not include `historical/` or `fixtures/`, so quarantined files do not weaken the gate. `--allow-historical-fail` only reports the undivided-column gap and still fails every other defect; the default check does not pass that flag.

## Check

```bash
make validate-closed-loop
# or
bash scripts/check-closed-loop-schema.sh
```

The honest fixture must pass. The quarantined undivided fixture must fail. No API key.
