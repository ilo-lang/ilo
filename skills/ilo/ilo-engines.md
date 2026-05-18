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
- **`--run-tree`.** You're using a closure that captures from an enclosing scope (`flt (x:n>b;>x thr) xs` where `thr` is outer). VM/Cranelift detect captures and fall back automatically with no error, so this flag is rarely needed; pass it only to force-pin the engine for debugging.
- **`--jit`.** Tight numeric loops (Mandelbrot, n-body, hot fold over millions of items). The JIT bails out to VM on unsupported features without warning, so use `--bench` to compare and confirm you're getting JIT speed.
- **`ilo compile`.** Shipping a binary, or running on a system without the ilo toolchain. Output is large; cold-start is ~zero.

## Feature/backend matrix

All four backends support: core ops, lists/maps/records/sums, HOFs, inline non-capturing lambdas, Results, HTTP, JSON, file I/O, MCP tools, HTTP tool provider.

Only the tree-walker runs **capturing** lambdas directly; VM, JIT, AOT detect captures and silently recompile the affected fn down to the tree-walker on first hit. No `ILO-R012` unless the call site is genuinely undefined.

## Benchmarking

```
ilo file.ilo --bench main args         All engines, text
ilo file.ilo --bench main args --json  All engines, JSON (one line per engine)
ilo compile file.ilo -o ./prog && ./prog args   AOT (build then time)
```

`--bench` runs tree, vm, jit on the same input; JIT cold-start washes out in the hot loop. JSON envelope: `{"schemaVersion":1,"engine":"tree|vm|jit","variant":?,"result":...,"iterations":...,"totalMs":...,"perCallNs":...}`. VM emits two records (`variant: "fresh"` and `"reusable"`).

## AOT specifics

```
ilo compile prog.ilo                   ./prog
ilo compile prog.ilo -o out main       Custom name + entry fn
ilo compile prog.ilo --bench           Bench-mode binary
```

The output is statically linked, ~9 MB, runs on the host arch. Top-level Result contract matches the source: `~v` -> stdout + exit 0, `^e` -> stderr + exit 1.
