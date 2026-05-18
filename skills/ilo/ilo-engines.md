---
name: ilo-engines
description: Use this when choosing between VM, JIT, or AOT execution. Covers the feature matrix, default behaviour, and when each backend matters.
---

# ilo execution engines

ilo has three public execution backends. The default (`ilo file.ilo`) runs the register VM, which covers ~all programs at strong speed. Pick a specific engine only when you have a reason.

## Engines

| Engine     | Flag         | Speed       | Notes                                     |
|------------|--------------|-------------|-------------------------------------------|
| VM         | `--run-vm`   | 10-100x baseline | Default. Covers every opcode. Bails internally to the tree-walker for a small set of HOF / regex / IO shapes the VM hasn't lifted natively yet. |
| Cranelift JIT | `--jit`   | 100-1000x   | Opt-in for hot numeric loops. Falls back to VM on bailout. |
| Cranelift AOT | `ilo compile` | 100-1000x | Standalone native binary (~9 MB). |
| LLVM JIT   | `--run-llvm` | similar to Cranelift | Behind `llvm` feature. Rarely needed. |

The tree-walking interpreter is no longer user-selectable: `--run-tree` / `--run` were removed from the public CLI in the 0.12.x soft-deprecation. The tree-walker stays in-tree as the internal runtime for HOF callbacks the VM/Cranelift haven't lifted natively (`map`/`flt`/`fld`/`srt`/`rsrt` with a ctx arg, `rgx`/`rgxall`/`rgxall1`, variadic `fmt`, 2-arg `rd`/`rdb`, 1-arg `sleep`, 2/3-arg `ct`/`rsrt`). The VM bails to it transparently; agents do not need to know it exists. Full removal is deferred to 0.13.0+ once the remaining bridge consumers are migrated natively.

## Default

`ilo file.ilo [args...]` runs the VM. This is the right choice >95% of the time. The default has been VM since v0.11 (see PR #390).

## When to pick which

- **Default (VM).** Anything that doesn't have a specific reason to be elsewhere. Closure captures, every HOF shape, regex, fmt, file IO all work; the VM bails to the internal tree-walker transparently for the few shapes it hasn't lifted yet.
- **`--jit`.** Tight numeric loops (Mandelbrot, n-body, hot fold over millions of items). The JIT bails out to VM on unsupported features without warning, so use `--bench` to compare and confirm you're getting JIT speed.
- **`ilo compile`.** Shipping a binary, or running on a system without the ilo toolchain. Output is large; cold-start is ~zero.

## Feature/backend matrix

All public backends support: core ops, lists/maps/records/sums, HOFs, inline non-capturing lambdas, capturing lambdas (Phase 2 closures), Results, HTTP, JSON, file I/O, MCP tools, HTTP tool provider.

## Benchmarking

```
ilo file.ilo --bench main args...      VM (default), reports ns/op
ilo file.ilo --jit --bench main args   JIT
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
