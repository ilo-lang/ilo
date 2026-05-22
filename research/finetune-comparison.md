# Fine-tune comparison: ilo-tuned local model vs Python on same model

**Status:** Research plan / methodology pre-registration  
**Date:** 2026-05-22  
**Ticket:** [ILO-414](https://linear.app/ilo-lang/issue/ILO-414)

---

## Motivation

The 2026-05-22 multi-class A/B run produced the following result on Opus, 10 personas:

| variant | mean tokens / pair | mean attempts | working |
|---|---|---|---|
| Opus pure-ilo (no py) | 62,388 | 5.0 | 9 / 10 |
| Opus pure-Python | 33,668 | 1.3 | 10 / 10 |

Python wins by **46 % on cold-load**. Even subtracting ~7K tokens for ilo skill-doc loading per run, ilo still costs ~64 % more in pure generation tokens than Python. This gap is the **spec-loading tax**: the model knows Python from training; it must learn ilo from the spec every session.

The manifesto's central claim — "ilo's denser source pays for itself once the language is known" — is not testable with Claude alone today (Anthropic does not expose fine-tuning for Opus or Sonnet). The cleanest available test is to **fine-tune an open-weights model on the ilo corpus**, then run both the ilo and Python variants on that same tuned model. Both sides have the same prior strength; only the language changes.

---

## Base model

**Primary:** Qwen 2.5 Coder 14B (instruct variant)  
- Strong code prior; fits a LoRA tune in ~16 GB working RAM on M3 Max  
- Estimated 4–6 h tune cycle  

**Fallback:** Qwen 2.5 Coder 7B (if 14B won't fit) or Qwen 2.5 Coder 32B (if 14B converges poorly)

---

## Tooling

**Primary:** `mlx-lm.lora` — Apple-native, fastest on M-series silicon  
**Fallback:** `peft` + PyTorch MPS if MLX hits a blocker

---

## Training corpus

Snapshot the repo at the `main` HEAD commit before training begins; record the commit hash in results.

**Include:**
- `SPEC.md`
- `ai.txt`
- `skills/ilo/*.md`
- `examples/*.@` (all merged examples in repo root and `examples/`)
- Any other `.@` source files at repo root

**Exclude:**
- `persona-runs/` — exclude to prevent label leakage from the evaluation corpus
- Generated artefacts, binaries, lock files

---

## Training format

Instruction-tuning pairs, each structured as:

```
prompt:      <persona-style task specification>
completion:  <working .@ program that satisfies the task>
```

Generate pairs from:
1. Existing `examples/*.@` files: use the filename + any embedded comment header as the prompt
2. A small hand-written set of dogfood specs (10–20) covering edge cases under-represented in examples

Target: **200–500 pairs**. A separate scripting pass (`scripts/gen-finetune-pairs.@` or similar) can automate extraction from `examples/`; hand-curation validates output quality.

---

## Hyperparameter starting point

| param | value |
|---|---|
| LoRA rank | 16 |
| epochs | 3 |
| learning rate | 1e-4 |
| batch size | 4 (or max that fits) |

Adjust rank, epochs, and LR based on held-out validation loss curve. Document final values in results.

---

## "Knows ilo" threshold

The tuned model is considered to **know ilo** if it achieves > 80 % working on a held-out persona slice (5–10 personas not in the training set) with **no skill-doc context in the prompt**. This threshold is reviewed against the actual loss curve before the A/B run proceeds.

---

## Evaluation harness

Re-use the same 10-persona dispatch prompts from the 2026-05-22 run (positions 1–10 of `/tmp/sampled-25.txt`).

**tuned-ilo variant:**
- Prompt: persona task spec only — no skill-doc loading, no `ai.txt` prefix
- Cycle: model output → `ilo check` / `ilo run` → retry on error
- Success: `ilo run` exits 0 with correct output

**tuned-Python variant:**
- Prompt: same persona task spec, standard Python dispatch shape
- Cycle: model output → `python3` → retry on error
- Same outcome scoring

**Python baseline sanity check:** run the tuned model on the Python variant before the full A/B. If it underperforms Opus Python badly (e.g. < 60 % working), the comparison is unfair in the other direction and should be documented before proceeding.

---

## Per-pair metrics

For each persona × variant:
- outcome: working / partial / failed
- generation tokens (prompt + completion, all attempts summed)
- attempts to first working result
- wall-clock seconds

---

## Results table

Report a three-way comparison:

| variant | model | mean tokens / pair | mean attempts | working |
|---|---|---|---|---|
| tuned-ilo | Qwen 2.5 Coder 14B (LoRA) | TBD | TBD | TBD |
| tuned-Python | Qwen 2.5 Coder 14B (LoRA) | TBD | TBD | TBD |
| pure-Python baseline | Claude Opus (2026-05-22) | 33,668 | 1.3 | 10 / 10 |

---

## Pre-registered falsification criterion

> If mean tokens for **tuned-ilo > 1.0× tuned-Python** on this corpus, the manifesto's central token-cost claim is empirically not supported on this task set even after removing the spec-loading tax.

Document the result either way. The outcome determines the next move:

- **tuned-ilo wins (tokens < tuned-Python):** The natural-surface concessions are confirmed unnecessary. The manifesto's prefix bet is vindicated. Work shifts to spec/skill density (ILO-382, ILO-384) and tuning-as-distribution-strategy.
- **Python wins even on tuned model:** ilo's positioning shifts toward token-density of *artifacts* (compiled programs, agent-to-agent payloads) rather than generation cost. The manifesto rewrites accordingly.

---

## Open questions

1. **Training-pair generation script.** Hand-curation of 200–500 pairs is the bulk of the work. Consider a sub-ticket to script extraction from `examples/*.@`.
2. **Tune budget.** The rank-16 / 3-epoch / 1e-4 starting point is a reasonable prior; the loss curve may call for adjustment.
3. **Model fallback policy.** If 14B converges poorly (loss plateau above ~1.5 nats on held-out), try 32B before concluding the corpus is too small.
4. **Token counting.** Qwen's tokenizer differs from Claude's; report raw Qwen token counts for the A/B table, note the Claude baseline used Claude's tokenizer.

---

## Dependencies

- **ILO-364** (closed-loop benchmark): shares the dispatch harness shape; ILO-414 is the fine-tune cap on Phase 5.
- **ILO-382** (modular skill cap) + **ILO-384** (dogfood regression loop): orthogonal non-fine-tune path to closing the cold-load gap. Run in parallel; either succeeding makes the manifesto's bet defensible at cold-load.

---

## Source references

- 2026-05-22 Opus pure-ilo vs pure-Python pilot (this session)
- `zero-gap-specs/lessons-from-zero.md` per-task economics table
- Blog draft: `dan-site/src/content/writing/agent-natural-ab-against-the-manifesto.mdx`
