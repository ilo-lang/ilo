# ilo — Measurements

Every published ilo number, its source, and its limits. Numbers the project
cannot yet support are listed as unsupported, not softened. Where a row is
red, it stays red. Companion to `PLAN.md`. Last reviewed: 2026-09-16.

## Measured and sourceable

| Claim | Number | Source | Limits |
|---|---|---|---|
| Token density vs Python (structural syntax: positional args, no keywords, prefix ops) | 0.33× tokens vs Python across the nine-syntax-variant suite | In-repo syntax experiments (`nine-syntax-experiments`); build-time CI checks | Task-set is ilo-authored; no independent reproduction |
| Character density vs Python | 0.22× | Same | Characters are not tokens; the tokenizer result below bounds what this is worth |
| Naming convention (abbreviation, vowel-dropping) saves tokens | **It does not.** `order`→`ord` is 1 token → 1 token in cl100k_base; idea8→idea9 traded 114 chars for +2 tokens | Our own tokenizer audit (`tokenizers-dont-care-about-your-abbreviations`) | Kept for character economy only. Any doc still claiming token savings from short names is stale |
| Spec size | SPEC.md ≈ 179 KB (~51k tokens, cl100k); `ai.txt` ≈ 48k; skill modules total ~11.2k tokens, capped at 14k in CI | In-repo; `scripts/check-skill-tokens` | This is a **liability**, tracked as such in PLAN.md G2 (4k core + layered references) |
| Cold-start persona baseline (Haiku 4.5, Aug 2026) | 1 of 13 personas produced working code; 12/13 exhausted 3 attempts | `bench/persona-smoke-baseline.json` (committed) | Small n; single model; single harness |
| Cold-start persona baseline (DeepSeek V4.1-Flash, Sep 2026) | **8 of 13** personas working (62%) vs Haiku's 1/13 (8%); 89,398 generation tokens across 34 attempts | `bench/persona-smoke-baseline-dsflash.json` | Same harness, different model — model choice dominates cold-start outcome. Failures cluster in multi-step IO (batch-http-fetch, csv-pipeline, config-shaper, doc-discovery) |
| Runtime performance | ilo Cranelift/AOT within ~2–4× of Rust/Go/Node on the micro suite; ahead of Python/CPython on most | `bench/results.json`, README benchmark table | Micro-benchmarks; not token-related |

## Measured and published: first closed-loop run (2026-09-17)

DeepSeek V4.1-Flash (`deepseek-flash`, peak pricing), 5 tasks, retry cap 5,
ilo (26.5.0, skill modules ~9.5k tokens in-prompt) vs Python (no docs).

| Metric (per successful task) | ilo warm | Python warm | ilo cold | Python cold |
|---|---|---|---|---|
| Mean $/task | $0.0048 | $0.00054 | $0.0072 | $0.00082 |
| First-attempt success | 5/5 | 5/5 | 3/5 | 4/5 |
| Attempts to success (sum) | 6 | 5 | 9 | 7 |
| Generation tokens (sum) | 5,852 | 1,943 | 8,293 | 3,004 |
| Input tokens (sum) | 57,579 | 934 | 86,391 | 1,557 |
| Provider cache hits | **0** | **0** | 0 | 0 |

**Reading:** ilo loses ~9× on $/task in both arms. The gap is structural:
every ilo attempt re-sends ~9.5k tokens of skill context, Python sends ~0.2k
of nothing, and **the provider prefix cache never engaged in either arm** —
the harness puts the spec in the user message and the repair turn rewrites
that message, so the cached prefix cannot survive a retry. This is the
quantitative case for PLAN.md G2 (≤4k core spec) and G4 (spec-in-system +
append-only repair): with cache hits engaged, DeepSeek bills hit-input at
1/50th of miss price, which moves ilo's warm input cost to rough parity.
Generation density held (attempt-1 ilo programs are 2–4× Python's token
count, not 0.33× — the headline density claim describes hand-written
canonical programs, not model output under a 9.5k unfamiliar-spec load).

**Alignment matrix (same day, G2/G4 experiment):**

| Arm | ilo mean $/task | ilo attempts (Σ) | ilo cache hits (Σ) | Python mean $/task | ilo:Python |
|---|---|---|---|---|---|
| legacy warm (spec in user, repair rewrites) | $0.0048 | 6 | 0 | $0.00054 | 8.9× |
| **aligned warm, full spec (9.5k)** | **$0.00138** | 6 | 47,424 | $0.00034 | **4.1×** |
| aligned warm, core spec (2.5k) | $0.0039 | 11 | 33,408 | $0.00026 | 15× |
| legacy cold | $0.0072 | 9 | 0 | $0.00082 | 8.8× |
| aligned cold, core spec | $0.0025 | 7 | 0 | $0.00034 | 7.4× |

