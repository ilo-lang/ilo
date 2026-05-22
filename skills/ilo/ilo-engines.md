---
name: ilo-engines
description: Use this when choosing between VM, JIT, or AOT execution. Covers the feature matrix, default behaviour, and when each backend matters.
---

# ilo execution engines

Two public backends. Default (`ilo file.@`) is the register VM; covers ~all programs at strong speed. Pick the JIT only with a reason.

## Engines

| Engine | Flag | Speed | Notes |
|--|--|--|--|
| VM | `--vm` | 10-100x | Default. Captures run natively. |
| Cranelift JIT | `--jit` | 100-1000x | Opt-in for hot numeric loops; bails to VM on unsupported. |
| Cranelift AOT | `ilo compile` | 100-1000x | Standalone native (~9 MB). |
| LLVM JIT | `--run-llvm` | ~Cranelift | Behind `llvm` feature. Rarely needed. |

Tree-walker removed as a user-selectable engine in 0.13.0. The shared runtime module (`src/runtime/`) stays in-tree as the VM's bail target for ~30 builtins routed through the tree-bridge: regex, fmt variadic, IO, sleep, ct/rsrt, closure-bind-ctx HOFs, crypto, calendar arithmetic.

## When to pick which

- **VM** default.
- **`--jit`** tight numeric loops; `--bench` confirms it ran.
- **`ilo compile`** shipping or running without the toolchain.

## Feature matrix

All three public backends support core ops, lists/maps/records/sums, HOFs, lambdas (with or without captures), Results, HTTP, JSON, file I/O, MCP and HTTP tools.

## Benchmarking

`ilo file.@ --bench main args` runs VM and JIT, reports per-engine `perCallNs`. `--json` for one envelope per engine. AOT timed via `ilo compile ... && ./prog args`.

## AOT

`ilo compile prog.@ [-o out] [main] [--bench]`. Output ~9 MB, host-arch native. Top-level Result contract matches source byte-for-byte.
