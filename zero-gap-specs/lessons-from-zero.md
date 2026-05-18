# Lessons from Vercel Zero — What We Learned and What We're Applying to ilo

Captured 2026-05-18 from a deep technical and strategic analysis of Vercel Labs' Zero (v0.1.2, released May 15 2026) against ilo (v0.11.8). This document records what we observed in Zero, what it implies for ilo's design and strategy, and what we deliberately chose not to copy.

This is the canonical "what changed in our thinking" doc. Other files (`00-strategy-v2.md`, `01-modular-skills.md`, etc.) implement specific pieces; this is the conceptual record behind them.

---

## 1. Where each language shines — workload categorisation

The single most important reframe from the analysis: ilo and Zero are not competitors. They optimise different parts of the agent stack and they win different workloads.

### Two distinct categories of agent work

| Category | Workload shape | Cost dominated by | Best language |
| --- | --- | --- | --- |
| **Category 1: Generation-dominated** | Interactive coding agents iterating heavily on the same codebase. Many LLM calls per task. Code lives briefly. | Generation tokens, retry tokens, spec loading | **ilo** |
| **Category 2: Deployment-dominated** | Agent generates a tool once, ships it, invocations dominate. One-shot generation. Code lives long. | Binary size, cold-start latency, runtime cost | **Zero** |

The mistake — both in our discourse and the broader agent-language conversation — is assuming there's one "agent workload" to optimise.

### Per-task economics (cached steady-state session)

Measured + estimated per typical task:

| Variant | Spec/task | Generation+repair | **Total/task** |
| --- | ---: | ---: | ---: |
| ilo today (monolithic spec) | 1,600 | 112 | 1,712 |
| Zero today | 250 | 188 | 438 |
| ilo + Phase 1 (modular spec) | 300 | 112 | 412 |
| ilo + Phase 1 + Phase 4 (typed fix plans) | 300 | 80 | 380 |
| ilo + aggressive 1k modular spec + Phase 4 | 100 | 80 | 180 |
| Python baseline (LLM already knows it) | 0 | 200 | 200 |

Key findings:

- **ilo today loses to Zero per task** because the 16 KB spec dominates everything in cached operation
- **Phase 1 alone** makes ilo competitive (412 vs 438)
- **Phase 1 + Phase 4** beats Zero by ~13% (380 vs 438)
- **Python is the floor** at 200 — both new languages pay a spec-load tax Python doesn't
- ilo only beats Python in cached steady-state at N≥3 tasks per session (with Phases 1+4)

### Where Zero wins

| Use case | ilo fit | Zero fit |
| --- | --- | --- |
| Single one-shot task | Worse than Python (spec tax) | Worse than Python (spec tax) |
| Sustained interactive (5+ tasks per session) | **Wins** | Loses to ilo, beats Python in some categories |
| Heavy iteration (50+ tasks per session) | **Wins decisively** | Competitive |
| High-volume tool deployment (1 tool, 1M invocations) | Bad fit (9 MB binary, large runtime) | **Wins decisively** (10 KB binary, no runtime) |
| Edge runtime targets (Cloudflare Workers, WASI) | Marginal (2.1 MB WASM today) | **Wins** (sub-1 MB binaries, capability-checked) |
| Embedding in another agent runtime | Bad fit (6.3 MB compiler, 249 deps) | Good fit (648 KB compiler, 0 deps) |

### Where ilo wins

| Use case | ilo fit | Zero fit |
| --- | --- | --- |
| Interactive coding agents (Claude Code, Cursor) | **Wins after Phase 1** | Heavier per token, slower iteration |
| Multi-agent orchestration (sub-agents writing code) | **Wins** — cheaper sub-agent generation | Token cost per sub-agent invocation is 3× ilo |
| Iterative refinement (LLM in tight loop with compiler) | **Wins** — dense source + verified before exec | Loses on generation tokens |
| Tools/MCP-heavy programs | **Wins** — first-class tool keyword | No first-class tool syntax |
| Dev loop with REPL | **Wins** — 4 engines for hot iteration | No REPL |

