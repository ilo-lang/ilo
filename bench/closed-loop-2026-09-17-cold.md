# Closed-loop benchmark: ilo vs Zero per-task economics

Generated: 2026-09-17 07:29 UTC  
Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  
Retry cap: 5

## Summary

This table shows per-task economics across languages and models.
Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.

| task | ilo/dsflash: gen | inp | att | time | outcome | python/dsflash: gen | inp | att | time | outcome |
|---|---|---|---|---|---|---|---|---|---|---|
| simple-function | 907 | 9560 | 1 | 5.21s | working | 123 | 193 | 1 | 1.46s | working |
| with-dependencies | 505 | 9569 | 1 | 3.28s | working | 53 | 192 | 1 | 0.9s | working |
| data-transform | 535 | 9575 | 1 | 3.29s | working | 213 | 201 | 1 | 1.78s | working |
| tool-interaction | 875 | 19274 | 2 | 5.47s | working | 460 | 210 | 1 | 2.47s | working |
| workflow-rollback | 7053 | 19363 | 2 | 35.07s | working | 1059 | 214 | 1 | 5.81s | working |

## Per-task details

### simple-function

**ilo / dsflash**  
- Generation tokens: 907  
- Input tokens (context): 9560  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 5.21s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 123  
- Input tokens (context): 193  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.46s  
- Outcome: **working**  

### with-dependencies

**ilo / dsflash**  
- Generation tokens: 505  
- Input tokens (context): 9569  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.28s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 53  
- Input tokens (context): 192  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 0.9s  
- Outcome: **working**  

### data-transform

**ilo / dsflash**  
- Generation tokens: 535  
- Input tokens (context): 9575  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.29s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 213  
- Input tokens (context): 201  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.78s  
- Outcome: **working**  

### tool-interaction

**ilo / dsflash**  
- Generation tokens: 875  
- Input tokens (context): 19274  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [465]  
- Wall time: 5.47s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 460  
- Input tokens (context): 210  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.47s  
- Outcome: **working**  

### workflow-rollback

**ilo / dsflash**  
- Generation tokens: 7053  
- Input tokens (context): 19363  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [2467]  
- Wall time: 35.07s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 1059  
- Input tokens (context): 214  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 5.81s  
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
