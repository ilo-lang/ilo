# Closed-loop benchmark: ilo vs Zero per-task economics

Generated: 2026-09-17 08:39 UTC  
Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  
Retry cap: 5

## Summary

This table shows per-task economics across languages and models.
Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.

| task | ilo/dsflash: gen | inp | att | time | outcome | python/dsflash: gen | inp | att | time | outcome |
|---|---|---|---|---|---|---|---|---|---|---|
| simple-function | 457 | 5067 | 1 | 3.38s | working | 120 | 175 | 1 | 1.48s | working |
| with-dependencies | 1138 | 10431 | 2 | 6.79s | working | 154 | 180 | 1 | 1.46s | working |
| data-transform | 2178 | 5078 | 1 | 12.22s | working | 87 | 186 | 1 | 1.2s | working |
| tool-interaction | 1404 | 15760 | 3 | 9.05s | working | 512 | 194 | 1 | 2.92s | working |
| workflow-rollback | 4772 | 16065 | 3 | 24.8s | working | 419 | 199 | 1 | 2.24s | working |

## Per-task details

### simple-function

**ilo / dsflash**  
- Generation tokens: 457  
- Input tokens (context): 5067  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.38s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 120  
- Input tokens (context): 175  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.48s  
- Outcome: **working**  

### with-dependencies

**ilo / dsflash**  
- Generation tokens: 1138  
- Input tokens (context): 10431  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [320]  
- Wall time: 6.79s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 154  
- Input tokens (context): 180  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.46s  
- Outcome: **working**  

### data-transform

**ilo / dsflash**  
- Generation tokens: 2178  
- Input tokens (context): 5078  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 12.22s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 87  
- Input tokens (context): 186  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.2s  
- Outcome: **working**  

### tool-interaction

**ilo / dsflash**  
- Generation tokens: 1404  
- Input tokens (context): 15760  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [42, 65]  
- Wall time: 9.05s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 512  
- Input tokens (context): 194  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.92s  
- Outcome: **working**  

### workflow-rollback

**ilo / dsflash**  
- Generation tokens: 4772  
- Input tokens (context): 16065  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [330, 131]  
- Wall time: 24.8s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 419  
- Input tokens (context): 199  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.24s  
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
