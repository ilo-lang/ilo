# Response to the agentlanguages.dev critique of ilo

2026-09-16 · responds to <https://agentlanguages.dev/languages/ilo.md>

The catalogue's entry is accurate. Each of its four findings, and what we
are doing about it:

## 1. "The spec has grown against the thesis"

Correct, and we said it first (`spec-budgets-for-agent-languages`, July 2026):
SPEC.md is ~51k tokens against Mog's load-bearing 3,200, and a model asked to
inline it skims, which produces plausible-but-wrong programs — the failure
ilo exists to prevent.

Response: PLAN.md G2. A bounded core spec (≤4k tokens) plus path-addressed
cluster references loaded on use, validated against the existing spec-only
generation-accuracy runs, with a CI eviction budget so growth requires a
linked persona-transcript artifact. The 36k difference ships or dies by
measurement, not argument.

## 2. "One of thirteen personas; twelve exhausted the ceiling"

Correct — that is the committed August baseline (Haiku 4.5, 3-attempt cap),
and we published it ourselves. In cost-per-successful-task terms it makes ilo
the most expensive language in the catalogue on that workload. We publish it
precisely because it is the number that hurts.

Response: PLAN.md G3. The retry term is the dominant cold cost, and ilo's
closed grammar can be emitted as per-state logit masks (constrained
decoding), making invalid ilo unemittable instead of retryable. The persona
suite is being re-run under masks; the delta publishes either way.

## 3. "Has not yet published the closed-loop benchmark"

Correct at the time of writing. The harness existed (ILO-364) but no run had
been published — a fair catch, and the field-wide problem is bigger than ilo:
every memory framework, token-compression vendor, and agent language grades
its own homework.

Response: PLAN.md G1, shipped 2026-09-16 as code: `closed-loop-bench.py` now
reports tokens per attempt, attempts to success, provider-cache hit/miss, and
$/successful-task, across cold (cache-busted) and warm arms, with Python as
the default baseline and any second CLI pluggable. First results are in
`bench/`, caveats in `bench/HONEST-NUMBERS.md`. The harness is offered as a
neutral instrument: run your language against it and publish.

## 4. "Adoption is minimal"

Correct: single-digit stars at the time of cataloguing, a few hundred
crates.io downloads, two issues ever. We add the harder diagnosis: ilo's
distribution problem is not installability — it is that the project led with
syntax density (its weakest, self-refuted lever: the naming convention moves
tokens by zero) instead of with measurement and the verified-compute uses
where density is load-bearing.

Response: PLAN.md G5–G6. Repositioning toward MCP tool implementations,
typed decision gates, and executable skill payloads; resume the persona→bug
pipeline that the catalogue itself called ilo's clearest contribution, on a
published cadence.

## What we are not doing

- Arguing the spec is fine. It isn't; it is being cut by measurement.
- Hiding rows. `bench/HONEST-NUMBERS.md` keeps unsupported claims in a
  retracted table, including our own.
- Treating the catalogue as hostile. The entry cites our own strategy docs
  and baselines more faithfully than our marketing did. That is the standard
  now.

— ilo-lang, per `PLAN.md`
