# Codegen Layer Design

WIP design doc for ilo's pluggable codegen layer. Not yet implemented. Tracks the architecture, decisions, and open questions as the refactor progresses.

## Goal

Decouple ilo's frontend (lex + parse + verify) from the codegen step so multiple backends can consume the same intermediate representation without duplicating frontend work.

## Why

Today ilo has one codegen path: Cranelift. The compiler also has a Python emitter, but it walks the AST directly, separate from the Cranelift pipeline. Adding more backends (WASM Component Model, transpile-to-Zero, transpile-to-C) without an abstraction means duplicating frontend handling per backend.

A typed intermediate representation (HIR) + a `Backend` trait makes each new backend small additive work.

## Non-goals

- **Not a modular runtime split.** `libilo.a` stays monolithic for now. Backend modularity is the work; runtime modularity is a separate, deferred project.
- **Not a Cranelift replacement.** Cranelift AOT stays. It becomes the first concrete `Backend` implementation. Default behaviour of `ilo build file.ilo` is unchanged.
- **Not user-visible.** From the user's perspective, `ilo build` keeps working. New backends opt in via `--backend X`.

## Architecture

```
ilo source
   ↓
[Lexer]
   ↓
[Parser]
   ↓
[AST]
   ↓
[Verifier]
   ↓
[Lowering pass]
   ↓
[HIR]
   ↓
[Backend trait]
   ↓ ↓ ↓ ↓ ↓
   Cranelift / Python / WASM / Zero / C ...
```

### HIR

A typed intermediate representation. Sits between the verified AST and concrete code emission. Stable shape that any backend can consume.

Open question: how much should HIR differ from the typed AST? Two camps:

- **Thin HIR**: typed AST + a few desugarings (guards lowered, pipes inlined). Easy to build. Backends still walk a tree.
- **Lower HIR**: SSA-style or three-address-code. More work to build, but easier for low-level backends (Cranelift, WASM) and for analyses (escape, linearity).

Recommendation for v1: thin HIR. Lower-HIR is an optimisation we can layer in later.

### Backend trait

```rust
pub trait Backend {
    /// Backend identifier used in CLI: `--backend cranelift`, `--backend wasm`, etc.
    const NAME: &'static str;

    /// Configuration the backend accepts (target, profile, output path, etc.).
    type Config;

    /// Produce the output artefact from the HIR.
    fn emit(&self, hir: &Hir, config: Self::Config) -> Result<Artefact, BackendError>;
}
```

Open question: should `emit` return a `Path` (write to disk and return location) or `Vec<u8>` (raw bytes that the caller writes)? Probably `Path`, since some backends invoke external tools (`cc`, `zero build`, `wasm-opt`).

### Concrete backends (planned)

| Backend | Status | Output | Strategy |
| --- | --- | --- | --- |
| `cranelift` | refactor existing into trait | native binary | unchanged default behaviour |
| `python` | refactor existing into trait | `.py` source | preserve `--emit python` form |
| `wasm` | new | `.wasm` + `.wit` | WASM Component Model |
| `zero` | new | `.0` source | transpile, invoke `zero build` |
| `c` | new (optional, later) | `.c` source | transpile, invoke `cc` |

### CLI surface (after refactor)

```
ilo build file.ilo                          # default: cranelift backend (unchanged)
ilo build file.ilo --backend wasm           # WASM Component Model
ilo build file.ilo --backend zero           # transpile to Zero
ilo build file.ilo --backend python         # transpile to Python
ilo build file.ilo --backend cranelift      # explicit form of default

ilo backends                                # list available backends
ilo backends --json                         # JSON-structured listing
```

## Phasing

1. **Define HIR** as Rust types in `src/hir/`. Build a lowering pass from typed AST to HIR. Round-trip test: AST → HIR → execute via current interpreter, results match.

2. **Define `Backend` trait** in `src/backend/mod.rs`. Initial stubs only.

3. **Refactor Cranelift into the trait**. Move `src/vm/compile_cranelift.rs` to `src/backend/cranelift/`. Implement `Backend`. Verify `ilo build` still produces an identical binary.

4. **Refactor Python emit into the trait**. Move `src/codegen/python.rs` to `src/backend/python/`. Implement `Backend`. Verify `--emit python` still produces identical output.

5. **Wire CLI**: `ilo build --backend X`. Default unchanged when flag omitted.

6. **Tests**: golden-file outputs for each backend on a corpus of small programs. Catches regressions during the refactor.

After this scaffolding ships, new backends (`wasm`, `zero`) are additive: implement the trait, register in the CLI, write tests.

## Open questions

- HIR shape: thin vs lower. Defer until the verifier output is more clearly factored.
- Config plumbing: each backend has different config (cranelift target triple, zero profile, wasm component vs raw, etc.). Type-erased config via `Box<dyn Any>` vs per-backend strongly typed? Lean towards strongly typed per backend, with a CLI dispatcher that parses args appropriately.
- Error type: `BackendError` should be JSON-serialisable so `ilo build --backend X --json` produces structured failure. Aligns with the JSON-output audit (adoption brief 3).
- Should backends compose? E.g., does the Zero backend invoke the WASM backend internally to produce a `.wasm` artefact from the transpiled `.0`? Probably not — each backend is responsible for its own pipeline, even if there's overlap. Composition is a later optimisation.

## Status

Stub only. No code beyond this design doc and an empty `mod.rs`. The actual refactor begins when Phase 4 (typed fix plans) ships and the CLI surface from Phase 1b is stable.

The brief for executing this work is at `/Users/dan/code/ilo-lang/zero-gap-specs/briefs/phase-5-codegen-layer-brief.md` (to be written).
