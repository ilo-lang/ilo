# HIR — High-level Intermediate Representation

Phase 5 Stage 5a. The HIR is the contract between ilo's frontend (lex / parse /
verify) and its backends (Cranelift AOT, Python emit, future WASM and Zero
transpiles). It lives between the verified AST and concrete code emission.

This doc records the shape decisions, departures from the AST, and known
limitations. Future stages — Backend trait, Cranelift refactor, Python refactor,
WASM, Zero — consume the HIR through `hir::Program`.

## Shape: thin

HIR is **the verified AST plus a small number of desugarings**. Not SSA. Not
three-address code. Not closure-lifted. The AST is already small and reasonably
flat; lowering further would multiply Stage-5a engineering effort without
visible benefit until an optimisation pass actually demands it.

A lower-level HIR (SSA or TAC) can be layered between this HIR and the backends
later if the Cranelift backend or a WASM optimiser starts asking for it.

## Pipeline

```
ilo source
  → Lexer
  → Parser            → ast::Program  (raw)
  → Alias resolution
  → Dot-var desugar   → ast::Program  (canonical)
  → Verifier          → ast::Program  + VerifyResult diagnostics
  → hir::lower        → hir::Program
  → Backend trait     → Cranelift / Python / WASM / Zero / ...
```

The verifier today does not decorate the AST with types — it returns a
diagnostics-only `VerifyResult`. The lowering pass therefore re-infers types
during the AST→HIR walk, using the same rules as the verifier (literals,
constructors, builtin return types). Where inference cannot determine a type
without re-running full type-checking, the slot is `Ty::Unknown`. This is
acceptable for Stage 5a: backends that need a concrete type can either
re-infer themselves or run their own pass over the HIR. Stage 5b can introduce
a richer type-annotation channel from the verifier if Cranelift demands it.

## Module layout

```
src/hir/
├── mod.rs        public surface, re-exports
├── types.rs      HIR types (mirrors verify::Ty)
├── expr.rs       HIR expressions
├── decl.rs       top-level declarations
├── program.rs    program-level Hir struct
├── lower.rs      ast::Program → hir::Program
├── raise.rs      hir::Program → ast::Program (for the throwaway walker)
├── walker.rs    `walk(hir, args)` — raises to AST + invokes interpreter
└── DESIGN.md
```

`lower.rs` is the lowering pass. `raise.rs` round-trips HIR back to AST so the
throwaway `walker.rs` can reuse the existing tree-walker for correctness tests.
This is deliberately throwaway: Stage 5f deletes `raise.rs` and `walker.rs`
once the real backends supersede them. Keeping a raise pass also gives us a
free invariant for free in Stage 5b — if the Cranelift refactor goes wrong,
the raise-to-AST path remains a working reference.

## Departures from the AST

The HIR differs from the AST in the following ways. Each is small and
purpose-driven; nothing is rewritten for its own sake.

### 1. Explicit tail expressions on function bodies

The AST represents a function body as a flat `Vec<Spanned<Stmt>>` where the
final statement may be an `Expr` whose value is the implicit return. The HIR
splits this:

```rust
pub struct Body {
    pub stmts: Vec<Stmt>,   // side-effecting prefix
    pub tail:  Option<Expr>,// implicit return value, if any
}
```

This makes the "what's the return value?" question O(1) at every backend
instead of "scan the last statement and special-case it." Stage 5b
(Cranelift refactor) will lean on this; the alternative was re-deriving the
tail in every backend.

### 2. Guards split by intent

The AST encodes three different guard shapes in one `Stmt::Guard` variant
(braced conditional with no else, braced conditional with else, braceless
early-return). The HIR splits them into:

- `Stmt::If { cond, then, else_ }` — braced conditional; `else_` is optional.
- `Stmt::GuardReturn { cond, value }` — braceless early-return.

Negation is folded into the lowering: `!cond{body}` becomes
`If { cond: not(cond), then: body, else_: None }`. Backends don't need to
care about `negated`.

### 3. Ternaries flattened to If-expressions

