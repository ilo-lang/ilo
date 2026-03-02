# The ilo Manifesto

## The Audience Is Not Human

Every programming language in use today was designed for people. The syntax, the error messages, the tooling — all optimised for a brain that reads left-to-right, tracks visual indentation, and cares about aesthetics.

AI agents are not that brain. They produce tokens sequentially. They consume tokens from a finite context window. Every token they spend — generating, reading, retrying — costs real time and real money.

ilo is designed for them.

## The Only Metric

**Total tokens from intent to working code.**

```
Total cost = spec loading + generation + context loading + error feedback + retries
```

Every design decision is evaluated against this number. If a feature reduces it, it's in. If it increases it, it's out. No exceptions for elegance, readability, or convention.

## The Five Principles

### 1. Token-Conservative

The north star. Every choice evaluated against total token cost across the full loop — not just "short syntax," but including retries, error feedback, and context loading.

A named argument like `amount: 42` costs more tokens than positional `42`. We initially worried positional args would cause parameter-swap errors — but across 10 syntax variants and 4 task types, positional args scored 10/10 generation accuracy. The swap concern was unfounded. Positional args are the single biggest token saver.

**What the agent cares about:** "How many tokens will this cost me end-to-end?"
**How this helps:** The language is as terse as possible *without increasing retry rate*. Where there's a tradeoff between generation cost and error rate, we optimise for total cost.

**Prefix notation** eliminates parentheses and saves tokens at every nesting level. `(a * b) + c` becomes `+*a b c` — 4 fewer characters, 1 fewer token. Deeper nesting saves more: `((a + b) * c) >= 100` becomes `>=*+a b c 100` — 7 fewer characters, 3 fewer tokens. Across 25 expression patterns, prefix notation saves 22% of tokens and 42% of characters vs infix. See the [prefix-vs-infix benchmark](research/explorations/prefix-vs-infix/) for the full analysis.

**Naming rule:** prefer single-word identifiers. Across all major LLM tokenisers (OpenAI, Anthropic), common English words are 1 token. Hyphenated compounds are always 2 — the hyphen forces a token split. Every hyphen in a name doubles its token cost. Abbreviations (`uid` vs `user`) save characters but not tokens — tokenisers encode common words as single tokens either way. Both styles score 10/10 in generation accuracy.

### 2. Constrained

Small vocabulary. Closed world. One way to do things.

When an agent generates the next token, how many valid options are there? Fewer valid next-tokens means fewer wrong choices means fewer retries. This isn't about limiting expressiveness — it's about making the right token obvious.

- **Closed world.** Every callable function is known ahead of time. The agent cannot hallucinate an API that doesn't exist.
- **Small vocabulary.** Fewer keywords, fewer constructs, one way to define a function, one way to call it, one way to handle errors.
- **Verification before execution.** All calls resolve, all types align, all dependencies exist — checked before running anything.

**What the agent cares about:** "At each generation step, how many valid tokens are there?"
**How this helps:** The language becomes a set of rails. Constrained generation can feed valid next-token sets back to the agent, making it *impossible* to generate invalid code.

### 3. Self-Contained

Each unit carries its own context: deps, types, rules.

An agent working on function A shouldn't need to load functions B through Z to understand what A does. The less context required per step, the fewer tokens consumed, the more of the context window is available for the actual task.

- **Explicit dependencies.** Each function declares exactly what it needs — by name, with types. No globals, no ambient state, no implicit imports.
- **Small units.** A function that fits in a few dozen tokens can be loaded, understood, and modified cheaply.
- **Spec as context.** Until foundation models are trained on ilo, agents need the spec somewhere they can access it — bundled with the program, fetched on demand, or installed locally.

**What the agent cares about:** "How much context do I need to load to work on this unit?"
**How this helps:** Minimal context loading per task. Each unit is self-describing. The agent never needs to hunt for definitions elsewhere.

### 4. Language-Agnostic

Minimise dependency on English or any natural language.

Early variants used short English-derived keywords (`fn`, `let`, `match`, `for`, `if`). Experiments showed structural tokens outperform keywords entirely — the winning syntax (idea8/idea9) replaced all keywords with single-character sigils:

- `?` conditional, `!` effect/call, `~` transform, `@` iterate, `>` pipe/return
- Only ~6 abbreviated keywords remain (`type`, `tool`, `wh`, `ret`, `brk`, `cnt`) — no full English words
- Agents learned the sigil set from spec + examples with 10/10 accuracy

Structural tokens won because they are unambiguous single tokens that cannot be confused with variable names or hallucinated into natural-language variations.

**What the agent cares about:** "Can I learn this language from its spec and examples, regardless of my training?"
**How this helps:** The spec is small enough to bundle with any program. Keywords are learned from structure, not from natural language understanding.

### 5. Graph-Native

Programs express relationships: calls, depends-on, has-type. Navigable as a graph, not just readable as linear text.

Traditional source code is a flat file. Understanding program structure requires parsing it mentally (or literally). ilo makes the graph explicit — every function declares its edges (what it calls, what it depends on, what it produces).

- **Edges are first-class.** "A calls B" is expressed directly, not inferred from reading A's body.
- **Queryable structure.** An agent can ask "what depends on X?" without loading the entire program.
- **Composable.** Units connect through declared interfaces. The graph is the program.

**What the agent cares about:** "Can I navigate program structure without loading everything?"
**How this helps:** Agents work on subgraphs, not entire codebases. Dependencies are explicit. Impact analysis is cheap.

## Principles We Considered and Dropped

**Deterministic** — falls out naturally from constrained + self-contained. An agent doesn't think about determinism; it thinks "did this work?" If the language is constrained and self-contained, determinism follows.

**Append-only** — solved by small self-contained units. If units are small enough, regenerating them is cheap and safe. No need for a structural constraint.

**Immediate feedback** — a property of the runtime/tooling, not the language itself. Important for the ecosystem, but not a language principle.

## The Name

*ilo* is Toki Pona for "tool" ([sona.pona.la/wiki/ilo](https://sona.pona.la/wiki/ilo)).

Toki Pona is a constructed language built around radical minimalism. ~120 words. 14 phonemes. Complex ideas expressed by combining simple terms. It constrains human expression to force clarity of thought.

ilo does the same for machine programmers. A minimal, verified vocabulary. Complex programs built by composing small, self-contained units. The constraint is the feature.

## What ilo Is Not

**Not a framework for building AI agents.** There are plenty of those. ilo is a language for agents to write programs *in*.

**Not optimised for human readability.** Humans can read it — it's not obfuscated — but no decision is made because it "looks cleaner" or "reads more naturally." If a design is uglier but costs fewer total tokens, it wins. Newlines, indentation, and multi-line comments are human concerns — agents don't need them. An entire ilo program can be one line. The formatter provides expanded output (`--expanded` / `-e`) when humans need to review.

**Not theoretical.** Every principle here addresses measured failure modes in AI-generated code: hallucinated APIs, context window exhaustion, wasted retry cycles from vague errors.

## What ilo Is

A **minimal, verified action space** — the smallest set of constructs an agent needs to express computational intent, with relationships made explicit and everything else stripped away.

## Further Reading

- [README.md](README.md) — project overview and results
- [OPEN.md](research/OPEN.md) — unresolved design questions
- [research/explorations/](research/explorations/) — all syntax variants and test harness
