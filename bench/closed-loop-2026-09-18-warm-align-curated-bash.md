# Closed-loop benchmark: ilo vs bash per-task economics

Generated: 2026-09-18 20:32 UTC  
Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  
Retry cap: 5

## Summary

This table shows per-task economics across languages and models.
Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.

| task | ilo/dsflash: gen | inp | att | time | outcome | bash/dsflash: gen | inp | att | time | outcome |
|---|---|---|---|---|---|---|---|---|---|---|
| simple-function | 1738 | 5002 | 1 | 8.05s | working | 11646 | 1412 | 4 | 53.16s | working |
| with-dependencies | 7918 | 21465 | 4 | 37.37s | working | 174 | 178 | 1 | 1.42s | working |
| data-transform | 6207 | 16042 | 3 | 29.67s | working | 100 | 181 | 1 | 1.04s | working |
| tool-interaction | 1179 | 15455 | 3 | 8.0s | working | 217 | 184 | 1 | 1.71s | working |
| workflow-rollback | 4696 | 26804 | 5 | 24.46s | working | 192 | 189 | 1 | 1.48s | working |
| hard-recursion | 2683 | 10167 | 2 | 12.04s | working | 284 | 175 | 1 | 2.6s | working |
| hard-records | 17970 | 27647 | 4 | 84.06s | working | 261 | 198 | 1 | 1.61s | working |
| hard-text-regex | 1190 | 10128 | 2 | 7.65s | working | 772 | 168 | 1 | 3.94s | working |
| pipeline-report | 29074 | 30116 | 5 | 119.95s | working | 383 | 283 | 1 | 2.1s | working |
| text-analysis | 9320 | 10704 | 2 | 38.61s | working | 919 | 234 | 1 | 3.49s | working |
| gcd-lcm | 6692 | 27650 | 5 | 31.51s | working | 148 | 177 | 1 | 1.19s | working |
| prime-count | 4982 | 21531 | 4 | 24.82s | working | 473 | 182 | 1 | 2.46s | working |
| fizzbuzz-range | 13956 | 28495 | 5 | 61.07s | working | 249 | 236 | 1 | 1.47s | working |
| flatten-sum | 1503 | 10364 | 2 | 7.52s | working | 243 | 203 | 1 | 1.51s | working |
| dedupe-order | 2961 | 15467 | 3 | 14.86s | working | 236 | 188 | 1 | 1.43s | working |
| run-length-encode | 4291 | 16024 | 3 | 20.25s | working | 306 | 179 | 1 | 1.57s | working |
| top-words | 18109 | 15855 | 3 | 82.06s | working | 581 | 573 | 2 | 3.69s | working |
| cron-expand | 26056 | 30149 | - | 112.21s | failed | 481 | 187 | 1 | 2.37s | working |
| record-summary | 8049 | 16131 | 3 | 37.03s | working | 638 | 263 | 1 | 3.19s | working |
| csv-aggregate | 17795 | 10689 | 2 | 79.42s | working | 465 | 241 | 1 | 2.37s | working |
| log-level-count | 5611 | 16459 | 3 | 26.59s | working | 330 | 227 | 1 | 1.9s | working |
| weekday-of-date | 2329 | 15765 | 3 | 10.83s | working | 724 | 174 | 1 | 3.27s | working |
| least-squares-slope | 13822 | 10395 | 2 | 56.42s | working | 622 | 205 | 1 | 2.86s | working |
| kmeans-assign | 19163 | 10808 | 2 | 80.37s | working | 1137 | 271 | 1 | 4.53s | working |

## Per-task details

### simple-function

**ilo / dsflash**  
- Generation tokens: 1738  
- Input tokens (context): 5002  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 8.05s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 11646  
- Input tokens (context): 1412  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [230, 538, 10803]  
- Wall time: 53.16s  
- Outcome: **working**  

### with-dependencies

**ilo / dsflash**  
- Generation tokens: 7918  
- Input tokens (context): 21465  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [579, 3457, 3831]  
- Wall time: 37.37s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 174  
- Input tokens (context): 178  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.42s  
- Outcome: **working**  

### data-transform

**ilo / dsflash**  
- Generation tokens: 6207  
- Input tokens (context): 16042  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [4632, 153]  
- Wall time: 29.67s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 100  
- Input tokens (context): 181  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.04s  
- Outcome: **working**  

### tool-interaction

**ilo / dsflash**  
- Generation tokens: 1179  
- Input tokens (context): 15455  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [479, 78]  
- Wall time: 8.0s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 217  
- Input tokens (context): 184  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.71s  
- Outcome: **working**  

### workflow-rollback

**ilo / dsflash**  
- Generation tokens: 4696  
- Input tokens (context): 26804  
- Attempts total: 5  
- Attempts to success: 5  
- Repair tokens by turn: [3595, 313, 159, 569]  
- Wall time: 24.46s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 192  
- Input tokens (context): 189  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.48s  
- Outcome: **working**  

### hard-recursion

**ilo / dsflash**  
- Generation tokens: 2683  
- Input tokens (context): 10167  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [929]  
- Wall time: 12.04s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 284  
- Input tokens (context): 175  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.6s  
- Outcome: **working**  

### hard-records

**ilo / dsflash**  
- Generation tokens: 17970  
- Input tokens (context): 27647  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [7907, 6820, 1250]  
- Wall time: 84.06s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 261  
- Input tokens (context): 198  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.61s  
- Outcome: **working**  