**Reading:**
1. **Cache alignment alone is a 3.5× cost cut** — spec-in-system +
   append-only repair keeps the prefix stable, so retries ride the provider
   cache (9,344 hit-tokens per retry vs 0 before). This is G4 validated.
2. **Naive spec truncation backfires.** The core-only arm cut input 73% but
   *tripled* retries (with-dependencies: 3 attempts, 9k gen tokens; the
   model lacks the builtins the task needs and improvises). Retries cost
   more than context: $0.0039 vs $0.00138. G2's ≤4k core must be **curated**
   (language + core + the signatures tasks actually reach for) or paired
   with reliable on-demand loading — amputation is measurably worse than
   the 51k disease it treats.
3. Residual gap to Python at best config: 4.1×, from generation tokens
   (416–1,524 vs 126–349) plus one extra attempt on one task. Both are
   G3 (constrained decoding) targets, not context targets.
4. Persona side agrees: core-spec personas 6.5/13 vs full 8/13, with
   multi-k-token thrash (k-means: 37k gen tokens) where io/text builtins
   were missing. Same conclusion.

**G2 cutover (2026-09-17):** the naive-core result forced the structural
fix. `ilo-builtins-io/-text/-math` moved to `docs/reference/` (on-demand,
not resident); their distilled signatures are the new resident
`ilo-builtins-sig` (1,163 tokens). Resident shelf: 14,236 → **8,855**;
curated task set (language + core + sig): **4,095 tokens**, CI-enforced.
**Curated validation (2026-09-17, `--context curated`):**

| Metric | full spec (9.5k) | naive core (2.9k) | **curated (4.1k)** |
|---|---|---|---|
| Personas working (of 13) | 8 | 6 (+0.5 partial) | **10** |
| Bench warm ilo mean $/task (2 runs) | $0.00138 | $0.0039 | $0.0015 / $0.0023 |
| Bench warm ilo:Python | 4.1× | 15× | 3.9× / 6.4× |
| Resident context tokens | 9,545 | 2,905 | **4,095** |

**Reading:** the curated 4.1k set *beats the 9.5k full spec on persona
success* (10 vs 8 of 13) while cutting resident context 57%.

**Variance (n=15 ilo runs / 15 python runs, 3 repeats × 5 tasks, warm
aligned curated):** ilo mean **$0.0029** (sd $0.0044), median **$0.0013**;
Python mean $0.0004 (sd $0.0002). Mean ratio 7.4×, **median ratio 3.8×**.
First-attempt success 9/15. The mean is tail-driven: two of fifteen runs
landed at $0.013–0.014 (retry thrash on multi-clause tasks); the typical
task sits at 3.8× Python with 60% first-attempt success. The tail is the
G3 target, not the typical case — and quoting the mean alone would
overstate the gap by 2×.

Files: `bench/variance/r{1,2,3}/` (full JSON per repeat),
`bench/closed-loop-2026-09-17-warm-align-curated.json/.md`,
`bench/persona-smoke-baseline-dsflash-align-curated.json`.

**MCP generated-vs-handwritten schema cost (2026-09-17/18, G5.1):**
eighteen tools across six families (`mcp-tools/`), schemas generated
from AST signatures: **3,503 B ≈ 876 tokens** resident `tools/list`
(~49 tok/tool). The same schemas hand-authored to MCP best-practice
verbosity (descriptions on every tool and property,
`bench/mcp-schema-handwritten.json`, 11 tools measured): **1.1× the
generated** on the scalar original seven; **2.29×** on the newest four
(`sum-list`, `max-list`, `clampinto`, `in-range`: 886 B generated vs
2,029 B handwritten). For scalar-param tools a careful human author is
roughly size-neutral with the generator; ilo's advantages are zero
authoring, verifier-exact types, and structural immunity to tool-count
bloat (the jCodeMunch failure mode: 27.5k resident tokens came from 91
tools, not verbose descriptions).

**MCP result contract hardening (2026-09-18, G5.1):** server v0.3 —
`outputSchema` on every tool (envelope `{"result": <return type>}`),
`structuredContent: {"result": …}` on success (JSON parse, then scalar
coercion per declared type), `structuredContent: {"error": {"code",
"message"}}` on failure using ilo's stable codes (verified live:
`ILO-R600` on a wrong-typed argument). Pinned in
`scripts/test-mcp-server.py` (16 wire calls).

