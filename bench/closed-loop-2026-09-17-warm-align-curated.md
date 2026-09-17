# Closed-loop benchmark: ilo vs Zero per-task economics

Generated: 2026-09-17 22:27 UTC  
Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  
Retry cap: 5

## Summary

This table shows per-task economics across languages and models.
Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.

| task | ilo/dsflash: gen | inp | att | time | outcome | bash/dsflash: gen | inp | att | time | outcome |
|---|---|---|---|---|---|---|---|---|---|---|
| simple-function | 1519 | 11333 | 3 | 8.92s | working | 114 | 175 | 1 | 1.34s | working |
| with-dependencies | 3881 | 20519 | - | 20.62s | partial | 22891 | 2138 | - | 97.24s | failed |
| data-transform | 2864 | 16271 | 4 | 15.6s | working | 252 | 186 | 1 | 1.83s | working |
| tool-interaction | 2474 | 19755 | - | 14.68s | partial | 578 | 194 | 1 | 3.16s | working |
| workflow-rollback | 4888 | 19970 | 5 | 25.24s | working | 351 | 199 | 1 | 2.02s | working |
| hard-recursion | 3795 | 19756 | 5 | 19.86s | working | 693 | 186 | 1 | 4.91s | working |
| hard-records | 6194 | 21124 | 5 | 32.85s | working | 568 | 195 | 1 | 2.93s | working |
| hard-text-regex | 2711 | 11303 | 3 | 13.95s | working | 529 | 169 | 1 | 2.84s | working |

## Per-task details

### simple-function

**ilo / dsflash**  
- Generation tokens: 1519  
- Input tokens (context): 11333  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [972, 165]  
- Wall time: 8.92s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 114  
- Input tokens (context): 175  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.34s  
- Outcome: **working**  

### with-dependencies

**ilo / dsflash**  
- Generation tokens: 3881  
- Input tokens (context): 20519  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [413, 1587, 1557, 268]  
- Wall time: 20.62s  
- Outcome: **partial**  

**bash / dsflash**  
- Generation tokens: 22891  
- Input tokens (context): 2138  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [280, 8666, 1224, 12414]  
- Wall time: 97.24s  
- Outcome: **failed**  

### data-transform

**ilo / dsflash**  
- Generation tokens: 2864  
- Input tokens (context): 16271  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [1438, 249, 383]  
- Wall time: 15.6s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 252  
- Input tokens (context): 186  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.83s  
- Outcome: **working**  

### tool-interaction

**ilo / dsflash**  
- Generation tokens: 2474  
- Input tokens (context): 19755  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [302, 1221, 128, 238]  
- Wall time: 14.68s  
- Outcome: **partial**  

**bash / dsflash**  
- Generation tokens: 578  
- Input tokens (context): 194  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.16s  
- Outcome: **working**  

### workflow-rollback

**ilo / dsflash**  
- Generation tokens: 4888  
- Input tokens (context): 19970  
- Attempts total: 5  
- Attempts to success: 5  
- Repair tokens by turn: [803, 817, 765, 1515]  
- Wall time: 25.24s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 351  
- Input tokens (context): 199  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.02s  
- Outcome: **working**  

### hard-recursion

**ilo / dsflash**  
- Generation tokens: 3795  
- Input tokens (context): 19756  
- Attempts total: 5  
- Attempts to success: 5  
- Repair tokens by turn: [274, 627, 335, 1926]  
- Wall time: 19.86s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 693  
- Input tokens (context): 186  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 4.91s  
- Outcome: **working**  

### hard-records

**ilo / dsflash**  
- Generation tokens: 6194  
- Input tokens (context): 21124  
- Attempts total: 5  
- Attempts to success: 5  
- Repair tokens by turn: [939, 316, 983, 3276]  
- Wall time: 32.85s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 568  
- Input tokens (context): 195  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.93s  
- Outcome: **working**  

### hard-text-regex

**ilo / dsflash**  
- Generation tokens: 2711  
- Input tokens (context): 11303  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [1265, 926]  
- Wall time: 13.95s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 529  
- Input tokens (context): 169  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.84s  
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
