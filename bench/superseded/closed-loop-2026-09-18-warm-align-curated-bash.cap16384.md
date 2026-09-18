# Closed-loop benchmark: ilo vs bash per-task economics

Generated: 2026-09-18 19:48 UTC  
Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  
Retry cap: 5

## Summary

This table shows per-task economics across languages and models.
Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.

| task | ilo/dsflash: gen | inp | att | time | outcome | bash/dsflash: gen | inp | att | time | outcome |
|---|---|---|---|---|---|---|---|---|---|---|
| simple-function | 2361 | 15370 | 3 | 12.42s | working | 45 | 173 | 1 | 1.08s | working |
| with-dependencies | 4318 | 27716 | 5 | 23.02s | working | 136 | 178 | 1 | 1.24s | working |
| data-transform | 4574 | 16286 | 3 | 21.86s | working | 194 | 181 | 1 | 1.27s | working |
| tool-interaction | 1877 | 15455 | 3 | 10.9s | working | 129 | 184 | 1 | 1.12s | working |
| workflow-rollback | 1131 | 15506 | 3 | 6.5s | working | 111 | 189 | 1 | 1.1s | working |
| hard-recursion | 2949 | 10156 | 2 | 13.67s | working | 159 | 175 | 1 | 2.37s | working |
| hard-records | 11266 | 27742 | - | 51.95s | partial | 241 | 198 | 1 | 1.63s | working |
| hard-text-regex | 801 | 15337 | 3 | 5.8s | working | 530 | 168 | 1 | 3.19s | working |
| pipeline-report | 21765 | 17246 | 3 | 89.47s | working | 615 | 283 | 1 | 2.91s | working |
| text-analysis | 12393 | 10565 | 2 | 50.07s | working | 910 | 234 | 1 | 3.98s | working |
| gcd-lcm | 4431 | 15590 | 3 | 21.14s | working | 216 | 177 | 1 | 1.44s | working |
| prime-count | 16148 | 10284 | 2 | 72.52s | working | 587 | 182 | 1 | 2.97s | working |
| fizzbuzz-range | 8899 | 10471 | 2 | 39.21s | working | 346 | 236 | 1 | 1.74s | working |
| flatten-sum | 2789 | 15599 | 3 | 14.48s | working | 303 | 203 | 1 | 1.68s | working |
| dedupe-order | 982 | 10309 | 2 | 5.92s | working | 316 | 188 | 1 | 1.86s | working |
| run-length-encode | 54787 | 26805 | - | 239.97s | failed | 319 | 179 | 1 | 1.71s | working |
| top-words | 13779 | 29188 | 5 | 63.21s | working | 368 | 192 | 1 | 2.09s | working |
| cron-expand | 9274 | 16485 | 3 | 41.23s | working | 279 | 187 | 1 | 1.37s | working |
| record-summary | 23118 | 16312 | 3 | 98.48s | working | 971 | 263 | 1 | 4.99s | working |
| csv-aggregate | 16475 | 22456 | 4 | 73.22s | working | 488 | 241 | 1 | 2.39s | working |
| log-level-count | 3309 | 10605 | 2 | 14.94s | working | 272 | 227 | 1 | 1.7s | working |
| weekday-of-date | 6241 | 10243 | 2 | 25.17s | working | 12416 | 1256 | 3 | 50.62s | working |
| least-squares-slope | 6114 | 16465 | 3 | 25.89s | working | 595 | 205 | 1 | 2.57s | working |
| kmeans-assign | 18859 | 17325 | 3 | 75.08s | working | 1254 | 271 | 1 | 4.67s | working |

## Per-task details

### simple-function

**ilo / dsflash**  
- Generation tokens: 2361  
- Input tokens (context): 15370  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [1438, 198]  
- Wall time: 12.42s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 45  
- Input tokens (context): 173  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.08s  
- Outcome: **working**  

### with-dependencies

**ilo / dsflash**  
- Generation tokens: 4318  
- Input tokens (context): 27716  
- Attempts total: 5  
- Attempts to success: 5  
- Repair tokens by turn: [350, 974, 2058, 730]  
- Wall time: 23.02s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 136  
- Input tokens (context): 178  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.24s  
- Outcome: **working**  

### data-transform

**ilo / dsflash**  
- Generation tokens: 4574  
- Input tokens (context): 16286  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [509, 3462]  
- Wall time: 21.86s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 194  
- Input tokens (context): 181  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.27s  
- Outcome: **working**  

### tool-interaction

**ilo / dsflash**  
- Generation tokens: 1877  
- Input tokens (context): 15455  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [1051, 300]  
- Wall time: 10.9s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 129  
- Input tokens (context): 184  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.12s  
- Outcome: **working**  

### workflow-rollback

**ilo / dsflash**  
- Generation tokens: 1131  
- Input tokens (context): 15506  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [233, 449]  
- Wall time: 6.5s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 111  
- Input tokens (context): 189  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.1s  
- Outcome: **working**  

### hard-recursion

