# SPEC-AGENT-NATURAL.md

A surface-syntax experiment for ilo. Tests the hypothesis that **leaning into the syntax agents already reach for reduces total token cost more than the per-program token overhead it adds**, because retry tokens dominate generation tokens once friction stacks up.

This is a v0 experiment spec, not a replacement for `SPEC.md`. Everything in `SPEC.md` continues to apply unless explicitly overridden here. Semantics, type system, builtin signatures, and runtime are unchanged. We are only changing **which surface forms the skill docs lead with**, plus a small number of **additive parser accepts** (`if`/`else`/`for`/`while`, multi-statement match arm bodies). Existing programs keep parsing.

This branch (`compat/agent-natural`, off `next`) is built to be benchmarked: the same ~115 dogfood personas already measured against `main` get re-run here, and the deltas in tokens, tool_uses, wall-clock, and outcome are aggregated against the baseline in `/Users/dan/code/ilo_feedback/logs.md`. If the experiment doesn't pay for itself, the branch dies and the manifesto wins.

---

## 1. Goals and non-goals

### 1.1 Hypothesis under test

Across the ~115 persona runs already recorded on main, the same handful of surface-syntax frictions recur per persona:

- `??` prefix vs infix (7+ personas)
- `?h cond a b` ternary vs `cond{...}` braced-cond vs `?r{~v:;^e:}` match - picking the right shape (most personas)
- Match arm bodies are single-expression-only - forces helper-fn extraction (5+ personas, including bash fallback in one case)
- `@x xs` and `wh cond` look unfamiliar - personas hesitate or reach for `for x in xs` and produce ILO-P003

The aggregate cost of these isn't generation tokens. It's **retry tokens**. A persona that writes `??v 0` and gets ILO-P009 pays the full retry loop: load the error, load the spec section, regenerate, sometimes a second retry if the fix re-introduces a different shape.

The hypothesis is that a small set of surface concessions to "what the agent already reaches for" reduces retry rate enough to outweigh the +2 to +5 tokens per construct that the more familiar shape costs.

We test this by re-running the full persona suite on this branch and comparing token / tool_use / outcome deltas against the existing baseline.

### 1.2 Goals (v0)

- Lower aggregate retry rate by removing the four highest-frequency surface frictions surfaced by the persona logs.
- Keep every existing ilo program parsing. This branch is additive on the lexer / parser; deletions are limited to skill docs.
- Produce a clean A/B against `main` with the same personas, same models, same prompts, same task set. The only variable is the surface spec the agent sees.
- Decide. After re-run, either roll the wins into `next` (and update `SPEC.md` / `ai.txt` / skill docs), or kill the branch and log "we tested it; it didn't pay".

### 1.3 Non-goals (v0)

- Not introducing new semantics. No new evaluation order. No new types. No new builtins.
- Not changing any builtin signature. `map fn xs` stays `map fn xs`. No method-call sugar.
- Not changing the function signature line (`f x:n>n;body`). That's the load-bearing token-saver and 0 personas have logged friction with it.
- Not changing the prefix-Polish call shape generally. We are leaving builtin calls and user-fn calls alone.
- Not changing record/list literal syntax.
- Not introducing whitespace-significant blocks. Newlines remain optional throughout.
- Not removing any existing form. Every change below is **add a synonym** + **lead the skill docs with the new form**.

### 1.4 What this experiment is willing to spend

A persona-average per-program token overhead of up to **+15% generation tokens** is acceptable if the aggregate retry-loop cost drops enough that **total tokens per task** (generation + retries + context) falls vs the baseline.

The falsification criterion is in section 6. If retry cost doesn't drop, or drops by less than the generation overhead, we lose.

---

## 2. Surface changes

Each subsection is a concrete syntactic decision with examples, the rationale, and the persona-log evidence motivating it.

### 2.1 Infix as canonical for arithmetic, comparison, and logical operators

