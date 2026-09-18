# Closed-loop benchmark: ilo vs nanolang per-task economics

Generated: 2026-09-18 21:22 UTC  
Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  
Retry cap: 5

## Summary

This table shows per-task economics across languages and models.
Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.

| task | ilo/dsflash: gen | inp | att | time | outcome | nanolang/dsflash: gen | inp | att | time | outcome |
|---|---|---|---|---|---|---|---|---|---|---|
| simple-function | 3227 | 20911 | 4 | 16.68s | working | 271 | 6231 | 1 | 1.97s | working |
| with-dependencies | 1493 | 10145 | 2 | 8.56s | working | 233 | 6236 | 1 | 1.86s | working |
| data-transform | 3517 | 10436 | 2 | 16.68s | working | 1480 | 13072 | 2 | 7.61s | working |
| tool-interaction | 2639 | 20835 | 4 | 14.49s | working | 444 | 6242 | 1 | 2.44s | working |
| workflow-rollback | 4310 | 15518 | 3 | 20.64s | working | 382 | 6247 | 1 | 2.2s | working |
| hard-recursion | 1700 | 15622 | 3 | 9.4s | working | 383 | 6233 | 1 | 1.95s | working |
| hard-records | 34252 | 28904 | 5 | 156.02s | working | 5250 | 27385 | 4 | 22.01s | working |
| hard-text-regex | 995 | 10118 | 2 | 6.75s | working | 687 | 6226 | 1 | 3.01s | working |
| pipeline-report | 52504 | 30251 | - | 220.44s | failed | 31040 | 21704 | 3 | 117.13s | working |
| text-analysis | 27129 | 30773 | - | 114.19s | failed | 10212 | 6334 | 1 | 37.01s | working |
| gcd-lcm | 7073 | 16054 | 3 | 31.27s | working | 347 | 6277 | 1 | 2.05s | working |
| prime-count | 10451 | 10263 | 2 | 44.96s | working | 726 | 6282 | 1 | 3.33s | working |
| fizzbuzz-range | 6972 | 10357 | 2 | 29.98s | working | 662 | 6336 | 1 | 28.11s | working |
| flatten-sum | 2050 | 16073 | 3 | 10.29s | working | 2351 | 20317 | 3 | 10.86s | working |
| dedupe-order | 2148 | 26542 | 5 | 12.85s | working | 5607 | 20724 | 3 | 21.4s | working |
| run-length-encode | 19236 | 16021 | 3 | 82.67s | working | 658 | 6279 | 1 | 2.83s | working |
| top-words | 11333 | 16240 | 3 | 52.6s | working | 4273 | 13107 | 2 | 15.81s | working |
| cron-expand | 13764 | 16075 | 3 | 62.88s | working | 12822 | 6287 | 1 | 50.47s | working |
| record-summary | 9218 | 10506 | 2 | 40.24s | working | 5371 | 6363 | 1 | 19.28s | working |
| csv-aggregate | 12198 | 22225 | 4 | 53.61s | working | 12882 | 6341 | 1 | 46.78s | working |
| log-level-count | 11261 | 29586 | - | 50.95s | failed | 1016 | 6327 | 1 | 4.39s | working |
| weekday-of-date | 4852 | 10272 | 2 | 19.97s | working | 1479 | 6274 | 1 | 5.44s | working |
| least-squares-slope | 2240 | 10423 | 2 | 10.34s | working | 3207 | 6305 | 1 | 12.73s | working |
| kmeans-assign | 5854 | 10774 | 2 | 24.56s | working | 2988 | 6371 | 1 | 11.91s | working |

## Per-task details

### simple-function

**ilo / dsflash**  
- Generation tokens: 3227  
- Input tokens (context): 20911  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [574, 787, 745]  
- Wall time: 16.68s  
- Outcome: **working**  

**nanolang / dsflash**  
- Generation tokens: 271  
- Input tokens (context): 6231  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.97s  
- Outcome: **working**  

### with-dependencies

**ilo / dsflash**  
- Generation tokens: 1493  
- Input tokens (context): 10145  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [1013]  
- Wall time: 8.56s  
- Outcome: **working**  

