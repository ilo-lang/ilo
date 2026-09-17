# Decision gates (G5.2)

Typed ilo functions that an agent harness calls instead of free-form LLM
judgement. The pattern: **same input, same output, every path
verifier-gated** — the calling agent branches on the output without
defensive code, and the guarantee is enforced by ilo's compiler, not by a
vendor's calibration claim.

Living example: [`mcp-tools/gates.ilo`](../mcp-tools/gates.ilo) — exposed as
MCP tools `gates:route-priority`, `gates:within-budget`, `gates:severity`
by `scripts/ilo-mcp-server.py`.

## The pattern

```ilo
-- route-priority: route a support ticket by priority keywords in the
-- subject. Deterministic: same input, same route, every time.
route-priority subject:t>t;urgent=has (lwr subject) "urgent";high=has (lwr subject) "asap";fallback=?high "priority-normal" "priority-low";?urgent "priority-high" fallback

-- within-budget: guard a spend request against a limit. Yes/no, no third state.
within-budget requested:n limit:n>b;<=requested limit

-- severity: score a value into discrete bands (boundaries verified)
severity value:n>t;<value 10 "low";<value 100 "medium";"high"
```

## Why ilo for this (vs a hosted decision-model API)

| | ilo gate | hosted decision model |
|---|---|---|
| Off-schema output | impossible (verifier + closed world) | impossible (constrained architecture) |
| Wrong value possible? | yes — but auditable in source | yes — confidence is vendor-calibrated, unverifiable |
| Self-hosted | yes (MIT) | no |
| Latency | sub-ms (native) / ~10ms (VM) | 70–500 ms + network |
| Cost per call | ~0 (part of an existing ilo run) | per-request API pricing |
| Change control | the source file, diffable | vendor's model version |

Where a hosted model wins: open-ended classification over unbounded option
sets, fuzzy judgement without enumerable rules, calibrated confidence
scores. Use ilo gates for decisions you can enumerate; use the model for
the rest — the MCP boundary is the same either way.

## Authoring rules

1. Pure function, typed params, explicit return type — the verifier does
   the rest.
2. Bind-first: `r=<complex expr>;` then use `r`. Operators don't take calls
   as operands.
3. Bare-bool ternary: `?urgent a b` (drop the `h` when the condition is a
   bool ref — the verifier hints this).
4. Guard clauses for early exits: `<=requested 0 "nothing to do";`.
5. `default-on-err (risky call) fallback` instead of Result-match
   boilerplate when the error payload is discardable.

## Verification

- `ilo check mcp-tools/gates.ilo` — type + capability gate before anything
  runs.
- Behaviour: `ilo mcp-tools/gates.ilo route-priority "urgent: refund"` →
  `priority-high`.
- Wire: `scripts/test-mcp-server.py` asserts gate calls end-to-end
  (priority-normal for ASAP-only subjects, `false` on over-budget).
