# ilo — Honest Numbers

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
Raw `*-warm-from-log.*` files are the first warm arm from the
pre-fix filename bug, kept for provenance.



## Claims we retract or refuse

| Claim | Verdict |
|---|---|

| "75% fewer tokens" (caveman, quoted on ilo-lang.ai until Aug 2026) | Retracted by the vendor 2026-07-03 (v1.9.1); their honest table: 65% prose-only output, **8.5%** agentic coding (JetBrains, n=86), net-negative on request-billed plans. We published the stale number five weeks past retraction; fixed in ilo-site PR #113 |
| "Positional args risk parameter-swap errors" (early manifesto worry) | Refuted by our own 10-variant × 4-task test: 10/10 accuracy. Positional args stay |
| Spec-only fluency claims ("agents learned the full vocabulary with 10/10 accuracy") | True of the 2026-Q1 micro suite only; not reproduced at current spec size or on the full persona set. Do not cite |
| Any per-task dollar figure published before 2026-09-17 | Unsupported; the first measured figures are the tables above |
## Standing method notes

- Token counts use cl100k_base (tiktoken) unless a provider's native usage
  accounting is available, in which case both are reported.
- Benchmarks are self-run. Where a third party has measured us (none yet),
  their number outranks ours. The fastest way to change that: run
  `scripts/closed-loop-bench.py` — it is seeded, pinned, and reproducible.
- Where ilo loses on a row, the row ships anyway. The day a red row is
  hidden, stop trusting the green ones.
