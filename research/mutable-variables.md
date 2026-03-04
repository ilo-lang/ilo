# Mutable Variables in ilo

Should ilo add mutable variables? This document examines how other languages handle mutability, which AI agent use cases appear to need it, what ilo can already express without it, and whether mutation is worth the cost.

---

## 1. How other languages handle mutability

### Rust — explicit opt-in with `let mut`

```rust
let x = 5;       // immutable — rebinding is a compile error
let mut y = 5;   // mutable — y = y + 1 is allowed
```

Rust defaults to immutable. Mutation requires a deliberate keyword at the declaration site. The compiler enforces this: assigning to a non-`mut` binding is a hard error. The motivation is both for the programmer (mutation is a signal worth marking) and for the compiler (immutability enables aggressive optimisation and safe parallelism).

Tradeoff: every mutable local requires extra syntax at declaration. In tight loops this is a non-issue. For functional transforms it nudges toward `map`/`fold` over explicit loops.

### Swift — `let`/`var` with value vs reference semantics

```swift
let x = 5       // immutable constant
var y = 5       // mutable variable
y += 1          // allowed
```

Swift adds an important dimension: `let` on a reference type still allows mutating the *object*, just not rebinding the *reference*. For value types (struct), `let` is deeply immutable. This distinction matters when reasoning about aliasing; it is less relevant for a language with no user-defined reference types.

Tradeoff: the `let`/`var` distinction is simple and readable. It forces a decision at declaration time, which is the right time to decide. The cost is 1 extra keyword.

### Kotlin — `val`/`var` mirroring Swift

```kotlin
val x = 5   // immutable
var y = 5   // mutable
y++
```

Kotlin's convention is to prefer `val`. IntelliJ warns when `var` could be `val`. The distinction is otherwise identical to Swift.

### Python / JavaScript — mutable by default

```python
x = 5
x = x + 1  # no ceremony — just reassign
```

```js
let x = 5;   // mutable (despite "let")
x = x + 1;
const y = 5; // immutable — reassignment is a TypeError
```

Python has no immutability mechanism for local variables at all. JavaScript's `const` prevents rebinding (not deep mutation). Both default to mutable. The cost: reasoning about a variable's value at any point requires tracing all assignments. The benefit: zero ceremony for simple mutation.

### Elixir / Erlang — rebinding is not mutation

```elixir
x = 5
x = x + 1  # this is rebinding, not mutation — creates a new binding
```

Elixir is process-isolated, message-passing, and built on immutable values. What looks like assignment is pattern matching with rebinding. No process can mutate another's state. "Mutation" in Elixir means `Agent`/`GenServer` (explicit state management processes), not local variable assignment.

Tradeoff: local rebinding is allowed for convenience (unlike Erlang, which forbids even that). The immutability guarantee is at the *value* level, not the *name* level. This is the same model ilo currently has — `x=expr` can appear twice with the same name, shadowing the earlier binding.

### Haskell — purely functional, no mutation at all

```haskell
let x = 5
    x' = x + 1  -- new name — Haskell forbids shadowing in the same scope
in x'
```

Haskell has no mutation in pure code. State that changes over time is modelled with `IORef`, `STRef`, or monadic threading. Loops are recursion. Accumulators are function parameters.

Tradeoff: total purity enables equational reasoning and lazy evaluation. The cost is verbosity for stateful algorithms (explicit monad threading).

### Clojure — immutable by default, explicit atoms for mutation

```clojure
(def x 5)                  ; immutable
(def counter (atom 0))     ; explicit mutable reference
(swap! counter inc)        ; mutation via swap! — thread-safe CAS
```

Clojure's model distinguishes *identity* (a named, mutable reference) from *value* (an immutable snapshot). Atoms, refs, and agents are explicit mutation containers. Normal `def`/`let` are immutable. Mutation is rare and deliberate.

Tradeoff: very clear separation between pure and stateful code. The cost: every counter or accumulator needs a container (`atom`) and explicit swap syntax. More tokens, less casual mutation.

### Summary table

