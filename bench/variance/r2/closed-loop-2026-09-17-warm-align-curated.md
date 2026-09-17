# Closed-loop benchmark: ilo vs Zero per-task economics

Generated: 2026-09-17 08:41 UTC  
Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  
Retry cap: 5

## Summary

This table shows per-task economics across languages and models.
Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.

| task | ilo/dsflash: gen | inp | att | time | outcome | python/dsflash: gen | inp | att | time | outcome |
|---|---|---|---|---|---|---|---|---|---|---|
| simple-function | 1265 | 5067 | 1 | 6.7s | working | 60 | 175 | 1 | 1.02s | working |
| with-dependencies | 488 | 5072 | 1 | 3.12s | working | 224 | 180 | 1 | 1.82s | working |
| data-transform | 720 | 5078 | 1 | 4.04s | working | 418 | 186 | 1 | 2.49s | working |
| tool-interaction | 200 | 5086 | 1 | 1.88s | working | 246 | 194 | 1 | 1.71s | working |
| workflow-rollback | 10669 | 10456 | 2 | 51.95s | working | 595 | 199 | 1 | 3.94s | working |

## Per-task details

### simple-function

**ilo / dsflash**  
- Generation tokens: 1265  
- Input tokens (context): 5067  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 6.7s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 60  
- Input tokens (context): 175  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.02s  
- Outcome: **working**  

### with-dependencies

**ilo / dsflash**  
- Generation tokens: 488  
- Input tokens (context): 5072  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.12s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 224  
- Input tokens (context): 180  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.82s  
- Outcome: **working**  

### data-transform

**ilo / dsflash**  
- Generation tokens: 720  
- Input tokens (context): 5078  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 4.04s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 418  
- Input tokens (context): 186  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.49s  
- Outcome: **working**  

### tool-interaction

**ilo / dsflash**  
- Generation tokens: 200  
- Input tokens (context): 5086  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.88s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 246  
- Input tokens (context): 194  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.71s  
- Outcome: **working**  

### workflow-rollback

**ilo / dsflash**  
- Generation tokens: 10669  
- Input tokens (context): 10456  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [354]  
- Wall time: 51.95s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 595  
- Input tokens (context): 199  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.94s  
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
