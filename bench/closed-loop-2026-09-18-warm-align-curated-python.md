# Closed-loop benchmark: ilo vs python per-task economics

Generated: 2026-09-18 20:13 UTC  
Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  
Retry cap: 5

## Summary

This table shows per-task economics across languages and models.
Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.

| task | ilo/dsflash: gen | inp | att | time | outcome | python/dsflash: gen | inp | att | time | outcome |
|---|---|---|---|---|---|---|---|---|---|---|
| simple-function | 2194 | 15413 | 3 | 11.2s | working | 97 | 173 | 1 | 1.27s | working |
| with-dependencies | 852 | 10147 | 2 | 5.12s | working | 79 | 178 | 1 | 1.01s | working |
| data-transform | 6365 | 10437 | 2 | 28.26s | working | 153 | 181 | 1 | 1.42s | working |
| tool-interaction | 1544 | 15446 | 3 | 26.13s | working | 113 | 184 | 1 | 1.03s | working |
| workflow-rollback | 4194 | 10250 | 2 | 18.95s | working | 361 | 458 | 2 | 3.15s | working |
| hard-recursion | 2321 | 15419 | 3 | 11.93s | working | 116 | 175 | 1 | 1.3s | working |
| hard-records | 18879 | 28365 | - | 82.68s | failed | 281 | 198 | 1 | 1.74s | working |
| hard-text-regex | 2418 | 10120 | 2 | 12.82s | working | 130 | 168 | 1 | 1.51s | working |
| pipeline-report | 18390 | 30746 | - | 79.26s | failed | 359 | 283 | 1 | 2.07s | working |
| text-analysis | 6571 | 16376 | 3 | 26.95s | working | 582 | 234 | 1 | 2.6s | working |
| gcd-lcm | 6144 | 27187 | - | 30.11s | failed | 149 | 177 | 1 | 1.38s | working |
| prime-count | 8971 | 28078 | - | 40.69s | failed | 327 | 182 | 1 | 1.71s | working |
| fizzbuzz-range | 2981 | 10372 | 2 | 13.59s | working | 326 | 236 | 1 | 1.87s | working |
| flatten-sum | 2698 | 10367 | 2 | 12.42s | working | 136 | 203 | 1 | 1.19s | working |
| dedupe-order | 2688 | 15968 | 3 | 14.1s | working | 189 | 188 | 1 | 1.44s | working |
| run-length-encode | 22018 | 28377 | 5 | 95.37s | working | 138 | 179 | 1 | 1.25s | working |
| top-words | 16073 | 29093 | - | 73.12s | partial | 163 | 192 | 1 | 1.38s | working |
| cron-expand | 12069 | 29917 | - | 52.78s | failed | 240 | 187 | 1 | 1.8s | working |
| record-summary | 20546 | 23206 | 4 | 88.26s | working | 376 | 263 | 1 | 1.94s | working |
| csv-aggregate | 21594 | 23247 | 4 | 93.58s | working | 265 | 241 | 1 | 1.52s | working |
| log-level-count | 22098 | 23387 | 4 | 95.97s | working | 167 | 227 | 1 | 1.54s | working |
| weekday-of-date | 4467 | 27478 | 5 | 20.83s | working | 660 | 174 | 1 | 2.7s | working |
| least-squares-slope | 8020 | 22901 | 4 | 35.98s | working | 489 | 205 | 1 | 2.55s | working |
| kmeans-assign | 17099 | 10825 | 2 | 70.27s | working | 383 | 271 | 1 | 2.03s | working |

## Per-task details

### simple-function

**ilo / dsflash**  
- Generation tokens: 2194  
- Input tokens (context): 15413  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [268, 1614]  
- Wall time: 11.2s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 97  
- Input tokens (context): 173  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.27s  
- Outcome: **working**  

### with-dependencies

**ilo / dsflash**  
- Generation tokens: 852  
- Input tokens (context): 10147  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [330]  
- Wall time: 5.12s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 79  
- Input tokens (context): 178  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.01s  
- Outcome: **working**  

### data-transform

**ilo / dsflash**  
- Generation tokens: 6365  
- Input tokens (context): 10437  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [5739]  
- Wall time: 28.26s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 153  
- Input tokens (context): 181  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.42s  
- Outcome: **working**  

### tool-interaction

**ilo / dsflash**  
- Generation tokens: 1544  
- Input tokens (context): 15446  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [1156, 97]  
- Wall time: 26.13s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 113  
- Input tokens (context): 184  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.03s  
- Outcome: **working**  

### workflow-rollback

**ilo / dsflash**  
- Generation tokens: 4194  
- Input tokens (context): 10250  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [3361]  
- Wall time: 18.95s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 361  
- Input tokens (context): 458  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [295]  
- Wall time: 3.15s  
- Outcome: **working**  

### hard-recursion

**ilo / dsflash**  
- Generation tokens: 2321  
- Input tokens (context): 15419  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [169, 1090]  
- Wall time: 11.93s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 116  
- Input tokens (context): 175  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.3s  
- Outcome: **working**  

### hard-records

**ilo / dsflash**  
- Generation tokens: 18879  
- Input tokens (context): 28365  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [1216, 1593, 3304, 9505]  
- Wall time: 82.68s  
- Outcome: **failed**  

