---
name: ilo-excerpt-csv-oneshape
description: One-shape micro excerpt for csv-sales-summary. Canonical conditional is ?h only.
---

# ilo micro — csv pipeline

Prefix. Fn / lambda: `(r:L t>b;body)` typed paren form. Types: `n t b`, `L t`, `M t _`.

**Strings:** `spl s sep` → `L t`. `fmt "..." args`. `num s` → `R n t`; unwrap `default-on-err (num s) 0` or helper `to-n`.

**Lists/HOFs:** `map (x:t>L t;spl x ",") rows`, `flt (r:L t>b;>=(to-n (at r 2)) 50) parsed`, `fld (a:n r:L t>n;+a (to-n (at r 2))) g1 0`, `at xs i`, `len`.

**Maps:** `grp key-fn xs`, `mkeys`, `mget`.

Conditional (only): `?h cond then else`. Example: `m=?h >ov 0 ov 0`. Nested else: `(?h cond then else)`.
Non-canonical here: `?h a b`, `?h{then}{else}`, `?cond{then}{else}`, match `?x{...}`, braceless guard.
HOF predicate returns `b`: comparison expr, as in the `flt` line. Not a braceless guard in the lambda.

Spacing required. Prefer 4+ char locals (`parsed`, `rows`) to avoid reserved 3-char collisions.
