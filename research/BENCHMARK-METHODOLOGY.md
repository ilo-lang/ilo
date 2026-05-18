# Benchmark Methodology — Closed-Loop Agent Benchmark

This document describes how to reproduce the results in
`research/closed-loop-bench/results/aggregated.json` and the four canonical
charts in `research/closed-loop-bench/charts/`.

## What we measure

For each `(variant, task, session_length, model, seed)` tuple, the driver
records:

| Field | Meaning |
| --- | --- |
| `total_tokens` | Sum of input + output + cache-read + cache-write tokens for the full session, normalised per task |
| `spec_load_tokens` | Tokens consumed by the system prompt (full price on the first task; ~10% on subsequent tasks via prompt caching) |
| `first_try_generation_tokens` | Output tokens for the model's first attempt |
| `retry_count` | Compile or test failures within the retry budget (cap = 5) |
| `retry_tokens` | Output tokens consumed in repair attempts |
| `time_seconds` | Wall-clock time per task |
| `success` | Whether the task passed within the retry budget |
| `usd_cost` | Estimated cost using `PRICING` in driver.py |
| `cache_hit_rate` | Anthropic cache reads / total input tokens |

Aggregation: median across `--seeds` runs (default 3), with population standard
deviation reported in `variance`.

## Variants under test

| Variant | Source language | Spec loaded |
| --- | --- | --- |
| `python-baseline` | Python 3 | none |
| `ilo-pre-phase-1` | ilo | full monolithic `ai.txt` (~16 KB) |
| `ilo-post-phase-1` | ilo | first ~30% of `ai.txt` (simulates modular skill loading) |
| `zero` | Zero v0.1.2 | curated 4.3 KB system prompt |

The `ilo-post-phase-4` variant from the design spec is deferred until Phase 4
ships its typed fix plans.

## Session lengths

N in `{1, 5, 20, 50, 100}`. The system prompt is loaded once per session; the
N tasks inside the session pay only the cached-read rate (~10% of the
input rate) for subsequent reads.

## Models

- `claude-haiku-4-5-20251001` (default — used in the mock sample dataset shipped)
- `claude-sonnet-4-5` (in the live matrix only)

## The repair loop

```
session_init():
  build cached_system_prompt = base + variant-specific spec
  initialise Anthropic Messages session with cache_control

for task_index in 1..N:
  task_prompt = task["prompt"]
  source = llm.generate(task_prompt)
  for attempt in 0..5:
    compile_result = variant.compile_check(source)
    if not compile_result.ok:
      source = llm.generate("Your previous output failed to compile: " + compile_result.error + ...)
      retry_count += 1
      continue
    test_result = variant.run_tests(source, task)
    if test_result.ok:
      success = True; break
    source = llm.generate("Your code compiles but tests failed: " + test_result.error + ...)
    retry_count += 1
  if attempt == 5: success = False
  record metrics
```

## Mock mode

For plumbing validation without API spend, the driver supports a `--mock`
flag. The mock LLM returns a deliberately broken version of the reference
impl on first attempt, then the correct impl on retry. This exercises every
code path in the loop. The mock dataset shipped in this PR is from a 300-cell
mock run (4 variants x 5 tasks x 5 lengths x 1 model x 3 seeds). The numbers
are **not publishable** — they're for plumbing validation only.

## Per-variant compile and test adapters

### Python

- Compile check: `ast.parse(source)` — raises `SyntaxError` on bad syntax.
- Test harness: writes the source to a tempfile, imports the requested
  function, invokes it with each `test_cases[i].args`, compares the result
  to `test_cases[i].expected` with a small float tolerance (1e-6).
- Error encoding for failed tests: a result of `(value, error_text)` is
  interpreted as a Result type — if `error_text is not None`, the actual
  output is recorded as `"ERROR"`.

### ilo

- Compile check: `ilo --ast <file>` — ilo emits a JSON error blob on stderr
  or stdout for parse and type errors. Adapter scans for
  `{"severity":"error", ...}` lines.
- Test harness: invokes `ilo <file> <function-name> <args...>`. Function names
  use hyphens in ilo by convention, so the adapter translates the canonical
  `function_name` from the task JSON (underscores) to hyphens before calling.
- Error encoding: ilo prints error Results to stderr with a leading `^`
  (e.g. `^divide by zero`) and exits non-zero. The adapter treats these as
  `"ERROR"`.

### Zero

- Compile check: `zero check <file>` — exits non-zero with a textual error
  on parse or type error.
- Zero requires a `main` function for a complete program. The adapter
  appends `pub fun main() -> Void {}` if the LLM didn't include one, so the
  task function compiles in isolation.
- **Test execution: compile-only.** Zero's runtime invocation requires a
  language-specific test harness that doesn't have a clean cross-language
  equivalent (Span/MutSpan signatures, raises-based error handling). For
  this benchmark the Zero variant is judged on compile success only. This
  is a documented simplification, not a hidden assumption. To remove it,
  add Zero-specific test runners in `variants.ZeroVariant.run_tests`.

## Pricing

Hardcoded in `driver.py::PRICING`. Update when Anthropic prices change.
Current rates (per million tokens, USD):

| Model | input | output | cache_read | cache_write |
| --- | ---: | ---: | ---: | ---: |
| claude-haiku-4-5-20251001 | 0.80 | 4.00 | 0.08 | 1.00 |
| claude-sonnet-4-5 | 3.00 | 15.00 | 0.30 | 3.75 |

## Reproducing

```bash
cd research/closed-loop-bench
export ANTHROPIC_API_KEY=sk-ant-...
./run_benchmark.sh
```

To reproduce the mock dataset shipped in this PR (no API key required):

```bash
./run_benchmark.sh --mock
```

## Known limitations

1. **Zero is compile-only** — see above. Documented and visible in any chart
   that compares Zero success rates.
2. **`ilo-post-phase-1` is simulated** — Phase 1 (modular skills) has shipped
   in the wider repo but the closed-loop driver loads a prefix slice of
   `ai.txt` as a proxy for "the 2 relevant skill modules per task." Once the
   actual `ilo skill get` invocation is wired into the variant prompt
   loader, this variant can be re-run for a more accurate measure.
3. **`ilo-post-phase-4` is not measured** in this PR — gated on Phase 4
   landing.
4. **Mock numbers are not publishable.** They exist to validate the harness
   end-to-end, not to support strategy claims. Run `--full` to produce a
   publishable dataset.
5. **Cache hit rates only meaningful in live mode.** Mock mode reports 0.0 or
   a synthetic 0.85 depending on whether the task is the first in its
   session.

## Version pinning

Reported in `aggregated.json::metadata`:
- `ran_at` — UTC timestamp
- `spec_sizes_chars` — approximate spec sizes per variant
- `retry_cap` — max retries per task (currently 5)

For a publishable run, also record:
- ilo binary version: `ilo --version`
- zero binary version: `zero --version --json`
- python version
- model identifiers
- `git rev-parse HEAD` of the ilo repo
