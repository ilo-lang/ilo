# Closed-loop benchmark: ilo vs Zero per-task economics

Generated: 2026-09-17 08:39 UTC  
Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  
Retry cap: 5

## Summary

This table shows per-task economics across languages and models.
Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.

| task | ilo/dsflash: gen | inp | att | time | outcome | python/dsflash: gen | inp | att | time | outcome |
|---|---|---|---|---|---|---|---|---|---|---|
| simple-function | 357 | 5067 | 1 | 2.53s | working | 106 | 175 | 1 | 1.21s | working |
| with-dependencies | 532 | 5072 | 1 | 3.56s | working | 760 | 180 | 1 | 4.5s | working |
| data-transform | 502 | 5078 | 1 | 3.24s | working | 372 | 186 | 1 | 2.5s | working |
| tool-interaction | 2089 | 10327 | 2 | 12.95s | working | 117 | 194 | 1 | 1.25s | working |
| workflow-rollback | 912 | 5091 | 1 | 5.33s | working | 233 | 199 | 1 | 1.52s | working |

## Per-task details

### simple-function

**ilo / dsflash**  
- Generation tokens: 357  
- Input tokens (context): 5067  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.53s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 106  
- Input tokens (context): 175  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.21s  
- Outcome: **working**  

### with-dependencies

**ilo / dsflash**  
- Generation tokens: 532  
- Input tokens (context): 5072  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.56s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 760  
- Input tokens (context): 180  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 4.5s  
- Outcome: **working**  

### data-transform

**ilo / dsflash**  
- Generation tokens: 502  
- Input tokens (context): 5078  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.24s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 372  
- Input tokens (context): 186  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.5s  
- Outcome: **working**  

### tool-interaction

**ilo / dsflash**  
- Generation tokens: 2089  
- Input tokens (context): 10327  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [79]  
- Wall time: 12.95s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 117  
- Input tokens (context): 194  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.25s  
- Outcome: **working**  

### workflow-rollback

**ilo / dsflash**  
- Generation tokens: 912  
- Input tokens (context): 5091  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 5.33s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 233  
- Input tokens (context): 199  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.52s  
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
