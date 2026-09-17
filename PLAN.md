# ilo Plan — Token Efficiency, Operationalised

Status: draft for review · 2026-09-16
Scope: goals, uses, tactics, and sequencing for the next two quarters, grounded in the manifesto, the agentlanguages.dev critique, the 42-language landscape survey, 2026 token-optimisation research, and this repo's own evidence.

---

## 1. Where we are

### 1.1 The thesis is winning; the project is not

Token efficiency became the industry's framing in 2026: caveman (106k★), ponytail (140k★), headroom (72k★), Anthropic's context editing and memory tool, prompt caching at 0.1× list price. Agent input is dominated by re-reading (tool schemas were 71–80% of a leaked 17k-word system prompt; one 113-subagent session here spent 55% of budget outside the parent conversation). The world moved toward ilo's metric.

ilo has not moved with it:

- **Paused.** 2,144 commits May 2 → Aug 5, then six weeks of silence. The persona→bug pipeline that the catalogue praised as ilo's "clearest contribution" stopped.
- **Unmeasured.** The closed-loop benchmark exists as code (`scripts/closed-loop-bench.py`, ILO-364) but no run has been published. `bench/results.json` holds May runtime micro-benchmarks. The agentlanguages critique caught precisely this: *"has not yet published the closed-loop benchmark its own strategy identifies as the thing that would make its central claim checkable."*
- **Overweight.** SPEC.md ≈ 51k tokens, `ai.txt` ≈ 48k, against Mog's load-bearing 3,200. Our own July analysis (`spec-budgets-for-agent-languages`) diagnosed this before the critique published it: agents skim a 40k spec and produce plausible-but-wrong code — the exact failure ilo exists to prevent.
- **Cold-broken.** The committed August baseline: 13 personas on Haiku 4.5, one success, twelve exhausting the retry cap. In cost-per-success terms that is the most expensive language in the catalogue, regardless of how terse successful programs are.
- **Mis-marketed.** The README headlines syntax density (0.33× tokens). Our own data says structure, not abbreviation, produced that (naming: 0.33×→0.33×), and manifesto principle 6 concedes tooling structure dominates cached steady-state cost. The marketing leads with the weakest lever.

### 1.2 The evidence base

| Source | Finding we act on |
|---|---|
| Manifesto | Metric: total tokens from intent to working code = spec + generation + context + errors + retries. Principle 6 (structured surface) is "the single biggest driver of per-task cost reduction in cached steady-state." |
| agentlanguages.dev critique | Spec grew against thesis; 1/13 cold-start baseline; benchmark unpublished; adoption minimal; roadmap private. |
| Our articles (`tokenizers-dont-care`, `65-percent-and-8-percent`, `spec-budgets`, `most-of-a-system-prompt`, `graft-vs-jcodemunch`, `referencing-a-skill`, `session-logs`) | Abbreviation saves characters, not tokens; benchmark shape = tokens/attempt × attempts/success; 4k layered spec proposal; resident-cost accounting; 2,225 resident tokens for 29 installed skills; per-message cost already in session logs. |
| Language landscape (42 repos) | Sustained engineering clusters on verification + diagnostics (aver, vow, zerolang, hale, nanolang); pure-syntax sprints die (codong, pact, tacit); Axis ships logit masks; Mog bounds the spec; Zero pairs small surface with repair-plan diagnostics and has distribution (Vercel); AILANG runs agent-driven sprints at 4.5k commits/6mo. |
| Token-opt research | Lever hierarchy: don't generate (ponytail −22% tokens, 100% safety) > don't load (context editing −84%; tool search −85%) > cache (0.1× reads; ~84% on the dominant prefix) > compress (caveman: 65% prose / 8.5% agentic; TOON 2–18% net in agents). Caveman's HONEST-NUMBERS discipline is the trust standard. Constrained decoding makes schema-guaranteed output table stakes (Jev); calibration + cost is the differentiator. |
| Caveman wrap benchmark | Reading-side compression: −33.2% on reading-heavy cases, oracle-checked; +9.9% where nothing compresses. Red rows stay red. |

