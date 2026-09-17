# Closed-loop benchmark: ilo vs Zero per-task economics

Generated: 2026-09-17 19:55 UTC  
Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  
Retry cap: 5

## Summary

This table shows per-task economics across languages and models.
Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.

| task | ilo/qwen-1.5b: gen | inp | att | time | outcome | zero/qwen-1.5b: gen | inp | att | time | outcome |
|---|---|---|---|---|---|---|---|---|---|---|
| simple-function | 3049 | 18204 | - | 203.88s | failed | 0 | 0 | - | 10.28s | failed |
| with-dependencies | 254 | 26245 | - | 22.14s | failed | 0 | 0 | - | 10.24s | failed |
| data-transform | 280 | 26320 | - | 23.98s | failed | 0 | 0 | - | 10.24s | failed |
| tool-interaction | 173 | 26121 | - | 17.02s | failed | 0 | 0 | - | 10.23s | failed |
| workflow-rollback | 221 | 26299 | - | 20.36s | failed | 0 | 0 | - | 10.23s | failed |
| hard-recursion | 259 | 26292 | - | 22.76s | failed | 0 | 0 | - | 10.24s | failed |
| hard-records | 980 | 28060 | - | 70.93s | failed | 0 | 0 | - | 10.24s | failed |
| hard-text-regex | 1074 | 29835 | - | 75.4s | failed | 0 | 0 | - | 10.24s | failed |

## Per-task details

### simple-function

**ilo / qwen-1.5b**  
- Generation tokens: 3049  
- Input tokens (context): 18204  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [1024, 1001]  
- Wall time: 203.88s  
- Outcome: **failed**  

**zero / qwen-1.5b**  
- Generation tokens: 0  
- Input tokens (context): 0  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: []  
- Wall time: 10.28s  
- Outcome: **failed**  

### with-dependencies

**ilo / qwen-1.5b**  
- Generation tokens: 254  
- Input tokens (context): 26245  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [53, 50, 49, 49]  
- Wall time: 22.14s  
- Outcome: **failed**  

**zero / qwen-1.5b**  
- Generation tokens: 0  
- Input tokens (context): 0  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: []  
- Wall time: 10.24s  
- Outcome: **failed**  

### data-transform

**ilo / qwen-1.5b**  
- Generation tokens: 280  
- Input tokens (context): 26320  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [56, 56, 56, 56]  
- Wall time: 23.98s  
- Outcome: **failed**  

**zero / qwen-1.5b**  
- Generation tokens: 0  
- Input tokens (context): 0  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: []  
- Wall time: 10.24s  
- Outcome: **failed**  

### tool-interaction

**ilo / qwen-1.5b**  
- Generation tokens: 173  
- Input tokens (context): 26121  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [34, 35, 34, 35]  
- Wall time: 17.02s  
- Outcome: **failed**  

**zero / qwen-1.5b**  
- Generation tokens: 0  
- Input tokens (context): 0  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: []  
- Wall time: 10.23s  
- Outcome: **failed**  

### workflow-rollback

**ilo / qwen-1.5b**  
- Generation tokens: 221  
- Input tokens (context): 26299  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [40, 40, 40, 40]  
- Wall time: 20.36s  
- Outcome: **failed**  

**zero / qwen-1.5b**  
- Generation tokens: 0  
- Input tokens (context): 0  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: []  
- Wall time: 10.23s  
- Outcome: **failed**  

### hard-recursion

**ilo / qwen-1.5b**  
- Generation tokens: 259  
- Input tokens (context): 26292  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [51, 53, 51, 51]  
- Wall time: 22.76s  
- Outcome: **failed**  

**zero / qwen-1.5b**  
- Generation tokens: 0  
- Input tokens (context): 0  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: []  
- Wall time: 10.24s  
- Outcome: **failed**  

### hard-records

**ilo / qwen-1.5b**  
- Generation tokens: 980  
- Input tokens (context): 28060  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [196, 196, 196, 196]  
- Wall time: 70.93s  
- Outcome: **failed**  

**zero / qwen-1.5b**  
- Generation tokens: 0  
- Input tokens (context): 0  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: []  
- Wall time: 10.24s  
- Outcome: **failed**  

### hard-text-regex

**ilo / qwen-1.5b**  
- Generation tokens: 1074  
- Input tokens (context): 29835  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [10, 15, 10, 15]  
- Wall time: 75.4s  
- Outcome: **failed**  

**zero / qwen-1.5b**  
- Generation tokens: 0  
- Input tokens (context): 0  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: []  
- Wall time: 10.24s  
- Outcome: **failed**  

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