**Current SPEC.md:** prefix is canonical, infix is "available for readability when needed". Skill docs lead with prefix.

**Agent-natural mode:** infix is canonical for arithmetic (`+ - * /`), comparison (`= != > < >= <=`), and logical (`& |`). Prefix forms remain fully accepted. Skill docs lead with infix.

```
-- canonical (was: prefix preferred)
total = a + b * c
ok    = x >= 0 & x <= 100
```

**Justification.** The manifesto's prefix argument is real and measured: across 25 expression patterns, prefix saves 22% tokens vs infix. But the persona logs show the friction is asymmetric: infix is **the agent's first try every time**, and the resulting retries dominate. Specific shapes that recur in the logs:

- `+ -*a b *c d` → ILO-P021 double-minus trap, with a hint pointing back to a prefix form the agent then has to re-derive.
- `*/a b c` parsed as `(a/b)*c` instead of `(a*b)/c` - silent arithmetic gotcha; bit a forensics persona and an RK4 persona.
- `++++r1 r2 r3 r4` silent-EOF on 4-deep prefix chains.
- `+ - +x y z` - prefix-chain ergonomics that compile but read backwards.

The 22% generation saving is real, but in practice ~25% of prefix programs in the persona corpus hit a parse re-roll or a silent-miscompile that costs more than 22% to recover from.

**v0 decision:** infix-canonical in skill docs and examples. Prefix continues to parse and continues to be the cheaper form for an agent that has been **trained on it**. We are testing whether *current* untrained models do better with infix because their training prior is so strong.

### 2.2 One conditional, with a Result-match carve-out

**Current SPEC.md:** ilo today has *three* conditional shapes plus several variations:

- `cond{body}` - braced conditional, **no early return**
- `cond a b` braceless guard - **early return** when condition is comparison/logical
- `?cond{a}{b}` - ternary, value-producing
- `?=cond a b` / `?>cond a b` / `?<cond a b` - prefix ternary family
- `?h cond a b` - prefix-ternary keyword form
- `?subj{arm:;arm:;_:}` - match
- `?r{~v:;^e:}` - Result match
- Plus negated variants of most of the above

The persona logs are full of agents picking the wrong shape and paying for it. Quoting from logs.md:

> `=mhas m k` inside `@` loop body triggers braceless-guard parse - `=mhas m k true{body}` was read as guard-condition `=mhas m k` with body `true` then unexpected `{body}`.

> `=cond{val}` reads like "if cond, return val" but it isn't. Required a dedicated footgun note in SPEC.md.

> bool match arms `?bk{"x"}{"y"}` parse incorrectly; required braced-conditional or helper-function workaround.

**Agent-natural mode:**

- **Primary conditional in skill docs:** `if cond { a } else { b }`. Value-producing. No early return (matches the current ternary semantics). `else` is optional; absent `else` produces `nil`.

  ```
  v = if x >= 0 { x } else { -x }
  if found { log "hit" }
  ```

- **Early return** is `if cond { ret a }` or the existing braceless guard `>=x 0 ret x`. Skill docs lead with the `if`-with-`ret` form (one shape, predictable, no spec-section-on-when-braces-trigger-early-return).

- **Result match keeps `?r{~v:body;^e:body}`**. This isn't a conditional, it's pattern-destructure on a tagged union. The persona logs show **no friction with the Result-match shape itself** - friction is with the body restriction (single-expression), which is fixed separately in 2.3. We retain it because it does a different job from `if`.

- **General match keeps `?subj{pat:body;pat:body;_:body}`**. Same reason: destructuring closed sums and numeric/text dispatch is a distinct operation from boolean branching.

- **`cond{body}` braceless-cond and `?h`/`?=`/`?>`/`?<` prefix-ternary stay parsing**. They are dropped from the skill docs and from the SKILL.md examples. Programs that use them still run. We deliberately stop teaching them.

