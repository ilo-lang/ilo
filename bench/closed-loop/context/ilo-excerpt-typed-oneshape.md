---
name: ilo-excerpt-typed-oneshape
description: One-shape micro excerpt for typed-rollup. Canonical conditional is ?h only.
---

# ilo micro — typed rollup

Prefix. Fn: `f x:n>n;body`. Types: `n t b`, `L n` list.

**Records:** `type probe{code:n;lat:n}` then `probe code:200 lat:120`. Access `p.code`.

Conditional (only): `?h cond then else`. Example: `m=?h >ov 0 ov 0`. Nested else: `(?h cond then else)`.
Non-canonical here: `?h a b`, `?h{then}{else}`, `?cond{then}{else}`, match `?x{...}`, braceless guard.
Classify: `?h =c 0 "down" (?h >lat 500 "degraded" "healthy")`.
Count: `h=?h =lbl "healthy" (+h 1) h`.
Loop: `@name xs{body}` — no `in`, not a decorator.

**Compare:** `=c 0` `>=c 500` `<c 200` `>lat 500`.

**Print:** `fmt "healthy={} degraded={} down={}" hs ds dns` with `str` on counts.

Spacing: every token whitespace-separated.