**python / dsflash**  
- Generation tokens: 281  
- Input tokens (context): 198  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.74s  
- Outcome: **working**  

### hard-text-regex

**ilo / dsflash**  
- Generation tokens: 2418  
- Input tokens (context): 10120  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [1695]  
- Wall time: 12.82s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 130  
- Input tokens (context): 168  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.51s  
- Outcome: **working**  

### pipeline-report

**ilo / dsflash**  
- Generation tokens: 18390  
- Input tokens (context): 30746  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [3377, 4023, 1233, 6264]  
- Wall time: 79.26s  
- Outcome: **failed**  

**python / dsflash**  
- Generation tokens: 359  
- Input tokens (context): 283  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.07s  
- Outcome: **working**  

### text-analysis

**ilo / dsflash**  
- Generation tokens: 6571  
- Input tokens (context): 16376  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [1256, 2779]  
- Wall time: 26.95s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 582  
- Input tokens (context): 234  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.6s  
- Outcome: **working**  

### gcd-lcm

**ilo / dsflash**  
- Generation tokens: 6144  
- Input tokens (context): 27187  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [235, 903, 2546, 1039]  
- Wall time: 30.11s  
- Outcome: **failed**  

**python / dsflash**  
- Generation tokens: 149  
- Input tokens (context): 177  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.38s  
- Outcome: **working**  

### prime-count

**ilo / dsflash**  
- Generation tokens: 8971  
- Input tokens (context): 28078  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [1782, 2358, 1259, 350]  
- Wall time: 40.69s  
- Outcome: **failed**  

**python / dsflash**  
- Generation tokens: 327  
- Input tokens (context): 182  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.71s  
- Outcome: **working**  

### fizzbuzz-range

**ilo / dsflash**  
- Generation tokens: 2981  
- Input tokens (context): 10372  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [735]  
- Wall time: 13.59s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 326  
- Input tokens (context): 236  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.87s  
- Outcome: **working**  

### flatten-sum

**ilo / dsflash**  
- Generation tokens: 2698  
- Input tokens (context): 10367  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [1877]  
- Wall time: 12.42s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 136  
- Input tokens (context): 203  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.19s  
- Outcome: **working**  

### dedupe-order

**ilo / dsflash**  
- Generation tokens: 2688  
- Input tokens (context): 15968  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [1703, 734]  
- Wall time: 14.1s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 189  
- Input tokens (context): 188  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.44s  
- Outcome: **working**  

### run-length-encode

**ilo / dsflash**  
- Generation tokens: 22018  
- Input tokens (context): 28377  
- Attempts total: 5  
- Attempts to success: 5  
- Repair tokens by turn: [2822, 7092, 2089, 482]  
- Wall time: 95.37s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 138  
- Input tokens (context): 179  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.25s  
- Outcome: **working**  

### top-words

**ilo / dsflash**  
- Generation tokens: 16073  
- Input tokens (context): 29093  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [4730, 7227, 992, 307]  
- Wall time: 73.12s  
- Outcome: **partial**  

**python / dsflash**  
- Generation tokens: 163  
- Input tokens (context): 192  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.38s  
- Outcome: **working**  

### cron-expand

**ilo / dsflash**  
- Generation tokens: 12069  
- Input tokens (context): 29917  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [1804, 1153, 2413, 1438]  
- Wall time: 52.78s  
- Outcome: **failed**  

**python / dsflash**  
- Generation tokens: 240  
- Input tokens (context): 187  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.8s  
- Outcome: **working**  

### record-summary

**ilo / dsflash**  
- Generation tokens: 20546  
- Input tokens (context): 23206  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [1624, 11760, 819]  
- Wall time: 88.26s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 376  
- Input tokens (context): 263  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.94s  
- Outcome: **working**  

### csv-aggregate

**ilo / dsflash**  
- Generation tokens: 21594  
- Input tokens (context): 23247  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [5618, 399, 6051]  
- Wall time: 93.58s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 265  
- Input tokens (context): 241  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.52s  
- Outcome: **working**  

### log-level-count

**ilo / dsflash**  
- Generation tokens: 22098  
- Input tokens (context): 23387  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [4355, 5671, 6904]  
- Wall time: 95.97s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 167  
- Input tokens (context): 227  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.54s  
- Outcome: **working**  

### weekday-of-date

**ilo / dsflash**  
- Generation tokens: 4467  
- Input tokens (context): 27478  
- Attempts total: 5  
- Attempts to success: 5  
- Repair tokens by turn: [588, 479, 1144, 369]  
- Wall time: 20.83s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 660  
- Input tokens (context): 174  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.7s  
- Outcome: **working**  

### least-squares-slope

**ilo / dsflash**  
- Generation tokens: 8020  
- Input tokens (context): 22901  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [1532, 4590, 403]  
- Wall time: 35.98s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 489  
- Input tokens (context): 205  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.55s  
- Outcome: **working**  

### kmeans-assign

**ilo / dsflash**  
- Generation tokens: 17099  
- Input tokens (context): 10825  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [10500]  
- Wall time: 70.27s  
- Outcome: **working**  

**python / dsflash**  
- Generation tokens: 383  
- Input tokens (context): 271  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.03s  
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
