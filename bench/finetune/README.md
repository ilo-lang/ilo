# Fine-tune corpus (Phase 4 seed)

`train.jsonl` (317) / `val.jsonl` (35) — deterministic content-hash split of
`bench/finetune-corpus.jsonl` (352 ilo-check-verified examples, core-spec
system prompts). Split manifest: `split-manifest.json`.

Format: chat JSONL — `system` (spec), `messages: [user task, assistant source]`.
Directly consumable by axolotl/unsloth/TRL-style SFT pipelines.

## Regenerate

    python3 scripts/make-finetune-corpus.py --ilo ./target/release/ilo --context core

## Purpose (PLAN.md Phase 4)

A small model fine-tuned on this corpus internalizes the spec — deleting the
spec-loading term from every future session (training = prompt caching with
infinite TTL). Val split is the regression gate: a fine-tune that scores
below the untrained baseline on `val` is rejected.
