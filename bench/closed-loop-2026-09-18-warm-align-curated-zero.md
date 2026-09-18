# Closed-loop benchmark: ilo vs zero per-task economics

Generated: 2026-09-18 21:02 UTC  
Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  
Retry cap: 5

## Summary

This table shows per-task economics across languages and models.
Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.

| task | ilo/dsflash: gen | inp | att | time | outcome | zero/dsflash: gen | inp | att | time | outcome |
|---|---|---|---|---|---|---|---|---|---|---|
| simple-function | 986 | 10139 | 2 | 5.84s | working | 1266 | 33601 | 1 | 30.9s | working |
| with-dependencies | 6873 | 21334 | 4 | 32.71s | working | 475 | 33606 | 1 | 3.09s | working |
| data-transform | 3795 | 15470 | 3 | 17.32s | working | 781 | 33609 | 1 | 4.16s | working |
| tool-interaction | 2663 | 15467 | 3 | 14.21s | working | 517 | 33612 | 1 | 3.14s | working |
| workflow-rollback | 1234 | 10183 | 2 | 6.73s | working | 3053 | 33617 | 1 | 14.04s | working |
| hard-recursion | 2090 | 10156 | 2 | 10.49s | working | 659 | 33603 | 1 | 3.8s | working |
| hard-records | 20187 | 22539 | 4 | 89.69s | working | 5440 | 136058 | 4 | 26.3s | working |
| hard-text-regex | 950 | 10118 | 2 | 5.42s | working | 684 | 33596 | 1 | 4.25s | working |
| pipeline-report | 13181 | 16529 | 3 | 55.24s | working | 33794 | 103256 | 3 | 125.43s | working |
| text-analysis | 8779 | 10524 | 2 | 38.55s | working | 13544 | 67926 | 2 | 47.01s | working |
| gcd-lcm | 2538 | 16021 | 3 | 13.62s | working | 2010 | 67482 | 2 | 9.85s | working |
| prime-count | 20995 | 22482 | 4 | 89.27s | working | 3419 | 67466 | 2 | 16.85s | working |
| fizzbuzz-range | 17128 | 21974 | 4 | 73.63s | working | 5241 | 67667 | 2 | 23.18s | working |
| flatten-sum | 4597 | 27008 | - | 22.94s | failed | 651 | 33631 | 1 | 3.78s | working |
| dedupe-order | 3951 | 20984 | 4 | 20.88s | working | 1887 | 33616 | 1 | 7.94s | working |
| run-length-encode | 17477 | 16081 | 3 | 77.09s | working | 1236 | 33607 | 1 | 5.74s | working |
| top-words | 19491 | 28744 | 5 | 89.68s | working | 2920 | 67692 | 2 | 13.16s | working |
| cron-expand | 9582 | 16021 | 3 | 42.59s | working | 9182 | 33615 | 1 | 34.11s | working |
| record-summary | 13352 | 16846 | 3 | 59.67s | working | 11559 | 102584 | 3 | 47.66s | working |
| csv-aggregate | 13078 | 28322 | 5 | 59.02s | working | 99497 | 176312 | 5 | 383.23s | working |
| log-level-count | 6576 | 22239 | 4 | 30.4s | working | 3368 | 67753 | 2 | 15.26s | working |
| weekday-of-date | 10510 | 22498 | 4 | 45.88s | working | 1156 | 33602 | 1 | 5.74s | working |
| least-squares-slope | 1938 | 10280 | 2 | 8.78s | working | 7042 | 33633 | 1 | 25.96s | working |
| kmeans-assign | 7350 | 16812 | 3 | 31.28s | working | 1689 | 33699 | 1 | 8.22s | working |

## Per-task details

### simple-function

**ilo / dsflash**  
- Generation tokens: 986  
- Input tokens (context): 10139  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [669]  
- Wall time: 5.84s  
- Outcome: **working**  

**zero / dsflash**  
- Generation tokens: 1266  
- Input tokens (context): 33601  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 30.9s  
- Outcome: **working**  

### with-dependencies

**ilo / dsflash**  
- Generation tokens: 6873  
- Input tokens (context): 21334  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [335, 1502, 4133]  
- Wall time: 32.71s  
- Outcome: **working**  