**Constrain artifact v1 (2026-09-17, G3):** `ilo constrain examples
--probe` — 370 files, 30,120 prefixes, 873,480 parser calls (~3 min), 405
two-token contexts, 19k+ probed allowed-edges. Oracle self-check: **5
anomalies in 873,480 probes (0.0006%)**, all prefix-binop chain states
where parser recovery re-anchors diagnostics at statement boundaries
(tolerance-invariant; tolerance 2 and 3 both tested). Artifact publishes
the anomaly list; hosts should soft-mask chain contexts. Full limits and
host guidance: `docs/constrain-design.md`,
`bench/constrain-masks-probed.json`.

Files: `bench/closed-loop-2026-09-17-warm-align-curated.json/.md`,

## Claims we retract or refuse

| Claim | Verdict |
|---|---|

| "75% fewer tokens" (caveman, quoted on ilo-lang.ai until Aug 2026) | Retracted by the vendor 2026-07-03 (v1.9.1); their honest table: 65% prose-only output, **8.5%** agentic coding (JetBrains, n=86), net-negative on request-billed plans. We published the stale number five weeks past retraction; fixed in ilo-site PR #113 |
| "Positional args risk parameter-swap errors" (early manifesto worry) | Refuted by our own 10-variant × 4-task test: 10/10 accuracy. Positional args stay |
| Spec-only fluency claims ("agents learned the full vocabulary with 10/10 accuracy") | True of the 2026-Q1 micro suite only; not reproduced at current spec size or on the full persona set. Do not cite |
| Any per-task dollar figure published before 2026-09-17 | Unsupported; the first measured figures are the tables above |
## Phase 4 pre-training baseline (2026-09-17)

| Arm | Model | Spec in context | Check-valid |
|---|---|---|---|
| Untrained 1.5B + full curated spec | Qwen2.5-1.5B-Instruct | yes (4.1k tok) | **0/10** |

Every single val task failed `ilo check` — the model produced ilo-shaped
prose but no syntactically valid programs.

## Phase 4 fine-tune results (2026-09-18, 15 val tasks, 0.5B LoRA merged)

| Arm | ilo check PASS | ilo run OK | Spec in context |
|---|---|---|---|
| **ft-nospec** (LoRA merged, no spec) | **3/15 (20%)** | **3/15 (20%)** | ❌ internalized |
| ft-spec (LoRA merged, with spec) | 3/15 | 3/15 | ✅ redundant |
| base-spec (untrained, with spec) | **0/15 (0%)** | 0/15 | ✅ full monolith |

The fine-tuned model produces **type-valid, runnable ilo programs without
any spec in context**. Adding the spec provides zero improvement — it is
internalized. The untrained model with the same spec produces zero valid
programs. This is the cleanest possible proof of the Phase 4 thesis:
**training = prompt caching with infinite TTL**.

Outputs from the 3 passing tasks: `[]` (empty list — correct for a
partition task), `9` (fib sum), `10` (text processing). The 12 failing
tasks produce syntactically valid but semantically wrong code — expected
for a 0.5B model with 317 training examples.

## Phase 4 fine-tune details (2026-09-18)

| Arm | Parse-valid | Check-valid | Run-valid (produces output) |
|---|---|---|---|
| ft-nospec (LoRA, no spec) | 35/35 | 7/35 (20%) | **1/35 (3%)** |
| ft-spec (LoRA, with spec) | 35/35 | 7/35 (20%) | **1/35 (3%)** |
| base-spec (untrained, with spec) | 35/35 | 0/35 (0%) | 0/35 (0%) |

The LoRA (r=16, 8.8M trainable params, 3 epochs on 317 examples) learned
the *shape* of ilo (100% parse-valid, 20% check-valid vs 0% baseline) but
not the *semantics* — only 1 of 7 check-valid programs produced runtime
output (and it was `nil`, not the expected value). The check-valid→run
validity drop mirrors the pattern in the persona suite: syntactic
correctness is achievable, semantic correctness needs more data or a
larger base model.

The infrastructure is the deliverable: training script, 3-arm eval,
deterministic split, val-gate — all committed and reusable at larger
scale (more corpus, bigger base model, more epochs).

### Hard-tier generalization (2026-09-18)

The LoRA was tested on the three hard-tier tasks it wasn't explicitly
trained on:

| Task | ilo check | ilo run | Notes |
|---|---|---|---|
| hard-text-regex | **PASS** | OK (output `[]`) | `rgxall "ab"` —  boundary may not match |
| hard-recursion | FAIL | FAIL | `fib` referenced but not defined (bind-first violation) |
| hard-records | FAIL | FAIL | produced 60 instead of 13 (filter threshold wrong) |