**nanolang / dsflash**  
- Generation tokens: 233  
- Input tokens (context): 6236  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.86s  
- Outcome: **working**  

### data-transform

**ilo / dsflash**  
- Generation tokens: 3517  
- Input tokens (context): 10436  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [3114]  
- Wall time: 16.68s  
- Outcome: **working**  

**nanolang / dsflash**  
- Generation tokens: 1480  
- Input tokens (context): 13072  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [724]  
- Wall time: 7.61s  
- Outcome: **working**  

### tool-interaction

**ilo / dsflash**  
- Generation tokens: 2639  
- Input tokens (context): 20835  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [1458, 191, 163]  
- Wall time: 14.49s  
- Outcome: **working**  

**nanolang / dsflash**  
- Generation tokens: 444  
- Input tokens (context): 6242  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.44s  
- Outcome: **working**  

### workflow-rollback

**ilo / dsflash**  
- Generation tokens: 4310  
- Input tokens (context): 15518  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [301, 3040]  
- Wall time: 20.64s  
- Outcome: **working**  

**nanolang / dsflash**  
- Generation tokens: 382  
- Input tokens (context): 6247  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.2s  
- Outcome: **working**  

### hard-recursion

**ilo / dsflash**  
- Generation tokens: 1700  
- Input tokens (context): 15622  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [222, 630]  
- Wall time: 9.4s  
- Outcome: **working**  

**nanolang / dsflash**  
- Generation tokens: 383  
- Input tokens (context): 6233  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.95s  
- Outcome: **working**  

### hard-records

**ilo / dsflash**  
- Generation tokens: 34252  
- Input tokens (context): 28904  
- Attempts total: 5  
- Attempts to success: 5  
- Repair tokens by turn: [1009, 5113, 11878, 13855]  
- Wall time: 156.02s  
- Outcome: **working**  

**nanolang / dsflash**  
- Generation tokens: 5250  
- Input tokens (context): 27385  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [422, 1864, 2416]  
- Wall time: 22.01s  
- Outcome: **working**  

### hard-text-regex

**ilo / dsflash**  
- Generation tokens: 995  
- Input tokens (context): 10118  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [267]  
- Wall time: 6.75s  
- Outcome: **working**  

**nanolang / dsflash**  
- Generation tokens: 687  
- Input tokens (context): 6226  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.01s  
- Outcome: **working**  

### pipeline-report

**ilo / dsflash**  
- Generation tokens: 52504  
- Input tokens (context): 30251  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [9948, 13699, 21794, 4332]  
- Wall time: 220.44s  
- Outcome: **failed**  

**nanolang / dsflash**  
- Generation tokens: 31040  
- Input tokens (context): 21704  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [3049, 750]  
- Wall time: 117.13s  
- Outcome: **working**  

### text-analysis

**ilo / dsflash**  
- Generation tokens: 27129  
- Input tokens (context): 30773  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [1051, 10899, 2537, 996]  
- Wall time: 114.19s  
- Outcome: **failed**  

**nanolang / dsflash**  
- Generation tokens: 10212  
- Input tokens (context): 6334  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 37.01s  
- Outcome: **working**  

### gcd-lcm

**ilo / dsflash**  
- Generation tokens: 7073  
- Input tokens (context): 16054  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [2350, 3595]  
- Wall time: 31.27s  
- Outcome: **working**  

**nanolang / dsflash**  
- Generation tokens: 347  
- Input tokens (context): 6277  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.05s  
- Outcome: **working**  

### prime-count

**ilo / dsflash**  
- Generation tokens: 10451  
- Input tokens (context): 10263  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [4640]  
- Wall time: 44.96s  
- Outcome: **working**  

**nanolang / dsflash**  
- Generation tokens: 726  
- Input tokens (context): 6282  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.33s  
- Outcome: **working**  

### fizzbuzz-range

**ilo / dsflash**  
- Generation tokens: 6972  
- Input tokens (context): 10357  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [3389]  
- Wall time: 29.98s  
- Outcome: **working**  

