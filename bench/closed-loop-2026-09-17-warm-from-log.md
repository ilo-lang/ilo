# Closed-loop benchmark: ilo vs Zero per-task economics

Generated: 2026-09-17 07:25 UTC  
Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  
Retry cap: 5

## Summary

This table shows per-task economics across languages and models.
Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.

| task | ilo/dsflash: gen | inp | att | time | outcome | python/dsflash: gen | inp | att | time | outcome |
|---|---|---|---|---|---|---|---|---|---|---|
| simple-function | 1701 | 19364 | 2 | 8.71s | working | 62 | 193 | 1 | 1.39s | working |
| with-dependencies | 558 | 9568 | 1 | 4.03s | working | 163 | 196 | 1 | 1.77s | working |
| data-transform | 961 | 9571 | 1 | 5.83s | working | 279 | 202 | 1 | 2.03s | working |
| tool-interaction | 2396 | 38693 | 4 | 14.31s | working | 303 | 210 | 1 | 1.91s | working |
| workflow-rollback | 2677 | 9585 | 1 | 13.69s | working | 2217 | 716 | 3 | 13.35s | working |

## Per-task details

### simple-function

**ilo / dsflash**  
- Generation tokens: 1701  
- Input tokens (context): 19364  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [999]  
- Wall time: 8.71s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 62  
- Input tokens (context): 193  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.39s  
- Outcome: **working**  

### with-dependencies

**ilo / dsflash**  
- Generation tokens: 558  
- Input tokens (context): 9568  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 4.03s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 163  
- Input tokens (context): 196  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.77s  
- Outcome: **working**  

### data-transform

**ilo / dsflash**  
- Generation tokens: 961  
- Input tokens (context): 9571  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 5.83s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 279  
- Input tokens (context): 202  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.03s  
- Outcome: **working**  

### tool-interaction

**ilo / dsflash**  
- Generation tokens: 2396  
- Input tokens (context): 38693  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [436, 1231, 224]  
- Wall time: 14.31s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 303  
- Input tokens (context): 210  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.91s  
- Outcome: **working**  

### workflow-rollback

**ilo / dsflash**  
- Generation tokens: 2677  
- Input tokens (context): 9585  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 13.69s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 2217  
- Input tokens (context): 716  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [309, 1649]  
- Wall time: 13.35s  
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
