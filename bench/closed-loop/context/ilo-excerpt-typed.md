---
name: ilo-excerpt-typed
description: Micro excerpt for typed-rollup (records + classify loop).
---

# ilo micro — typed rollup

Prefix. Fn: `f x:n>n;body`. Types: `n t b`, `L n` list.

**Records:** `type probe{code:n;lat:n}` then `probe code:200 lat:120`. Access `p.code`.

**Guards (fn body):** braceless `=c 0 "down"` early-returns. Chain classifier arms; final bare value is else.

**Loops:** `@p ps{body}`. Bind `h=+h 1` inside.

**Compare:** `=c 0` `>=c 500` `<c 200` `>lat 500`.

**Print:** `fmt "healthy={} degraded={} down={}" hs ds dns` with `str` on counts.

Spacing: every token whitespace-separated.

Do **not** use braceless guards inside paren-lambdas (ILO-P023); classifier chains live in named fns.