**ilo / dsflash**  
- Generation tokens: 2949  
- Input tokens (context): 10156  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [2858]  
- Wall time: 13.67s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 159  
- Input tokens (context): 175  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.37s  
- Outcome: **working**  

### hard-records

**ilo / dsflash**  
- Generation tokens: 11266  
- Input tokens (context): 27742  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [957, 8483, 150, 520]  
- Wall time: 51.95s  
- Outcome: **partial**  

**bash / dsflash**  
- Generation tokens: 241  
- Input tokens (context): 198  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.63s  
- Outcome: **working**  

### hard-text-regex

**ilo / dsflash**  
- Generation tokens: 801  
- Input tokens (context): 15337  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [329, 271]  
- Wall time: 5.8s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 530  
- Input tokens (context): 168  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.19s  
- Outcome: **working**  

### pipeline-report

**ilo / dsflash**  
- Generation tokens: 21765  
- Input tokens (context): 17246  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [1137, 5419]  
- Wall time: 89.47s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 615  
- Input tokens (context): 283  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.91s  
- Outcome: **working**  

### text-analysis

**ilo / dsflash**  
- Generation tokens: 12393  
- Input tokens (context): 10565  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [9921]  
- Wall time: 50.07s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 910  
- Input tokens (context): 234  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.98s  
- Outcome: **working**  

### gcd-lcm

**ilo / dsflash**  
- Generation tokens: 4431  
- Input tokens (context): 15590  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [242, 2600]  
- Wall time: 21.14s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 216  
- Input tokens (context): 177  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.44s  
- Outcome: **working**  

### prime-count

**ilo / dsflash**  
- Generation tokens: 16148  
- Input tokens (context): 10284  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [13147]  
- Wall time: 72.52s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 587  
- Input tokens (context): 182  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.97s  
- Outcome: **working**  

### fizzbuzz-range

**ilo / dsflash**  
- Generation tokens: 8899  
- Input tokens (context): 10471  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [7050]  
- Wall time: 39.21s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 346  
- Input tokens (context): 236  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.74s  
- Outcome: **working**  

### flatten-sum

**ilo / dsflash**  
- Generation tokens: 2789  
- Input tokens (context): 15599  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [756, 1384]  
- Wall time: 14.48s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 303  
- Input tokens (context): 203  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.68s  
- Outcome: **working**  

### dedupe-order

**ilo / dsflash**  
- Generation tokens: 982  
- Input tokens (context): 10309  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [596]  
- Wall time: 5.92s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 316  
- Input tokens (context): 188  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.86s  
- Outcome: **working**  

### run-length-encode

**ilo / dsflash**  
- Generation tokens: 54787  
- Input tokens (context): 26805  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [5214, 16384, 7591, 14229]  
- Wall time: 239.97s  
- Outcome: **failed**  

**bash / dsflash**  
- Generation tokens: 319  
- Input tokens (context): 179  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.71s  
- Outcome: **working**  

### top-words

**ilo / dsflash**  
- Generation tokens: 13779  
- Input tokens (context): 29188  
- Attempts total: 5  
- Attempts to success: 5  
- Repair tokens by turn: [5236, 534, 5180, 778]  
- Wall time: 63.21s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 368  
- Input tokens (context): 192  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.09s  
- Outcome: **working**  

### cron-expand

**ilo / dsflash**  
- Generation tokens: 9274  
- Input tokens (context): 16485  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [972, 3893]  
- Wall time: 41.23s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 279  
- Input tokens (context): 187  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.37s  
- Outcome: **working**  

### record-summary

**ilo / dsflash**  
- Generation tokens: 23118  
- Input tokens (context): 16312  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [9051, 8633]  
- Wall time: 98.48s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 971  
- Input tokens (context): 263  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 4.99s  
- Outcome: **working**  

### csv-aggregate

**ilo / dsflash**  
- Generation tokens: 16475  
- Input tokens (context): 22456  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [6494, 602, 2718]  
- Wall time: 73.22s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 488  
- Input tokens (context): 241  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.39s  
- Outcome: **working**  

### log-level-count

**ilo / dsflash**  
- Generation tokens: 3309  
- Input tokens (context): 10605  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [1871]  
- Wall time: 14.94s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 272  
- Input tokens (context): 227  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.7s  
- Outcome: **working**  

### weekday-of-date

**ilo / dsflash**  
- Generation tokens: 6241  
- Input tokens (context): 10243  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [3009]  
- Wall time: 25.17s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 12416  
- Input tokens (context): 1256  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [1936, 9402]  
- Wall time: 50.62s  
- Outcome: **working**  

### least-squares-slope

**ilo / dsflash**  
- Generation tokens: 6114  
- Input tokens (context): 16465  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [3668, 269]  
- Wall time: 25.89s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 595  
- Input tokens (context): 205  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.57s  
- Outcome: **working**  

### kmeans-assign

**ilo / dsflash**  
- Generation tokens: 18859  
- Input tokens (context): 17325  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [10503, 631]  
- Wall time: 75.08s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 1254  
- Input tokens (context): 271  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 4.67s  
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