**zero / dsflash**  
- Generation tokens: 475  
- Input tokens (context): 33606  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.09s  
- Outcome: **working**  

### data-transform

**ilo / dsflash**  
- Generation tokens: 3795  
- Input tokens (context): 15470  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [2542, 525]  
- Wall time: 17.32s  
- Outcome: **working**  

**zero / dsflash**  
- Generation tokens: 781  
- Input tokens (context): 33609  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 4.16s  
- Outcome: **working**  

### tool-interaction

**ilo / dsflash**  
- Generation tokens: 2663  
- Input tokens (context): 15467  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [1038, 67]  
- Wall time: 14.21s  
- Outcome: **working**  

**zero / dsflash**  
- Generation tokens: 517  
- Input tokens (context): 33612  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.14s  
- Outcome: **working**  

### workflow-rollback

**ilo / dsflash**  
- Generation tokens: 1234  
- Input tokens (context): 10183  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [732]  
- Wall time: 6.73s  
- Outcome: **working**  

**zero / dsflash**  
- Generation tokens: 3053  
- Input tokens (context): 33617  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 14.04s  
- Outcome: **working**  

### hard-recursion

**ilo / dsflash**  
- Generation tokens: 2090  
- Input tokens (context): 10156  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [844]  
- Wall time: 10.49s  
- Outcome: **working**  

**zero / dsflash**  
- Generation tokens: 659  
- Input tokens (context): 33603  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.8s  
- Outcome: **working**  

### hard-records

**ilo / dsflash**  
- Generation tokens: 20187  
- Input tokens (context): 22539  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [12281, 4215, 1902]  
- Wall time: 89.69s  
- Outcome: **working**  

**zero / dsflash**  
- Generation tokens: 5440  
- Input tokens (context): 136058  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [3538, 248, 543]  
- Wall time: 26.3s  
- Outcome: **working**  

### hard-text-regex

**ilo / dsflash**  
- Generation tokens: 950  
- Input tokens (context): 10118  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [662]  
- Wall time: 5.42s  
- Outcome: **working**  

**zero / dsflash**  
- Generation tokens: 684  
- Input tokens (context): 33596  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 4.25s  
- Outcome: **working**  

### pipeline-report

**ilo / dsflash**  
- Generation tokens: 13181  
- Input tokens (context): 16529  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [6370, 456]  
- Wall time: 55.24s  
- Outcome: **working**  

**zero / dsflash**  
- Generation tokens: 33794  
- Input tokens (context): 103256  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [11909, 3767]  
- Wall time: 125.43s  
- Outcome: **working**  

### text-analysis

**ilo / dsflash**  
- Generation tokens: 8779  
- Input tokens (context): 10524  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [6838]  
- Wall time: 38.55s  
- Outcome: **working**  

**zero / dsflash**  
- Generation tokens: 13544  
- Input tokens (context): 67926  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [10586]  
- Wall time: 47.01s  
- Outcome: **working**  

### gcd-lcm

**ilo / dsflash**  
- Generation tokens: 2538  
- Input tokens (context): 16021  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [816, 636]  
- Wall time: 13.62s  
- Outcome: **working**  

**zero / dsflash**  
- Generation tokens: 2010  
- Input tokens (context): 67482  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [1094]  
- Wall time: 9.85s  
- Outcome: **working**  

### prime-count

**ilo / dsflash**  
- Generation tokens: 20995  
- Input tokens (context): 22482  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [2551, 6168, 9373]  
- Wall time: 89.27s  
- Outcome: **working**  

**zero / dsflash**  
- Generation tokens: 3419  
- Input tokens (context): 67466  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [2848]  
- Wall time: 16.85s  
- Outcome: **working**  

### fizzbuzz-range

**ilo / dsflash**  
- Generation tokens: 17128  
- Input tokens (context): 21974  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [1243, 8522, 6188]  
- Wall time: 73.63s  
- Outcome: **working**  

**zero / dsflash**  
- Generation tokens: 5241  
- Input tokens (context): 67667  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [1536]  
- Wall time: 23.18s  
- Outcome: **working**  

### flatten-sum

