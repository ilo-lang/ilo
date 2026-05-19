---
name: ilo-engines
description: Use this when choosing between tree, VM, JIT, or AOT execution. Covers the feature matrix, default behaviour, and when each backend matters.
---

# ilo execution engines

Four backends. Default (`ilo file.ilo`) is the register VM; covers ~all programs at strong speed. Pick a specific engine only with a reason.

## Engines

| Engine | Flag | Speed | Notes |
|--|--|--|--|
| Tree-walk | `--run-tree` | 1x | Canonical-semantics reference. |
| VM | `--run-vm` | 10-100x | Default. Native closures. |
| Cranelift JIT | `--jit` | 100-1000x | Opt-in for hot numeric loops; VM fallback on bailout. |
| Cranelift AOT | `ilo compile` | 100-1000x | Standalone native (~9.7 MB). Native closures via embedded `CompiledProgram`. |
| LLVM JIT | `--run-llvm` | ~Cranelift | Behind `llvm` feature. Rarely needed. |

`--run` aliases `--run-tree`.

## When to pick which

- **VM** default.
- **`--run-tree`** reference semantics for debugging. Captures run natively on VM/JIT.
- **`--jit`** tight numeric loops; `--bench` confirms it ran.
- **`ilo compile`** shipping or running without the toolchain.

## Feature matrix

All four support core ops, lists/maps/records/sums, HOFs, lambdas (with or without captures), Results, HTTP, JSON, file I/O, MCP and HTTP tools.

## Benchmarking

`ilo file.ilo --bench main args` runs tree, vm, jit on the same input and reports per-engine `perCallNs`; JIT cold-start washes out in the hot loop. Add `--json` for one envelope per engine: `{"schemaVersion":1,"engine":"tree|vm|jit","variant":?,"result":...,"iterations":...,"totalMs":...,"perCallNs":...}`. VM emits two records (`variant: "fresh"` and `"reusable"`). AOT timed via `ilo compile ... && ./prog args`.

## AOT

`ilo compile prog.ilo [-o out] [main] [--bench]`. Output statically linked, ~9 MB, host-arch native. Top-level Result contract matches the source byte-for-byte.
