# Mog comparator attempt — findings (2026-09-17)

First cross-language comparator attempt for the closed-loop bench
(PLAN.md G1: "Mog invited as comparators"). Parked — documenting the
blockers so the retry (or Mog's maintainers) have a head start.

## What worked

- `voltropy/mog` builds clean: `mogc` (compiler, 9s), `libmog_runtime.a`.
- With-deps and workflow-rollback tasks compile, link, run correctly
  (`28`; `5`/`0`).
- Link recipe: `mogc file.mog -o out --link <runtime>/libmog_runtime.a`
  (undocumented in the guide; bare `mogc -o` emits an object that fails to
  link against the GC runtime).

## Blockers (Mog-side, v0.1.0 standalone toolchain)

1. **`/` on ints + `println(int)`**: `n * ((n + 1) / 2)` for n=10 prints
   `701348603494160` — float-through-int garbage at the print boundary.
2. **f-string on an int function result segfaults** (exit 139) on the same
   program.
3. **`for-in` mutation of an outer variable**: `total := total + sq`
   inside the loop then printing gives pointer-ish garbage — smells like
   the loop body is a closure and the outer store isn't written back, or
   the same print bug as (1).
4. **`process.getenv` typing**: `v := await process.getenv("...")` binds a
   `Future<i64>`; neither `?` nor plain await yields the documented
   string. Capability returns appear undocumented in the guide.

Items 1–3 mean the two pure-compute tasks can't produce a verifiable
number yet; item 4 blocks the env task (arguably by design: Mog hosts
provide I/O, so an env-var task may be the wrong shape for Mog).

## What the retry needs

- Mog maintainer confirmation of int printing (bug vs misuse).
- A documented standalone host that grants `process` capabilities to an
  arbitrary `.mog` file (the repo's `examples/host.rs` grants `env`, not
  `process`).
- Alternatively: the bench harness gains a per-language "host adapter"
  slot (compile command + runner + capabilities wiring) instead of a
  single `--lang2-bin`.

Reproduce: `/tmp/mog-tasks/*.mog` against `voltropy/mog` @ HEAD
(2026-09-17), compiler built with `cargo build --release`.