**ilo / dsflash**  
- Generation tokens: 4597  
- Input tokens (context): 27008  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [550, 2510, 135, 498]  
- Wall time: 22.94s  
- Outcome: **failed**  

**zero / dsflash**  
- Generation tokens: 651  
- Input tokens (context): 33631  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.78s  
- Outcome: **working**  

### dedupe-order

**ilo / dsflash**  
- Generation tokens: 3951  
- Input tokens (context): 20984  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [1286, 1472, 77]  
- Wall time: 20.88s  
- Outcome: **working**  

**zero / dsflash**  
- Generation tokens: 1887  
- Input tokens (context): 33616  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 7.94s  
- Outcome: **working**  

### run-length-encode

**ilo / dsflash**  
- Generation tokens: 17477  
- Input tokens (context): 16081  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [4228, 4332]  
- Wall time: 77.09s  
- Outcome: **working**  

**zero / dsflash**  
- Generation tokens: 1236  
- Input tokens (context): 33607  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 5.74s  
- Outcome: **working**  

### top-words

**ilo / dsflash**  
- Generation tokens: 19491  
- Input tokens (context): 28744  
- Attempts total: 5  
- Attempts to success: 5  
- Repair tokens by turn: [10888, 607, 3032, 1206]  
- Wall time: 89.68s  
- Outcome: **working**  

**zero / dsflash**  
- Generation tokens: 2920  
- Input tokens (context): 67692  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [1349]  
- Wall time: 13.16s  
- Outcome: **working**  

### cron-expand

**ilo / dsflash**  
- Generation tokens: 9582  
- Input tokens (context): 16021  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [4769, 251]  
- Wall time: 42.59s  
- Outcome: **working**  

**zero / dsflash**  
- Generation tokens: 9182  
- Input tokens (context): 33615  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 34.11s  
- Outcome: **working**  

### record-summary

**ilo / dsflash**  
- Generation tokens: 13352  
- Input tokens (context): 16846  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [3743, 6399]  
- Wall time: 59.67s  
- Outcome: **working**  

**zero / dsflash**  
- Generation tokens: 11559  
- Input tokens (context): 102584  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [7102, 508]  
- Wall time: 47.66s  
- Outcome: **working**  

### csv-aggregate

**ilo / dsflash**  
- Generation tokens: 13078  
- Input tokens (context): 28322  
- Attempts total: 5  
- Attempts to success: 5  
- Repair tokens by turn: [302, 2116, 8436, 808]  
- Wall time: 59.02s  
- Outcome: **working**  

**zero / dsflash**  
- Generation tokens: 99497  
- Input tokens (context): 176312  
- Attempts total: 5  
- Attempts to success: 5  
- Repair tokens by turn: [42868, 11704, 5562, 3201]  
- Wall time: 383.23s  
- Outcome: **working**  

### log-level-count

**ilo / dsflash**  
- Generation tokens: 6576  
- Input tokens (context): 22239  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [4606, 288, 482]  
- Wall time: 30.4s  
- Outcome: **working**  

**zero / dsflash**  
- Generation tokens: 3368  
- Input tokens (context): 67753  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [1653]  
- Wall time: 15.26s  
- Outcome: **working**  

### weekday-of-date

**ilo / dsflash**  
- Generation tokens: 10510  
- Input tokens (context): 22498  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [1606, 2653, 2664]  
- Wall time: 45.88s  
- Outcome: **working**  

**zero / dsflash**  
- Generation tokens: 1156  
- Input tokens (context): 33602  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 5.74s  
- Outcome: **working**  

### least-squares-slope

**ilo / dsflash**  
- Generation tokens: 1938  
- Input tokens (context): 10280  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [653]  
- Wall time: 8.78s  
- Outcome: **working**  

**zero / dsflash**  
- Generation tokens: 7042  
- Input tokens (context): 33633  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 25.96s  
- Outcome: **working**  

### kmeans-assign

**ilo / dsflash**  
- Generation tokens: 7350  
- Input tokens (context): 16812  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [2368, 1916]  
- Wall time: 31.28s  
- Outcome: **working**  

**zero / dsflash**  
- Generation tokens: 1689  
- Input tokens (context): 33699  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 8.22s  
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
