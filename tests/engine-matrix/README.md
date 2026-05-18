# engine-matrix test corpus

Small `.ilo` programs that each exercise one feature. Used by `research/engine-audit-2026-05.md` to populate the feature-by-engine support matrix.

Each file has a one-line comment header describing the feature and the expected stdout. Re-run any time the engine surface area changes:

    ./tests/engine-matrix/run-matrix.sh

Engines exercised:

| Engine            | Invocation                                                |
|-------------------|-----------------------------------------------------------|
| Tree walker       | `ilo --run-tree FILE`                                     |
| Register VM       | `ilo --run-vm FILE`                                       |
| Cranelift JIT     | `ilo --jit FILE`                                          |
| Cranelift AOT     | `ilo compile FILE -o out && ./out`                        |

`run-matrix.sh` produces a Markdown matrix on stdout suitable for pasting into the audit doc.
