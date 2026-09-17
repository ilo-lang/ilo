# Closed-loop benchmark: ilo vs Zero per-task economics

Generated: 2026-09-17 08:02 UTC  
Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  
Retry cap: 5

## Summary

This table shows per-task economics across languages and models.
Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.

| task | ilo/dsflash: gen | inp | att | time | outcome | python/dsflash: gen | inp | att | time | outcome |
|---|---|---|---|---|---|---|---|---|---|---|
| simple-function | 521 | 9545 | 1 | 3.77s | working | 126 | 175 | 1 | 1.41s | working |
| with-dependencies | 416 | 9550 | 1 | 3.01s | working | 158 | 180 | 1 | 1.43s | working |
| data-transform | 1524 | 9556 | 1 | 8.53s | working | 260 | 186 | 1 | 1.93s | working |
| tool-interaction | 665 | 19283 | 2 | 5.13s | working | 318 | 194 | 1 | 2.2s | working |
| workflow-rollback | 677 | 9569 | 1 | 3.75s | working | 349 | 199 | 1 | 2.19s | working |

## Per-task details

### simple-function

**ilo / dsflash**  
- Generation tokens: 521  
- Input tokens (context): 9545  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.77s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 126  
- Input tokens (context): 175  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.41s  
- Outcome: **working**  

### with-dependencies

**ilo / dsflash**  
- Generation tokens: 416  
- Input tokens (context): 9550  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.01s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 158  
- Input tokens (context): 180  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.43s  
- Outcome: **working**  

### data-transform

**ilo / dsflash**  
- Generation tokens: 1524  
- Input tokens (context): 9556  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 8.53s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 260  
- Input tokens (context): 186  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.93s  
- Outcome: **working**  

### tool-interaction

**ilo / dsflash**  
- Generation tokens: 665  
- Input tokens (context): 19283  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [94]  
- Wall time: 5.13s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 318  
- Input tokens (context): 194  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.2s  
- Outcome: **working**  

### workflow-rollback

**ilo / dsflash**  
- Generation tokens: 677  
- Input tokens (context): 9569  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.75s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 349  
- Input tokens (context): 199  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.19s  
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
