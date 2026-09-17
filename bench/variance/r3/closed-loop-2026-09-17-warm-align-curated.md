# Closed-loop benchmark: ilo vs Zero per-task economics

Generated: 2026-09-17 08:43 UTC  
Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  
Retry cap: 5

## Summary

This table shows per-task economics across languages and models.
Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.

| task | ilo/dsflash: gen | inp | att | time | outcome | python/dsflash: gen | inp | att | time | outcome |
|---|---|---|---|---|---|---|---|---|---|---|
| simple-function | 1440 | 5067 | 1 | 7.57s | working | 100 | 175 | 1 | 1.31s | working |
| with-dependencies | 11294 | 22033 | 4 | 58.56s | working | 242 | 180 | 1 | 2.03s | working |
| data-transform | 1051 | 10337 | 2 | 6.84s | working | 181 | 186 | 1 | 1.71s | working |
| tool-interaction | 911 | 10325 | 2 | 6.55s | working | 441 | 194 | 1 | 2.64s | working |
| workflow-rollback | 2201 | 15807 | 3 | 13.23s | working | 197 | 199 | 1 | 1.93s | working |

## Per-task details

### simple-function

**ilo / dsflash**  
- Generation tokens: 1440  
- Input tokens (context): 5067  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 7.57s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 100  
- Input tokens (context): 175  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.31s  
- Outcome: **working**  

### with-dependencies

**ilo / dsflash**  
- Generation tokens: 11294  
- Input tokens (context): 22033  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [1292, 77, 8616]  
- Wall time: 58.56s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 242  
- Input tokens (context): 180  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.03s  
- Outcome: **working**  

### data-transform

**ilo / dsflash**  
- Generation tokens: 1051  
- Input tokens (context): 10337  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [159]  
- Wall time: 6.84s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 181  
- Input tokens (context): 186  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.71s  
- Outcome: **working**  

### tool-interaction

**ilo / dsflash**  
- Generation tokens: 911  
- Input tokens (context): 10325  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [139]  
- Wall time: 6.55s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 441  
- Input tokens (context): 194  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.64s  
- Outcome: **working**  

### workflow-rollback

**ilo / dsflash**  
- Generation tokens: 2201  
- Input tokens (context): 15807  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [427, 787]  
- Wall time: 13.23s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 197  
- Input tokens (context): 199  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.93s  
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
