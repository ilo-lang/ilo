---
name: ilo-engines
description: Use this when choosing between tree, VM, JIT, or AOT execution. Covers the feature matrix, default behaviour, and when each backend matters.
---

# ilo execution engines

ilo has four execution backends. The default (`ilo file.ilo`) runs the register VM, which covers ~all programs at strong speed. Pick a specific engine only when you have a reason.

## Engines

| Engine     | Flag         | Speed       | Notes                                     |
|------------|--------------|-------------|-------------------------------------------|
| Tree-walk  | `--run-tree` | 1x baseline | Feature-complete. Required for capturing lambdas. |
| VM         | `--run-vm`   | 10-100x     | Default. No capturing lambdas (auto fallback). |
| Cranelift JIT | `--jit`   | 100-1000x   | Opt-in for hot numeric loops. Falls back to VM on bailout. |
| Cranelift AOT | `ilo compile` | 100-1000x | Standalone native binary (~9 MB). |
| LLVM JIT   | `--run-llvm` | similar to Cranelift | Behind `llvm` feature. Rarely needed. |

`--run` is an alias for `--run-tree`.

## Default

`ilo file.ilo [args...]` runs the VM. This is the right choice >95% of the time. The default has been VM since v0.11 (see PR #390).

## When to pick which

- **Default (VM).** Anything that doesn't have a specific reason to be elsewhere.
- **`--run-tree`.** Reference semantics for debugging, or when you want to pin to the canonical interpreter. Phase 2 closure capture (`flt (x:n>b;>x thr) xs` where `thr` is outer) runs natively on the VM and JIT too, no engine pinning needed for captures any more.
- **`--jit`.** Tight numeric loops (Mandelbrot, n-body, hot fold over millions of items). The JIT bails out to VM on unsupported features without warning, so use `--bench` to compare and confirm you're getting JIT speed.
- **`ilo compile`.** Shipping a binary, or running on a system without the ilo toolchain. Output is large; cold-start is ~zero.

## Feature/backend matrix

All four backends support: core ops, lists/maps/records/sums, HOFs, inline lambdas (Phase 1 non-capturing and Phase 2 capturing), Results, HTTP, JSON, file I/O, MCP tools, HTTP tool provider.

Phase 2 closure capture (the lambda body references a variable from an enclosing scope) runs natively on tree, VM, and Cranelift JIT: free variables are snapshot by value at the call site and appended to the call frame. The AOT backend currently miscompiles HOFs that take a function value (including capturing closures) and is the only engine that still needs `--run-tree` or `--run-vm` for that case; tracked as a separate fix.

## Benchmarking

```
ilo file.ilo --bench main args...      VM (default), reports ns/op
ilo file.ilo --jit --bench main args   JIT
ilo file.ilo --run-tree --bench main   Tree
ilo compile file.ilo -o ./prog && ./prog args   AOT
```

JIT cold-start includes Cranelift compilation; for short programs that swamps the run time. Use `--bench` which loops the hot path.

## AOT specifics

```
ilo compile prog.ilo                   ./prog
ilo compile prog.ilo -o out main       Custom name + entry fn
ilo compile prog.ilo --bench           Bench-mode binary
```

The output is statically linked, ~9 MB, runs on the host arch. Top-level Result contract matches the source: `~v` -> stdout + exit 0, `^e` -> stderr + exit 1.