`Expr::Ternary` and `?expr{arms}` used as a value both lower to the same
underlying form. Stage 5a keeps `Expr::Match` for `?expr{arms}` (because
patterns are richer than a true/false split) but rewrites `Ternary` to
`If` — symmetric with the statement-level split above.

### 4. Pipes already gone

The AST does not carry pipes — the parser desugars `x>>f>>g` into nested
calls before AST construction. Nothing to do at the HIR layer.

### 5. Alias / Use / Error decls dropped

`Decl::Alias` is pure sugar (resolved at verify time). `Decl::Use` is resolved
before verification. `Decl::Error` is a parser error-recovery poison node and
the verifier rejects programs that contain them. The HIR omits all three.

### 6. Spans preserved, but optional

Every HIR node carries an optional `Span` for diagnostics. Spans are not
load-bearing for execution; backends that don't care about them can ignore the
field.

## Types

`hir::Ty` mirrors `verify::Ty` exactly. We re-export the verifier's enum
rather than duplicate it so future changes to the type lattice (e.g.
introducing effect rows) propagate without a parallel update. `Ty::Unknown`
is the escape hatch for the bits of the AST whose static type isn't
determinable from a local inspection.

## Things the HIR does NOT do (yet)

The brief is explicit: thin in v1. The following are out of scope and tracked
as open questions for later stages.

### Closure lifting

The parser already lifts inline lambdas to `__lit_N` top-level functions and
emits `Expr::MakeClosure { fn_name, captures }`. The HIR carries this through
unchanged. A backend that needs strictly-typed closures (e.g. WASM Component
Model) will need its own pass to flatten captures into struct fields. Stage 5b
will add a helper if Cranelift needs one.

### `with` / record update

Today `Expr::With` desugars at runtime into a copy-on-write record clone. HIR
keeps `With` as a single node. A lower HIR would expand it to an explicit
clone-and-update sequence. Defer.

### Match exhaustiveness

The verifier checks exhaustiveness; HIR does not record the result. Backends
that care (WASM, native) can either re-derive it or trust the verifier ran.
Stage 5b: revisit if Cranelift's match emit benefits from explicit "this is
exhaustive, no default needed" markers.

### Effect / capability annotations

Phase 6+ work. Not modelled at HIR Stage 5a.

## Round-trip strategy (Stage 5a only)

The brief calls for `tests/hir_roundtrip.rs` that asserts AST-walk output
matches HIR-walk output across every `examples/*.ilo` file with an annotated
`-- run:` / `-- out:` pair.

Stage 5a implements `hir::walk` as `lower → raise → interpreter::run`. This is
the cheapest correctness gate: it proves the lowering preserves enough
information to reconstruct an equivalent AST. Once Stage 5b lands the real
Cranelift backend driven from HIR, the raise + walker stub can go.

## Acceptance gates met by this design

- [x] HIR exists with module layout `src/hir/{types,expr,decl,program,lower,raise,walker}.rs`
- [x] `hir::lower(ast, verify_out) → Result<hir::Program, LowerError>`
- [x] `hir::walk(hir, args) → Value` for the round-trip test
- [x] Documented departures from AST above
- [x] Documented open questions / deferrals above

## Open questions for Stage 5b+

1. **Type annotations on `Expr`.** Today every HIR expression carries an
   optional `Ty` slot computed by `lower.rs`. Cranelift may want a *required*
   non-`Unknown` type on every node; if so, Stage 5b extends the verifier to
   emit a typed-AST output that lowering can consume directly. Decision
   deferred until Cranelift's lowering pass starts being written.

2. **Effects channel.** When ilo grows explicit effect rows (Phase 6+), the
   HIR will need either an effect annotation per call or a separate "effect
   map" structure. Punt.

3. **HIR stability.** Treating HIR as a public Rust API would prevent breaking
   downstream crates as new HIR nodes appear. Today it's `pub` but
   undocumented as stable. Stage 5b should mark crate-internal-only via doc
   comments and keep HIR private until Phase 6 settles.