| Language | Default | Mutation mechanism | Annotation cost |
|----------|---------|-------------------|-----------------|
| Rust | immutable | `let mut` | 1 keyword at declaration |
| Swift/Kotlin | immutable | `var` | 1 keyword at declaration |
| Elixir | immutable values | rebinding (cosmetic) | 0 |
| Haskell | immutable | `IORef`/monad | high — architecture change |
| Clojure | immutable | `atom`/`swap!` | 2 tokens per mutation |
| Python/JS | mutable | none — just reassign | 0 |

The pattern: languages designed after 1995 default to immutable and make mutation explicit. Languages designed before (or Python, which prioritised simplicity) default to mutable and make immutability explicit.

---

## 2. AI agent use cases that appear to need mutation

### Counters and retry loops

```python
retries = 0
while retries < 3:
    result = call_api()
    if result.ok:
        break
    retries += 1
```

This is the canonical "needs mutation" example. The counter increments per iteration. In a purely functional language, this becomes a recursive function with the counter as a parameter.

### Accumulating results

```python
results = []
for item in items:
    r = process(item)
    if r.ok:
        results.append(r.value)
```

Building a filtered list over iteration. In functional languages this is `filter(map(items, process))` or a fold.

### State machines

```python
state = "idle"
for event in events:
    if state == "idle" and event == "start":
        state = "running"
    elif state == "running" and event == "stop":
        state = "idle"
```

Tracking a current state across events. The state variable mutates on each transition. In functional languages, the current state is a parameter threaded through a recursive function.

### Caching / memoization within a function

```python
cache = {}
def fib(n):
    if n in cache:
        return cache[n]
    result = fib(n-1) + fib(n-2)
    cache[n] = result
    return result
```

A mutable dict accumulates computed values. ilo programs are short-lived (one task → one program → one result) and operate on small inputs. Memoization across invocations is a non-concern. Memoization within a single function call is possible via closure or explicit recursive threading.

### Building a string or list incrementally

```python
parts = []
for item in items:
    parts.append(format(item))
result = ", ".join(parts)
```

Accumulating formatted strings. The list accumulates across iterations.

### Are these genuinely needing mutation?

Almost all of these cases can be expressed functionally:

- **Counters**: recursion with counter as parameter, or `wh` loop with rebinding
- **Accumulation**: `@` (foreach) with accumulator, or `fld` (fold) when implemented
- **State machines**: recursion with state as parameter
- **Memoization**: not relevant to ilo's execution model
- **Incremental list building**: `@` loop with `+=` append

The key insight is that "needing mutation" and "being most naturally expressed with mutation" are different things. Most agent tasks are simple enough that the functional form is equally natural.

---

## 3. Analysis: does ilo need mutable variables?

### What ilo can already handle

**Counters via recursion:**
```
-- Count occurrences of value v in list xs
cnt xs:L n v:n>n;=len xs 0 0;h=hd xs;t=tl xs;r=cnt t v;=h v{+r 1};+r 0
```

**Accumulation via foreach:**
```
-- Sum a list of numbers
sm xs:L n>n;s=0;@x xs{s=+s x};+s 0
```

This already works. The `@` body can rebind `s` on each iteration. `s` after the loop holds the final value. This is the current idiom — no mutation annotation needed.

**Filtering via foreach:**
```
-- Keep only positive numbers from a list
pos xs:L n>L n;out=[];@x xs{>x 0{out=+=out x}};+out []
```

**Retry loop via while:**
```
-- Try up to 3 times, return first success or last error
retry url:t>R t t;i=0;r=^"";wh <i 3{i=+i 1;r=get url;?r{~v:ret ~v;^_:_}};r
```

**State machine via recursion:**
```
-- Traffic light cycle: red → green → yellow → red
nxt st:t>t;=st "red"{"green"};=st "green"{"yellow"};"red"
```

