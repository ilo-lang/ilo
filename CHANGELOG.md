# Changelog

## 0.12.1

### Breaking

- `ls dir` renamed to `lsd dir`. Six rerun10 personas tripped ILO-P011 on `ls=rdl! p` because `ls` was reserved; rename frees `ls` for user code. `walk`, `glob` unchanged.

### Fixed

- CSV/TSV reader now tracks quote state across record separators. A cell containing `\n` (which the writer correctly emits as a quoted multi-line field per RFC 4180) used to be re-parsed as two rows, so `rd path "csv"` silently disagreed with `wr path data "csv"`. The reader is now a single-pass scanner over the whole document and round-trips multi-line quoted fields, embedded quotes, and CRLF line endings byte-stably across tree and VM. Surfaced by csv-pipeline rerun10.

## 0.12.0 - 2026-05-19

### Breaking

- VM is the default engine. No need to pass `--run-vm`.
- `$` is now the `run` sigil (was the HTTP `get` alias). Use `get url` for HTTP.
- `post` is now `pst`. Drop the vowel like every other I/O verb (`rd`, `wr`, `srt`, `flt`).
- `--run-tree` is removed. Tree-walker stays inside the VM for HOF dispatch but is no longer user-selectable.

### Added

- `run cmd args > R (M t t) t` argv-list process spawn. Returns `{stdout, stderr, code}` on success. Result Err only on spawn failure; non-zero exits land in the `code` field. No shell, no interpolation, so no command-injection vector.
- `ls dir`, `walk dir`, `glob dir pat` filesystem traversal. Each returns `R (L t) t` so missing-dir and permission-denied are typed at the boundary.
- `env-all > R M t t` returns the full process environment as a Map, pairs with `env key`.
- `ilo run`, `ilo check`, `ilo build` verbs. Bare-arg form still works.
- `--json` on every subcommand. Schemas documented in `JSON_OUTPUT.md`.
- `--bench` JSON output includes an `engine` field so you can tell which engine produced the timing numbers.
- VS Code extension at `extensions/vscode/`. Syntax highlighting, snippets, `--` comment handling, Cursor install script.
- Tree-sitter grammar at `github.com/ilo-lang/tree-sitter-ilo`, covers 98% of the example corpus. Wires into Neovim, Helix, Zed.
- Modular skill: six pages (`ilo-language`, `ilo-builtins`, `ilo-errors`, `ilo-tools`, `ilo-engines`, `ilo-agent`) under 5k tokens each.
- `ILO-R015` AOT runtime fault diagnostic. Hard faults emit JSON to stderr before the OS reports the exit code.
- Engine audit corpus at `tests/engine-matrix/` covering every engine on every feature shape.
- Closed-loop benchmark harness at `research/closed-loop-bench/`.
- Memory-model guide at `site/docs/guide/memory-model.md`.

### Changed

- Closure-capturing HOFs (`srt`, `grp`, `uniqby` with inline lambdas) run natively on VM and Cranelift JIT. No more tree-bridge fallback.
- RC fast paths for sole-owned values on `rev` / `srt` / `flt`. In-place mutation when no other reference exists.
- Error-code namespaces allocated and stable; ranges documented in SPEC.

### Fixed

- AOT sum types compile (cranelift string-constant interning no longer collides across functions).
- AOT default entry resolves to `main` instead of "first declared function", so `ilo compile file.ilo -o out && ./out` works.
- AOT hard faults emit `ILO-R015` JSON instead of raw SIGSEGV.
- SPEC drift on closure-capture: it was claimed tree-only, but VM and JIT handled it from Phase 2 onward. AOT was the actual lag and is now documented honestly.

## 0.11.6 - 2026-05-17

- HeapObj::ListView foundation and OP_WINDOW reshape to emit ListView, dropping window-construction RC traffic from O(n·k) to O(n). Bio microbenches went from 4-6s to 0.18-0.49s.
- Inline lambdas Phase 1: parenthesised function literals lift to synthetic top-level decls; closure-capture lands later in 0.12.0.
- `rgxall1` flat-capture form, `ct` count-by-predicate builtin.
- Bare-bool prefix ternary `?h a b`.
- Source spans thread through Cranelift JIT runtime-error helpers.
- ILO-P021 rejects the `--N` prefix-binop trap.
- EOF parse errors anchor on the dangling token instead of line 1 col 1.
- CLI hyphenated subcommands and non-ident positionals route to `main`.

## 0.11.5 - 2026-05-16

- Cranelift JIT catches panics and falls back to non-JIT engines (handles the AArch64 near-call relocation assertion seen on `rustc 1.85`).
- HOF tree-bridge error parity on Cranelift.
- `?bool{a}{b}` sugar for prefix ternary, closing a five-release papercut.
- Brace-block function bodies accepted; multi-line hint shows both shapes.
- `x!` bare-ident bang fix (v0.11.4 P0 silently returning nil).
- `flt`/`map` fused over window into a stride-1 in-place loop.

## 0.11.4 - 2026-05-16

- Fixed Cranelift JIT `srt`-after-`map` TLS desync silent miscompile.
- Restored documented auto-run for `main` and inline programs.
- `OP_LISTAPPEND` rebind shape routes through the in-place helper on Cranelift JIT.

## 0.11.3 - 2026-05-16

- New builtin: `mapr` for short-circuit Result propagation across `map`.
- `padl` / `padr` accept an optional pad-char arg for zero-pad and dot-leader patterns.
- `fmt` rejects printf-style format specs (`{:...}`) at parse time instead of silently returning the literal.
- `sum` and `avg` now work on VM and Cranelift, not just tree.
- Shadow-rebind register aliasing fix on VM and Cranelift.
- `xs.i` desugars to `at xs i` when `i` is a bound variable.
- Parser rejects builtin-named binding LHS with ILO-P011, with rename suggestion.
- `at xs i` auto-floors fractional indices.
- Lexer decodes `\f \b \v \a \0 \/` escape sequences.

