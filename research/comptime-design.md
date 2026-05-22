# Comptime / Zero-Cost Metaprogramming — Design Research

**Date:** 2026-05-22
**Ticket:** ILO-74
**Scope:** Evaluate four metaprogramming models against ilo's six Manifesto principles. Recommend one path with MVP plan and follow-up tickets.

---

## Background

ilo's closed-world constraint and verification-before-execution model create a natural opening for compile-time evaluation: if the verifier already resolves every call before running anything, it can in principle evaluate *some* calls at that stage too. The question is which metaprogramming model fits ilo's token-cost lens and constrained-generation goals without importing complexity that would expand the spec, widen the valid-next-token set, or require agents to reason about a new phase of execution.

---

## Option 1 — Zig-Style `comptime` Blocks

### What it is

Zig's `comptime` keyword forces an expression or block to be evaluated at compile time. Any value known at compile time (literals, `comptime` parameters, other `comptime` expressions) can participate. The output becomes a compile-time constant visible throughout the program. If evaluation fails, it is a compile error.

```zig
fn min_bytes(comptime T: type) usize {
    return @sizeOf(T);
}
const N = comptime min_bytes(u32);  // evaluates to 4 at compile
```

### Applicability to ilo

ilo has no type-as-value concept and no runtime polymorphism, so the most powerful Zig uses (type-level generic programming, inline-array-length derivation) have no direct mapping. What does map:

- **Constant folding of pure expressions.** `comptime *3 4` → `12` in the output. The verifier already walks pure expressions; extending it to evaluate them is a small step.
- **Compile-time string building.** Useful for constructing format strings, URL prefixes, or lookup tables that agents would otherwise hard-code repeatedly.
- **Assertion/invariant checks.** `comptime assert >MAX_SIZE 0` fails the build rather than a runtime guard — guarantees that agents writing programs with constants never ship logically broken configurations.

### Cost

- **Spec size:** Adds a new keyword (or sigil) and a new execution phase. Every agent loading the spec must learn: what is comptime-evaluable, what is not, and what happens when comptime evaluation fails. Estimated spec overhead: ~300–500 tokens.
- **Verifier complexity:** The verifier must run a subset interpreter at check time. The "comptime-evaluable" subset must be defined precisely (pure functions, no I/O, no `rd`/`wr`/`env`, no random). Any ambiguity widens the retry surface.
- **Error surface:** Compile-time evaluation errors need their own diagnostic codes. Agents reading `ILO-C001: comptime evaluation failed` must understand a new error class.

### What it enables

- True zero-cost constants: no runtime branch, no heap allocation.
- Self-documenting invariants: `comptime assert` is cheaper to generate and read than a runtime guard with a message.
- Lookup-table generation: hash maps built at compile time from literal data (small stdlib supplement).

### What it complicates

- Increases the "what is legal here?" surface. Agents writing loops or calls inside `comptime` blocks must distinguish the comptime-safe subset from the full language.
- Any non-pure function accidentally used in a `comptime` context produces an error that requires a conceptual model of evaluation phases.
- Interaction with closures (Phase 2) is unclear: a `comptime` block that captures a runtime variable must error; detecting this requires analysis that does not exist yet.

---

## Option 2 — Rust-Style `const fn`

### What it is

Rust allows functions decorated with `const` to be called in constant-evaluation contexts (array lengths, `const` bindings, static initialisers). The function body is restricted to a "const-evaluable" subset of Rust. At a `const` call site, the compiler evaluates the function and substitutes the result.

```rust
const fn triple(x: usize) -> usize { x * 3 }
const N: usize = triple(4);  // N = 12 at compile time
```

### Applicability to ilo

ilo functions are already pure by construction (no mutable globals, no ambient state). This means ilo's "const fn equivalent" is nearly every non-I/O function — the boundary is I/O builtins (`rd`, `wr`, `env`, `now-ms`, `rand-bytes`, etc.), not purity of user code.

The practical form: annotate call sites (not definitions) as `comptime`-evaluated. Or: the verifier detects pure constant-value call chains and folds them automatically without any annotation.

### Cost

- **If annotation-based:** Same spec cost as Option 1 — new keyword, new error class, new agent-visible distinction between const and non-const functions.
- **If automatic:** No spec cost at all. The verifier silently folds pure literal expressions. No new tokens for agents to emit. The only user-visible effect: some programs run faster. Downside: agents cannot *rely* on folding; it is an optimisation, not a guarantee. Assertions-at-compile-time become impossible.
- **Rust's actual experience:** The `const fn` subset has grown substantially over time and is still not complete. The restriction set is hard to explain without extensive documentation.

