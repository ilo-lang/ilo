<!-- generated 2026-08-08 from bench/results.json (release/26.8, Cranelift JIT, M2 Pro) -->

## Performance

Wall-clock time, median of 10 runs (lower is better).

| Benchmark | ilo-jit | ilo-vm | Python | Node | ilo vs Python |
|-----------|---------|--------|--------|------|--------------|
| fib | 40.0 ms | 60.2 ms | 64.7 ms | 3.6 ms | **1.6x faster** (JIT) |
| hof | — | 122.5 ms | 87.9 ms | 37.9 ms | 1.4x slower (VM) |
| listproc | 18.7 ms | 56.5 ms | 97.1 ms | 1.4 ms | **5.2x faster** (JIT) |
| pattern-match | 61.2 ms | 116.0 ms | 155.2 ms | 1.8 ms | **2.5x faster** (JIT) |
| sum-loop | 18.7 ms | 42.8 ms | 53.3 ms | 916 µs | **2.8x faster** (JIT) |

**ilo wins 4/5 against Python.** hof uses VM fallback (closures bail JIT).
All pairwise CIs non-overlapping (statistically significant).