## 0.11.2 - 2026-05-15

- Inline lambdas Phase 1: parenthesised function literals lift to synthetic decls (closure-capture in Phase 2).
- Wire `rgx`, `rgxall`, `fmt`, `rd`, `rdb` through VM and Cranelift via tree bridge.
- `lst xs i v` plus `lset` alias for list-update.
- `chars s` builtin: explode string into single-char strings.
- `sleep ms` for pure-ilo polling tails.
- `frq` drops the type-prefix from output keys, matching `grp` convention.
- O(n²) → O(n) `mset` accumulator via RC=1 in-place HashMap mutation.
- `.?` returns nil on missing field, not just on nil object.
- Multi-line bodies inside brackets, parens, and `>>` pipe chains.
- `hd`/`tl`/`at` out-of-range errors harmonised across tree, VM, Cranelift.
- Entry function returning `Value::Err` exits 1.

## 0.11.1 - 2026-05-13

- CLI runs single-fn files automatically, lists multi-fn files. `--ast` gates AST dump.
- `ord` and `chr` for per-char codepoint round-trip.
- `rgxall` multi-match capture-group extraction for HTML scraping.
- `match` arms accept brace-block bodies.
- Reserved keywords accepted as field names at dot-access.
- camelCase accepted at post-dot field access.
- `at s i` on text no longer allocates a `Vec` per call.

## 0.11.0 - 2026-05-13

- Removed the custom ARM64 JIT backend. Cranelift JIT is the optimising path.
- New math builtins: `pow`, `sqrt`, `log`, `exp`, `sin`, `cos`, `tan`, `log10`, `log2`, `atan2`.
- `at xs i` for nth-element list access, with Python-style negative indexing.
- Builtins as HOF args (verifier + interpreter).
- `!` on Optional types across verifier, interpreter, VM, Cranelift.
- Nested generic types like `R (L n) t`.
- Snake_case field names in dot-access position.
- Prefix-binop expressions accepted as call arguments.
- Scientific-notation float literals.
- `??` accepted as a prefix operator.

## 0.10.3 - 2026-05-11

- Friendly errors for identifier-confusion cases (similar-name typos suggest the right binding).
- Skill documents the three ways to run ilo from an agent.

## 0.10.2 - 2026-05-11

- Fixed `slc` and `mset` silent miscompilation in loops.
- Release workflow publishes `pi-ilo-lang` to npm.
- Skills-ref validate in lint job.

## 0.10.1 - 2026-05-02

- AOT compilation via `ilo compile`. Full AOT opcode parity with the JIT.
- `OP_RECFLD_NAME` implemented in JIT and AOT.
- O(n²) → O(n) list-append; 5x speedup on `foreach` accumulator workloads.
- SKILL.md converted to the Agent Skills spec-conformant format.
- Renamed `--run-interp` to `--run-tree`.
- `ai.txt` tracked as source, drift-checked in CI.

## 0.10.0 - 2026-03-08

- Space-separated list literals and heterogeneous lists.
- `_` type changes from nil to any/unknown.
- Coverage rounds for VM, verifier, interpreter.

## 0.9.1 - 2026-03-08

- Rust safety review pass: removed problematic unwraps, scoped RAII for `ACTIVE_REGISTRY` pointer, debug assertions for `as_heap_ref`.
- Enum-based builtin dispatch.
- 221 new VM tests for interpreter parity.

## 0.9.0 - 2026-03-07

- Long-form aliases for builtins (e.g. `length` for `len`).
- Removed unwraps and unnecessary clones in cleanup pass.

## 0.8.2 - 2026-03-07

- Interactive REPL with nvim-style commands.

## 0.8.1 - 2026-03-07

- Full infix operator support.
- npm WASM package for universal installation.
- `mod` builtin for modulo / remainder.
- Guard-in-loop warning (ILO-W001).
- Idiomatic hints system.
- `==` accepted as sugar for `=` (equality).
- And/Or short-circuit fix on left-operand register clobbering.

## 0.8.0 - 2026-03-07

- P2 data builtins: `grp`, `flat`, `sum`/`avg`, `rgx`.

## 0.7.0 - 2026-03-06

- Internal refactor release (see [GitHub release](https://github.com/ilo-lang/ilo/releases/tag/v0.7.0) for diff).

## 0.6.0 - 2026-03-06

- JIT-arm64 handles mprotect failure.
- MCP stdin error handling no longer panics on partial reads.
- Tools JSON output no longer panics on unwrap.

## 0.5.0 - 2026-03-04

- First Cranelift JIT compiler pass.

## 0.4.0 - 2026-03-03

- Bytecode VM lands as a second engine alongside the tree-walker interpreter.

## 0.3.0 - 2026-03-01

- Type system, verify pass, error codes (the `ILO-XXXX` namespace begins here).

## 0.2.0 - 2026-03-01

- Builtins expand: collections (`map`, `flt`, `fld`), text helpers, basic I/O.

## 0.1.2 - 2026-02-27

- Lexer and parser bug fixes; manifest expanded.

## 0.1.1 - 2026-02-27

- Initial CLI flag set: `--ai`, `--tools`, `--help`.
- README and SPEC drafts.

## 0.1.0 - 2026-02-27

Initial public release. Tree-walker interpreter, prefix-notation language, manifesto published. Token-conservative design target set at one-third Python's tokens on the canonical example.
