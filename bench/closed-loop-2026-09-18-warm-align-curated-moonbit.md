# Closed-loop benchmark: ilo vs moonbit per-task economics

Generated: 2026-09-18 21:17 UTC  
Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  
Retry cap: 5

## Summary

This table shows per-task economics across languages and models.
Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.

| task | ilo/dsflash: gen | inp | att | time | outcome | moonbit/dsflash: gen | inp | att | time | outcome |
|---|---|---|---|---|---|---|---|---|---|---|
| simple-function | 7638 | 15329 | 3 | 35.99s | working | 435 | 6093 | 2 | 3.32s | working |
| with-dependencies | 2710 | 26514 | 5 | 15.24s | working | 123 | 2777 | 1 | 1.35s | working |
| data-transform | 1546 | 10183 | 2 | 8.17s | working | 1894 | 2780 | 1 | 8.35s | working |
| tool-interaction | 793 | 15434 | 3 | 5.42s | working | 955 | 2783 | 1 | 5.4s | working |
| workflow-rollback | 1702 | 10235 | 2 | 8.83s | working | 461 | 6148 | 2 | 3.07s | working |
| hard-recursion | 3796 | 10188 | 2 | 16.75s | working | 287 | 2774 | 1 | 1.9s | working |
| hard-records | 11664 | 15821 | 3 | 47.41s | working | 1069 | 2797 | 1 | 4.12s | working |
| hard-text-regex | 1712 | 15365 | 3 | 9.35s | working | 1437 | 2767 | 1 | 6.03s | working |
| pipeline-report | 9003 | 10567 | 2 | 36.92s | working | 8424 | 6647 | 2 | 31.32s | working |
| text-analysis | 11399 | 10523 | 2 | 47.73s | working | 2958 | 6376 | 2 | 12.93s | working |
| gcd-lcm | 4624 | 10211 | 2 | 21.92s | working | 767 | 5804 | 2 | 4.32s | working |
| prime-count | 16281 | 27084 | - | 70.09s | failed | 450 | 2781 | 1 | 2.15s | working |
| fizzbuzz-range | 4414 | 21702 | 4 | 20.76s | working | 980 | 2835 | 1 | 4.2s | working |
| flatten-sum | 3882 | 16065 | 3 | 19.41s | working | 1659 | 2802 | 1 | 7.05s | working |
| dedupe-order | 9659 | 27825 | - | 46.3s | failed | 771 | 2787 | 1 | 3.95s | working |
| run-length-encode | 12528 | 15747 | 3 | 56.67s | working | 3563 | 5699 | 2 | 15.8s | working |
| top-words | 27747 | 28845 | - | 122.1s | failed | 5731 | 10827 | 3 | 24.35s | working |
| cron-expand | 12950 | 29354 | - | 74.43s | failed | 5139 | 2786 | 1 | 22.12s | working |
| record-summary | 8967 | 22929 | 4 | 40.58s | working | 1745 | 2862 | 1 | 7.73s | working |
| csv-aggregate | 9864 | 28820 | - | 43.59s | failed | 19266 | 21803 | - | 75.97s | partial |
| log-level-count | 8317 | 10624 | 2 | 35.65s | working | 631 | 2826 | 1 | 2.91s | working |
| weekday-of-date | 2967 | 10274 | 2 | 13.46s | working | 1504 | 2773 | 1 | 5.91s | working |
| least-squares-slope | 9121 | 16678 | 3 | 39.61s | working | 3523 | 2804 | 1 | 13.88s | working |
| kmeans-assign | 16861 | 10795 | 2 | 73.07s | working | 2749 | 2870 | 1 | 10.84s | working |

## Per-task details

### simple-function

**ilo / dsflash**  
- Generation tokens: 7638  
- Input tokens (context): 15329  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [5190, 1834]  
- Wall time: 35.99s  
- Outcome: **working**  

**moonbit / dsflash**  
- Generation tokens: 435  
- Input tokens (context): 6093  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [240]  
- Wall time: 3.32s  
- Outcome: **working**  

### with-dependencies