**Why not just keep `?h cond a b`?** It is denser. But the persona evidence is that agents who don't already know it write `?cond{a}{b}` first, hit the bool-vs-comparison disambiguator, and retry. The `if/else` form is recognised by every model's training prior on the first token. The generation cost goes up ~6 tokens per branch; the retry cost goes down to zero on this construct.

**The parser already gives a hint for `if` today** (`reserved_keyword_message`). Under this branch the hint is removed for `if`/`else` and the keywords parse for real.

### 2.3 Match arm bodies accept block statements

**Current SPEC.md:** match arm body is a single expression. Multi-statement bodies require pulling the body out into a named helper function.

This is logged across at least 5 personas:

> Match arm bodies cannot contain multiple statements. `~v:{stmt1;stmt2}` and `~v:stmt1;stmt2` both fail - the arm body is a single expression only. This forced the pattern of wrapping all complex logic in named helper functions, which then ran into the non-last-function safe-ending constraint. Required significant restructuring across every query.

> `rdl!` requires enclosing function to return `R`, but the `~v:...` match arm syntax cannot have block bodies - so the two patterns conflict.

> Inline lambdas to `grp`/`map`/`flt` with multi-statement bodies don't work: `grp(v>t;v)` fails with `ILO-P003`.

**Agent-natural mode:** match arms accept `pat: { stmt; stmt; expr }`. The brace-wrapped form is a block whose value is its last expression (same semantics as braced bodies elsewhere). Single-expression arms continue to work unchanged.

```
?r {
  ~rows: { n = len rows; total = sum rows; total / n }
  ^e:    log e
}
```

Bare single-expression arms parse exactly as today; the only addition is that the arm body alternatively accepts `{` ... `}`.

**Token cost.** +2 tokens per block arm. Personas were currently paying +1 helper function declaration (~10-20 tokens) plus extra plumbing per multi-statement arm; this is a clear net win.

### 2.4 Loops: `for`/`while` accepted alongside `@`/`wh`

**Current SPEC.md:** `@x xs{body}` for foreach, `@i a..b{body}` for range, `wh cond{body}` for while.

Persona log evidence is softer here than for conditionals - personas mostly learn `@` and `wh` quickly. But the hesitation-then-retry cost is real, particularly on the first generation. ILO-P003 surfaces with a hint pointing back to `@x xs{body}`.

**Agent-natural mode:** accept the following as aliases. Skill docs lead with the long form.

```
for x in xs { ... }            -- aliases @x xs{...}
for i in 0..n { ... }          -- aliases @i 0..n{...}
while cond { ... }             -- aliases wh cond{...}
```

`@` and `wh` keep parsing. The choice of leading shape in the skill docs is the `for`/`while` long form: it matches the agent's training prior, the per-program token cost is small (~2 tokens per loop), and removing the "which symbol was the loop keyword again" lookup cost is what we're paying for.

This is the most genuinely manifesto-tense change in v0. See section 6 for the falsification criterion.

### 2.5 Function declaration: unchanged

`f x:n>n;body` stays. The signature line is the densest part of the language and zero personas have logged friction with it.

We deliberately do **not** add `fn f(x: n) -> n { body }` even though it matches the agent prior. The token cost is too high (+8 to +12 per declaration, multiplied by every helper a persona writes) and the friction signal isn't there in the logs.

### 2.6 Nil-coalesce: keep infix, lead with infix in docs

`??` is currently spec'd as infix-only at expression position; the `??x default` prefix form errors with ILO-P009 at statement-start. This is logged by 7+ personas and is by far the most-recurring single complaint.

**Agent-natural mode:** no syntactic change. `??` remains infix-only. But skill docs lead with the infix shape (`v??0`) explicitly and add an inline gotcha line ("`??` is infix-only - never start a statement with `??`"). This is doc-only and free, but it belongs in the surface spec because the friction is surface-level.

Note: a parallel fix-track on `main` is welcome to either add a prefix `??` form or fold a friendlier error. That's not this experiment's job.

