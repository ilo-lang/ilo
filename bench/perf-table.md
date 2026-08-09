<!-- generated 2026-08-08 from bench/results.json (release/26.8, Cranelift JIT, M2 Pro) -->

## Performance

Wall-clock time, median of 10 runs (lower is better).

| Benchmark | ilo-jit | ilo-vm | Python | ilo vs Python |
| --- | --- | --- | --- | --- |
| fib | 48.2 ms | 68.6 ms | 75.6 ms | **2x faster** |
| hof | 674.0 ms | 131.7 ms | 91.0 ms | 7.4x slower |
| listproc | 19.0 ms | 56.9 ms | 97.8 ms | **5x faster** |
| pattern-match | 62.0 ms | 117.3 ms | 158.3 ms | **3x faster** |
| sum-loop | 19.0 ms | 43.6 ms | 53.3 ms | **3x faster** |

**ilo wins 4/5 against Python on execution speed.**

Note: JIT performance regressed since the May baseline (listproc was 1.5ms, now 19ms; pattern-match was 3.1ms, now 62ms). The ratios vs Python are still favourable but the absolute JIT speed needs investigation. hof (closures) remains the one loss — closures bail JIT to tree interpreter.
