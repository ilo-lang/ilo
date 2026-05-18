# Closed-Loop Agent Benchmark

End-to-end measurement of token cost, time, and success rate for an LLM-driven
agent that must compile + test its output. Compares `python`, `ilo`
(pre- and post-Phase-1 modular skills), and Vercel's `zero` across realistic
session lengths.

Phase 2 of the ilo strategy programme. The keystone empirical artefact.

## What's in here

```
closed-loop-bench/
  driver.py              Main loop: generation -> compile -> repair -> retry
  variants.py            Per-language compile + test adapters
  charts.py              Renders the 4 canonical charts from aggregated.json
  run_benchmark.sh       Reproducible execution script
  tasks/                 5 task JSON files (prompt + test cases + reference impls)
  prompts/               Base + per-language system prompts
  results/
    aggregated.json      Median + variance per (variant, task, N, model)
    raw/                 One JSON per (variant, task, N, model, seed) cell
  data/
    aggregated.csv       Flat CSV mirror of aggregated.json
  charts/
    chart1-tokens-vs-n.{png,svg}    Total tokens per task vs session length
    chart2-cost-composition.{png,svg}  Cost composition at N=20
    chart3-success-rate.{png,svg}   Success rate by variant
    chart4-usd-cost.{png,svg}       USD per completed task
    summary.txt          Text fallback
```

## Quick start

```bash
# Validate the loop end-to-end without an API key (mock LLM, ~3 minutes)
./run_benchmark.sh --mock

# Full live matrix (requires ANTHROPIC_API_KEY, budget ~$100-150)
export ANTHROPIC_API_KEY=sk-ant-...
./run_benchmark.sh
```

## What's actually shipped in this PR

The harness, all 5 task definitions, the variants module, the chart generator,
and a **300-cell mock dataset** that exercises the full loop. The numbers in
the mock dataset are NOT publishable — they exist solely to validate plumbing.

The live matrix (5 variants x 5 tasks x 5 session lengths x 2 models x 3 seeds
= 750 cells, ~$100-150 in Anthropic spend) is gated on access to an
`ANTHROPIC_API_KEY` and was not run in this PR's timebox. The run script is
ready; the dataset will be filled in by running `./run_benchmark.sh` against
the live API.

This is intentional — the brief explicitly accepts "harness + smaller sample,
document the gap" if the full 5x5x3 run isn't feasible in the timebox.

## Mock mode

In mock mode the driver substitutes the real Anthropic client for a
deterministic stub that returns:
1. A broken version of the reference impl (first try) - exercises the compile
   error path and the repair prompt
2. The correct reference impl (retry) - exercises the success path

This validates every code path in the closed loop:
- task JSON parsing
- system prompt loading
- per-variant compile invocation (real `ilo` and `zero` binaries are used)
- error parsing back to the LLM
- test harness execution
- token + time + cache-hit accounting
- aggregation + CSV + chart generation

What mock mode does *not* validate:
- real Anthropic prompt caching behaviour
- real model variance across seeds
- the relative difficulty different models have generating each language

## Live mode

Set `ANTHROPIC_API_KEY` and run `./run_benchmark.sh` (or
`python3 driver.py --full`). The driver:

- creates a session per (variant, task, N, model, seed) tuple
- caches the system prompt via Anthropic's `cache_control` mechanism
- runs N tasks back-to-back inside the cached session
- repairs up to 5 times on compile or test failure
- records `input_tokens`, `output_tokens`, `cache_read_input_tokens`,
  `cache_creation_input_tokens` from the API response
- estimates USD cost using `PRICING` in driver.py - update when Anthropic
  prices change

Budget cap: stop runs when cumulative spend exceeds `--budget` (default $300).

## Extending

To add a new language, implement an adapter in `variants.py` with
`compile_check` and `run_tests` methods, register it in `VARIANTS`, and add a
`reference_impls.<lang>` entry to each task JSON.

To add a new task, drop a JSON file in `tasks/` matching the existing schema:
- `id`, `name`, `description`, `prompt`
- `function_name` (the function the test harness will call)
- `test_cases`: list of `{args, expected}`
- `reference_impls`: `{python, ilo, zero}` source strings

## Reproducing the published numbers

See `BENCHMARK-METHODOLOGY.md` in the repo root (`research/`) for full
methodology, version pinning, and known limitations.
