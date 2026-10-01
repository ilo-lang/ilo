# Quarantine — undivided closed-loop cells

`closed-loop-2026-08-03.json` is a **shape fixture**, not the 2026-08-03 run.

That run is not in git history. The fixture copies the cell shape the pre-repair harness still writes, which is the shape that failed the honest schema (5/5 cells with `generation_tokens` and neither `thinking_tokens` nor `generated_chars`). Every number is zero. `measurement` is false. Do not cite it.

It sits here so `bench/closed-loop-*.json` — the path new harness output uses — must pass [`../metric-schema.json`](../metric-schema.json). The main harness is still pre-repair; see [`../MANIFESTO-METRIC.md`](../MANIFESTO-METRIC.md).