### 1.3 Diagnosis, one line

ilo owns verified density (generation + retry terms) but pays a resident tax (spec) it no longer controls, on a cold path it hasn't engineered for, with a measurement instrument built and never run.

---

## 2. North star

**Cost per successful task**, cold and warm:

```
$/task = (Σ_attempts tokens_out × price_out + tokens_in × effective_price_in) × E[attempts | success]
effective_price_in = price_in × (1 − cache_hit_fraction × 0.9)   # 0.1× cached reads
```

Reported per task, per model tier (small / frontier), per language, with `tokens/attempt`, `attempts/success`, `success_rate`, `wall_time`. Every goal below must move this number or the measurement of it. Vanity metrics (tokens per program, character counts) are demoted to diagnostics.

---

## 3. Goals

### G1 — Ship the closed-loop benchmark as a neutral instrument (P0)

The critique's sharpest point is our largest opportunity: **nobody in the field has published this** — Mem0, Supermemory, MemOS, shadcn/lint, and TypeSafe all grade their own homework. A neutral, reproducible harness is a field asset that also happens to measure ilo.

Acceptance:
- `scripts/closed-loop-bench.py` extended: `--cold` arm (fresh context per attempt) alongside warm; price tables; `$/task`; Python wired in via existing `--lang2` plumbing as the default baseline; Mog and Zero invited as comparators.
- Task set drawn from `examples/` (369 programs doubling as the regression suite), tiered simple/medium/hard.
- One published run: `bench/closed-loop-<date>.json` + markdown with HONEST-NUMBERS-style caveats, including rows where ilo loses.
- Reproduction: seeded, pinned model versions, commands in the writeup.

### G2 — Bounded, layered, referenced spec (P0)

Acceptance:
- Core spec ≤ 4,000 tokens in-context; per-cluster reference files loaded on use (path-addressed referencing, @skills-style: one-line resident descriptions, bodies fetched on trigger — our own measurement: 2,225 resident tokens for 29 full installs is the failure mode to avoid).
- Validation: the 4k layered spec must match the 40k monolith on the existing spec-only generation-accuracy runs. If it matches, the other 36k were never load-bearing; if it doesn't, the delta names what the core was missing.
- CI budget with eviction: no spec growth without a linked persona-transcript artifact (the existing ≥3-transcripts / >40-token criterion, now mechanical); deletions counted as wins.
- `ai.txt` either regenerated from the core or retired — two "real specs" is how drift happened.

### G3 — Constrained decoding for small models (P0)

The August baseline says the retry term dominates cold cost. ilo's closed, LL(1)-ish grammar is *more* maskable than Axis's, which already ships `--constrain`/`--logit-masks`. This makes invalid ilo unemittable at decode time — the retry term attacked preventively instead of correctively.

Acceptance:
- `ilo constrain` emits parser state machine + per-state token masks (Axis JSON format as compat target).
- Persona suite rerun on Haiku-class with masks in the harness: publish success-rate and $/task delta vs the 1/13 baseline.
- If the delta is small, publish that too — it calibrates the constrained-decoding hype with ilo data.

### G4 — Cache-native harness (P1)

Acceptance:
- `ilo harness` emits the cache-aligned system prompt: spec-first, tools-first, task-last; `^26.5` pragma doubles as the cache key (version bump = the only invalidation event).
- CI asserts byte-stable prefix per version.
- `ilo bench --cache` reports effective tokens at 0.1× next to raw, both in benchmark output.
- Docs: provider caching mechanics, break-even arithmetic, and the honest limit (caching narrows the spec-cost disadvantage vs Python-in-weights; it never reaches zero, and only within TTL windows).

### G5 — Uses: verified agent-compute runtime (P1)