---

## 3. What we are not touching

Listing explicitly to keep scope honest:

- **Builtin names and signatures.** `map fn xs`, `flt fn xs`, `fld fn xs init`, `srt xs`, `cat xs sep`, `spl s d`, `len xs`, `hd xs`, `tl xs`, all builtin aliases - unchanged. No method-call sugar.
- **Prefix-Polish call shape generally.** Calls are `map fn xs`, not `xs.map(fn)`.
- **Record/list literals.** `[a, b, c]`, `point x:1 y:2`. Unchanged.
- **`>` return-type marker.** Stays in function declarations.
- **Reserved words for binding/function-name positions.** `if`, `for`, `while`, `else` become control-flow keywords in this branch and are still rejected as identifier names. The list grows by 4. Persona logs show 0 collisions today.
- **Error code namespaces and messages.** Unchanged on the parse paths that still error; the new accepts simply don't produce errors any more.
- **The verifier.** Type checking, exhaustiveness, dependency analysis, RC, every backend - untouched.
- **`brk`/`cnt`/`ret`.** Unchanged.
- **Inline lambdas `(x:t>t;body)`.** Unchanged.
- **Pipe `>>`.** Unchanged.
- **`!` auto-unwrap, `!!` panic-unwrap, `^` throw, `~` ok-wrap.** Unchanged.
- **String literals, escape sequences.** Unchanged.
- **File version pragma `^YY.M`.** Unchanged.
- **`.@` extension.** Unchanged.
- **Field access (`.`, `.?`).** Unchanged.

---

## 4. Implementation strategy

Doc-only spec. Engineering work needed to make this testable is sketched below for context; it is not part of writing the spec.

### 4.1 Existing parser state (`src/parser/mod.rs`)

The parser already has the structural pieces we need:

- `parse_match_stmt` (line 1369) and `parse_match_arm` (1588) - the arm body in `parse_match_arm` is read as a single expression. Block bodies are not currently accepted.
- `parse_brace_ternary_after_subject` (1519) and the bare-bool / prefix / `?h` / brace ternary family. These handle the existing `?` conditional surface and remain unchanged.
- `parse_foreach` (1771), `parse_braceless_guard_body` (1869), `parse_brace_body` (1898), `parse_match_expr` (2241).
- A `reserved_keyword_message` path (around line 559) currently catches `if` at statement-position and emits an ILO-P003 hint pointing at `?cond{a}{b}`. The `let`/`return`/`fn`/`def`/`var`/`const` cases also live here.

So the changes split into two categories:

**(a) Skill-docs only changes (no parser work).**

- `skills/ilo/SKILL.md` and `ai.txt` lead with infix arithmetic, lead with `if/else`, lead with `for x in xs` / `while cond`, drop braceless-guard examples from the front page, retain `?r{~v:;^e:}` as the canonical Result match.

**(b) Additive parser accepts.**

- Remove the reserved-keyword interception for `if`, `else`, `for`, `while` at statement-position (keep them rejected as identifier names).
- Add `parse_if_stmt`: `if <expr> { <body> }` and `if <expr> { <body> } else { <body> }`. Desugar to the existing brace-ternary AST node so the verifier and backends require zero changes.
- Add `parse_for_stmt`: `for <ident> in <range-or-list-expr> { <body> }`. Desugar to `Expr::ForEach` / range-foreach exactly as `@x xs{...}` does today.
- Add `parse_while_stmt`: `while <cond> { <body> }`. Desugar to `Stmt::While` exactly as `wh cond{...}`.
- In `parse_match_arm`, accept an optional `{` after the `:`. If present, parse a brace-body; the arm's value is the last statement's expression. Desugar to a block expression (which already exists as the rhs of let-bindings inside arms).

All four changes share the property that **the AST emitted is identical to the existing keywords' AST**. The verifier, every code generator (tree, VM, Cranelift JIT, Cranelift AOT), every test harness - none of them see anything new.