For event-driven state machines (state evolves with input):
```
-- Process events one at a time, track state
step st:t ev:t>t;=st "idle"{=ev "start"{"running"}{"idle"}};=st "running"{=ev "stop"{"idle"}{"running"}};"idle"

-- Process all events
run evs:L t>t;s="idle";@ev evs{s=step s ev};+s ""
```

**Building a list incrementally:**
```
-- Format each item and collect
fmt xs:L t>L t;out=[];@x xs{out=+=out +x "!"};+out []
```

### What cannot be expressed without rebinding

The `wh` loop already uses variable rebinding. The SPEC shows this explicitly:

```
f>n;i=0;s=0;wh <i 5{i=+i 1;s=+s i};s
```

Variable rebinding (`i=+i 1`) inside while loops updates the variable. This is already in the language as an implementation choice for the `wh` construct. So ilo *already has* local rebinding — the question is whether to formalise this as "mutation" with explicit syntax.

### Cases that genuinely require state that outlives a function call

These are the true mutation cases:
- Shared mutable state between concurrent tasks
- Accumulating state across multiple calls (e.g., a running total across separate invocations)
- Event listeners / reactive state

ilo has no concurrency model and programs are single-execution. These cases don't apply.

### Token cost: functional vs imperative alternatives

| Pattern | Functional (ilo today) | Imperative (hypothetical `mut`) |
|---------|----------------------|-------------------------------|
| Sum a list | `s=0;@x xs{s=+s x};+s 0` (8 tokens) | same — `wh` already allows rebinding |
| Retry loop | `i=0;wh <i 3{...i=+i 1...}` (already works) | same |
| Counter in recursion | `cnt t n+1` (pass as param, 0 overhead) | `mut c=0;c=+c 1` (adds 1 keyword) |
| State machine | recursion, 0 overhead | rebinding already works in loops |

The functional alternatives are not significantly longer in ilo because:
1. `@` (foreach) with accumulator is already idiomatic
2. `wh` (while) with rebinding is already in the language
3. Recursion with parameters is 0 overhead at the call site

### Verifier cost of tracking mutation

If mutation were annotated (`mut x=0`), the verifier would need to:
- Track which variables are mutable at each point
- Allow type-preserving reassignment only on `mut` variables
- Error on reassignment of non-mutable variables
- Propagate type through the chain of reassignments

This is non-trivial. Immutability is what makes the current verifier simple: every binding is a new name with a fixed type. With mutation, the verifier needs SSA-style type merging at join points (after loops, after branches). This is a significant complexity increase for a runtime verification system meant to be fast and clear in its errors.

---

## 4. If ilo were to add mutation: the options

### Option A: `mut x=0` to declare, `x=+x 1` to update (Rust-style opt-in)

```
-- Declare mutable, then reassign
mut s=0;@x xs{s=+s x};+s 0
```

Syntax cost: `mut` is 1 token at declaration. Reassignment syntax is already `x=expr`. No new update operator needed — `x=+x 1` reads as "rebind x to x+1".

Verifier impact: the verifier must distinguish `mut` from non-`mut` bindings. Reassigning a non-`mut` binding would be an error. This adds one bit of state per binding in the verifier's environment.

Token cost vs current: +1 token per mutable variable (`mut` keyword). Since the current language already allows rebinding in loop bodies (by implementation), this would just formalise what already works.

Problem: `mut` is 1 token but is also a 3-character English word — it tokenises as a single token in cl100k_base. Cost is low. But the question is whether forbidding rebinding outside `mut` declarations actually catches bugs that matter for AI-generated code.

### Option B: Special loop variable — mutation only inside loops

```
-- Only @-loop variables and wh-body variables can be rebound
-- No explicit annotation needed
@x xs{x=transform x}   -- x is rebindable because it's a loop var
s=0;@_ xs{s=+s 1}      -- s is rebound in loop body (allowed because in loop scope)
```

This would mean: inside a `@` or `wh` body, any variable in the enclosing scope can be rebound. Outside loops, all bindings are immutable.

Token cost: 0 — no new syntax. The scoping rule is enforced by the verifier.

