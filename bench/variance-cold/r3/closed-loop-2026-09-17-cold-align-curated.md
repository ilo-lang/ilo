# Closed-loop benchmark: ilo vs Zero per-task economics

Generated: 2026-09-17 11:22 UTC  
Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  
Retry cap: 5

## Summary

This table shows per-task economics across languages and models.
Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.

| task | ilo/dsflash: gen | inp | att | time | outcome | python/dsflash: gen | inp | att | time | outcome |
|---|---|---|---|---|---|---|---|---|---|---|
| simple-function | 278 | 5101 | 1 | 2.09s | working | 72 | 192 | 1 | 1.1s | working |
| with-dependencies | 1960 | 16235 | 3 | 11.3s | working | 182 | 193 | 1 | 1.71s | working |
| data-transform | 231 | 5110 | 1 | 1.76s | working | 1648 | 204 | 1 | 8.4s | working |
| tool-interaction | 350 | 5122 | 1 | 2.26s | working | 310 | 210 | 1 | 1.96s | working |
| workflow-rollback | 8348 | 10388 | 2 | 37.41s | working | 346 | 215 | 1 | 2.17s | working |

## Per-task details

### simple-function

**ilo / dsflash**  
- Generation tokens: 278  
- Input tokens (context): 5101  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.09s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 72  
- Input tokens (context): 192  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.1s  
- Outcome: **working**  

### with-dependencies

**ilo / dsflash**  
- Generation tokens: 1960  
- Input tokens (context): 16235  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [670, 858]  
- Wall time: 11.3s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 182  
- Input tokens (context): 193  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.71s  
- Outcome: **working**  

### data-transform

**ilo / dsflash**  
- Generation tokens: 231  
- Input tokens (context): 5110  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.76s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 1648  
- Input tokens (context): 204  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 8.4s  
- Outcome: **working**  

### tool-interaction

**ilo / dsflash**  
- Generation tokens: 350  
- Input tokens (context): 5122  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.26s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 310  
- Input tokens (context): 210  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.96s  
- Outcome: **working**  

### workflow-rollback

**ilo / dsflash**  
- Generation tokens: 8348  
- Input tokens (context): 10388  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [1134]  
- Wall time: 37.41s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 346  
- Input tokens (context): 215  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.17s  
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
