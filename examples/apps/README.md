# examples/apps

Real-world example programs harvested from persona-rerun workspaces. Each
demonstrates a complete agent task in ilo: parse some input, transform it,
emit a result. They round out the flat `examples/` directory (which exists
mostly to pin individual language features) with end-to-end shapes.

Every file has `-- run:` / `-- out:` (or `-- err:`) markers, so the existing
`tests/examples_engines.rs` harness exercises them across all engines on each
CI run. Input fixtures live in `fixtures/` and are referenced as
`examples/apps/fixtures/<name>` from the program — paths are relative to the
package root, which is the working dir when cargo runs the harness.

If a program prints multiple lines, a `quiet` companion entry returns a
single-line summary suitable for `-- out:` comparison.
