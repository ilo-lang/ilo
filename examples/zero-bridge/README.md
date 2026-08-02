# Zero bridge example

Demonstrates the ilo to Zero transpile pipeline introduced in 0.13.0
(Phase 5 Stage 5e). The example writes `Hello, Zero!\n` to stdout.

## Build paths

```sh
# Inspect the generated Zero source.
ilo build hello.ilo --0
cat hello.0

# Build through the pinned `zero` compiler to a native binary.
ilo build hello.ilo --0bin -o hello-zerobin
./hello-zerobin
```

Both paths produce identical `.0` source. `--0bin` adds the subprocess
`zero build` step on top.

## When to reach for `--0bin` vs the default Cranelift native build

Use the default `ilo build hello.ilo` (Cranelift) when you want the
fastest path to a native binary that links against ilo's runtime.

Use `--0bin` when you want a binary built by Zero's toolchain instead -
useful when targeting environments that prefer Zero binaries, or when
auditing the generated Zero source as part of a code-review handoff.

## Pinned toolchain

ilo 0.13.0 targets `zero 0.1.2`. The pin is recorded in `.zero-version`
at the repo root. See `docs/zero-transpile-capabilities.md` for the full
construct mapping and the upgrade procedure.