### 4.2 Order of work (sketch only - not part of this spec)

1. Match-arm block bodies. Lowest-risk; smallest blast radius; biggest persona win per log evidence.
2. `if`/`else`. Aliased to brace-ternary. Add ILO-N201-style note that `if` without `else` returns nil.
3. `while`. Aliased to `wh`.
4. `for x in xs` / `for i in 0..n`. Aliased to `@`.
5. Skill docs + `ai.txt` rewrite.
6. Persona re-run.

Each step has its own tests + persona-relevant `examples/*.ilo` file before merging into the branch tip.

### 4.3 Tests required before re-run

For each of the four parser accepts, we need:

- A unit test that the new form parses to the same AST as the existing form (golden-AST comparison).
- A cross-engine test that asserts the new form runs identically on tree, VM, Cranelift JIT, and Cranelift AOT.
- An `examples/*.ilo` file demonstrating the new form, with `-- run:` and `-- out:` directives so `tests/examples_engines.rs` exercises it.
- An "existing form still parses" test - a copy of an `examples/*.ilo` from main, asserting nothing regressed.

---

## 5. Measurement plan

### 5.1 Baseline

The baseline is `/Users/dan/code/ilo_feedback/logs.md` as of the most recent persona run on `main`. ~115 persona entries. Per entry we have:

- task description
- outcome (worked / worked-after-N-fixes / partial / failed-fallback-to-bash)
- friction items (free-form)
- some entries include tokens / tool_uses / wall-clock (the persona token-cost logging rule)

For entries that lack token/tool_use numbers, we backfill from the originating session transcripts before kicking off the re-run, so the A/B has matched columns.

### 5.2 Re-run design

- **Same persona prompts.** Verbatim. The prompt template only loads `skills/ilo/SKILL.md` and a one-line spec pointer, so leading the skill doc with infix/if/for/while *is* the experimental treatment.
- **Same model.** Haiku-class for personas, per the steady-state runbook.
- **Same task set.** Every persona in the baseline is re-run.
- **Same harness.** `ilo_feedback` logging on, all sessions captured with token / tool_use / duration.
- **Branch under test.** `compat/agent-natural` (this branch, off `next`).

### 5.3 Metrics

Per persona we record:

| Metric | Source |
|--------|--------|
| Generation tokens | session stats |
| Tool-use count | session stats |
| Wall-clock | session stats |
| Outcome | persona report (`working` / `partial` / `failed` / `bash-fallback`) |
| First-try parse rate | count of ILO-P*** errors before first successful run |
| Retry count to first working program | count of `ilo run` invocations before exit 0 |
| Friction items logged | count of bullets in the persona's friction section |

### 5.4 Aggregation

Two summary numbers:

- **Total tokens per persona, mean across all personas.** Headline.
- **% of personas with outcome=working on first try.** Headline.

Secondary:

- **% of personas reaching working after N fixes**, for N in {1, 2, 3, 4+}.
- **% reduction in friction items logged.**
- **Wall-clock delta.**

### 5.5 Statistical interpretation

n ≈ 115 is enough that a mean token delta of >10% with consistent sign is meaningful. We don't run formal hypothesis tests; the bar is qualitative: did the surface change buy us less retry cost than it spent on generation?

The decision rule is in section 6.

---

## 6. Risk register

### 6.1 Training-data bifurcation

**Risk.** Two ilo dialects now exist. An agent trained on the canonical-prefix corpus and an agent trained on the agent-natural corpus will write subtly different programs. Spec confusion compounds: which is the "real" ilo?

**Mitigation.** v0 is explicitly an A/B branch. If it wins, `next` adopts the new surface as canonical and the canonical-prefix forms degrade to legacy / accepted-but-not-taught. There is exactly one canonical surface at any time. The branch does not get to coexist long-term with `main`'s canonical.

### 6.2 Per-program token-cost overhead

