# Provider prompt caching — mechanics, arithmetic, honest limits

Companion to PLAN.md G4. Everything here is provider-documented or
measured in this repo (`bench/closed-loop-2026-09-17-*.json`).

## Why this page exists

 ilo's metric is total tokens from intent to working code. Cache alignment
 attacks the *context loading* and *retry* terms for free — no language
 change required. The 2026-09-17 alignment matrix measured it at
 **3.5× cost cut** on identical work (`$0.0048` → `$0.00138` per task,
 warm) purely from prompt shape.

## The three providers (as of 2026-09)

| Provider | Mechanism | Cache write | Cache read | TTL |
|---|---|---|---|---|
| Anthropic | explicit `cache_control` breakpoints (or auto-cache) | 1.25× (5-min) / 2× (1-h) | **0.1×** | 5 min default; 1 h opt-in; refreshed on read |
| OpenAI | automatic prefix caching (≥1024-token prefix) | 1.25× on newer models | ~0.1× | minutes, unspecified |
| Google Gemini | implicit (≥4,096-token prefix on 3.x) + explicit `CachedContent` | normal (explicit) | **0.1×** | 1 h default; storage billed |
| DeepSeek | automatic prefix caching, reported per call as `prompt_cache_hit_tokens` / `prompt_cache_miss_tokens` | — | **0.02× peak / 0.02× off-peak** (hit vs miss: $0.006 vs $0.3 peak) | automatic |

DeepSeek's pricing is the outlier that makes alignment dramatic: a cache
hit costs **1/50th** of a miss.

## The arithmetic (Anthropic-class pricing: $3/M input, $0.30/M cached)

A coding agent re-sends an 80k-token stable prefix (system + tools + spec)
every turn, appending ~2k new tokens, 20 turns:

- Uncached: 20 × 80k × $3/M ≈ **$4.80**
- Cached (1 write at 1.25×, 19 reads at 0.1×): $0.30 + $0.456 ≈ **$0.76**
  → **84% saved**; longer conversations approach the ~90% floor.

Measured in this repo's own runs (DeepSeek flash, warm aligned curated):
47,424 cache-hit input tokens across 5 tasks — retries rode the cache for
free after the alignment fix, versus 0 hits in the legacy shape.

## The four alignment rules

1. **Stable prefix first.** Static content (system prompt, tools, spec)
   earliest; volatile content (task text, tool results) last. Anthropic's
   `tools` param caches as the earliest prefix — declare tools first.
2. **Append-only conversation.** Never rewrite an earlier message: any
   edit invalidates the cached prefix from that point. This is why the
   harness's *legacy* shape (repair rewrites the user turn) had **zero
   cache hits**, and the aligned shape (repair appends
   assistant+user turns) engages the cache on every retry.
3. **Keep traffic inside the TTL.** 5-minute caches refresh on read; an
   active loop keeps itself warm. Use `ttl: "1h"` for spiky multi-session
   agents.
4. **One invalidation event, deliberately chosen.** For ilo that event is
   the `^26.5` file pragma / spec version — the digest gate
   (`scripts/ilo-harness.py --check`, CI job `harness-stability`) fails
   the build if the prefix drifts for any other reason.

## ilo-specific contract

- `scripts/ilo-harness.py` emits the aligned prompt (spec-first,
  tools-contract, task-last) and pins a sha256 digest per context.
- `scripts/closed-loop-bench.py --cache cold` busts the cache per attempt
  (nonce in the system prefix) — the arm that measures honest cold-start
  economics. `--cache warm` is the steady-state arm.
- Cost fields in every record: `input_cache_hit_tokens`,
  `input_cache_miss_tokens`, `cache_savings_usd` (what the hits saved
  versus miss pricing).

## The honest limit

Caching narrows the spec-cost disadvantage of any language that ships a
spec — including ilo — but it never reaches zero, only applies within TTL
windows, and only per-provider. The unbeatable competitor is a language
already in the model's weights (Python): training is prompt caching with
infinite TTL. That asymmetry is why PLAN.md Phase 4 is a fine-tune (train
the spec into the weights) rather than more caching engineering.