Problem: the scoping rule is subtle. An agent generating code would need to know that rebinding is only allowed inside loop bodies. This makes the language harder to describe in a spec and harder for an LLM to generate correctly.

### Option C: Don't add explicit mutation — rely on rebinding + recursion

The status quo. `wh` already allows rebinding inside the loop body (this is documented in the SPEC). `@` can accumulate state via rebinding. Recursion handles the rest.

Token cost: 0 new syntax. Current idioms:
```
-- Running counter
i=0;wh <i 10{i=+i 1;...}

-- Accumulator
acc=[];@x xs{acc=+=acc x}

-- State via recursion
step st ev>t;...
run evs:L t>t;s="idle";@e evs{s=step s e};+s ""
```

This is already the ilo idiom. The SPEC documents it. No language change needed.

### Option D: `var` keyword, Kotlin/Swift style

```
var i=0      -- mutable, must declare type implicitly
let s="hi"   -- immutable (existing default behaviour)
```

Token cost: `var` is 1 token. Requires the verifier to distinguish `var` from `let`. `let` would need to be added as a keyword too (currently ilo uses bare `x=expr`), or the default bare binding would remain immutable and `var` would opt in.

Syntax tension: ilo currently uses `x=expr` for binds (the `let` keyword was explicitly dropped as a cost saving — see OPEN.md: "Syntax Questions"). Adding `var` as the mutable keyword would mean `var x=expr` for mutable and `x=expr` for immutable. That is actually clean and low-overhead.

But: do we then need to also add `let` for clarity? Kotlin-style `val`/`var` is symmetrical and clear. ilo's `x=expr`/`var x=expr` is asymmetrical. Asymmetry can be confusing for an LLM generating code.

---

## 5. Recommendation: do not add explicit mutation syntax

### Verdict

ilo should not add a `mut` keyword, `var` keyword, or any explicit mutation annotation. The existing rebinding semantics — already present and documented in `wh` loops — are sufficient. Here is why.

### Reason 1: The current model already works for the real cases

All AI agent use cases that appear to need mutation can be expressed in current ilo:

- Counters: `i=0;wh <i n{i=+i 1;...}` — rebinding already works
- Accumulation: `acc=[];@x xs{acc=+=acc x}` — rebinding already works
- State machines: recursion with state parameter, or `@` with rebinding
- Retry loops: `wh` with rebinding

The SPEC already documents and blesses this pattern. No new language feature is needed.

### Reason 2: Token cost of adding syntax is non-zero; benefit is marginal

