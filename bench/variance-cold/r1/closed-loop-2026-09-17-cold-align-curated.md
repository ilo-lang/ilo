# Closed-loop benchmark: ilo vs Zero per-task economics

Generated: 2026-09-17 11:20 UTC  
Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  
Retry cap: 5

## Summary

This table shows per-task economics across languages and models.
Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.

| task | ilo/dsflash: gen | inp | att | time | outcome | python/dsflash: gen | inp | att | time | outcome |
|---|---|---|---|---|---|---|---|---|---|---|
| simple-function | 313 | 5100 | 1 | 2.52s | working | 122 | 191 | 1 | 1.23s | working |
| with-dependencies | 524 | 5106 | 1 | 3.12s | working | 169 | 193 | 1 | 1.37s | working |
| data-transform | 384 | 5113 | 1 | 2.53s | working | 207 | 537 | 2 | 2.2s | working |
| tool-interaction | 507 | 5120 | 1 | 3.44s | working | 363 | 207 | 1 | 2.14s | working |
| workflow-rollback | 1271 | 5126 | 1 | 6.48s | working | 912 | 215 | 1 | 4.23s | working |

## Per-task details

### simple-function

**ilo / dsflash**  
- Generation tokens: 313  
- Input tokens (context): 5100  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.52s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 122  
- Input tokens (context): 191  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.23s  
- Outcome: **working**  

### with-dependencies

**ilo / dsflash**  
- Generation tokens: 524  
- Input tokens (context): 5106  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.12s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 169  
- Input tokens (context): 193  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.37s  
- Outcome: **working**  

### data-transform

**ilo / dsflash**  
- Generation tokens: 384  
- Input tokens (context): 5113  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.53s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 207  
- Input tokens (context): 537  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [91]  
- Wall time: 2.2s  
- Outcome: **working**  

### tool-interaction

**ilo / dsflash**  
- Generation tokens: 507  
- Input tokens (context): 5120  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.44s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 363  
- Input tokens (context): 207  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.14s  
- Outcome: **working**  

### workflow-rollback

**ilo / dsflash**  
- Generation tokens: 1271  
- Input tokens (context): 5126  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 6.48s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 912  
- Input tokens (context): 215  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 4.23s  
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