**Risk.** Every `if/else`, every `for ... in`, every `while`, every block-bodied match arm spends more tokens than the form it replaces. A persona that writes 20 conditionals and 5 loops pays ~80-100 extra generation tokens.

**Mitigation.** The hypothesis *is* that retry cost dominates. If it doesn't, this risk fires and we kill the branch. See 6.5.

### 6.3 Manifesto coherence

**Risk.** The manifesto says: "If a feature reduces total tokens, it's in. If it increases it, it's out. No exceptions for elegance, readability, or convention." The agent-natural surface is, on its face, a concession to "convention."

**Mitigation.** It is a concession to **agent priors**, not human convention. The manifesto's metric is total tokens including retries. The retry-loop cost is the operative variable. Until models are trained on ilo, the spec-loading + retry-loop terms dominate, and reducing those at the cost of a generation-loop tax is consistent with the principle. The manifesto's principle 1 explicitly says: *"spec clarity is itself a token cost - a confusing spec means more retries."* This experiment tests whether the same logic applies to surface familiarity.

If the data says no, the experiment loses and the manifesto wins. That's the point of running it.

### 6.4 Silent miscompiles from the additive accepts

**Risk.** Adding `if` / `for` / `while` as new keywords could mis-parse existing programs whose identifiers shadow them. Adding block bodies to match arms could change parse precedence in unexpected ways.

**Mitigation.** `if`, `for`, `while`, `else` are already reserved words in the existing parser (binding-position rejection with ILO-P011-class hints). Programs on `main` cannot bind them today, so no existing program can collide. Match-arm block bodies are gated by an opening `{` immediately after the arm's `:`, which is currently a parse error - no existing program reaches that path.

### 6.5 Falsification criterion

The experiment is killed if **any of**:

- **Total mean tokens per persona increases** vs baseline by >5%.
- **% of personas with outcome=working** stays flat or decreases.
- **Net friction items logged** does not decrease.

The experiment is **adopted into `next`** if **all of**:

- Total mean tokens per persona decreases vs baseline by >10%.
- % outcome=working increases.
- Net friction items logged decreases.

The experiment is **partial** if results fall between. In the partial case, we adopt only the change(s) that map to specific friction-bucket reductions (e.g. match arm block bodies clearly won, `if/else` didn't pay off ↦ adopt the former, drop the latter). Per-change attribution comes from the per-persona friction-bullet diff.

A decision write-up gets appended to `ilo_assessment_feedback.md` under `## ✅ Addressed` (if the surface gets adopted) or to a new `## ❌ Killed experiments` section if it doesn't, so future agents know we tried this and what the data said.

---

## 7. Open questions

Not blockers for v0; flagging for the post-re-run discussion.

- Does `if cond` (no `else`) returning `nil` cause type-inference confusion in let-bindings? `v = if c { 1 }` infers `O n` rather than `n`. Personas may hit this and reach for unwraps.
- Is `for i in 0..n` parser ambiguity worse than `@i 0..n`? Both share a range syntax; the keyword change is purely the lead-in token.
- Should the skill docs keep one boxed example of the prefix form, labelled "token-tight alternative", for the agents that have learned prefix and want to use it? Or does the dual presentation itself cost tokens at spec-loading time? Currently leaning towards: omit, keep the spec lean; the prefix form continues to work undocumented but the language tour shows only one path.

---

## 8. Pointers

- `MANIFESTO.md` - the five principles this experiment defends itself against.
- `SPEC.md` - canonical spec; everything not overridden here applies.
- `ai.txt` - token-minimal agent-facing spec; needs a parallel agent-natural variant if this lands.
- `skills/ilo/SKILL.md` - leads the skill docs; the primary surface for the experimental treatment.
- `/Users/dan/code/ilo_feedback/logs.md` - the persona transcript baseline this experiment measures against.
- `src/parser/mod.rs` - the parser file the implementation work touches.
