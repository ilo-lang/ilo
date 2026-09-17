# Closed-loop benchmark: ilo vs Zero per-task economics

Generated: 2026-09-17 08:02 UTC  
Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  
Retry cap: 5

## Summary

This table shows per-task economics across languages and models.
Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.

| task | ilo/dsflash: gen | inp | att | time | outcome | python/dsflash: gen | inp | att | time | outcome |
|---|---|---|---|---|---|---|---|---|---|---|
| simple-function | 1362 | 7677 | 2 | 7.39s | working | 143 | 190 | 1 | 1.34s | working |
| with-dependencies | 454 | 3695 | 1 | 3.14s | working | 495 | 196 | 1 | 2.9s | working |
| data-transform | 95 | 3702 | 1 | 1.23s | working | 129 | 199 | 1 | 1.35s | working |
| tool-interaction | 442 | 7578 | 2 | 3.57s | working | 198 | 209 | 1 | 1.71s | working |
| workflow-rollback | 1589 | 3715 | 1 | 6.89s | working | 233 | 213 | 1 | 1.81s | working |

## Per-task details

### simple-function

**ilo / dsflash**  
- Generation tokens: 1362  
- Input tokens (context): 7677  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [191]  
- Wall time: 7.39s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 143  
- Input tokens (context): 190  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.34s  
- Outcome: **working**  

### with-dependencies

**ilo / dsflash**  
- Generation tokens: 454  
- Input tokens (context): 3695  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.14s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 495  
- Input tokens (context): 196  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.9s  
- Outcome: **working**  

### data-transform

**ilo / dsflash**  
- Generation tokens: 95  
- Input tokens (context): 3702  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.23s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 129  
- Input tokens (context): 199  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.35s  
- Outcome: **working**  

### tool-interaction

**ilo / dsflash**  
- Generation tokens: 442  
- Input tokens (context): 7578  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [109]  
- Wall time: 3.57s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 198  
- Input tokens (context): 209  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.71s  
- Outcome: **working**  

### workflow-rollback

**ilo / dsflash**  
- Generation tokens: 1589  
- Input tokens (context): 3715  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 6.89s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 233  
- Input tokens (context): 213  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.81s  
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