### What it enables

- Same as Option 1 but with less explicit control.
- If automatic: near-zero cost to adopt; the verifier gains a constant-folding pass as a pure optimisation.

### What it complicates

- Automatic folding is invisible, which is fine for agents (they don't need to know about it) but means compile-time assertion is impossible.
- Annotation-based `const fn` imports Rust's conceptual split between "const world" and "runtime world." For a language with ilo's constrained generation goal, this widens the valid-next-token set at every function definition site.

---

## Option 3 — Lisp-Style Macros

### What it is

Lisp macros operate on the AST: a macro receives unevaluated syntax and returns new syntax, which is then compiled. This enables arbitrary code generation at compile time — new control-flow constructs, DSLs, code duplication elimination.

```lisp
(defmacro when (cond &body body)
  `(if ,cond (progn ,@body) nil))
```

### Applicability to ilo

ilo's prefix-notation AST is structurally similar to S-expressions, which makes macro expansion conceptually natural. However:

- ilo's Constrained principle explicitly values a *closed world* and a *small valid-next-token set*. Macros break the closed world by definition: a macro is a new callable that may not be in the spec. An agent that has not loaded the macro definition cannot know what tokens are valid at its call site.
- Macros require agents to reason about two levels of code: the macro definition (which manipulates AST nodes) and the call site (which uses the macro). This doubles the conceptual surface for any feature that uses macros.
- Hygienic macros require a scope model for compile-time names, which does not exist in ilo and would take significant verifier work.
- The debugging story for macro errors is the weakest of all four options — an error in expanded code is hard to map back to the source macro call.

### What it enables

- Arbitrary syntax extension. Any language feature could in principle be expressed as a macro.
- Eliminates the "should this be a builtin?" question for common patterns — users write macros.

### What it complicates

- Fundamentally breaks the closed-world guarantee. Every macro is a new unknown in the agent's token model.
- Spec cannot remain small: agents must load macro definitions before generating code that uses them.
- Contradicts the Graph-Reducible principle: macros create implicit dependencies that are not expressed in the `use` import graph.
- The Manifesto's "Constrained" principle explicitly warns against open-ended generation. Macros are the canonical form of open-ended generation.

### Assessment

Macros are the highest-power option and the worst fit for ilo. They solve problems ilo does not have (extending syntax, building DSLs) and create problems ilo is explicitly designed to avoid (open worlds, wide valid-next-token sets, invisible dependencies).

**Verdict: REJECT.**

---

## Option 4 — Template / Dependent Types

### What it is

Template types (C++, generics) allow types to be parameterised by other types or values. Dependent types (Idris, Agda, Lean) allow types to depend on runtime values — enabling invariants like "list of length N" to be expressed at the type level.

```idris
-- Dependent type: vector of exactly length n
data Vect : Nat -> Type -> Type where
  Nil  : Vect 0 a
  (::) : a -> Vect n a -> Vect (S n) a
