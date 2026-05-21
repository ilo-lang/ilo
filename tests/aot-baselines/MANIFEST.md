# AOT object-file baseline corpus

Pre-refactor Cranelift AOT object-file sha256 baselines used as the regression
fixture for the Phase 5 Stage 5b backend-trait refactor.

## Capture point

Captured on `feature/codegen-layer` at the tip of Stage 5a (commit
`6d0b33f5cd54d59c06e6c7d3e906d87f733e942c`, `hir: round-trip test across
examples/ corpus`), immediately before the Stage 5b refactor began.

The original pre-Stage-5a baselines at `/tmp/ilo-baselines/` (commit
`73ca38c` on main, before HIR work) are not used by the in-repo test
because adding the HIR module changed `libilo.a` content, which changed
linked-binary bytes even though Cranelift codegen was unchanged. Asserting
byte-identity at the linked-binary level is therefore not a useful gate
against Stage 5b. The Stage-5a-tip object-file baselines do isolate
Cranelift codegen output from `libilo.a` content.

## What the baselines record

`obj-baselines.tsv` is tab-separated, no header:

| Column | Meaning |
| --- | --- |
| 1 | example basename (no `.ilo` extension) |
| 2 | entry function the AOT compile was passed |
| 3 | sha256 hex of the Cranelift-emitted `.o` file |

The `.o` file is the relocatable object Cranelift's `ObjectModule::finish()`
produces. It contains:

- Generated machine code for every function in the bytecode chunks
- The `main()` shim that handles arg parsing and result printing
- `Linkage::Import` symbol declarations for `libilo.a` helpers
- A relocation table for those imports
- A serialised type registry blob

It does NOT contain `libilo.a` itself, the system code-signing blob, or
any linker-introduced randomness (`LC_UUID`). All of those live in the
final linked binary, not the `.o`.

## Determinism

Empirically verified at capture time and at Stage 5b validation time:

- Two back-to-back `ILO_KEEP_OBJ=1 ilo build` invocations on the same
  source produced byte-identical `.o` files (`shasum -a 256` matched
  exactly).
- Compiling the same source with different `-o` output paths still
  produced byte-identical `.o` files (the linker step embeds the output
  filename into the code signature; that's a binary-level artefact, not
  an object-level one).
- All 136 entries in the corpus matched between Stage 5a tip and Stage 5b
  refactor, confirming the refactor preserves Cranelift codegen exactly.

## How to capture

The `ilo build` AOT path writes a `<output_path>.o` file during the link
step and removes it on success. Set `ILO_KEEP_OBJ=1` to preserve the
object file. Then iterate over the `-- run:` annotated examples in
`examples/` and record the sha256 of each `.o`:

```sh
while read -r name args expected sha status; do
  [ "$status" = OK ] || continue
  fn=$(echo "$args" | awk '{print $1}')
  tmp=$(mktemp)
  ILO_KEEP_OBJ=1 ./target/release/ilo build "examples/$name.ilo" -o "$tmp" $fn
  shasum -a 256 "$tmp.o"
  rm -f "$tmp" "$tmp.o"
done < /path/to/run-headers.tsv
```

A pre-existing baseline corpus (220 examples, 136 compile-OK) lives at
`/tmp/ilo-baselines/results.tsv`; the in-repo `obj-baselines.tsv` is
derived from it.

## When to regenerate

Regenerate `obj-baselines.tsv` when:

- Cranelift codegen intentionally changes (e.g. a new opcode lowering, a
  performance optimisation that produces different IR). Document the
  change in the PR; this corpus is a strict regression gate.
- The Cranelift dependency bumps version (its emit can change between
  versions).
- The host architecture or `rustc` toolchain changes (object format
  details vary).

Do NOT regenerate when a refactor "should be" behaviour-preserving but
the baselines disagree. Investigate the diff first.

## Scope

- 136 of 220 runnable examples compile under AOT. The other 84 are
  pre-existing AOT-compile failures (duplicate symbol bugs, unsupported
  opcodes) that this refactor is not in scope to fix.
- The 87 runtime-mismatch baselines noted in `/tmp/ilo-baselines/
  verify-mismatches.tsv` (binaries that segfault when run with no args)
  do NOT affect this test — we assert object-file byte identity, not
  runtime behaviour. The existing `tests/examples_engines.rs` cross-engine
  parity suite covers runtime correctness.

## Capture environment

- Host: macOS 15.5 arm64
- Rust: stable matching `rust-toolchain` (Cargo.toml `rust-version = 1.85`)
- Build: `cargo build --release --features cranelift`
- Examples corpus: 220 `-- run:`-annotated files
