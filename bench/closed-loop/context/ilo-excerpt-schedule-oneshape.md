---
name: ilo-excerpt-schedule-oneshape
description: One-shape micro excerpt for schedule-window. Canonical conditional is ?h only.
---

# ilo micro — schedule

Prefix language. Fn: `f x:n>n;body` (`;` sep, last expr returns). Types: `n` num, `t` text, `b` bool.

Ops prefix: `+a b` `*a b` `-a b` `/a b` `>a b` `=a b`. Spacing required: `) 2` not `)2`.

Print: `fmt "overlap_min={}" (str m)`. `str x` to text.

Math: `min a b` `max a b`.

Conditional (only): `?h cond then else`. Example: `m=?h >ov 0 ov 0`. Nested else: `(?h cond then else)`.
Non-canonical here: `?h a b`, `?h{then}{else}`, `?cond{then}{else}`, match `?x{...}`, braceless guard.

Script/`main`: `main>t; ...; fmt "..." ...`

Example:
```
main>t
  a0=540;a1=720;b0=660;b1=840
  lo=max a0 b0; hi=min a1 b1; ov=- hi lo
  m=?h >ov 0 ov 0
  fmt "overlap_min={}" (str m)
```
