# Closed-loop benchmark: ilo vs Zero per-task economics

Generated: 2026-08-03 23:01 UTC  
Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  
Retry cap: 5

## Summary

This table shows per-task economics across languages and models.
Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.

| task | ilo/sonnet: gen | inp | att | time | outcome |
|---|---|---|---|---|---|
| simple-function | 281 | 33953 | - | 19.44s | failed |
| with-dependencies | 319 | 26046 | 5 | 19.27s | working |
| data-transform | 90 | 7902 | 1 | 4.12s | working |
| tool-interaction | 259 | 50110 | - | 16.51s | failed |
| workflow-rollback | 165 | 13543 | 2 | 9.21s | working |

## Per-task details

### simple-function

**ilo / sonnet**  
- Generation tokens: 281  
- Input tokens (context): 33953  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [62, 54, 63, 58]  
- Wall time: 19.44s  
- Outcome: **failed**  

### with-dependencies

**ilo / sonnet**  
- Generation tokens: 319  
- Input tokens (context): 26046  
- Attempts total: 5  
- Attempts to success: 5  
- Repair tokens by turn: [67, 66, 64, 63]  
- Wall time: 19.27s  
- Outcome: **working**  

### data-transform

**ilo / sonnet**  
- Generation tokens: 90  
- Input tokens (context): 7902  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 4.12s  
- Outcome: **working**  

### tool-interaction

**ilo / sonnet**  
- Generation tokens: 259  
- Input tokens (context): 50110  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [45, 61, 45, 57]  
- Wall time: 16.51s  
- Outcome: **failed**  

### workflow-rollback

**ilo / sonnet**  
- Generation tokens: 165  
- Input tokens (context): 13543  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [84]  
- Wall time: 9.21s  
- Outcome: **working**  

## Notes

- Zero language CLI was not available in this environment; Zero column deferred.
  To add Zero, run: `python3 scripts/closed-loop-bench.py --lang2-name zero --lang2-bin <path-to-zero> --lang2-ext .zero`
- Skill documentation is loaded once per process (steady-state caching).
- One-shot economics (first attempt only) can be derived from `repair_tokens_by_turn` in the JSON.
- Re-run at any time; output files are date-stamped.

## Deferred

- Zero CLI integration (ILO-364 Phase 5.b): blocked on Zero being installable in CI.
- Pre/post [ILO-360](https://linear.app/ilo-lang/issue/ILO-360) comparison: run baseline now, re-run after typed fix plans land.
- Empirical retry-cap tuning: first run curves are in `repair_tokens_by_turn`; adjust `--retry-cap` once flattening point is visible.
