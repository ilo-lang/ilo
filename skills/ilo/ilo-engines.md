---
name: ilo-engines
description: Use this when choosing between tree, VM, JIT, or AOT execution. Covers the feature matrix, default behaviour, and when each backend matters.
---

# ilo execution engines

Four backends. Default (`ilo file.ilo`) is the register VM; covers ~all programs at strong speed. Pick a specific engine only with a reason.

## Engines

| Engine | Flag | Speed | Notes |
|--|--|--|--|
| Tree-walk | `--run-tree` | 1x | Feature-complete. Required for capturing lambdas. |
| VM | `--run-vm` | 10-100x | Default. Captures auto fall-back. |
| Cranelift JIT | `--jit` | 100-1000x | Opt-in for hot numeric loops; bails to VM on unsupported. |
| Cranelift AOT | `ilo compile` | 100-1000x | Standalone native (~9 MB). |
| LLVM JIT | `--run-llvm` | ~Cranelift | Behind `llvm` feature. Rarely needed. |

`--run` aliases `--run-tree`.

## When to pick which

<<<<<<< HEAD
- **Default (VM).** Anything that doesn't have a specific reason to be elsewhere.
- **`--run-tree`.** Reference semantics for debugging, or when you want to pin to the canonical interpreter. Phase 2 closure capture (`flt (x:n>b;>x thr) xs` where `thr` is outer) runs natively on the VM and JIT too, no engine pinning needed for captures any more.
- **`--jit`.** Tight numeric loops (Mandelbrot, n-body, hot fold over millions of items). The JIT bails out to VM on unsupported features without warning, so use `--bench` to compare and confirm you're getting JIT speed.
- **`ilo compile`.** Shipping a binary, or running on a system without the ilo toolchain. Output is large; cold-start is ~zero.
=======
- **VM** default.
- **`--run-tree`** force-pin for debugging (captures auto-fall-back).
- **`--jit`** tight numeric loops; `--bench` confirms it ran.
- **`ilo compile`** shipping or running without the toolchain.
>>>>>>> e8d1ecc437f0632f933bf5bfba17c9bfa06d0067

## Feature matrix

<<<<<<< HEAD
All four backends support: core ops, lists/maps/records/sums, HOFs, inline lambdas (Phase 1 non-capturing and Phase 2 capturing), Results, HTTP, JSON, file I/O, MCP tools, HTTP tool provider.

Phase 2 closure capture (lambda references an outer variable) runs natively on tree, VM, and JIT: free vars snapshot by value at the call site. AOT currently miscompiles HOFs taking function values; use `--run-vm` for that case.
=======
All four support core ops, lists/maps/records/sums, HOFs, non-capturing lambdas, Results, HTTP, JSON, file I/O, MCP and HTTP tools. Capturing lambdas: tree only; VM/JIT/AOT silently recompile the affected fn to tree on first hit.
>>>>>>> e8d1ecc437f0632f933bf5bfba17c9bfa06d0067

## Benchmarking

`ilo file.ilo --bench main args` runs tree, vm, jit on the same input and reports per-engine `perCallNs`; JIT cold-start washes out in the hot loop. Add `--json` for one envelope per engine: `{"schemaVersion":1,"engine":"tree|vm|jit","variant":?,"result":...,"iterations":...,"totalMs":...,"perCallNs":...}`. VM emits two records (`variant: "fresh"` and `"reusable"`). AOT timed via `ilo compile ... && ./prog args`.

## AOT

`ilo compile prog.ilo [-o out] [main] [--bench]`. Output statically linked, ~9 MB, host-arch native. Top-level Result contract matches the source byte-for-byte.
