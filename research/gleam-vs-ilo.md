# Gleam vs ilo — Feature Audit for 0.13.0

**Date:** 2026-05-22
**Auditor:** ILO-37
**Scope:** Gleam 1.x language features vs ilo 0.12.x; evaluate fit against the ilo Manifesto six principles.

---

## Background

Gleam is a statically typed functional language targeting Erlang OTP and JavaScript. It prioritises clarity, safety, and friendly error messages for human developers. ilo is a token-optimised language for AI agents — every decision evaluated against total token cost. These two languages have different primary audiences but overlap in: type-safe functional programming, explicit error handling, and pattern matching. This audit compares them to find features worth absorbing, surfaces where ilo is ahead, and areas of overlap.

---

## Features Gleam Has That ilo Does Not

### 1. `use` Expression — Flattener for Callback Chains

**Gleam:** `use` eliminates callback nesting by making all code following `use` an anonymous function passed as the final argument. Canonical usage: flattening `Result`-returning function chains without nested `case`.

```gleam
use user <- result.try(get_user(id))
use profile <- result.try(get_profile(user))
profile.name
```

**ilo today:** Chains of `R T E` operations require either nested `?r{~v:...;^e:...}` match arms or the `!` auto-unwrap sigil inside a Result-returning function. The `!` sigil is ilo's current answer but only propagates errors — it does not flatten arbitrary callbacks.

**Manifesto fit:** HIGH. The `use` pattern directly reduces nesting depth, which reduces token cost and cognitive load for agents. The Manifesto explicitly calls out guard-based flat flow as preferable to nested if/else; `use` is the same principle applied to monadic chains. The ticket description ranks this #1.

**Recommendation:** Absorb as a variant form — specifically a `>>=` chain sugar or `use`-style flattener for `R T E` sequences. Candidate syntax: `use name <- expr` mirroring Gleam, or a new sigil. Needs design work to fit ilo's prefix-first model. File a design ticket.

**Verdict: ABSORB** — design ticket recommended.

---

### 2. Typed `todo` / `panic` Expressions

**Gleam:** `todo` and `panic` are first-class typed expressions. They can appear anywhere a value is expected (e.g. in a `case` arm, as a function body stub). The type checker accepts them because they diverge — they never produce a value. `todo` is for unimplemented code; `panic` is for unreachable invariants.

```gleam
fn do_thing(x: Thing) -> Result(Value, Error) {
  todo  -- type-checks as Result(Value, Error)
}
```

**ilo today:** ilo has no `todo`/`panic` equivalent. An unimplemented function requires returning a placeholder value (e.g. `^"not implemented"`), which poisons the `R T E` type and may cause cascading type errors in callers. There is no way to mark code as intentionally unfinished.

**Manifesto fit:** HIGH. `todo` reduces the token cost of scaffolding: agents write stubs as they decompose problems, then fill them in. Without `todo`, every stub must return a valid-typed placeholder, adding noise and potentially misleading the type checker. The Manifesto's Constrained principle wants the right token to be obvious; a `todo` that always type-checks is maximally constrained.

**Recommendation:** Absorb. Suggest `todo` as a statement/expression that is typed as the inferred return type of the enclosing function. `panic` can be a variant with a message. Low implementation cost — the verifier already handles `^` (throw) as an escape; `todo` is the same mechanism with a standard message.

**Verdict: ABSORB** — straightforward implementation ticket.

---

### 3. `let assert` — Deliberate Crash Pattern

**Gleam:** `let assert Ok(value) = result` is an explicit crash-on-mismatch that documents programmer intent. Different from a regular pattern match — it says "I assert this will always be Ok; crash if not."

**ilo today:** The `!` sigil does auto-unwrap at the call site (`expr!`). However, there is no structural equivalent for arbitrary patterns — you can auto-unwrap `R T E` and `O T`, but you cannot assert a custom type variant and extract its fields in a single form.

**Manifesto fit:** MEDIUM. Useful for test code and prototyping where agents want to assert known shapes without full error handling. The token savings are modest (versus `?result{~v:v;^_:^"assert failed"}`), but the intent-signalling is valuable.