Stop competing for "the language agents write apps in." Aim ilo where verified density + determinism are load-bearing:

1. **MCP tool implementations.** Tool schemas are the dominant resident cost (886 vs 27,526 tokens measured between two real servers). `ilo serve --mcp`: tool schema + handler from one ilo declaration, typed JSON diagnostics, verify-before-execute. Publish 3–5 example tools with measured resident-schema cost vs hand-written equivalents.
2. **Typed decision gates (the open Jev).** ilo pure functions + verifier = state → typed verdict, errors impossible by construction, self-hosted, free. The agent loop needs a cheap classify/route/guard layer; schema-guaranteed output is table stakes (constrained decoding), and ilo adds auditability Jev can't.
3. **Executable skill payloads.** Where behaviour is distributed to agents (skills, plugins — Agent Plugins 1.0 format), the deterministic core can be ilo: verify-on-install, small resident footprint, referencing per G2.
4. **The benchmark harness itself** runs on ilo where sensible. Dogfooding is the distribution.

Anti-uses (declared): memory frameworks (absorbing into models/frameworks), proxy/compression layers (caveman/headroom exist — document `headroom wrap ilo` interplay instead), formal-proof theatre (the verifier is a retry-cost tool, not Verus/Lean), human ergonomics (unchanged stance).

### G6 — Sustain the measurement loop (P0)

The process was the catalogue's praise; it stopped.