### hard-text-regex

**ilo / dsflash**  
- Generation tokens: 1190  
- Input tokens (context): 10128  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [898]  
- Wall time: 7.65s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 772  
- Input tokens (context): 168  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.94s  
- Outcome: **working**  

### pipeline-report

**ilo / dsflash**  
- Generation tokens: 29074  
- Input tokens (context): 30116  
- Attempts total: 5  
- Attempts to success: 5  
- Repair tokens by turn: [2890, 432, 2707, 13347]  
- Wall time: 119.95s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 383  
- Input tokens (context): 283  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.1s  
- Outcome: **working**  

### text-analysis

**ilo / dsflash**  
- Generation tokens: 9320  
- Input tokens (context): 10704  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [5212]  
- Wall time: 38.61s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 919  
- Input tokens (context): 234  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.49s  
- Outcome: **working**  

### gcd-lcm

**ilo / dsflash**  
- Generation tokens: 6692  
- Input tokens (context): 27650  
- Attempts total: 5  
- Attempts to success: 5  
- Repair tokens by turn: [665, 1372, 2027, 1557]  
- Wall time: 31.51s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 148  
- Input tokens (context): 177  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.19s  
- Outcome: **working**  

### prime-count

**ilo / dsflash**  
- Generation tokens: 4982  
- Input tokens (context): 21531  
- Attempts total: 4  
- Attempts to success: 4  
- Repair tokens by turn: [1127, 389, 527]  
- Wall time: 24.82s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 473  
- Input tokens (context): 182  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.46s  
- Outcome: **working**  

### fizzbuzz-range

**ilo / dsflash**  
- Generation tokens: 13956  
- Input tokens (context): 28495  
- Attempts total: 5  
- Attempts to success: 5  
- Repair tokens by turn: [3205, 6636, 460, 410]  
- Wall time: 61.07s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 249  
- Input tokens (context): 236  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.47s  
- Outcome: **working**  

### flatten-sum

**ilo / dsflash**  
- Generation tokens: 1503  
- Input tokens (context): 10364  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [783]  
- Wall time: 7.52s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 243  
- Input tokens (context): 203  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.51s  
- Outcome: **working**  

### dedupe-order

**ilo / dsflash**  
- Generation tokens: 2961  
- Input tokens (context): 15467  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [377, 1938]  
- Wall time: 14.86s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 236  
- Input tokens (context): 188  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.43s  
- Outcome: **working**  

### run-length-encode

**ilo / dsflash**  
- Generation tokens: 4291  
- Input tokens (context): 16024  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [1746, 860]  
- Wall time: 20.25s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 306  
- Input tokens (context): 179  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.57s  
- Outcome: **working**  

### top-words

**ilo / dsflash**  
- Generation tokens: 18109  
- Input tokens (context): 15855  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [15200, 268]  
- Wall time: 82.06s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 581  
- Input tokens (context): 573  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [97]  
- Wall time: 3.69s  
- Outcome: **working**  

### cron-expand

**ilo / dsflash**  
- Generation tokens: 26056  
- Input tokens (context): 30149  
- Attempts total: 5  
- Attempts to success: None  
- Repair tokens by turn: [10190, 810, 6747, 3251]  
- Wall time: 112.21s  
- Outcome: **failed**  

**bash / dsflash**  
- Generation tokens: 481  
- Input tokens (context): 187  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.37s  
- Outcome: **working**  

### record-summary

**ilo / dsflash**  
- Generation tokens: 8049  
- Input tokens (context): 16131  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [2141, 1247]  
- Wall time: 37.03s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 638  
- Input tokens (context): 263  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.19s  
- Outcome: **working**  

### csv-aggregate

**ilo / dsflash**  
- Generation tokens: 17795  
- Input tokens (context): 10689  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [13981]  
- Wall time: 79.42s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 465  
- Input tokens (context): 241  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.37s  
- Outcome: **working**  

### log-level-count

**ilo / dsflash**  
- Generation tokens: 5611  
- Input tokens (context): 16459  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [3756, 262]  
- Wall time: 26.59s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 330  
- Input tokens (context): 227  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 1.9s  
- Outcome: **working**  

### weekday-of-date

**ilo / dsflash**  
- Generation tokens: 2329  
- Input tokens (context): 15765  
- Attempts total: 3  
- Attempts to success: 3  
- Repair tokens by turn: [659, 280]  
- Wall time: 10.83s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 724  
- Input tokens (context): 174  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 3.27s  
- Outcome: **working**  

### least-squares-slope

**ilo / dsflash**  
- Generation tokens: 13822  
- Input tokens (context): 10395  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [12829]  
- Wall time: 56.42s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 622  
- Input tokens (context): 205  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 2.86s  
- Outcome: **working**  

### kmeans-assign

**ilo / dsflash**  
- Generation tokens: 19163  
- Input tokens (context): 10808  
- Attempts total: 2  
- Attempts to success: 2  
- Repair tokens by turn: [13810]  
- Wall time: 80.37s  
- Outcome: **working**  

**bash / dsflash**  
- Generation tokens: 1137  
- Input tokens (context): 271  
- Attempts total: 1  
- Attempts to success: 1  
- Repair tokens by turn: []  
- Wall time: 4.53s  
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