Adding `mut x=0` vs `x=0` costs 1 token per mutable variable. For a language where 1 token matters, this is only justified if it catches real bugs. For AI agent programs:
- Programs are short (one function, one task)
- Variables are few (ilo's density means few intermediate names)
- Mutation bugs are rare (the agent doesn't confuse its own variable names)

The benefit of "mutation is annotated" accrues over long-lived codebases where human engineers need to reason about aliasing. ilo programs are single-use, agent-generated, and short. The annotation buys nothing.

### Reason 3: Enforcement would make the verifier complex for marginal gain

Currently the verifier treats every binding as a new name with a fixed type. This is simple and fast. Adding mutation tracking requires:
- Distinguishing mutable vs immutable bindings in the type environment
- Allowing type-preserving reassignment only on mutable names
- Tracking type through reassignment chains (loop variable `i` starts as `n`, stays `n`)
- Handling join points after loops (what is `acc`'s type after the loop? it started as `L n` and remained `L n`, but the verifier must check this)

This is SSA-style analysis. It is not impossible, but it adds significant complexity to a verifier that is currently simple enough to be described in one paragraph. The complexity cost is not justified by the user benefit.

### Reason 4: ilo's execution model makes mutation less necessary than in other languages

Languages need mutable variables for:
1. **Shared state between threads**: ilo has no concurrency model
2. **State persistence across calls**: ilo programs are single-execution
3. **In-place update of large data structures**: ilo values are small (tool results, not databases)
4. **Performance**: ilo's performance target is "fast enough for agent tasks"; it is not a systems language

The motivations that make mutation essential in other languages do not apply here.

### Reason 5: The LLM generation model favours consistent rules

ilo's spec is the prompt for the LLM that generates ilo code. A rule like "variables are always rebound with `x=expr`, no annotation needed" is simpler to generate correctly than "variables are immutable by default; use `mut x=expr` for mutable ones; but rebinding in loop bodies is always allowed; the verifier will catch violations."

Simpler rules → fewer generation errors → fewer retries → lower total token cost. This is the manifesto metric.

### What to document instead

The spec should explicitly document the rebinding model:

- `x=expr` creates a binding. If `x` is already in scope, the new binding shadows the old one.
- Inside `wh` and `@` bodies, rebinding the outer scope variable updates it for subsequent iterations. This is the idiomatic way to accumulate state in a loop.
- Outside loops, shadowing is rarely useful and may indicate a logic error (the verifier could optionally warn).

This documents the status quo without adding syntax or changing semantics.

### The one case worth revisiting: explicit warning on shadowing

If an agent accidentally writes:

```
s=0;s=+s 1;s   -- shadows, doesn't accumulate
```

outside a loop, the second `s=+s 1` shadows the first `s=0` and then `s` resolves to `1`. This is correct ilo but probably not what was intended (the first binding is dead). A verifier warning for "binding shadows unused earlier binding" would catch this class of bug without requiring explicit mutation annotations.

This is a diagnostic improvement, not a language change. It follows the existing verifier warning infrastructure (C3 suggestions) and costs zero tokens at generation time.

---

## Concrete ilo patterns for "mutation-like" use cases

### Summing a list

```
sm xs:L n>n;s=0;@x xs{s=+s x};+s 0
```

`s` is rebound on each iteration. After the loop, `+s 0` forces a binary expression (required for non-last function safe ending — see SPEC patterns).

### Counting items matching a predicate

```
cnt xs:L n pred:n>n;c=0;@x xs{r=pred x;r{c=+c 1}};+c 0
```

`c` accumulates. The guard `r{c=+c 1}` increments only when predicate returns true.

### Building a filtered list

```
pos xs:L n>L n;out=[];@x xs{>x 0{out=+=out x}};+out []
```

`out` grows on each positive element. `+=out x` appends `x` to `out`, rebound immediately.

### Retry with exponential backoff (conceptual)

```
retry url:t>R t t;i=0;last=^"no attempts";wh <i 3{r=get url;?r{~v:ret ~v;^e:last=^e};i=+i 1};last
```

`i` counts retries, `last` holds the most recent error. Both rebound in the loop body. Returns early on success (`ret ~v`), returns the last error on exhaustion.

### State machine over event stream

```
-- Transition function: pure, no mutation
tr st:t ev:t>t;=st "idle"{=ev "start"{"running"}{"idle"}};=st "running"{=ev "stop"{"idle"}{"running"}};"idle"

-- Runner: accumulate state via rebinding
run evs:L t>t;s="idle";@ev evs{s=tr s ev};+s ""
```

The transition function `tr` is pure. The runner `run` threads state through `@`, rebinding `s` on each event. No mutation annotations needed.

### Fibonacci iteratively (avoiding recursion overhead)

```
fibi n:n>n;<=n 1 n;a=0;b=1;i=0;wh <i n{t=+a b;a=b;b=t;i=+i 1};+b 0
```

Three rebound variables (`a`, `b`, `i`) in a while loop. Idiomatic ilo.

---

## Summary

Mutable variables are not needed as a distinct language construct in ilo. The language already has:
- Variable rebinding in loop bodies (`wh`, `@`)
- Recursion with accumulator parameters
- `@` foreach with mutable accumulator idiom
- `wh` while with multiple rebound variables

Adding explicit mutation syntax (`mut`, `var`) would cost tokens at generation, complexity in the verifier, and description complexity in the spec, while providing essentially no benefit for the short, single-execution, agent-generated programs ilo is designed for.

The right response to "does ilo need mutable variables?" is: it already has the capability it needs, expressed through rebinding, and that is enough.