Acceptance:
- Persona pipeline resumed on a cadence (weekly fleet run, monthly published).
- Bot-fleet the regression + persona matrix (nanolang's MAC-fleet model): task-ID'd commits, findings filed as language bugs with the (a)/(b)/(c) builtin criterion attached.
- A public cadence artifact: short monthly note — runs run, bugs filed, spec bytes up or down. The roadmap moves off the private tracker or the private tracker stops being the only one.

---

## 4. Sequencing

| Phase | Window | Work | Exit criterion |
|---|---|---|---|
| 0 | Days | HONEST-NUMBERS.md in-repo (every published number, sourced, with caveats — including the tokenizer result and the 1/13 baseline); resume persona pipeline; repo note answering the critique point-by-point | Page merged; fleet running |
| 1 | ~30d | G1 benchmark v1 (cold+warm, pricing, Python); G2 4k core spec experiment | Published run + layered-spec verdict |
| 2 | 60–90d | G3 constrained-decoding spike + persona rerun; G4 `ilo harness` + `--cache` reporting | Masked Haiku delta published; cache-aligned default |
| 3 | Quarter | G5 MCP runtime + decision-gate docs + 3–5 tools; distribution parity (skills/npm/plugin packaging; MCP server in the plugin) | External user runs an ilo tool or gate in a real harness |
| 4 | Later | Fine-tune a small model on the 369-example corpus + spec to delete the spec term entirely (training is prompt caching with infinite TTL — the only true answer to Python-in-weights; the $8k/mo session-log corpus is training data). Arrow-typed tabular builtins for the three-plane pattern. | Spec-loading term ≈ 0 for tuned models |

Dependencies: G1 unblocks everything (it is the metric made real). G2 and G3 are independent; both feed G1's next run. G5 depends on G1 (claims need numbers) and G2 (resident cost is the selling point).

---

## 5. Risks

| Risk | Mitigation |
|---|---|
| Benchmark shows ilo losing $/task to cached Python on cold, one-shot tasks | Expected; publish anyway. The warm steady-state and small-model arms are where the thesis wins, and an honest loss redirects effort before more spec is written |
| Masked decoding needs host-harness cooperation | Fallback: verification-fail-fast + typed fix plans already shorten retries; masks make it preventive. Ship masks as opt-in JSON, not a runtime requirement |
| Spec regrows after the 4k experiment | CI eviction budget is mechanical, not self-enforced — the 51k outcome is what self-enforcement produces |
| Adoption stays low regardless | Reframe success: the benchmark + harness are field infrastructure; ilo the language is the first beneficiary. Zero's lesson is distribution; ours is measurement first |
| Solo-maintainer throughput | Bot-fleet the mechanical layers; cut anything that doesn't move $/task or its measurement. The frozen-cave pattern (caveman froze 3 repos) is permitted: freeze, don't half-maintain |

---

## 6. Success metrics (review monthly against this doc)

1. Published closed-loop runs (target: monthly cadence by Q4)
2. $/task cold and warm, ilo vs Python, small-model tier — trend
3. Core spec tokens (target ≤ 4k) and CI budget adherence (zero unlinked additions)
4. Haiku-class persona success rate with masks (target: materially above 1/13; exact bar set by first masked run)
5. Resident tokens for the default ilo harness (target: ≤ 4k spec + minimal tool surface, referenced not installed)
6. One external reproduction of the benchmark by a non-maintainer

---

## 7. One-line version

Stop selling the syntax. Ship the measurement, shrink the resident tax, make small models able to speak ilo at all, and aim the language at tools, gates, and skills — the places where verified density is the point and token cost is the bill.

---

## Appendix A — Research base (2026-09-16)

Full survey behind this plan: the agentlanguages.dev catalogue (42 projects),
a commit-history review of all 38 cloneable repos (6-month window), and a
token-optimisation landscape scan. Key evidence, compressed.

### A.1 Agent-language landscape

- **Sustained engineering clusters on verification + diagnostics**: aver
  (2,899 commits/6mo), vow (1,913), vera (2,264), zerolang (1,188 since May
  init), hale (1,537), nanolang (1,509, bot-fleet "MAC" committers).
  Pure-syntax sprints die: codong (92 commits, Mar–Apr, silent since),
  pact-lang (223, Apr only), tacit (257, Apr–May), laze/lume (one weekend /
  one day).
- **ilo** was the 2nd-highest-velocity repo surveyed (2,144 commits May–Aug)
  before its six-week pause.
- **Moves worth copying**: Axis ships grammar-state logit masks
  (`--constrain`, `--logit-masks`) for decode-time validity (→ G3); Mog
  bounds its whole spec at 3,200 tokens (→ G2); Zero pairs JSON diagnostics
  + typed fix plans with Vercel distribution; AILANG runs bot-driven
  sprints (4.5k commits, "Voight-Kampff" agents); Pact/Fabro ship
  single-binary + MCP-first distribution.
- **8 of 42 have no public repo** (MoonBit, Prove, Pel, Quasar, Plumbing are
  closed or paper-only). The catalogue's own critique of ilo was the most
  evidence-grounded entry — it cited our private strategy docs and committed
  baselines.

### A.2 Token-optimisation levers (verified hierarchy)

1. **Don't generate** — ponytail ("lazy senior dev" ladder): −22% tokens,
   −27% time, 100% safety on its 12-task benchmark; beats terse-output
   styles in agentic settings.
2. **Don't load** — Anthropic context editing −84% tokens (100-turn eval);
   tool search −85% of tool-definition tokens (tool schemas were 71–80% of a
   leaked 17k-word system prompt; resident-cost accounting: graft 886 vs
   jCodeMunch 27,526 schema tokens). @skills referencing: path-addressed
   skills, one-line resident descriptions (measured: 2,225 resident tokens
   for 29 installed skills).
3. **Cache** — provider prompt caching bills cache hits at 0.1× (Anthropic
   explicit, OpenAI/Gemini implicit); worked example: 80k stable prefix × 20
   turns = $4.80 → $0.76 (−84%). Serving layer: PagedAttention/vLLM,
   RadixAttention, DeepSeek MLA (~93% KV reduction), FP8/FP4 KV quantization,
   LMCache/Mooncake. Rule: append-only history, stable prefix, tools-first,
   volatile last. DeepSeek reports `prompt_cache_hit_tokens` natively —
   used by the bench's warm arm.
4. **Compress** — caveman: 65% output on prose vs **8.5%** agentic coding
   (JetBrains, 86 tasks, quality flat p=0.82); its proxy: −33.2% on
   reading-heavy tool output (54 pinned runs, oracle-checked). headroom:
   reversible compression proxy (72k★), honest per-compressor tables
   (realistic JSON 26–54%, not the 60–95% headline). TOON: −14.5% vs
   *compact* JSON; independent studies find 2–18% net in agents and no
   accuracy effect. Prompt compression (LLMLingua-2) verified 2–5× with a
   quality cliff; semantic caching has ~68% false-hit rates (CacheEval).
5. **Train it into weights** — the terminus (caveman's cavegemma labs
   direction): foundation training is prompt caching with infinite TTL and
   the only real answer to "Python is already in the weights".

### A.3 Agent memory frameworks

Five families: in-context blocks (Letta, Anthropic memory tool,
CLAUDE.md/AGENTS.md), extract-and-store (Mem0 ~65k★, Supermemory, Memobase),
temporal knowledge graphs (Graphiti/Zep ~31k★, cognee, HippoRAG 2), memory
OS (MemOS), file/wiki (memU ~14k★, A-MEM). Convergences: background
consolidation everywhere; ADD-only/temporal invalidation replaces deletes;
MCP servers + coding-agent plugins are table stakes. Benchmarks (LoCoMo,
LongMemEval) are vendor-run and mutually contradictory (64–94 on the same
sets) — the field's measurement gap, which G1's harness partially addresses
for the language slice.

### A.4 Adjacent signals

- **shadcn/lint**: lint as machine-verifiable contract with agent-teaching
  error messages (v0.1.0, ~1.9k★ in days) — same move as ilo's typed fix
  plans, applied to design systems.
- **TypeSafe Jev** ("System One" decision model, launched 2026-09-15):
  state + pre-declared typed questions → typed probabilistic answers,
  single pass, no autoregression; closed API, no paper. Schema-guaranteed
  output is commodity (constrained decoding); the moat claim is RLCD
  calibration. ilo's open answer: verified pure functions as decision gates
  (→ G5.2).
- **Apache Arrow** as the agent data plane: DuckDB executes, Arrow carries,
  the model sees only bounded JSON previews + resource links. Columnar beats
  all text encodings once results exceed ~100 rows / ~10 KB.
- **Cost accountability**: per-message cost is already in local agent
  session logs (this machine: $7,976.69/30d; a 113-subagent session spent
  55% outside the parent conversation). The bench meters from the same
  principle: provider-reported usage, nothing estimated.

### A.5 "Stanford NLP Shepard" — negative result

No such project exists. `github.com/stanfordnlp/shepard` → 404; the
stanfordnlp org has no repo by that name; no paper or blog under it
(verified 2026-09-16). Unrelated namesakes: Shepard Labs LLC (commercial Go
tooling), "Shepherd: A Critic for Language Model Generation" (Wang et al.
2023, arXiv:2308.04592 — Stanford-affiliated critique model, not a router),
"Shepherd" agent-runtime substrate (arXiv:2605.10913), "LLM Shepherding"
(arXiv:2601.22132).

The real occupants of the routing slot the question was aiming at:
- **Smoothie** (Stanford Hazy Research, 2024-12-10,
  hazyresearch.stanford.edu/blog/2024-12-10-smoothie): label-free
  inference-time router picking the best LLM per prompt; 75.0 routing
  accuracy vs 65.4 random.
- **RouteLLM** (arXiv:2406.18665): cost-aware routing between strong and
  weak models.

Relevance to this plan: routing is the missing lever between G3 (make small
models able to speak ilo) and the cost metric — a verified ilo decision
gate (G5.2) plus a router is the "cheap model first, escalate on
uncertainty" cascade. Logged as an input to Phase 4, not a dependency.