**Recommendation:** Absorb in a limited form: extend `!` to pattern positions on custom sum types. Low priority vs #1 and #2.

**Verdict: CONSIDER** — lower priority.

---

### 4. Opaque Types (Smart Constructors)

**Gleam:** `pub opaque type Email { Email(String) }` — the type is public but the constructor is private. External callers must use a validated constructor function; internal module code can access the raw value.

**ilo today:** ilo has `S a b c` sum types (closed text variants) but no general opaque wrapper. There is no way to define a validated newtype that prevents callers from constructing invalid values directly.

**Manifesto fit:** LOW-MEDIUM. Opaque types enforce invariants without runtime cost, which is valuable for correctness. However, the Manifesto's Constrained principle already enforces a closed world — all callables are known ahead of time. The primary agent safety comes from the type system, not from encapsulation. Opaque types are most useful for library authors; in the ilo agent-use-case, programs tend to be small and self-contained.

**Recommendation:** Skip for 0.13.0. Worth revisiting if ilo gains a proper module/package system with user-distributed libraries.

**Verdict: SKIP (for now).**

---

### 5. Record Update Syntax

**Gleam:** `let updated = User(..user, name: "New Name")` — spread-and-override for records. Only changed fields need to be specified.

**ilo today:** ilo has anonymous records (dot-access on map values, `RunResult` from `run2`) but no first-class user-defined records with update syntax. Maps can be updated with `mset` but typed records cannot be partially updated — they require full reconstruction.

**Manifesto fit:** MEDIUM. Record update syntax saves tokens when modifying large records. However, ilo's type system doesn't have structural records as a first-class user-defined construct (only `S` sum types and map-backed anonymous records). This is a larger type system change.

**Recommendation:** Skip for 0.13.0. Depends on broader record-type work.

**Verdict: SKIP.**

---

### 6. Pipeline `|>` with Argument Position Flexibility

**Gleam:** `a |> f(b)` → `f(a, b)` — the piped value always fills the first argument. Gleam's `|>` is positional (first slot), same as ilo's `>>`.

**ilo today:** ilo has `>>` pipe (`a>>f` → `f a`) and also prefix notation which subsumes most pipeline needs. ilo's pipe is unary (single-argument functions). Gleam's pipeline is essentially the same but allows extra arguments in the call position.

**Manifesto fit:** N/A — ilo's `>>` already covers the same use case.

**Verdict: ALREADY COVERED.**

---

### 7. Erlang/OTP Concurrency and Actor Model

**Gleam:** Deep integration with Erlang's actor model — processes, message passing, supervisors, fault tolerance. This is a core Gleam differentiator for building distributed, fault-tolerant systems.

**ilo today:** ilo has `get-many` for concurrent HTTP fan-out and `par-map` (in feature branch) for parallel map. No actor model, no message passing, no supervision trees.

**Manifesto fit:** LOW. The Manifesto is explicitly about minimising token cost for agents writing programs. Distributed actor systems are a different target audience. ilo is designed for scripting and task automation, not for building OTP services.

**Recommendation:** Skip. This is an architectural divergence, not a feature to absorb.

**Verdict: SKIP.**

---

### 8. JavaScript Compilation Target

**Gleam:** Compiles to both Erlang and JavaScript with TypeScript definition generation. First-class cross-runtime support.

**ilo today:** ilo has WASM support and a JS/WASM runtime path. Not a full JS compilation target in the Gleam sense.

**Manifesto fit:** LOW — out of scope for language feature audit.

**Verdict: OUT OF SCOPE.**

---

### 9. Module-Level Constants

**Gleam:** `const max_retries = 3` — module-level immutable values. Can be any literal, not function calls.

**ilo today:** ilo has no module-level constants. All bindings live inside function bodies. Top-level `name=expr` is an `ILO-P102` error that suggests wrapping in `main>_;`.