**ilo / dsflash**  
- Generation tokens: 2710  
- Input tokens (context): 26514  
- Attempts total: 5  
- Attempts to success: 5  
- Repair tokens by turn: [254, 1012, 334, 942]  
- Wall time: 15.24s  
- Outcome: **working**  

**moonbit / dsflash**  
- Generation tokens: 123  
- Input tokens (context): 2777  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.35s  
- Outcome: **working**  

### data-transform

**ilo / dsflash**  
- Generation tokens: 1546  
- Input tokens (context): 10183  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [884]  
- Wall time: 8.17s  
- Outcome: **working**  

**moonbit / dsflash**  
- Generation tokens: 1894  
- Input tokens (context): 2780  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 8.35s  
- Outcome: **working**  

### tool-interaction

**ilo / dsflash**  
- Generation tokens: 793  
- Input tokens (context): 15434  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [139, 309]  
- Wall time: 5.42s  
- Outcome: **working**  

**moonbit / dsflash**  
- Generation tokens: 955  
- Input tokens (context): 2783  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 5.4s  
- Outcome: **working**  

### workflow-rollback

**ilo / dsflash**  
- Generation tokens: 1702  
- Input tokens (context): 10235  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [935]  
- Wall time: 8.83s  
- Outcome: **working**  

**moonbit / dsflash**  
- Generation tokens: 461  
- Input tokens (context): 6148  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [78]  
- Wall time: 3.07s  
- Outcome: **working**  

### hard-recursion

**ilo / dsflash**  
- Generation tokens: 3796  
- Input tokens (context): 10188  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [1848]  
- Wall time: 16.75s  
- Outcome: **working**  

**moonbit / dsflash**  
- Generation tokens: 287  
- Input tokens (context): 2774  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.9s  
- Outcome: **working**  

### hard-records

**ilo / dsflash**  
- Generation tokens: 11664  
- Input tokens (context): 15821  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [2102, 356]  
- Wall time: 47.41s  
- Outcome: **working**  

**moonbit / dsflash**  
- Generation tokens: 1069  
- Input tokens (context): 2797  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 4.12s  
- Outcome: **working**  

### hard-text-regex

**ilo / dsflash**  
- Generation tokens: 1712  
- Input tokens (context): 15365  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [452, 999]  
- Wall time: 9.35s  
- Outcome: **working**  

**moonbit / dsflash**  
- Generation tokens: 1437  
- Input tokens (context): 2767  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 6.03s  
- Outcome: **working**  

### pipeline-report

**ilo / dsflash**  
- Generation tokens: 9003  
- Input tokens (context): 10567  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [4879]  
- Wall time: 36.92s  
- Outcome: **working**  

**moonbit / dsflash**  
- Generation tokens: 8424  
- Input tokens (context): 6647  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [3694]  
- Wall time: 31.32s  
- Outcome: **working**  

### text-analysis

**ilo / dsflash**  
- Generation tokens: 11399  
- Input tokens (context): 10523  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [3462]  
- Wall time: 47.73s  
- Outcome: **working**  

**moonbit / dsflash**  
- Generation tokens: 2958  
- Input tokens (context): 6376  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [213]  
- Wall time: 12.93s  
- Outcome: **working**  

### gcd-lcm

**ilo / dsflash**  
- Generation tokens: 4624  
- Input tokens (context): 10211  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [3399]  
- Wall time: 21.92s  
- Outcome: **working**  

**moonbit / dsflash**  
- Generation tokens: 767  
- Input tokens (context): 5804  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [360]  
- Wall time: 4.32s  
- Outcome: **working**  

### prime-count

**ilo / dsflash**  
- Generation tokens: 16281  
- Input tokens (context): 27084  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [7140, 2092, 362, 469]  
- Wall time: 70.09s  
- Outcome: **failed**  

**moonbit / dsflash**  
- Generation tokens: 450  
- Input tokens (context): 2781  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.15s  
- Outcome: **working**  

### fizzbuzz-range

**ilo / dsflash**  
- Generation tokens: 4414  
- Input tokens (context): 21702  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [1503, 362, 898]  
- Wall time: 20.76s  
- Outcome: **working**  