### The honest pitch derived from this

> **For agent workflows where the LLM iterates heavily on the same codebase, ilo is the cheapest language to use — including against Zero. For deploying tools at high invocation volume, use Zero. For one-shot generation, use Python.**

Not "ilo is the agent language." Conditional and defensible.

---

## 2. The six-layer modularity insight

Zero's spec is 4× smaller than ilo's (4,300 vs 16,000 tokens, measured). Per-task it's 8–16× cheaper. This isn't because Zero is a simpler language (it has more features). It's because Zero's spec architecture has six discrete properties ilo's spec lacks. We're applying all six to ilo in Phase 1.

The six layers:

1. **Files** — separate markdown per concern (7 files in Zero's `skill-data/`)
2. **Metadata** — YAML frontmatter with `name`, `description` (machine-parseable routing)
3. **CLI accessor** — `zero skills list/get/path` subcommands (programmatic access)
4. **Routing semantics** — every description starts with "Use this when..." (uniform agent routing)
5. **Self-containment** — each module works standalone, accepts content duplication (no required cross-references)
6. **Version bundling** — skills travel inside the compiler binary, version-locked (no drift)

Just splitting files into separate markdown documents (layer 1 only) doesn't work. All six layers compound. Missing any of them degrades the per-task spec-load efficiency.

**Key insight**: the spec is a **user interface for agents**, not documentation for humans. Once you accept that framing, the six layers become obvious. While the spec is treated as docs, the temptation is to write tutorials, cross-reference sections, explain motivations — all of which add tokens without saving generation cost.

---

## 3. Zero's coherent design stack

Reading Zero's design as a chain of forced choices, not a feature list:

```
Goal: agents ship tiny deployable tools
  ↓ requires
Tiny binaries (sub-10 KB)
  ↓ requires
No runtime, AOT only, direct emit
  ↓ enables and requires
Multi-target compile (web/WASI/native)
  ↓ requires
Compile-time capability gating
  ↓ requires
Effect system in the type system (raises {Set})
  ↓ benefits from
JSON diagnostics (agents read errors)
  ↓ benefits from
Stable error codes + repair plans (cheap retry)
  ↓ benefits from
A compiler that is itself tiny, in C, no deps (648 KB, 0 deps)
```

Each step locks in the next. Remove any one and the stack stops being coherent. This is unusually disciplined design.

### ilo's coherent stack (the inverse)

For comparison, ilo's design chain:

```
Goal: agents generate code cheaply
  ↓ requires
Minimum tokens per program (artifact is source, not binary)
  ↓ requires
Dense source syntax (sigils, prefix, no English keywords)
  ↓ requires
Constrained vocabulary (closed world, fixed builtins)
  ↓ enables
Verification before execution (graph-reducible)
  ↓ requires
Self-contained functions (explicit deps)
  ↓ enables
Loading only the relevant subgraph per task
  ↓ benefits from
Multiple execution engines (REPL, JIT for dev iteration)
  ↓ benefits from
First-class tool/MCP integration
  ↓ benefits from
Compact in-context spec (modular after Phase 1)
  ↓ benefits from
A rich compiler in Rust (Cranelift, batteries-included)
```

Same shape, opposite endpoints. Zero's compiler is tiny because the artefact must be tiny. ilo's compiler is rich because the source is the artefact and dev velocity matters.

**Lesson**: design coherence is itself a property. Each language's design choices reinforce the next when made deliberately. ilo's chain should be made explicit (it implicitly exists in the manifesto), so future contributors can see why a particular feature is or isn't in scope.

---

## 4. Things we're applying to ilo

Direct ports from what Zero does well:

### a) Modular skills with six layers (Phase 1)

See `01-modular-skills.md`. Highest-priority work. Existential for ilo's per-task economics.

### b) Typed fix plans + fixSafety taxonomy (Phase 4)

See `04-typed-fix-plans.md`. Zero's `zero fix --plan --json` emits structured repair candidates with safety labels: `format-only` / `behavior-preserving` / `api-changing` / `target-changing` / `requires-human-review`.

For ilo we rename `target-changing` to `engine-changing` (ilo's targets are execution engines, not deployment platforms). The taxonomy otherwise transfers cleanly.

This is amplifying technology, not foundational. It makes ilo's case more durable once Phase 1 has made the case viable.

### c) Stable error codes + `--explain` command

ilo already has ILO-XXXX codes. We're extending coverage and adding `ilo explain <code> --json` for structured error documentation, matching Zero's `zero explain`.

### d) "Use this when..." description discipline

Every skill's description starts with this phrase. Uniform routing language so agents see consistent classification across all modules. We're applying this in Phase 1.

### e) Bundled, version-locked skills

Skills embed in the binary via `include_str!`. No remote registry, no version drift. Already partially true for ilo via the plugin marketplace; Phase 1 formalises it.

### f) JSON-first compiler output

`ilo check --json`, `ilo --output json` already exist. We're hardening the JSON schema with golden-file tests in Phase 4 so the contract is stable.

### g) Structured mismatch facts in diagnostics

Zero's diagnostics include `expected` and `actual` as data, not just prose. ilo will add the same in Phase 4 work.

### h) Manifesto-level acknowledgement of "one layer of the stack"

Added as Principle 6. Codifies the composes-not-subsumes posture so future scope-creep ("should we add a borrow checker?") gets answered structurally.

---

## 5. Things we're deliberately NOT copying

Discipline in refusal is what keeps ilo from becoming Zero-but-worse. Specific rejections:

### a) Borrow checker / ownership types

Zero exposes `ref<T>`, `mutref<T>`, `owned<T>` annotations on function parameters. ilo does not, and won't. Three reasons, tied directly to manifesto principles:

**Principle 1 (Token-Conservative)**: every borrow annotation costs source tokens. `process x:ref<item>` costs more than `process x:item`. Across hundreds of functions in a real program, the cumulative token cost is significant. ilo's design centre is that the agent pays per token across the full loop; spending tokens on memory annotations the runtime can handle itself is a bad trade.

**Principle 4 (Language-Agnostic)**: borrow vocabulary (`ref`, `mut`, `owned`, lifetime parameters) is English- and Rust-specific. ilo aims for structural tokens that don't lean on natural-language understanding, with control flow encoded as sigils and types as single-character codes. Borrow types pull ilo back toward English-keyword design.

**One layer of the stack, not the whole** (see section 4h above and the "What ilo Is Not" framing in the manifesto): compile-time memory safety with annotations is Zero's territory, and Rust's. ilo composes with whatever runtime hosts the compiled output. For deployments where compile-time memory verification matters, ilo can transpile to a language that does that work. Adding borrow types to ilo would mean competing in a niche we explicitly decline.

What ilo does instead: reference counting plus functional purity, with future compiler-side optimisation (escape analysis, linearity inference) that aims for similar runtime efficiency without source-visible annotations. The honest tradeoff: RC has real runtime overhead vs Rust- or Zero-style static ownership, and that overhead is the cost of source simplicity. For ilo's audience (agents paying per token) the trade lands the right way. See section 10 for the open research bet on escape analysis as the future direction.

The result: ilo programs are memory-safe (no use-after-free, no data races, no leaks except cycles which the cycle detector handles), with zero source-token cost for memory management. Borrow types are correct for Rust and Zero's audiences. They are the wrong answer for ilo's.

### b) Direct emit to ELF/Mach-O/COFF

Zero has its own emitter for each binary format. ilo uses Cranelift. Building our own emitters would add maintenance burden without strategic benefit — ilo's binaries are large because of the runtime, not because of codegen.

### c) Multi-target build profiles

`zero build --target wasm32-web --profile release-small` etc. ilo's target story is execution engine (tree/VM/JIT/AOT), not deployment platform. We compose with Zero/Component Model for deployment targeting.

### d) `zero size` / `zero mem` / `zero ship`

These exist because Zero competes on binary size and deployment ergonomics. ilo doesn't compete there. Adding equivalent commands would imply we do.

### e) Capability parameters as user-visible syntax

Zero's `world: World` parameter and target-gated capabilities (`TAR002`) require source-level annotation. For ilo this would inflate tokens per function. We may add an opt-in capability annotation later (research bet), but not as a default.

### f) Choice types / fixed-size arrays / value generics

Zero's `choice Result { ok: T, err: E }`, `[N]T`, `<T, static N: usize>` — systems-language features. ilo's `R`, `O`, `S` and dynamic lists cover the same use cases with denser syntax.

### g) Explicit ABI tracking (`zero abi check/dump`)

Zero exposes ABI compatibility as a first-class concern because binaries get redistributed independently of the compiler. ilo's deployment story doesn't require this.

### h) Direct competition on Vercel's distribution

We don't try to out-market. ilo's asymmetric weapon is published measurement, not marketing reach.

---

## 6. Things we discovered Zero is missing that ilo can claim

Symmetric to section 4: places where Zero leaves room.

### a) Token-efficient source

Zero is 3× heavier than ilo on source tokens. By design — they bet on familiarity, not density. ilo's structural advantage on this axis is unrecoverable for Zero without abandoning their core syntax choices.

### b) First-class tool/MCP integration as language syntax

Zero treats tool calls as host concerns. ilo's `tool name "description" (args) > return-type` is a language-level construct. For agent workloads doing heavy tool calling, this is a real ergonomic win.

### c) Multi-engine dev story

Zero has one execution mode (AOT to native). ilo has tree-walker, VM, JIT, and AOT. The REPL alone is a category Zero can't easily match without major rework. Dev velocity matters for iterative agent workflows.

### d) Closed-world semantics

Every callable in ilo is known at compile time. Agents can't hallucinate APIs. Zero allows imports but doesn't enforce closed-world to the same degree.

### e) Verification-before-execution as a guarantee

ilo's verifier runs before any code executes. Zero's compile pass also runs before execution but the verifier is less ambitious. ilo could lean harder on this as a safety claim.

### f) Measurement discipline

ilo has a 9-variant token-efficiency bake-off (`research/explorations/`) with real data. Zero doesn't publish equivalent comparisons. ilo's measurement culture is a durable advantage that propagates as published artefacts.

---

## 7. The broader landscape (context that shaped the strategy)

### MoonBit

- v0.7.1 docs published, v1.0 planned for 2026
- Open-source compiler, multi-backend (WASM-GC, JS, native)
- 27 KB hello-world WASM (smaller than Rust's 50–200 KB)
- WebAssembly Component Model support
- Academic backing

If ilo ever needs an alternative deployment target to Zero, MoonBit is the credible second option. The codegen-layer abstraction (deferred) would let ilo target either.

### WASI Component Model

- WASI Preview 2 stable (April 2026)
- Component Model with typed-interface capability grants
- The open standard the edge-runtime niche is converging on
- Cloudflare Workers, Fastly Compute, Vercel Edge, Wasmtime, Wasmer all target it

Long-term, ilo's "right" deployment story is probably **emit Component Model directly**, not transpile to a specific competing language. This is the most platform-independent path.

### Token-efficiency benchmarks already exist

- **J** ranks #1 in published benchmarks at ~70 tokens/task
- **APL** ranks #4, penalised for glyph tokenization
- **Haskell/F#** at 115–118 tokens via type inference
- Martin Alderson has published methodology

ilo at ~58 tokens/task on the 5-task suite is in J's range. The honest pitch is "J-class density without J's BPE penalty" — competitive but not unique. Capability-checked safety + agent protocol features are what compound the source-density win.

### Capability-based security has real tailwind

- WASI Component Model capability grants
- Microsoft Security: "When prompts become shells" RCE vulnerabilities in agent frameworks
- ICLR 2026 OpenAgentSafety benchmark paper
- Ryan Rasti's object-capability SQL sandboxing for LLM agents

Compile-time-proven capabilities are an interesting research direction, but the niche is crowded (Roc, Koka, Unison, Effekt all have effect/capability systems). ilo's claim, if made, is the *combination* of dense source + capability proofs + agent-loop ergonomics — not capability proofs alone.

### Vercel's distribution edge is smaller than initially apparent

- Zero is a Vercel Labs side-bet, not a main product
- v0 generates React/TS, not Zero — Vercel's main agent product doesn't use Zero
- ~3–5 engineers on the team, not 30
- Zero is "experiment worth tracking, not production dependency" per Vercel's own framing

ilo is fighting one R&D team with ~10× the resources, not a $50B company with infinite marketing. The asymmetry is real but not infinite.

### The agent-language category may not exist

This is the deepest threat — bigger than Zero specifically. Production agent code is Python/TS plus frameworks (LangGraph, CrewAI, Anthropic computer-use, OpenAI Assistants). Frameworks add agent-specific affordances to existing languages. No new language has won the niche yet.

Both Zero and ilo may be wrong-headed bets on a category that doesn't materialise. The honest hedge: produce artefacts (measurements, methodology, comparisons) that propagate independently of whether the category materialises. The published benchmark and modular-skills methodology are valuable to the field whether ilo wins or not.

---

## 8. Strategic implications crystallised

### What we're optimising for

The honest goal is some mix of:
- **Influence**: ideas get adopted into other languages and platforms (high probability of success)
- **Career-defining**: ilo + writing + benchmark becomes "the rigorous agent-language-economics body of work" (medium probability)
- **Adoption**: actual users (low probability in 10 weeks; possible in 6+ months)

Explicitly NOT optimising for:
- Stars (Vercel wins by definition)
- Feature parity with Zero (Vercel wins on engineering capacity)
- Commercial outcomes (wrong investment shape)

### The asymmetric weapon

**Published empirical measurement.** Vercel won't run benchmarks that disadvantage Zero. ilo can publish ilo-vs-Zero-vs-Python data with full methodology. Credibility born of disclosure.

This is why Phase 2 (closed-loop benchmark) is the strategy keystone. Without it, every claim is contestable. With it, the conversation becomes structural — and Vercel can't structure their way out of measured numbers.

### The composes-not-subsumes posture

Manifesto Principle 6. Codified so future contributors don't try to make ilo a deployment language. ilo stays small in scope; that's the moat.

If ilo also tries to do what Zero does, it loses on Vercel's engineering capacity. If ilo focuses on what Zero structurally can't (dense source, multi-engine dev, first-class MCP), it has a defensible position.

### The two-layer stack thesis

The narrative: **ilo for thinking, Zero for shipping.** Two layers of the same agent stack, complementary not competing.

This is the framing the public post should land. It's:
- True (the technical analysis supports it)
- Generous (acknowledges Zero's strengths)
- Defensible (Vercel can't argue against being credited as the deployment layer)
- Unable-to-be-written-by-Vercel (they have no upstream language to point at)

---

## 9. Numbers worth memorising

Quick-reference table of the load-bearing data:

| Metric | ilo today | ilo + Phase 1 | ilo + Phases 1+4 | Zero | Python |
| --- | ---: | ---: | ---: | ---: | ---: |
| Spec total tokens | 16,000 | ≤ 5,000 | ≤ 5,000 | 4,300 | 0 |
| Spec/task cached | 1,600 | 300 | 300 | 250 | 0 |
| Per-task generation | 70 | 70 | 70 | 180 | 200 |
| Per-task retry | 42 | 42 | 10 | 8 | — |
| **Per-task total** | **1,712** | **412** | **380** | **438** | **200** |
| 5-task suite (source only) | 288 | — | — | 886 | 871 |
| Output binary (return 42) | 9 MB | — | — | <10 KB | — |
| Compiler binary | 6.3 MB | — | — | 648 KB | — |
| Compiler deps | 249 | — | — | 0 runtime | — |
| Compiler language | Rust | — | — | C | — |
| Source-token density vs Python | 0.33× | 0.33× | 0.33× | 1.02× | 1.00× |

---

## 10. Open research bets (not in the 10-week plan, but tracked)

Things that might matter later, deferred until evidence supports them:

### a) Compile-time capability proofs as ilo's safety claim

Adding optional capability annotations (`!Invalid|Timeout` style for error sets, similar for I/O capabilities). Would let agent platforms deploy ilo output without runtime sandboxes. Research-grade speculation today. Re-evaluate if Phase 3 publishing surfaces interest from platform engineers.

### b) Transpile to WASI Component Model directly

Long-term deployment story. If the closed-loop benchmark validates ilo wins category 1 and we want to claim category-2 cooperation, emit Component Model rather than transpiling to a specific competing language. Most platform-independent path.

### c) Codegen layer abstraction (HIR + Backend trait)

Decouple frontend from Cranelift so future backends (Zero emit, C emit, Component Model emit, WASM emit) plug in cleanly. Not urgent until there's actual second-backend demand.

### d) Self-hosting

Long-term: rewrite ilo's compiler in ilo. Rite of passage for mature languages. Years away.

### e) Escape analysis and linearity inference for RC elision

ilo's memory model is RC plus functional purity, with no source-visible borrow annotations (see section 5a). The research bet is that a compiler-side pass — escape analysis to stack-allocate non-escaping values, linearity inference to elide refcount ops on uniquely-owned bindings — can recover most of the runtime efficiency that Rust- and Zero-style ownership annotations buy, without any source-token cost. Worth a serious prototype once the closed-loop benchmark proves the source-density win is real and the runtime overhead becomes the next bottleneck.

### f) Multi-agent orchestration as a first-class concern

If ilo positions as the implementation language for sub-agents in multi-agent stacks (rather than the top-level orchestrator), there may be optimisations specific to that workload — e.g., parallel-safe primitives, sub-agent-context-window-friendly layouts. Wait for the use case to emerge before designing.

### f) Compile-time memory optimisation via escape analysis or automatic linearity

ilo today uses RC for all heap values. Compile-time analysis could eliminate RC for values the compiler proves are non-escaping or linear, without changing the source language. Three composable sub-options, not mutually exclusive: escape analysis (Go-style: infer which values escape, stack-allocate the rest), automatic linearity inference (Roc-style: prove uniqueness, skip RC for unique values), and region inference (OCaml-style: group allocations by scope, free regions at scope exit). Real perf win, no manifesto cost, source unchanged, no user annotations. Big engineering effort: 6+ months minimum for a credible implementation, and Roc has had a team working on automatic linearity for years. Defer until one of: RC overhead surfaces as a real user pain point (current data: none), ilo grows contributor capacity to take on compiler research, or Phase 5+ work after the strategic 10-week plan is complete.

---

## 11. The line that ties this together

> **ilo is the source language AI agents use when token cost matters. It does not compete with Vercel's Zero on deployment; it composes with Zero (or its successors) on the layer below. Its defensible moats are dense source, agent-friendly diagnostics, and published measurement discipline. Distribution will favour Zero; substance favours ilo. Strategy is to produce empirical artefacts that propagate without distribution support, and let the work outlast the news cycle.**

That's the whole thing.

---

## Source files cross-reference

This doc is the conceptual record. The implementation specs are:

- `00-strategy-v2.md` — the 10-week execution plan
- `01-modular-skills.md` — Phase 1 detail
- `02-closed-loop-benchmark.md` — Phase 2 detail
- `03-publish-the-data.md` — Phase 3 detail
- `04-typed-fix-plans.md` — Phase 4 detail
- `~/code/ilo-lang/ilo/MANIFESTO.md` — amended philosophical foundation (2026-05-18)
- `~/code/ilo-lang/ilo/research/zero-comparison.md` — comprehensive technical comparison
- `~/code/dan-site/src/content/writing/zero-and-ilo-two-layer-agent-stack.mdx` — public blog draft of the two-layer thesis
- `~/code/ilo-lang/ilo/research/explorations/zero-equivalent/` — the Zero-source-token measurements