```

### Applicability to ilo

ilo already has type variables (`a`, `b`, etc.) as weak generics — the verifier accepts any type for `a` without consistency-checking across call sites. This is intentionally shallow: the Manifesto notes that type variables provide "weak generics" and that consistency checking is not done. Upgrading to true parametric generics (where `L a → L a` guarantees element-type consistency) would catch real bugs but requires significant verifier work.

Full dependent types are almost certainly out of scope: they require a proof assistant-style type theory, interactive type elaboration, and a fundamentally different kind of spec. The spec-loading cost for agents would be enormous.

Lighter forms — size-indexed types, non-empty list types, bounded integers — are feasible but add type constructors that agents must learn and emit correctly.

### What it enables

- Catch more type errors at verify time (e.g., `hd` on a provably-empty list).
- Encode invariants in types instead of runtime guards.

### What it complicates

- Every new type constructor expands the spec and the valid-next-token set.
- Agents generating typed signatures must correctly parameterise generic types, or the verifier rejects them.
- Interaction with ilo's existing `O T`, `R T E`, `L T`, `M K V` type language is complex — adding a fourth dimension (size) to list types, for example, requires changes throughout the standard library.

### Assessment

Lightweight generics (fixing type variable consistency) are worth pursuing independently of comptime. Full dependent types or sized types are high-cost, low-fit. This option is not the right focus for comptime/metaprogramming.

**Verdict: OUT OF SCOPE FOR THIS TICKET.** Track as a separate generics research ticket.

---

## Recommendation: Automatic Constant Folding (Option 2, annotation-free)

### Rationale

The Manifesto's token-cost lens cuts clearly here. The options ranked by spec cost (tokens agents must load to use the feature):

| Option | Spec tokens added | Closed world preserved | Agent error surface |
|--------|------------------|------------------------|---------------------|
| Automatic folding | ~0 | Yes | None (silent optimisation) |
| `comptime` annotation | ~400 | Yes | New error class |
| `const fn` annotation | ~400 | Yes | New error class |
| Macros | Unbounded | No | Breaks entirely |
| Dependent types | ~800+ | Yes | Large |

Automatic constant folding is the only option that:
1. Adds zero tokens to the spec (nothing for agents to learn)
2. Preserves the closed-world guarantee
3. Delivers real value (shorter bytecode, no-cost compile-time constants)
4. Requires no new syntax for agents to emit

The argument *for* annotated `comptime` over automatic folding is compile-time assertions: `comptime assert >MAX 0` fails the build rather than being silently skipped. This is a real use case. However, it can be achieved more cheaply by a dedicated `assert` builtin that is evaluated at verify time when its arguments are literal — no `comptime` keyword needed, no new execution phase to explain.

**Chosen path:** Automatic constant folding in the verifier + a `comptime-assert` builtin (single new stdlib name) for invariant checking.

---

## MVP Plan

### Phase 1 — Verifier constant-folding pass (no user-visible change)

**What:** Add a constant-folding pass to the verifier that evaluates pure expressions over literals. Foldable: arithmetic/comparison/string operations over literal values, calls to user functions whose entire argument list is compile-time-known and whose bodies contain no I/O builtins.

**Scope:** Pure arithmetic and comparison operators over number/text literals. Function inlining deferred to Phase 2.

**Success criterion:** `+*3 4 5` in a constant binding position emits a constant `17` in the bytecode rather than two instructions. Regression tests: no behaviour change on existing corpus.

**Agent-visible effect:** None. The verifier runs faster on literal-heavy programs. Generated bytecode is smaller.

**Estimated effort:** 1–2 weeks (verifier pass only, no CLI changes).

### Phase 2 — Pure function inlining at constant call sites

**What:** Extend Phase 1 to inline calls to user-defined pure functions when all arguments are compile-time-known. A function is pure if its body references no I/O builtins (`rd`, `wr`, `env`, `now-ms`, `rand-bytes`, `http-*`, etc.) either directly or transitively.

**Scope:** Add a purity inference pass (no annotation required). Mark functions in the function table as `pure: true/false`. The constant-folding pass uses this to decide whether to evaluate a call.

**Success criterion:** A program with a pure helper computing a constant used in 10 places folds to a single constant with no function call overhead.

**Estimated effort:** 2–3 weeks (purity inference + inlining).

### Phase 3 — `comptime-assert` builtin

**What:** Add `comptime-assert cond msg` to the stdlib. At verify time, if `cond` is a compile-time-constant `false`, the verifier emits `ILO-C001: compile-time assertion failed: <msg>` and halts. If `cond` is not compile-time-known, `comptime-assert` is a no-op (it does not become a runtime assertion — use a guard for that).

**Signature:** `comptime-assert b:b msg:t>_`

**Agent usage:**
```
comptime-assert >MAX-BATCH 0 "MAX-BATCH must be positive"
comptime-assert <MAX-BATCH 1001 "MAX-BATCH must not exceed 1000"
```

**Spec addition:** One builtin name, one diagnostic code `ILO-C001`. ~50 tokens in the spec.

**Estimated effort:** 3–5 days.

---

## Follow-up Tickets

| Title | Why |
|-------|-----|
| Purity inference for the function table | Prerequisite for Phase 2 inlining; also useful for future parallelism analysis |
| `comptime-assert` builtin (ILO-C001 diagnostic) | Phase 3 above; stand-alone if Phase 1/2 slip |
| Lightweight generic type consistency (type variable unification) | Separate from comptime but surfaced in this research; fixes the `L a → L a` weakness |
| Benchmark: constant-folding token savings in real programs | Validates the "zero spec cost" claim with data; feeds the Manifesto's economics table |

---

## What This Research Does Not Recommend

- **A `comptime` keyword.** The spec cost is not justified when automatic folding delivers the same performance wins silently.
- **Lisp-style macros.** Breaks the closed-world guarantee, which is load-bearing for constrained generation.
- **Dependent/sized types.** High spec cost, wide agent error surface, no clear win in the token-cost table.
- **Annotation-required `const fn`.** Every function definition becomes a new decision point for agents. Automatic purity inference removes the annotation burden entirely.