**nanolang / dsflash**  
- Generation tokens: 662  
- Input tokens (context): 6336  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 28.11s  
- Outcome: **working**  

### flatten-sum

**ilo / dsflash**  
- Generation tokens: 2050  
- Input tokens (context): 16073  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [616, 1135]  
- Wall time: 10.29s  
- Outcome: **working**  

**nanolang / dsflash**  
- Generation tokens: 2351  
- Input tokens (context): 20317  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [323, 321]  
- Wall time: 10.86s  
- Outcome: **working**  

### dedupe-order

**ilo / dsflash**  
- Generation tokens: 2148  
- Input tokens (context): 26542  
- Attempts total: 5  
- Attempts to success: 5  
- Repair tokens by turn: [346, 366, 698, 202]  
- Wall time: 12.85s  
- Outcome: **working**  

**nanolang / dsflash**  
- Generation tokens: 5607  
- Input tokens (context): 20724  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [3041, 414]  
- Wall time: 21.4s  
- Outcome: **working**  

### run-length-encode

**ilo / dsflash**  
- Generation tokens: 19236  
- Input tokens (context): 16021  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [13350, 389]  
- Wall time: 82.67s  
- Outcome: **working**  

**nanolang / dsflash**  
- Generation tokens: 658  
- Input tokens (context): 6279  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.83s  
- Outcome: **working**  

### top-words

**ilo / dsflash**  
- Generation tokens: 11333  
- Input tokens (context): 16240  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [8120, 532]  
- Wall time: 52.6s  
- Outcome: **working**  

**nanolang / dsflash**  
- Generation tokens: 4273  
- Input tokens (context): 13107  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [419]  
- Wall time: 15.81s  
- Outcome: **working**  

### cron-expand

**ilo / dsflash**  
- Generation tokens: 13764  
- Input tokens (context): 16075  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [2961, 3801]  
- Wall time: 62.88s  
- Outcome: **working**  

**nanolang / dsflash**  
- Generation tokens: 12822  
- Input tokens (context): 6287  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 50.47s  
- Outcome: **working**  

### record-summary

**ilo / dsflash**  
- Generation tokens: 9218  
- Input tokens (context): 10506  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [3640]  
- Wall time: 40.24s  
- Outcome: **working**  

**nanolang / dsflash**  
- Generation tokens: 5371  
- Input tokens (context): 6363  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 19.28s  
- Outcome: **working**  

### csv-aggregate

**ilo / dsflash**  
- Generation tokens: 12198  
- Input tokens (context): 22225  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [5712, 342, 709]  
- Wall time: 53.61s  
- Outcome: **working**  

**nanolang / dsflash**  
- Generation tokens: 12882  
- Input tokens (context): 6341  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 46.78s  
- Outcome: **working**  

### log-level-count

**ilo / dsflash**  
- Generation tokens: 11261  
- Input tokens (context): 29586  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [1524, 1820, 5939, 515]  
- Wall time: 50.95s  
- Outcome: **failed**  

**nanolang / dsflash**  
- Generation tokens: 1016  
- Input tokens (context): 6327  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 4.39s  
- Outcome: **working**  

### weekday-of-date

**ilo / dsflash**  
- Generation tokens: 4852  
- Input tokens (context): 10272  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [436]  
- Wall time: 19.97s  
- Outcome: **working**  

**nanolang / dsflash**  
- Generation tokens: 1479  
- Input tokens (context): 6274  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 5.44s  
- Outcome: **working**  

### least-squares-slope

**ilo / dsflash**  
- Generation tokens: 2240  
- Input tokens (context): 10423  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [339]  
- Wall time: 10.34s  
- Outcome: **working**  

**nanolang / dsflash**  
- Generation tokens: 3207  
- Input tokens (context): 6305  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 12.73s  
- Outcome: **working**  

### kmeans-assign

**ilo / dsflash**  
- Generation tokens: 5854  
- Input tokens (context): 10774  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [3202]  
- Wall time: 24.56s  
- Outcome: **working**  

**nanolang / dsflash**  
- Generation tokens: 2988  
- Input tokens (context): 6371  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 11.91s  
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
