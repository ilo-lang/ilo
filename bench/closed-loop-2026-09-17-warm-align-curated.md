# Closed-loop benchmark: ilo vs Zero per-task economics

Generated: 2026-09-17 08:26 UTC  
Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  
Retry cap: 5

## Summary

This table shows per-task economics across languages and models.
Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.

| task | ilo/dsflash: gen | inp | att | time | outcome | python/dsflash: gen | inp | att | time | outcome |
|---|---|---|---|---|---|---|---|---|---|---|
| simple-function | 303 | 5067 | 1 | 2.69s | working | 126 | 175 | 1 | 1.47s | working |
| with-dependencies | 1019 | 5072 | 1 | 6.24s | working | 62 | 180 | 1 | 1.25s | working |
| data-transform | 376 | 5078 | 1 | 2.96s | working | 438 | 186 | 1 | 2.61s | working |
| tool-interaction | 1362 | 15768 | 3 | 9.52s | working | 241 | 194 | 1 | 1.98s | working |
| workflow-rollback | 5994 | 5091 | 1 | 31.37s | working | 375 | 474 | 2 | 3.41s | working |

## Per-task details

### simple-function

**ilo / dsflash**  
- Generation tokens: 303  
- Input tokens (context): 5067  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.69s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 126  
- Input tokens (context): 175  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.47s  
- Outcome: **working**  

### with-dependencies

**ilo / dsflash**  
- Generation tokens: 1019  
- Input tokens (context): 5072  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 6.24s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 62  
- Input tokens (context): 180  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.25s  
- Outcome: **working**  

### data-transform

**ilo / dsflash**  
- Generation tokens: 376  
- Input tokens (context): 5078  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.96s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 438  
- Input tokens (context): 186  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.61s  
- Outcome: **working**  

### tool-interaction

**ilo / dsflash**  
- Generation tokens: 1362  
- Input tokens (context): 15768  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [265, 89]  
- Wall time: 9.52s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 241  
- Input tokens (context): 194  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.98s  
- Outcome: **working**  

### workflow-rollback

**ilo / dsflash**  
- Generation tokens: 5994  
- Input tokens (context): 5091  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 31.37s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 375  
- Input tokens (context): 474  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [302]  
- Wall time: 3.41s  
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
