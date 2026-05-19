---
name: ilo-engines
description: Use this when choosing between VM, JIT, or AOT execution. Covers the feature matrix, default behaviour, and when each backend matters.
---

# ilo execution engines

Three public backends. Default (`ilo file.ilo`) is the register VM; covers ~all programs at strong speed. Pick another only with a reason.

## Engines

| Engine | Flag | Speed | Notes |
|--|--|--|--|
| VM | `--run-vm` | 10-100x | Default. Captures run natively. |
| Cranelift JIT | `--jit` | 100-1000x | Opt-in for hot numeric loops; bails to VM on unsupported. |
| Cranelift AOT | `ilo compile` | 100-1000x | Standalone native (~9 MB). |
| LLVM JIT | `--run-llvm` | ~Cranelift | Behind `llvm` feature. Rarely needed. |

Tree-walker is internal-only in 0.12.1: `--run-tree` / `--run` removed (unknown-flag error). Stays in-tree as the VM's bail target for regex, fmt variadic, IO, and closure-bind-ctx HOFs.

## When to pick which

- **VM** default.
- **`--jit`** tight numeric loops; `--bench` confirms it ran.
- **`ilo compile`** shipping or running without the toolchain.

## Feature matrix

All three public backends support core ops, lists/maps/records/sums, HOFs, lambdas (with or without captures), Results, HTTP, JSON, file I/O, MCP and HTTP tools.

## Benchmarking

`ilo file.ilo --bench main args` runs VM and JIT on the same input and reports per-engine `perCallNs`; JIT cold-start washes out in the hot loop. Add `--json` for one envelope per engine: `{"schemaVersion":1,"engine":"vm|jit","variant":?,"result":...,"iterations":...,"totalMs":...,"perCallNs":...}`. VM emits two records (`variant: "fresh"` and `"reusable"`). AOT timed via `ilo compile ... && ./prog args`.

## AOT

`ilo compile prog.ilo [-o out] [main] [--bench]`. Output ~9 MB, host-arch native. Top-level Result contract matches source byte-for-byte.
