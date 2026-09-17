# Closed-loop benchmark: ilo vs Zero per-task economics

Generated: 2026-09-17 08:02 UTC  
Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  
Retry cap: 5

## Summary

This table shows per-task economics across languages and models.
Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.

| task | ilo/dsflash: gen | inp | att | time | outcome | python/dsflash: gen | inp | att | time | outcome |
|---|---|---|---|---|---|---|---|---|---|---|
| simple-function | 799 | 3675 | 1 | 4.45s | working | 223 | 175 | 1 | 1.76s | working |
| with-dependencies | 9017 | 11943 | 3 | 44.77s | working | 107 | 180 | 1 | 1.32s | working |
| data-transform | 334 | 3686 | 1 | 1.96s | working | 298 | 186 | 1 | 2.18s | working |
| tool-interaction | 2681 | 16190 | 4 | 15.25s | working | 97 | 194 | 1 | 1.04s | working |
| workflow-rollback | 2487 | 3699 | 1 | 11.87s | working | 316 | 199 | 1 | 2.23s | working |

## Per-task details

### simple-function

**ilo / dsflash**  
- Generation tokens: 799  
- Input tokens (context): 3675  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 4.45s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 223  
- Input tokens (context): 175  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.76s  
- Outcome: **working**  

### with-dependencies

**ilo / dsflash**  
- Generation tokens: 9017  
- Input tokens (context): 11943  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [843, 7768]  
- Wall time: 44.77s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 107  
- Input tokens (context): 180  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.32s  
- Outcome: **working**  

### data-transform

**ilo / dsflash**  
- Generation tokens: 334  
- Input tokens (context): 3686  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.96s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 298  
- Input tokens (context): 186  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.18s  
- Outcome: **working**  

### tool-interaction

**ilo / dsflash**  
- Generation tokens: 2681  
- Input tokens (context): 16190  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [1418, 93, 61]  
- Wall time: 15.25s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 97  
- Input tokens (context): 194  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.04s  
- Outcome: **working**  

### workflow-rollback

**ilo / dsflash**  
- Generation tokens: 2487  
- Input tokens (context): 3699  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 11.87s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 316  
- Input tokens (context): 199  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.23s  
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
