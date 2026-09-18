# Closed-loop benchmark: ilo vs ailang per-task economics

Generated: 2026-09-18 21:19 UTC  
Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  
Retry cap: 5

## Summary

This table shows per-task economics across languages and models.
Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.

| task | ilo/dsflash: gen | inp | att | time | outcome | ailang/dsflash: gen | inp | att | time | outcome |
|---|---|---|---|---|---|---|---|---|---|---|
| simple-function | 3930 | 10150 | 2 | 18.81s | working | 668 | 3281 | 1 | 4.81s | working |
| with-dependencies | 2369 | 15419 | 3 | 13.08s | working | 670 | 3286 | 1 | 3.69s | working |
| data-transform | 3370 | 10436 | 2 | 15.81s | working | 1700 | 3289 | 1 | 7.67s | working |
| tool-interaction | 2955 | 15442 | 3 | 38.74s | working | 2148 | 6719 | 2 | 10.91s | working |
| workflow-rollback | 1565 | 10188 | 2 | 29.38s | working | 16954 | 3297 | 1 | 77.6s | working |
| hard-recursion | 3106 | 15524 | 3 | 14.3s | working | 713 | 3283 | 1 | 3.6s | working |
| hard-records | 30814 | 29727 | - | 135.75s | failed | 6364 | 6854 | 2 | 28.48s | working |
| hard-text-regex | 971 | 10120 | 2 | 6.09s | working | 1086 | 3276 | 1 | 5.82s | working |
| pipeline-report | 43809 | 31344 | - | 184.29s | failed | 23292 | 23347 | 5 | 97.59s | working |
| text-analysis | 14844 | 10446 | 2 | 61.54s | working | 9693 | 7165 | 2 | 38.51s | working |
| gcd-lcm | 2697 | 10196 | 2 | 13.05s | working | 4283 | 6848 | 2 | 18.48s | working |
| prime-count | 17116 | 28416 | 5 | 75.37s | working | 5800 | 6972 | 2 | 25.45s | working |
| fizzbuzz-range | 9784 | 28980 | - | 44.72s | failed | 2304 | 6986 | 2 | 12.01s | working |
| flatten-sum | 2924 | 16013 | 3 | 14.69s | working | 3396 | 3353 | 1 | 17.11s | working |
| dedupe-order | 1672 | 10174 | 2 | 8.7s | working | 3994 | 6977 | 2 | 16.56s | working |
| run-length-encode | 20172 | 28120 | - | 84.18s | failed | 6303 | 11007 | 3 | 27.86s | working |
| top-words | 16888 | 28238 | 5 | 77.8s | working | 2172 | 3342 | 1 | 9.93s | working |
| cron-expand | 27116 | 29606 | 5 | 118.88s | working | 1412 | 3337 | 1 | 7.36s | working |
| record-summary | 9031 | 10461 | 2 | 41.95s | working | 19599 | 7249 | 2 | 83.28s | working |
| csv-aggregate | 8001 | 10550 | 2 | 36.39s | working | 15066 | 11635 | 3 | 61.69s | working |
| log-level-count | 10476 | 23109 | 4 | 71.66s | working | 11205 | 7128 | 2 | 49.39s | working |
| weekday-of-date | 9124 | 28675 | - | 40.05s | failed | 3084 | 3324 | 1 | 12.89s | working |
| least-squares-slope | 5629 | 16100 | 3 | 25.35s | working | 1301 | 3355 | 1 | 6.1s | working |
| kmeans-assign | 11430 | 10861 | 2 | 44.62s | working | 4536 | 7377 | 2 | 16.92s | working |

## Per-task details

### simple-function

**ilo / dsflash**  
- Generation tokens: 3930  
- Input tokens (context): 10150  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [3071]  
- Wall time: 18.81s  
- Outcome: **working**  

**ailang / dsflash**  
- Generation tokens: 668  
- Input tokens (context): 3281  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 4.81s  
- Outcome: **working**  

### with-dependencies

**ilo / dsflash**  
- Generation tokens: 2369  
- Input tokens (context): 15419  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [759, 988]  
- Wall time: 13.08s  
- Outcome: **working**  

**ailang / dsflash**  
- Generation tokens: 670  
- Input tokens (context): 3286  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.69s  
- Outcome: **working**  

