---
name: ilo-engines
description: Use this when choosing between VM, JIT, or AOT execution. Covers the feature matrix, default behaviour, and when each backend matters.
---

# ilo execution engines

Three public backends. Default (`ilo file.ilo`) is the register VM; covers ~all programs at strong speed. Pick a specific engine only with a reason.

## Engines

| Engine | Flag | Speed | Notes |
|--|--|--|--|
| VM | `--run-vm` | 10-100x | Default. Captures run natively. |
| Cranelift JIT | `--jit` | 100-1000x | Opt-in for hot numeric loops; bails to VM on unsupported. |
| Cranelift AOT | `ilo compile` | 100-1000x | Standalone native (~9 MB). |
| LLVM JIT | `--run-llvm` | ~Cranelift | Behind `llvm` feature. Rarely needed. |

The tree-walking interpreter is internal-only as of 0.12.1; `--run-tree` and `--run` were removed from the public CLI and now error with the unknown-flag guard. The interpreter stays in-tree as the dispatch target for bridge ops (regex, fmt variadic, fmt2, rd/rdb/rdjl, sleep, ls/walk/glob/run, env-all, jkeys, ct, rsrt, and the closure-bind ctx variants of map/flt/fld/srt); the VM bails to it transparently. Cross-engine parity is pinned by the bridge regression tests.

## When to pick which

- **VM** default.
- **`--jit`** tight numeric loops; `--bench` confirms it ran.
- **`ilo compile`** shipping or running without the toolchain.

## Feature matrix

All three public backends support core ops, lists/maps/records/sums, HOFs, lambdas (with or without captures), Results, HTTP, JSON, file I/O, MCP and HTTP tools. AOT miscompiles HOFs taking function values; use `--run-vm` for that case.

## Benchmarking

`ilo file.ilo --bench main args` runs VM and JIT on the same input and reports per-engine `perCallNs`; JIT cold-start washes out in the hot loop. Add `--json` for one envelope per engine: `{"schemaVersion":1,"engine":"vm|jit","variant":?,"result":...,"iterations":...,"totalMs":...,"perCallNs":...}`. VM emits two records (`variant: "fresh"` and `"reusable"`). AOT timed via `ilo compile ... && ./prog args`.

## AOT

`ilo compile prog.ilo [-o out] [main] [--bench]`. Output statically linked, ~9 MB, host-arch native. Top-level Result contract matches the source byte-for-byte.