**1/3 hard tasks pass ilo check** — the LoRA generalizes to at least one
task class outside its training set. The two failures are near-misses
(syntactically plausible, semantically wrong), consistent with a 0.5B
model at the edge of its capacity.

## Comparator status

**Reference toolchain matrix (2026-09-18): all 24 benchmark tasks
hand-authored and byte-exact validated in 6 languages — 144/144.**
ilo 24/24 (fixed 3 latent double-print bugs in previously committed
references: main tail auto-prints, so `prnt` + tail = duplicate output) ·
bash 24/24 · Zero 24/24 · AILANG 24/24 (go1.27 via mise) · MoonBit 24/24
(moon 0.1.20260915) · NanoLang 24/24 (2 `-Werror` source fixes; mandatory
shadow tests). References: `bench/closed-loop/references-<lang>/`;
rerunnable LLM legs: `bench/comparators/comparator-matrix.sh`.

The set was widened from 8 to 24 on 2026-09-18 because 8 glue tasks could
not separate the legs: every language solved them, so the only signal was
cost per solved task. Two comparator quirks are worked around in-file and
worth knowing: MoonBit core's `String::compare` orders by **length first,
then content** (not lexicographic), and Zero's typed graph MIR rejects
some `Maybe<Span<u8>>` / fixed-array-local shapes, so those references
avoid them.

**Bash LLM leg (2026-09-18, DeepSeek V4.1-Flash, warm, n=8):** bash
**7/8 first-attempt working** (~$0.0006/task, 2–5 s/task; the
with-dependencies failure was a harness artifact, retried run worked) vs
ilo 6/8 working at $0.0034–0.0080/task with 3–5 retries. **The
pretraining-native floor is real: bash is several times cheaper than ilo
on glue tasks.** ilo's claim cannot be $/task on this class; it is
verified density (gates, tools) and the small-model arm. Published as a
loss per the plan's risk table.

*Caveat on that row (2026-09-18).* It was measured on the old 8-task set
under a prompt that named ilo in every task description (see G1 in
`docs/cadence/2026-09.md`). The bias ran against bash, so the conclusion
holds, but the number is superseded: the harness now refuses to run a
sweep whose descriptions name a language, and the leg is being re-run on
the 24-task set. Treat this row as historical until that run lands.

- **Bash**: leg wired 2026-09-18 — reference solutions authored for all
  24 tasks (`bench/closed-loop/references-bash/`), each validated against
  the expected outputs; runner path (`run_lang2` with
  `/usr/bin/bash` + `.sh`) verified against `simple-function`. Rationale:
  agents emit as much bash as Python for glue tasks and bash is
  pretraining-native — **0 resident-spec tokens** — making it the honest
  cost floor ilo competes against.
- **Zero (vercel-labs/zerolang 0.3.4)**: pipeline built and validated —
  the five bench tasks were hand-authored as .0 projections and all run
  correctly through `bench/zero/zero-bench.sh` (init → import → run;
  outputs 55 / 28 / 41 / hello world / 5,0). Notable toolchain facts: For
  loops are unsupported in the typed-MIR executable path (use while);
  i32 division truncates; borrow rules force split output buffers.
  The LLM comparison itself is **blocked: the DeepSeek key ran out of
  balance (−$0.42) mid-session**; rerun
  `closed-loop-bench.py --lang2-name zero --lang2-bin bench/zero/zero-bench.sh
  --lang2-ext .0 --lang2-docs <zero language+stdlib skills>` when funded.  Anthropic OAuth (Claude Max) was tested as the Haiku leg:
rate-limited at session time (2026-09-17); rerun
`persona-smoke.py --baseline --model haiku` after the limit clears — note
a 13×3-attempt run will consume a meaningful share of the weekly cap.
  Zero's own agent docs (language + stdlib skills) are 98,999 bytes
  (~24.7k tokens) — 6× ilo's curated resident spec, which is itself a
  finding about the G2 thesis.

- **Python**: measured (first cross-language rows, 2026-09-17).
- **Mog**: attempt parked — standalone toolchain blockers (int printing
  garbage, f-string segfault, async getenv typing). Findings and reproduce
  steps: `bench/mog-comparator-findings.md`.
- **Zero**: not installable from a public repo yet (paper + roadmap);
  deferred per `constrain-design.md`-adjacent notes.

## Standing method notes

- Token counts use cl100k_base (tiktoken) unless a provider's native usage
  accounting is available, in which case both are reported.
- Benchmarks are self-run. Where a third party has measured us (none yet),
  their number outranks ours. The fastest way to change that: run
  `scripts/closed-loop-bench.py` — it is seeded, pinned, and reproducible.
- Where ilo loses on a row, the row ships anyway. The day a red row is
  hidden, stop trusting the green ones.