### data-transform

**ilo / dsflash**  
- Generation tokens: 3370  
- Input tokens (context): 10436  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [2867]  
- Wall time: 15.81s  
- Outcome: **working**  

**ailang / dsflash**  
- Generation tokens: 1700  
- Input tokens (context): 3289  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 7.67s  
- Outcome: **working**  

### tool-interaction

**ilo / dsflash**  
- Generation tokens: 2955  
- Input tokens (context): 15442  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [2672, 65]  
- Wall time: 38.74s  
- Outcome: **working**  

**ailang / dsflash**  
- Generation tokens: 2148  
- Input tokens (context): 6719  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [366]  
- Wall time: 10.91s  
- Outcome: **working**  

### workflow-rollback

**ilo / dsflash**  
- Generation tokens: 1565  
- Input tokens (context): 10188  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [570]  
- Wall time: 29.38s  
- Outcome: **working**  

**ailang / dsflash**  
- Generation tokens: 16954  
- Input tokens (context): 3297  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 77.6s  
- Outcome: **working**  

### hard-recursion

**ilo / dsflash**  
- Generation tokens: 3106  
- Input tokens (context): 15524  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [587, 1561]  
- Wall time: 14.3s  
- Outcome: **working**  

**ailang / dsflash**  
- Generation tokens: 713  
- Input tokens (context): 3283  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.6s  
- Outcome: **working**  

### hard-records

**ilo / dsflash**  
- Generation tokens: 30814  
- Input tokens (context): 29727  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [8246, 268, 3069, 16880]  
- Wall time: 135.75s  
- Outcome: **failed**  

**ailang / dsflash**  
- Generation tokens: 6364  
- Input tokens (context): 6854  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [2002]  
- Wall time: 28.48s  
- Outcome: **working**  

### hard-text-regex

**ilo / dsflash**  
- Generation tokens: 971  
- Input tokens (context): 10120  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [617]  
- Wall time: 6.09s  
- Outcome: **working**  

**ailang / dsflash**  
- Generation tokens: 1086  
- Input tokens (context): 3276  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 5.82s  
- Outcome: **working**  

### pipeline-report

**ilo / dsflash**  
- Generation tokens: 43809  
- Input tokens (context): 31344  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [27561, 7875, 1276, 4924]  
- Wall time: 184.29s  
- Outcome: **failed**  

**ailang / dsflash**  
- Generation tokens: 23292  
- Input tokens (context): 23347  
- Attempts total: 5  
- Attempts to success: 5  
- Repair tokens by turn: [3685, 811, 1189, 451]  
- Wall time: 97.59s  
- Outcome: **working**  

### text-analysis

**ilo / dsflash**  
- Generation tokens: 14844  
- Input tokens (context): 10446  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [11472]  
- Wall time: 61.54s  
- Outcome: **working**  

**ailang / dsflash**  
- Generation tokens: 9693  
- Input tokens (context): 7165  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [1566]  
- Wall time: 38.51s  
- Outcome: **working**  

### gcd-lcm

**ilo / dsflash**  
- Generation tokens: 2697  
- Input tokens (context): 10196  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [1141]  
- Wall time: 13.05s  
- Outcome: **working**  

**ailang / dsflash**  
- Generation tokens: 4283  
- Input tokens (context): 6848  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [709]  
- Wall time: 18.48s  
- Outcome: **working**  

### prime-count

**ilo / dsflash**  
- Generation tokens: 17116  
- Input tokens (context): 28416  
- Attempts total: 5  
- Attempts to success: 5  
- Repair tokens by turn: [5540, 2410, 1817, 4054]  
- Wall time: 75.37s  
- Outcome: **working**  

**ailang / dsflash**  
- Generation tokens: 5800  
- Input tokens (context): 6972  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [338]  
- Wall time: 25.45s  
- Outcome: **working**  

### fizzbuzz-range

**ilo / dsflash**  
- Generation tokens: 9784  
- Input tokens (context): 28980  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [1186, 1280, 3310, 3887]  
- Wall time: 44.72s  
- Outcome: **failed**  

**ailang / dsflash**  
- Generation tokens: 2304  
- Input tokens (context): 6986  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [363]  
- Wall time: 12.01s  
- Outcome: **working**  

