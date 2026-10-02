---
name: one-shape-conditional
description: Arm C card for gold-pack NX-01v2. Not a harness module. Excerpts copy the two shape lines byte-for-byte.
---

# One-shape conditional

Authority for gold-pack Arm C (`--context task-oneshape` on
`--task-set artefact-exp07`). Each loaded excerpt teaches exactly one
conditional. This card is not loaded as context.

Conditional (only): `?h cond then else`. Example: `m=?h >ov 0 ov 0`. Nested else: `(?h cond then else)`.
Non-canonical here: `?h a b`, `?h{then}{else}`, `?cond{then}{else}`, match `?x{...}`, braceless guard.

`cond` is a bool expression (comparison or call). Both arms are values.
A nested else must be parenthesised, or the parser stops after two arms.
Do not emit the shapes named on the non-canonical line.

HOF predicates return `b` via a comparison expression. No braceless
guard inside a lambda (ILO-P023). Example: `(r:L t>b;>=(to-n (at r 2)) 50)`.

Docs soft edge, no compiler change. On the 2026-10-02 Arm B live,
ILO-P009 at `@` was the only code shared by two reds (record-normalize,
typed-rollup). Those excerpts say: loop `@name xs{body}`, no `in`, not
a decorator. P007 and P023 were not shared by two reds; no new rail.

Pinned `goldens-exp07` stay as recorded. schedule-window already uses
`?h`. csv, record, and typed still use a non-canonical form; rewrite
them in their own PR. This pack does not weaken them.

Offline:

    python3 scripts/closed-loop-bench.py --dry-run \
      --task-set artefact-exp07 --context task-oneshape