**Manifesto fit:** MEDIUM. Constants reduce repeated literal tokens — a named `max-retries` is clearer than scattering `3` everywhere, and agents benefit from consistent naming. However, ilo's self-contained principle means each function carries its own context; a module-level constant is ambient state. The token cost of defining and using a constant vs inlining the literal is probably neutral for small programs.

**Recommendation:** Skip for 0.13.0. If ilo gains multi-file module support, revisit.

**Verdict: SKIP.**

---

### 10. Alternative Patterns with `|` in Case Arms

**Gleam:**
```gleam
case x {
  1 | 2 | 3 -> "small"
  _ -> "large"
}
```

**ilo today:** ilo's `?` match supports multi-arm pattern matching. The SPEC mentions `|` alternatives are on the roadmap (ticket description item #3 explicitly calls out auditing `?` for `|` alternatives). Currently ilo match arms do not support `|` alternatives within a single arm.

**Manifesto fit:** HIGH. `|` alternatives reduce token cost when multiple values map to the same result. Without them, agents must duplicate the arm body or introduce an intermediate function.

**Recommendation:** Absorb. Ticket description item #3 already identifies this. The implementation is an extension to the match parser to accept `|`-separated patterns for a single arm body.

**Verdict: ABSORB** — already on the radar per ticket description.

---

### 11. Multiple Subjects in Case

**Gleam:**
```gleam
case a, b {
  True, True -> "both"
  True, False -> "only a"
  _, _ -> "neither or b only"
}
```

**ilo today:** ilo's `?` match operates on a single subject. Multi-subject matching requires either nested match expressions or tuple-like encoding.

**Manifesto fit:** MEDIUM-HIGH. Multi-subject matching eliminates the need to nest `?` expressions for comparing two values simultaneously, reducing depth and token count.

**Recommendation:** Absorb. Extend `?` syntax to accept comma-separated subjects. Ticket description item #3 mentions "exhaustive multi-subject" audit.

**Verdict: ABSORB** — already on the radar per ticket description.

---

### 12. Pattern Guards with `if`

**Gleam:**
```gleam
case score {
  n if n >= 90 -> "A"
  n if n >= 80 -> "B"
  _ -> "C"
}
```

**ilo today:** Guards in ilo are standalone guard statements (`>=score 90 "A"`), not guards attached to pattern arms. There is no way to attach a condition to a specific match arm — guards are top-level and always cause early return.

**Manifesto fit:** MEDIUM. Guards attached to match arms enable more expressive pattern matching without separate nesting. However, ilo's top-level guard syntax already handles the numeric range pattern shown above. The savings are incremental.

