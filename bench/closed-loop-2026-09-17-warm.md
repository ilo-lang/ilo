# Closed-loop benchmark: ilo vs Zero per-task economics

Generated: 2026-09-17 07:28 UTC  
Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  
Retry cap: 5

## Summary

This table shows per-task economics across languages and models.
Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.

| task | ilo/dsflash: gen | inp | att | time | outcome | python/dsflash: gen | inp | att | time | outcome |
|---|---|---|---|---|---|---|---|---|---|---|
| simple-function | 1614 | 19337 | 2 | 9.61s | working | 81 | 175 | 1 | 0.93s | working |
| with-dependencies | 752 | 9551 | 1 | 3.96s | working | 251 | 180 | 1 | 1.9s | working |
| data-transform | 770 | 9557 | 1 | 4.52s | working | 181 | 186 | 1 | 1.37s | working |
| tool-interaction | 1185 | 19240 | 2 | 7.63s | working | 201 | 194 | 1 | 1.65s | working |
| workflow-rollback | 6378 | 19335 | 2 | 30.55s | working | 106 | 199 | 1 | 1.45s | working |

## Per-task details

### simple-function

**ilo / dsflash**  
- Generation tokens: 1614  
- Input tokens (context): 19337  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [1211]  
- Wall time: 9.61s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 81  
- Input tokens (context): 175  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 0.93s  
- Outcome: **working**  

### with-dependencies

**ilo / dsflash**  
- Generation tokens: 752  
- Input tokens (context): 9551  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.96s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 251  
- Input tokens (context): 180  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.9s  
- Outcome: **working**  

### data-transform

**ilo / dsflash**  
- Generation tokens: 770  
- Input tokens (context): 9557  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 4.52s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 181  
- Input tokens (context): 186  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.37s  
- Outcome: **working**  

### tool-interaction

**ilo / dsflash**  
- Generation tokens: 1185  
- Input tokens (context): 19240  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [474]  
- Wall time: 7.63s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 201  
- Input tokens (context): 194  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.65s  
- Outcome: **working**  

### workflow-rollback

**ilo / dsflash**  
- Generation tokens: 6378  
- Input tokens (context): 19335  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [1513]  
- Wall time: 30.55s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 106  
- Input tokens (context): 199  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.45s  
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
