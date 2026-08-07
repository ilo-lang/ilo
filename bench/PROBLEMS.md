# ilo Benchmark Problems — 2026-08-07

## TL;DR

The manifesto claims ilo is more token-dense than Python. The benchmark
shows this is **true for certain task categories** (HOF pipelines, function
composition) but **false or unprovable for others** (string parsing,
multi-way conditionals). Three of ten benchmark tasks fail entirely because
the model generates code the parser rejects on spacing/tokenisation grounds.
The overall result is not publishable.

---

## Problem 1: Parser rejects compact tokenisation (3/10 tasks fail)

The model writes correct logic in every failing task. The failures are
purely tokenisation strictness — the parser requires whitespace where the
model omits it.

### Specific patterns

| What the model writes | What ilo needs | Why it fails |
|----------------------|----------------|-------------|
| `fmt2(at e 1)2` | `fmt2 (at e 1) 2` | No-space `func(args)` parses as paren-form call (one arg). Trailing `2` is unexpected. |
| `?h>=wa 90"A"` | `?h >=wa 90 "A"` | Number immediately followed by string literal. The lexer tokenises correctly but the parser's ternary operand consumption stops early. |
| `*r ss i` | `r=*ws i; *r (ss i)` | Prefix-binop chain ambiguity: `*r ss i` parses as `*(r, ss, i)` (3-arg multiply) instead of the intended `*r (ss i)`. |

### Impact

Each failure burns 800-1000 generation tokens across 5 repair attempts.
The repair loop cannot fix these because the error messages don't clearly
indicate the spacing issue.

### Status

- Nested ternary fixed (ILO-537): `?h cond a ?h cond2 b c` now works in all
  three parser paths (function body, binding, loop).
- Spacing/reserved-name issues: documented in skill docs (ILO-536) but the
  model still generates the patterns on ~30% of runs.
- Paren-form spacing: not fixed. Would require parser leniency (accept
  `func(args)token` as `func (args) token`) with regression risk.

---

## Problem 2: Reserved-name collisions

The model uses builtin names as local variables. Most common offenders:

| Reserved name | Model uses it as | Frequency |
|--------------|-----------------|-----------|
| `tl` | "total" or "tail result" | text-analysis task |
| `avg` | "average" variable | text-analysis, grade-calculator |
| `sum` | already a builtin (works) | — |
| `len` | "length" variable | occasional |

### Why it happens

The skill docs list reserved names, but:
1. The list is long (~70 names) and the model doesn't internalise all of them
2. `tl`, `avg`, `len` are natural variable names in every other language
3. The error message (`ILO-P011: X is a builtin and cannot be used as a binding name`) is clear but the repair loop doesn't always find a good alternative

### Status

- ILO-536: expanded the "most reached-for" list in skill docs
- The repair loop should rename automatically (ILO-501 fix_plan now covers
  all occurrences) but the model doesn't always read the fix_plan

---

## Problem 3: High variance at low N

| Task | Run 1 | Run 2 | Run 3 | Swing |
|------|-------|-------|-------|-------|
| function-pipeline (ilo) | 139 tok | 638 tok | 153 tok | 4.6x |
| matrix-operations (ilo) | 258 tok | 516 tok | 776 tok | 3.0x |
| simple-function (ilo) | 41 tok | 64 tok | 35 tok | 1.8x |

The variance comes from:
1. **Repair loop depth**: if attempt 1 fails, attempts 2-5 each add ~150-200 tokens
2. **Model temperature**: the same prompt produces different code each run
3. **Doc sensitivity**: the model's output changes when skill docs are updated, even for unrelated tasks

### Impact

N=1 is unreliable for any single task. N=3 smooths micro-tasks but the
large tasks still swing wildly. N=5+ would be needed for publishable
results but costs ~$2.50/run in API credits.

---

## Problem 4: Skill-doc token inflation

Adding documentation to help the model avoid mistakes has a side effect:
it inflates the skill-module token count, which increases input context,
which the persona-smoke CI flags as a regression.

### This session's doc additions

| Module | Token delta |
|--------|-------------|
| ilo-language | +388 |
| ilo-builtins-io | +45 |
| ilo-builtins-math | +44 |
| ilo-builtins-text | +27 |
| ilo-errors | +31 |
| **Total** | **+535** |

This pushed mean generation tokens from 535 to 702 (+31%) on the first
CI run. The threshold was bumped from 15% to 20% to account for the
stochastic nature of LLM generation (ILO-487).

### The trade-off

More docs = fewer repair attempts (good) but larger context (bad).
The net effect on generation tokens is unclear and varies by task.

---

## Problem 5: Task-size sensitivity

The density advantage is real but only compounds on larger programs.

### Micro-tasks (1-3 lines)

ilo and Python produce similar token counts. The per-statement savings
(prefix notation, no `def`/`return`) are real but too small to matter:

```
ilo:     main>n;s=*n +n 1;/s 2;s       (35 tokens)
Python:  def f(n): return n*(n+1)//2    (36 tokens)
```

### HOF pipeline tasks (5-10 lines)

ilo wins clearly. Prefix HOF chains and compact lambdas are denser:

```
ilo:     153 tokens (function-pipeline)
Python:  196 tokens
         ↓ 22% fewer tokens in ilo
```

### String parsing / conditional tasks (15+ lines)

ilo loses or fails. The density advantage is negated by:
- Parser strictness on compact tokenisation
- Reserved-name collisions
- Prefix-binop ambiguity on complex expressions

### Implication

The benchmark needs **larger tasks** (20-50 lines) where the density
advantage compounds across many statements. The current 5 large tasks
are too varied — some exercise ilo's strengths (pipelines), others
exercise its weaknesses (string parsing).

---

## What would make the result publishable

1. **Fix the paren-form spacing issue** — either parser leniency or a
   better error hint that the repair loop can act on. This alone would
   rescue pipeline-report (the worst failure at ~1000 tokens wasted).

2. **Add 3-5 pipeline-heavy tasks** (20+ lines) where ilo's density
   advantage compounds. Data ETL, multi-function composition, nested
   HOF chains.

3. **Run N=5 minimum** to stabilise the variance. Budget: ~$2.50/run.

4. **Accept the task-category framing** — "ilo is denser for HOF
   pipeline tasks" is a defensible claim even if it's not universal.

---

## What we fixed this session

| Fix | Impact |
|-----|--------|
| Nested ternary parser (ILO-537) | `?h cond a ?h cond2 b c` now works everywhere |
| `rou x digits` (ILO-535) | Eliminates scale/round/unscale boilerplate |
| `env-or` builtin | Eliminates Result-unwrapping for env-var-with-default |
| `zgunzip` builtin (ILO-498) | gzip decompress |
| fix_plan all-occurrence renames (ILO-501) | Repair loop converges instead of fixing one occurrence at a time |
| Hoisting advisory (ILO-504) | Undefined-variable errors now suggest inline-lambda capture |
| Skill docs (ILO-536) | Spacing warning, reserved names, conditional clarity |
| Persona-smoke CI (ILO-487) | Secret set, baseline refreshed, threshold calibrated |