### flatten-sum

**ilo / dsflash**  
- Generation tokens: 2924  
- Input tokens (context): 16013  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [411, 1383]  
- Wall time: 14.69s  
- Outcome: **working**  

**ailang / dsflash**  
- Generation tokens: 3396  
- Input tokens (context): 3353  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 17.11s  
- Outcome: **working**  

### dedupe-order

**ilo / dsflash**  
- Generation tokens: 1672  
- Input tokens (context): 10174  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [798]  
- Wall time: 8.7s  
- Outcome: **working**  

**ailang / dsflash**  
- Generation tokens: 3994  
- Input tokens (context): 6977  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [1424]  
- Wall time: 16.56s  
- Outcome: **working**  

### run-length-encode

**ilo / dsflash**  
- Generation tokens: 20172  
- Input tokens (context): 28120  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [11642, 2795, 1531, 943]  
- Wall time: 84.18s  
- Outcome: **failed**  

**ailang / dsflash**  
- Generation tokens: 6303  
- Input tokens (context): 11007  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [316, 323]  
- Wall time: 27.86s  
- Outcome: **working**  

### top-words

**ilo / dsflash**  
- Generation tokens: 16888  
- Input tokens (context): 28238  
- Attempts total: 5  
- Attempts to success: 5  
- Repair tokens by turn: [7062, 3260, 2341, 543]  
- Wall time: 77.8s  
- Outcome: **working**  

**ailang / dsflash**  
- Generation tokens: 2172  
- Input tokens (context): 3342  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 9.93s  
- Outcome: **working**  

### cron-expand

**ilo / dsflash**  
- Generation tokens: 27116  
- Input tokens (context): 29606  
- Attempts total: 5  
- Attempts to success: 5  
- Repair tokens by turn: [2742, 1306, 8231, 399]  
- Wall time: 118.88s  
- Outcome: **working**  

**ailang / dsflash**  
- Generation tokens: 1412  
- Input tokens (context): 3337  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 7.36s  
- Outcome: **working**  

### record-summary

**ilo / dsflash**  
- Generation tokens: 9031  
- Input tokens (context): 10461  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [2119]  
- Wall time: 41.95s  
- Outcome: **working**  

**ailang / dsflash**  
- Generation tokens: 19599  
- Input tokens (context): 7249  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [838]  
- Wall time: 83.28s  
- Outcome: **working**  

### csv-aggregate

**ilo / dsflash**  
- Generation tokens: 8001  
- Input tokens (context): 10550  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [4037]  
- Wall time: 36.39s  
- Outcome: **working**  

**ailang / dsflash**  
- Generation tokens: 15066  
- Input tokens (context): 11635  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [1337, 2953]  
- Wall time: 61.69s  
- Outcome: **working**  

### log-level-count

**ilo / dsflash**  
- Generation tokens: 10476  
- Input tokens (context): 23109  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [1119, 2838, 1661]  
- Wall time: 71.66s  
- Outcome: **working**  

**ailang / dsflash**  
- Generation tokens: 11205  
- Input tokens (context): 7128  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [559]  
- Wall time: 49.39s  
- Outcome: **working**  

### weekday-of-date

**ilo / dsflash**  
- Generation tokens: 9124  
- Input tokens (context): 28675  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [1055, 981, 3694, 650]  
- Wall time: 40.05s  
- Outcome: **failed**  

**ailang / dsflash**  
- Generation tokens: 3084  
- Input tokens (context): 3324  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 12.89s  
- Outcome: **working**  

### least-squares-slope

**ilo / dsflash**  
- Generation tokens: 5629  
- Input tokens (context): 16100  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [1220, 3403]  
- Wall time: 25.35s  
- Outcome: **working**  

**ailang / dsflash**  
- Generation tokens: 1301  
- Input tokens (context): 3355  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 6.1s  
- Outcome: **working**  

### kmeans-assign

**ilo / dsflash**  
- Generation tokens: 11430  
- Input tokens (context): 10861  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [2285]  
- Wall time: 44.62s  
- Outcome: **working**  

**ailang / dsflash**  
- Generation tokens: 4536  
- Input tokens (context): 7377  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [1314]  
- Wall time: 16.92s  
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