**moonbit / dsflash**  
- Generation tokens: 980  
- Input tokens (context): 2835  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 4.2s  
- Outcome: **working**  

### flatten-sum

**ilo / dsflash**  
- Generation tokens: 3882  
- Input tokens (context): 16065  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [1324, 1976]  
- Wall time: 19.41s  
- Outcome: **working**  

**moonbit / dsflash**  
- Generation tokens: 1659  
- Input tokens (context): 2802  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 7.05s  
- Outcome: **working**  

### dedupe-order

**ilo / dsflash**  
- Generation tokens: 9659  
- Input tokens (context): 27825  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [514, 401, 4025, 4435]  
- Wall time: 46.3s  
- Outcome: **failed**  

**moonbit / dsflash**  
- Generation tokens: 771  
- Input tokens (context): 2787  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.95s  
- Outcome: **working**  

### run-length-encode

**ilo / dsflash**  
- Generation tokens: 12528  
- Input tokens (context): 15747  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [3316, 161]  
- Wall time: 56.67s  
- Outcome: **working**  

**moonbit / dsflash**  
- Generation tokens: 3563  
- Input tokens (context): 5699  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [496]  
- Wall time: 15.8s  
- Outcome: **working**  

### top-words

**ilo / dsflash**  
- Generation tokens: 27747  
- Input tokens (context): 28845  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [2939, 3057, 2765, 15305]  
- Wall time: 122.1s  
- Outcome: **failed**  

**moonbit / dsflash**  
- Generation tokens: 5731  
- Input tokens (context): 10827  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [733, 340]  
- Wall time: 24.35s  
- Outcome: **working**  

### cron-expand

**ilo / dsflash**  
- Generation tokens: 12950  
- Input tokens (context): 29354  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [842, 2052, 329, 6311]  
- Wall time: 74.43s  
- Outcome: **failed**  

**moonbit / dsflash**  
- Generation tokens: 5139  
- Input tokens (context): 2786  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 22.12s  
- Outcome: **working**  

### record-summary

**ilo / dsflash**  
- Generation tokens: 8967  
- Input tokens (context): 22929  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [4330, 900, 427]  
- Wall time: 40.58s  
- Outcome: **working**  

**moonbit / dsflash**  
- Generation tokens: 1745  
- Input tokens (context): 2862  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 7.73s  
- Outcome: **working**  

### csv-aggregate

**ilo / dsflash**  
- Generation tokens: 9864  
- Input tokens (context): 28820  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [1807, 1644, 3923, 843]  
- Wall time: 43.59s  
- Outcome: **failed**  

**moonbit / dsflash**  
- Generation tokens: 19266  
- Input tokens (context): 21803  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [987, 481, 595, 478]  
- Wall time: 75.97s  
- Outcome: **partial**  

### log-level-count

**ilo / dsflash**  
- Generation tokens: 8317  
- Input tokens (context): 10624  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [7219]  
- Wall time: 35.65s  
- Outcome: **working**  

**moonbit / dsflash**  
- Generation tokens: 631  
- Input tokens (context): 2826  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.91s  
- Outcome: **working**  

### weekday-of-date

**ilo / dsflash**  
- Generation tokens: 2967  
- Input tokens (context): 10274  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [1652]  
- Wall time: 13.46s  
- Outcome: **working**  

**moonbit / dsflash**  
- Generation tokens: 1504  
- Input tokens (context): 2773  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 5.91s  
- Outcome: **working**  

### least-squares-slope

**ilo / dsflash**  
- Generation tokens: 9121  
- Input tokens (context): 16678  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [926, 5771]  
- Wall time: 39.61s  
- Outcome: **working**  

**moonbit / dsflash**  
- Generation tokens: 3523  
- Input tokens (context): 2804  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 13.88s  
- Outcome: **working**  

### kmeans-assign

**ilo / dsflash**  
- Generation tokens: 16861  
- Input tokens (context): 10795  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [12957]  
- Wall time: 73.07s  
- Outcome: **working**  

**moonbit / dsflash**  
- Generation tokens: 2749  
- Input tokens (context): 2870  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 10.84s  
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
