# TODO

## Performance

- [ ] Interpreter flat-scope rewrite — `feature/optimize-interpreter` was rejected (unsound unsafe, broke outer-scope mutation, removed FnRef/Map/tools). Needs clean rewrite: flat `Vec<(String, Value)>` with full-range `get`/`set` + scope marks, keeping all existing functionality.

## Agent / tool integration

- [x] Tool graph — `ilo tools --graph`: type-level composition map showing which tools can feed each other
- [ ] "Typed shell" mode — interactive tool composition with type-guided completion

## Tooling

- [ ] LSP / language server — completions, diagnostics, hover for editor integration
- [ ] REPL — interactive evaluation for exploration and debugging
- [ ] Playground — web-based editor with live evaluation (WASM target)

## Codegen targets

- [ ] JavaScript / TypeScript emit — like Python codegen but for JS ecosystem
- [ ] WASM emit — compile to WebAssembly for browser/edge execution

## Program structure

- [ ] Multi-file programs / module system (programs are small by design — may never need this)
- [ ] Imports — `use "other.ilo"` to compose programs from multiple files

---

## Completed

### Language hardening
- [x] Reserve keywords at lexer level — `if`, `return`, `let`, `fn`, `def`, `var`, `const` now lex as dedicated tokens (KwIf, KwReturn, etc.)

### Type system (Phase E)
- [x] Optional type — `O T` nullable values
- [x] Sum types — `S a b c` closed sets of variants
- [x] Map type — `M k v` key-value collections + 7 builtins (mmap, mget, mset, mhas, mkeys, mvals, mdel)
- [x] Type variables — single-letter type params for generic functions

### Control structures
- [x] Pattern matching on type — `?x{n v:...; t v:...}`
- [x] While loop `wh cond{body}`
- [x] Break/continue `brk`/`cnt`
- [x] Range iteration `@i 0..n{body}`
- [x] Early return `ret expr`
- [x] Pipe operator `>>` for chaining calls
- [x] Nil-coalesce `??`, safe field navigation `.?`
- [x] Destructuring bind `{a;b}=expr`

### VM / performance
- [x] Bump arena for records — arena-allocated structs, promote to heap on escape
- [x] JIT inlining — arithmetic, comparisons, branching, field access, alloc
- [x] No-Vec OP_CALL — push args directly onto stack, 1.6x faster function calls

### Builtins
- [x] `env` — read environment variables (`env "PATH"` → `R t t`)
- [x] `get`/`$` — HTTP GET returning `R t t`
- [x] `len`, `str`, `num`, `abs`, `min`, `max`, `flr`, `cel`, `rnd`, `now`
- [x] `cat`, `has`, `hd`, `tl`, `rev`, `srt`, `slc`, `spl`
- [x] `map`, `flt`, `fld` — higher-order functions
- [x] `jpth`, `jdmp`, `jpar` — JSON path/dump/parse

### Agent integration (Phase D)
- [x] D1: ToolProvider, HttpProvider, StubProvider, Value↔JSON
- [x] D2: MCP stdio client, auto-discover tools, inject into AST
- [x] D3: `ilo tools` — list/discover with `--human`/`--ilo`/`--json` output
- [x] D4: `ilo serv` — JSON stdio agent loop with phase-structured errors

### Error infrastructure (Phases B/C)
- [x] B: Spans, Diagnostic model, ANSI/JSON renderers, error codes (ILO-L/P/T/R)
- [x] C1: Error recovery — multiple errors per file, poison nodes
- [x] C2: Error codes + `--explain ILO-T001`
- [x] C3: Suggestions/fix-its — did-you-mean, type coercion hints, cross-language syntax detection
- [x] C4: Runtime source mapping — spans and call stacks on runtime errors

### Basics
- [x] List literals, unary ops, logical AND/OR/NOT, string comparison
- [x] All comparison operators extend to text (lexicographic)
- [x] Type verifier, match exhaustiveness, arity checks at all call sites
- [x] Python codegen, `--explain` formatter
- [x] Type aliases `alias name type`