**Recommendation:** Consider as part of the `|` alternatives / multi-subject work (ticket description item #3 bundles these together). Not standalone priority.

**Verdict: CONSIDER** — bundle with alternatives work.

---

### 13. Documentation Comments (`///`)

**Gleam:** First-class doc comments extracted by the toolchain for documentation generation.

**ilo today:** ilo has `--` single-line comments only. No doc comment convention, no toolchain-extracted documentation.

**Manifesto fit:** LOW for agents (agents don't read doc comments), but MEDIUM for human-facing tooling. The Manifesto acknowledges spec clarity as a token cost; good docs reduce spec-loading cost.

**Recommendation:** Low priority. Could add a `---` doc comment convention without language change.

**Verdict: SKIP for 0.13.0.**

---

### 14. `@deprecated` Attribute

**Gleam:** Mark functions as deprecated with a migration message. Compiler emits warnings when deprecated functions are called.

**ilo today:** No deprecation mechanism. Breaking changes require CHANGELOG entries and version pragma (`^26.5`).

**Manifesto fit:** LOW for the current single-file use case. Relevant if ilo gains library distribution.

**Verdict: SKIP.**

---

### 15. String Prefix Patterns (`<>` in case)

**Gleam:**
```gleam
case s {
  "Hello, " <> name -> name
  _ -> "unknown"
}
```

**ilo today:** ilo string matching in `?` is exact equality on literal text. No prefix/suffix extraction in pattern position.

**Manifesto fit:** MEDIUM. String prefix patterns save tokens vs `has s "Hello, "` + `slc` to extract the suffix. Agents frequently need to route on string prefixes (e.g. command dispatch).

**Recommendation:** Consider for a future release. Not critical for 0.13.0.

**Verdict: CONSIDER** — future release.

---

## Features ilo Has That Gleam Does Not

### 1. Prefix Notation

ilo's prefix operators (`+a b`, `*a b`, `>=a b`) eliminate parentheses at every nesting level, saving 22% tokens and 42% characters vs infix (per internal benchmark). Gleam uses standard infix notation exclusively.

**Assessment:** Core ilo differentiator. Not worth changing.

### 2. Auto-Unwrap Sigil (`!` and `!!`)

`expr!` unwraps `R T E` inside a Result-returning function, propagating errors automatically. `expr!!` crashes on Err. Gleam has `use` for chaining but no sigil shorthand.

**Assessment:** ilo's `!` is more terse for the common single-step unwrap case. `use` is better for multi-step chains. Both are worth keeping.

### 3. Built-in HTTP, File I/O, and Process Spawn

ilo ships `get`, `pst`, `rd`, `wr`, `run`, `run2`, etc. as first-class builtins. Gleam has standard library modules but they are not builtins — they require imports.

**Assessment:** ilo's closed-world builtin model is core to the Constrained principle. Not a Gleam feature to absorb.

### 4. Structured JSON (`--json`) CLI Output

Every ilo CLI subcommand outputs machine-readable JSON. Gleam has no equivalent agent-facing structured output contract.

**Assessment:** Core ilo differentiator. Not applicable to Gleam.

### 5. Guard-Based Flat Control Flow

ilo's guards are flat, vertical, and constant-depth regardless of condition count. Gleam uses nested `case` for conditional chains. ilo's approach consistently saves tokens for multi-condition flows.

**Assessment:** Core ilo differentiator. Gleam's `use` partially addresses this for monadic chains but not for pure boolean guards.

### 6. Prefix Ternary (`?=`, `?>`, `?<`, etc.)

Inline conditional expressions using operator+condition shorthand. No equivalent in Gleam.

**Assessment:** ilo-specific optimisation. Not a Gleam feature.

### 7. Deep Stdlib (Linear Algebra, Statistics, FFT, Crypto)

ilo ships `matmul`, `solve`, `lstsq`, `fft`, `stdev`, `quantile`, `hmac-sha256`, `rand-bytes`, etc. as builtins. Gleam's stdlib is intentionally minimal; these would require external packages.

**Assessment:** Core ilo value-add for agent use cases. Not applicable to Gleam.

### 8. Token-Optimised Short Names

`flt`, `fld`, `srt`, `hd`, `tl`, `trm`, `slc`, `unq` vs Gleam's `list.filter`, `list.fold`, `list.sort`, etc. ilo saves 1–3 tokens per call.

**Assessment:** Core ilo design. Not applicable to Gleam.

### 9. Nil-Coalesce `??` as Syntax

`??x default` / `a??b` is a first-class operator. Gleam uses `option.unwrap(opt, default)` (a function call, higher token cost).

**Assessment:** ilo advantage. Not a Gleam feature to absorb.

### 10. CalVer Runtime Pragma (`^26.5`)

File-level minimum-runtime declaration. Gleam uses semantic versioning in package config.

**Assessment:** ilo-specific mechanism. Not applicable.

---

## Areas of Overlap

| Feature | Gleam | ilo | Notes |
|---------|-------|-----|-------|
| Result type | `Result(T, E)` | `R T E` | Same semantics, different syntax |
| Optional type | `Option(T)` | `O T` | Same semantics |
| Sum types | `type Color { Red Green Blue }` | `S red green blue` | Gleam's are richer (can hold data per variant); ilo's are text-only closed enums |
| Pattern matching | `case` with exhaustiveness | `?` with exhaustiveness | Gleam has more arm features; ilo has prefix guards |
| Closures | `fn(x) { ... }` | `(x:t>t;...)` | Both support captures |
| Higher-order functions | `list.map(xs, fn)` | `map fn xs` | Same concept, different syntax/token cost |
| Pipelines | `\|>` | `>>` | Same semantics |
| Static types | Full inference | Explicit with inference | Gleam infers more; ilo requires explicit param types |
| No nulls | `Option`/`Result` only | `O T`/`R T E` only | Both eliminate null |
| No exceptions | `Result` for errors | `R T E` + `^` | Both avoid exceptions |
| Immutability | All values immutable | All values immutable | Same |
| Expression-based | No `return` keyword | Last expr is return | Same |

---

## Recommendations Summary

### Absorb for 0.13.0

| # | Feature | Priority | Rationale |
|---|---------|----------|-----------|
| 1 | **`use`-style flattener for `R T E` chains** | HIGH | Reduces nesting depth for multi-step error chains; manifesto fit is strong. Design as `use name <- expr` or extend `!` to multi-step form. |
| 2 | **Typed `todo` / `panic` expressions** | HIGH | Enables scaffolding without placeholder return values; low implementation cost (extend `^` mechanism). |
| 3 | **`\|` alternatives in match arms** | MEDIUM-HIGH | Token savings for multi-value same-result arms; already identified in ticket description. |
| 4 | **Multi-subject `?` match** | MEDIUM | Eliminates nested match for two-value comparisons; already identified in ticket description. |

### Consider for Later

| Feature | Notes |
|---------|-------|
| Pattern guards on match arms (`if`) | Bundle with `\|` alternatives work |
| String prefix patterns in match | Useful for command dispatch, not critical for 0.13.0 |
| `let assert` extension | Extend `!` to sum-type variant positions |

### Skip

| Feature | Reason |
|---------|--------|
| Opaque types | Needs module system; agent programs are self-contained |
| Record update syntax | Needs first-class record types |
| Module-level constants | Conflicts with self-contained principle; defer to module system work |
| Erlang/OTP actor model | Different audience entirely |
| JS compilation target | Architectural, not a language feature |
| Doc comments | Low agent value |
| `@deprecated` | Needs package distribution |
| Label shorthand (`name:`) | Labelled args already rejected by manifesto |

---

## New Tickets to File

### High Priority

**ILO-A1: `use`-style R T E chain flattener**
Design and implement a syntax for flattening multi-step `R T E` chains without nesting. Take inspiration from Gleam's `use` expression but evaluate against ilo's prefix-first model. Should reduce token cost of 3+ step error chains by eliminating nested `?{}` arms.

**ILO-A2: `todo` and `panic` typed expressions**
Add `todo` as a typed expression that satisfies any return type and emits a runtime error if reached. Add `panic msg` as a variant with a custom message. Both should appear valid to the type verifier (like `^` today). Token cost of implementation: extend `^` path in the verifier with a `Todo` node kind.

### Medium Priority

**ILO-A3: `|` alternatives in `?` match arms**
Extend the `?` match parser to support `|`-separated patterns in a single arm: `?x{"a"|"b":"found";"c":"other"}`. Reduces arm duplication. Bundle design with multi-subject match (ILO-A4).

**ILO-A4: Multi-subject `?` match**
Extend `?` to accept comma-separated subjects: `?a b{"ok" "ok":"both ok";"ok" _:"a ok";_ _:"other"}`. Eliminates nested `?{}` for two-value comparisons. Bundle design with ILO-A3.

---

## Bonus: Gleam Tour / Documentation Model

Gleam ships a 60-lesson interactive tour at `tour.gleam.run` with an `/everything` flat page that renders all tour content on one page. This is highly effective for agent spec-loading: a single fetch gives the entire language in one context window.

ilo's spec is SPEC.md — already a single flat file, which is equivalent. The `/everything` pattern is already implemented for ilo. No action needed, but worth noting that ilo is already ahead of most languages on this axis.

---

*End of audit. Four absorb candidates, four defer candidates, nine skips. The `use`-chain flattener and typed `todo`/`panic` are the highest-value items for 0.13.0.*
