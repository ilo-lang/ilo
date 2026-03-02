# JIT Compilation: From 67ns to 2ns

The ilo register VM runs `tot` at ~67ns/call. LuaJIT does ~1ns, V8 does ~18ns. The remaining gap is dispatch overhead — even with type-specialized opcodes, each instruction still pays for a u32 decode + match branch. JIT compilation eliminates dispatch entirely by emitting native machine code.

We built three JIT backends to compare approaches.

## The test function

```
tot p:n q:n r:n>n;s=*p q;t=*s r;+s t
```

This computes `p*q + p*q*r`. At each level of the stack:

```
ilo source:    tot p:n q:n r:n>n;s=*p q;t=*s r;+s t
VM bytecode:   MUL_NN R3,R0,R1 | MUL_NN R4,R3,R2 | ADD_NN R5,R3,R4 | RET R5
ARM64 native:  fmul d3,d0,d1   | fmul d4,d3,d2   | fadd d0,d3,d4   | ret
```

4 native instructions. No dispatch, no type checks, no stack manipulation.

## Three backends

### 1. Custom JIT (arm64)

Raw AArch64 machine code. No compiler framework, no IR, no dependencies beyond `libc` for `mmap`. VM registers R0-R30 map 1:1 to hardware FP registers d0-d30. Function args arrive in d0-d7 per AAPCS64 — perfectly aligned with VM params, so zero shuffling.

The emitter walks VM bytecode and outputs 32-bit ARM64 encodings directly:

```rust
fn arm64_fmul(rd: u8, rn: u8, rm: u8) -> u32 {
    0x1E600800 | ((rm as u32) << 16) | ((rn as u32) << 5) | rd as u32
}
```

Constants are embedded as literal data after the code section, loaded via PC-relative `ADR`+`LDR`. Memory is allocated with `mmap(MAP_JIT)`, written, then flipped to executable with `mprotect`. On macOS this uses `pthread_jit_write_protect_np` for W^X compliance.

**aarch64 only.** Always available on Apple Silicon, no feature flags needed.

### 2. Cranelift

The compiler framework used by Wasmtime. Translates VM bytecode to Cranelift IR — each opcode becomes one IR instruction (`MUL_NN` → `ins().fmul()`). Cranelift handles register allocation and instruction selection automatically.

```toml
cargo build --features cranelift
```

**Cross-platform.** Works on ARM64 and x86_64. Zero external dependencies (pure Rust). Fast compilation, slightly less optimized output than LLVM.

### 3. LLVM (via inkwell)

The backend behind rustc, Clang, Swift, and Julia. Same translation pattern as Cranelift but targeting LLVM IR. Brings the heaviest optimization pipeline (equivalent to `clang -O2`).

```toml
cargo build --features llvm
```

**Requires LLVM 18 installed.** Cross-platform. Heaviest dependency but most optimized output for complex functions.

## Eligibility

JIT only kicks in for pure-numeric functions — all params typed `:n`, only arithmetic/comparison ops + return. The eligibility check walks the bytecode:

```
Eligible: ADD_NN, SUB_NN, MUL_NN, DIV_NN, ADDK_N, SUBK_N, MULK_N, DIVK_N,
          LOADK (number), MOVE, NEG, RET
```

Non-eligible functions (strings, records, lists, control flow, function calls) fall back to the interpreter. No silent failures — you get a clear error if you try to JIT a non-numeric function.

## Benchmarks

All measurements on Apple M4 Pro, `cargo build --release --features cranelift`, `tot(10, 20, 30)` = 6200, 10k iterations after warmup.

### ilo backends

| Backend | Per call | vs Interpreter |
|---------|----------|----------------|
| Rust interpreter | 1,383ns | 1.0x |
| Register VM | 129ns | 10.7x faster |
| Register VM (reusable) | 66ns | 20.9x faster |
| Python transpiled | 80ns | 17.3x faster |
| **Custom JIT (arm64)** | **2ns** | **691x faster** |
| **Cranelift JIT** | **2ns** | **691x faster** |

### External runtimes — interpreted

| Runtime | Per call |
|---------|----------|
| CPython | 80ns |
| Ruby | 42ns |
| PHP | 35ns |
| Lua | 28ns |

### External runtimes — JIT

| Runtime | Per call |
|---------|----------|
| Node.js / V8 | 18ns |
| LuaJIT | 1ns |
| PyPy3 | 117ns |

### External runtimes — AOT (compiled)

| Runtime | Per call |
|---------|----------|
| Go | 2ns |
| C (cc -O2) | 0.4ns |
| Rust (rustc -O) | 0.5ns |

### The full stack

| Layer | Per call | Speedup over previous |
|-------|----------|-----------------------|
| Rust interpreter | 1,383ns | — |
| Register VM | 129ns | 10.7x |
| Register VM (reusable) | 66ns | 2.0x |
| Custom JIT / Cranelift | 2ns | 33x |

Total speedup from interpreter to JIT: **~690x**.

The Custom JIT (arm64) and Cranelift backends produce essentially identical performance for this function — both emit the same 4 floating-point instructions. ilo's JIT backends match Go at ~2ns and are within 2x of LuaJIT (~1ns). Only C and Rust AOT beat them (0.4-0.5ns), where the compiler can eliminate the function call entirely.

## Usage

```bash
# Run with a specific backend
./ilo example.ilo --run-jit tot 10 20 30         # ARM64 (aarch64 only)
./ilo example.ilo --run-cranelift tot 10 20 30    # Cranelift
./ilo example.ilo --run-llvm tot 10 20 30         # LLVM

# Benchmark all available backends
./ilo example.ilo --bench tot 10 20 30
```

## File layout

```
src/vm/
  mod.rs            — opcode constants (pub(crate)), NanVal, VM interpreter
  jit_arm64.rs      — hand-rolled ARM64 emitter (#[cfg(target_arch = "aarch64")])
  jit_cranelift.rs  — Cranelift backend (#[cfg(feature = "cranelift")])
  jit_llvm.rs       — LLVM/inkwell backend (#[cfg(feature = "llvm")])
```

## What's next

The JIT currently handles the simplest case — straight-line numeric code. Extensions:

- **Branching** — `if`/`match` via ARM64 conditional branches or Cranelift block parameters
- **Loops** — `foreach` with loop-back edges (Cranelift makes this easy with its block/SSA model)
- **Function calls** — inline small callees or emit proper call sequences
- **Caching** — compile once per function, reuse across calls (currently the benchmark does this, but `--run-jit` recompiles each time)
