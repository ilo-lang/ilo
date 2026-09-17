# Verified Zero reference tasks

The five closed-loop bench tasks hand-authored in zerolang and verified
against `zero run` (v0.3.4, 2026-09-17):

| Task | Expected output |
|---|---|
| simple-function.0 | `55` |
| with-dependencies.0 | `28` |
| data-transform.0 | `41` |
| tool-interaction.0 | `hello world` (BENCH_VAR unset) |
| workflow-rollback.0 | `5` then `0` on separate lines |

Toolchain findings from authoring (see `../mog-comparator-findings.md` for
the Mog equivalent): `For` loops are unsupported in the typed-MIR
executable path (use `while`); i32 division truncates; BOR001 borrow rules
require split output buffers for sequential formatted writes; local
bindings need explicit type annotations.

Rerun the funded LLM leg with:

    closed-loop-bench.py --lang2-name zero \
      --lang2-bin bench/zero/zero-bench.sh --lang2-ext .0 \
      --lang2-docs <zero language+stdlib skills> --model <funded model>
