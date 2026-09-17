# Closed-loop benchmark: ilo vs Zero per-task economics

Generated: 2026-09-17 11:21 UTC  
Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  
Retry cap: 5

## Summary

This table shows per-task economics across languages and models.
Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.

| task | ilo/dsflash: gen | inp | att | time | outcome | python/dsflash: gen | inp | att | time | outcome |
|---|---|---|---|---|---|---|---|---|---|---|
| simple-function | 644 | 5100 | 1 | 3.63s | working | 138 | 190 | 1 | 1.33s | working |
| with-dependencies | 1020 | 5106 | 1 | 4.97s | working | 103 | 197 | 1 | 1.1s | working |
| data-transform | 584 | 5113 | 1 | 3.23s | working | 314 | 202 | 1 | 1.99s | working |
| tool-interaction | 703 | 10393 | 2 | 4.75s | working | 155 | 209 | 1 | 1.35s | working |
| workflow-rollback | 649 | 5122 | 1 | 3.95s | working | 312 | 217 | 1 | 2.13s | working |

## Per-task details

### simple-function

**ilo / dsflash**  
- Generation tokens: 644  
- Input tokens (context): 5100  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.63s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 138  
- Input tokens (context): 190  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.33s  
- Outcome: **working**  

### with-dependencies

**ilo / dsflash**  
- Generation tokens: 1020  
- Input tokens (context): 5106  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 4.97s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 103  
- Input tokens (context): 197  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.1s  
- Outcome: **working**  

### data-transform

**ilo / dsflash**  
- Generation tokens: 584  
- Input tokens (context): 5113  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.23s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 314  
- Input tokens (context): 202  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.99s  
- Outcome: **working**  

### tool-interaction

**ilo / dsflash**  
- Generation tokens: 703  
- Input tokens (context): 10393  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [167]  
- Wall time: 4.75s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 155  
- Input tokens (context): 209  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.35s  
- Outcome: **working**  

### workflow-rollback

**ilo / dsflash**  
- Generation tokens: 649  
- Input tokens (context): 5122  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.95s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 312  
- Input tokens (context): 217  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.13s  
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
