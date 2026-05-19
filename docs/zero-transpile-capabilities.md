# Zero transpile capabilities

Reference matrix for the `ilo build --0` / `--0bin` Zero backend
(Phase 5 Stage 5e, ilo 0.13.0). Pinned toolchain: `zero 0.1.2`.

## Pinned toolchain

| Property | Value |
| --- | --- |
| Compiler version | `zero 0.1.2` |
| Pin file | `.zero-version` (repo root) |
| Host | darwin-arm64 (cross-target gated; see below) |
| Default install path | `/Users/dan/.zero/bin/zero` |
| Install one-liner | `curl https://zerolang.ai/install.sh \| sh` |

When the `zero` binary is missing from PATH and `/Users/dan/.zero/bin/zero`,
`--0bin` fails with `ILO-B303` and points at the install one-liner plus
the pinned version.

## Entry shape

Every transpiled Zero program emits the same `main` shape:

```zero
pub fun main(world: World) -> Void raises {
    check world.out.write("...\n")
}
```

`fn main()` is rejected by `zero check` 0.1.2 and is **not** emitted.

## Construct mapping

### Clean (1:1 or near-1:1) -- targeted across the lifetime of the backend

| ilo | Zero | Notes |
| --- | --- | --- |
| Number literal `42` | `42` | Same |
| String literal `"x"` | `"x"` | Escape sequences identical |
| Boolean `true` / `false` | `true` / `false` | Same |
| Arithmetic `+a b` | `a + b` | ilo prefix to Zero infix |
| Comparison `>a b` | `a > b` | Same |
| `?cond{a}{b}` ternary | `if cond { a } else { b }` | Direct lowering |
| `wh c{body}` while | `while c { body }` | Same |
| `@v xs{body}` foreach | `for v in xs { body }` | Same |
| `ret expr` | `return expr` | Same |
| Record decl | `struct` | One struct per ilo record |
| Sum decl | `enum` | One enum per ilo sum |
| Pipes `x>>f>>g` | `g(f(x))` | Lowered at HIR stage |
| `prnt "x"` | `check world.out.write("x\n")` | Stage 5e v1 supports this |

### Shim (works but needs RC-to-ownership translation)

| ilo | Zero shim |
| --- | --- |
| Shared list `L T` | Owned `Vec<T>` with explicit `.clone()` on multi-use |
| Shared map `M K V` | Owned `Map<K, V>` with explicit `.clone()` |
| Shared record passed to two functions | Insert `.clone()` at the second use site |

ilo's RC-by-construction model means any value can be referenced freely.
Zero's ownership model requires a single owner. The shim is: when HIR
shows a value used N times in N call sites, emit N-1 `.clone()` calls.
This loses some efficiency vs hand-written Zero but is correct.

### Unsupported (v1)

| ilo | Why | Error code |
| --- | --- | --- |
| Closures with capture | Zero closures don't capture by value in 0.1.2 | `ILO-B302` |
| Dynamic tool dispatch (`tool` decls invoked at runtime via MCP) | Zero has no runtime tool registry | `ILO-B302` |
| Lambdas with captured locals | Same closure-capture issue | `ILO-B302` |
| Higher-order `f a b` where `f` is a function value | Zero functions are not first-class values pre-1.0 | `ILO-B302` |

## Stage 5e v1 walker scope

The Stage 5e walker is intentionally narrow, matching the WASM
backend's hello-world subset. It supports:

- A top-level function as the entry
- A body that is a sequence of `prnt "<literal>"` expression statements
  (text, number, or bool literal arguments)
- An optional tail expression that is `~v` (Ok) or a bare literal --
  these are no-ops at the Zero boundary

Anything outside the subset surfaces as `BackendError::CodegenFailed`
with an `ILO-B3##` code and a hint pointing at the Cranelift native
backend. The walker is widened in later stages as HIR carries enough
information for arithmetic, branching, and the shim cases above.

## Error namespace (`ILO-B3##`)

| Code | Meaning |
| --- | --- |
| `ILO-B301` | `zero check`/`zero build` rejected the emitted source |
| `ILO-B302` | HIR construct not supported by the Zero backend yet |
| `ILO-B303` | `zero` compiler missing on PATH (`--0bin` only) |
| `ILO-B304` | IO failure writing artefact |
| `ILO-B305` | Entry function not found |

All `BackendError` variants round-trip through `BackendError::to_json()`
for `ilo build --json`.

## `--0` vs `--0bin`

- `--0` emits `.0` source. No subprocess. Fast. Use when you want to
  read or edit the Zero output.
- `--0bin` emits `.0` source then invokes `zero build` to produce a
  native binary. Slower (subprocess overhead + Zero compilation). Use
  when you want a binary built by Zero's toolchain.

Both paths produce identical `.0` source. The `--0bin` path adds the
build step on top.

## Subprocess invocation

```
zero check <file>.0                          # validate (optional, fast)
zero build <file>.0 --json --out <binary>    # native binary, JSON diagnostics
```

The Zero backend passes `--json` by default for machine-readable
diagnostics. Both stdout and stderr are captured because Zero 0.1.2
prints diagnostics to stdout, not stderr. The exit code is reliable.

## Cross-target compilation

`zero doctor` reports `target compiler: missing` on Stage 5e's pinned
build -- only affects cross-host builds. Native darwin-arm64 is fine.
`--0bin` defaults to the host target.

## When the Zero compiler upgrades

The pin file (`.zero-version` at the repo root) records the exact Zero
version Stage 5e targets. Upgrades require:

1. Run the full Zero backend test suite against the new Zero version
   (`cargo test --release --features cranelift --test zero_emit
   --test zero_binary --test zero_capability`).
2. Update `.zero-version` and the `PINNED_ZERO_VERSION` constant in
   `src/backend/zero/mod.rs`.
3. Update this matrix if syntax or builtin support changed.
4. CHANGELOG entry under the patch release.

Pre-1.0 Zero will move fast. Treat version upgrades as deliberate work,
not opportunistic.
