# WASM backend capability matrix

Phase 5 Stage 5d. Companion to `src/backend/wasm/` and the
`ilo build file.ilo --wasm` CLI form. Source of truth for which ilo
builtins are available on which WASM target. Capability mismatches surface
at emit time as `BackendError::CodegenFailed { code: "ILO-B201", .. }`.

The default target is `wasm32-component` (Component Model wrapper). Pick
another with `--target`.

## Targets

| Flag                                | Output                                              | When to use it                                              |
|-------------------------------------|-----------------------------------------------------|-------------------------------------------------------------|
| `--wasm` (no `--target`)            | `<name>.wasm` + `<name>.wit` (Component Model wrap) | Cloudflare Workers, Fastly Compute, Wasmtime, Wasmer        |
| `--wasm --target wasm32-wasip1`     | `<name>.wasm`                                       | Wasmtime / Wasmer with WASI preview1 host                   |
| `--wasm --target wasm32-wasip2`     | `<name>.wasm`                                       | Future: WASI preview2 host. Same wire format as p1 today.   |
| `--wasm --target wasm32-unknown-unknown` (alias `wasm32-web`) | `<name>.wasm`            | Browser / host-provided shim. No WASI host imports.         |

## Builtin support

| Builtin   | wasm32-wasip1 | wasm32-component (default) | wasm32-unknown-unknown |
|-----------|:-------------:|:--------------------------:|:----------------------:|
| `prnt`    | yes (stdout)  | yes (`wasi:cli/stdout`)    | no — `ILO-B201`        |
| `now`     | yes           | yes (`wasi:clocks`)        | no                     |
| `now-ms`  | yes           | yes                        | no                     |
| `env`     | yes           | yes (`wasi:cli/environment`)| no                    |
| `rd`      | yes (WASI fs) | yes (`wasi:filesystem`)    | no                     |
| `wr`      | yes (WASI fs) | yes (`wasi:filesystem`)    | no                     |
| `get`     | partial       | yes (`wasi:http`)          | no                     |
| `post`    | partial       | yes (`wasi:http`)          | no                     |
| `run`     | no            | no                         | no                     |
| Pure ops  | yes           | yes                        | yes                    |

**Notes.**
- "Pure ops" covers arithmetic, list/map operations, comparisons, lambda
  capture, and any other ilo expression that has no host dependency.
- `run` (subprocess spawn) is unsupported on every WASM target. Use the
  native Cranelift backend (drop `--wasm`).
- `wasm32-unknown-unknown` provides no host imports — embedding the wasm in
  a JavaScript/Rust host that injects callbacks is on the user.

## Error shape

Trying to use an unsupported builtin on a target surfaces at emit time, not
at run time:

```
WASM compile error: builtin `prnt` is not supported on wasm32-unknown-unknown.
hint: use --target wasm32-wasip1 or --target wasm32-component (default).
`prnt` needs WASI host imports.
```

The structured form for `ilo build --json`:

```json
{
  "kind": "codegen_failed",
  "code": "ILO-B201",
  "message": "builtin `prnt` is not supported on wasm32-unknown-unknown. hint: ..."
}
```

Error codes are namespaced per backend: `ILO-B1##` Cranelift, `ILO-B2##`
WASM, `ILO-B3##` Zero (future), `ILO-B4##` Python (future). The WASM range:

| Code       | Meaning                                                |
|------------|--------------------------------------------------------|
| `ILO-B201` | Builtin not supported on the chosen WASM target        |
| `ILO-B202` | HIR construct not yet lowered by the WASM backend      |
| `ILO-B203` | `wasm-tools component new` subprocess failure          |
| `ILO-B204` | IO failure writing artefact (`.wasm` / `.wit`)         |
| `ILO-B205` | Entry function not found                               |

## What Stage 5d covers

Stage 5d ships the hello-world subset: top-level `prnt` calls with string,
number, or bool literal arguments. Anything else (arithmetic, branching,
loops, lambdas, user-defined helpers, list/map operations as side effects)
returns `BackendError::UnsupportedFeature` and the user is steered at the
native Cranelift backend. The HIR walker grows incrementally over the
subsequent stages.

## Component Model wrap

`wasm-tools component new` is invoked as a subprocess (not linked into
`libilo.a`). It requires the WASI preview1 adapter, which ships in-tree
at `assets/wasi-adapter/wasi_snapshot_preview1.reactor.wasm` (~52KB,
pinned to the Wasmtime v25 release). The adapter is written to a temp
file at build time and passed via `--adapt wasi_snapshot_preview1=...`.

The sibling `.wit` is auto-generated from the entry function name and
declares an exported `run: func()` plus an imported `wasi:cli/stdout`.
Future stages widen this as more capabilities come online.

## Cloudflare Workers

Cloudflare Workers accepts WASM Component Model components. The path is:

1. `ilo build my-handler.ilo --wasm` produces `my-handler.wasm` +
   `my-handler.wit`.
2. Drop those next to a `wrangler.toml` that points at the wasm module.
3. `wrangler deploy` — done.

`examples/wasm-edge/` ships a starter showing the full layout. Stage 5d
treats Cloudflare deploy as a manual smoke test, not CI-gated; the
component output is valid Component Model wasm and Wasmtime-runnable,
which is the strong constraint.

## Toolchain pins

| Tool        | Version    | Source                              |
|-------------|------------|-------------------------------------|
| wasmtime    | 44.0.1     | Homebrew (subprocess test runner)   |
| wasm-tools  | 1.249.0    | Homebrew (subprocess Component wrap)|
| wasm-encoder | 0.249     | crates.io (library; emit)           |
| wasmparser  | 0.249      | crates.io (dev-dep validator)       |
| WASI adapter | Wasmtime v25 reactor build | bundled in `assets/wasi-adapter/` |

All three crate-level deps are version-locked to the `wasm-tools 1.249` line.
