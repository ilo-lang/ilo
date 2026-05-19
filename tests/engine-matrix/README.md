# engine-matrix test corpus

Small `.ilo` programs that each exercise one feature. Used by `research/engine-audit-2026-05.md` to populate the feature-by-engine support matrix.

Each file has a one-line comment header describing the feature and the expected stdout. Re-run any time the engine surface area changes:

    ./tests/engine-matrix/run-matrix.sh

Engines exercised:

| Engine            | Invocation                                                |
|-------------------|-----------------------------------------------------------|
| Register VM       | `ilo --vm FILE`                                       |
| Cranelift JIT     | `ilo --jit FILE`                                          |
| Cranelift AOT     | `ilo compile FILE -o out && ./out`                        |

The tree-walker is no longer a public engine: `--run-tree` was removed in the 0.12.x soft-deprecation. It stays in-tree as the runtime for HOF callbacks that VM/Cranelift bail to, so VM coverage transitively exercises tree-walker behaviour for the remaining bridge ops.

`run-matrix.sh` produces a Markdown matrix on stdout suitable for pasting into the audit doc.
