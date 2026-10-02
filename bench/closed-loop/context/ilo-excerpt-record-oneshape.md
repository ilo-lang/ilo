---
name: ilo-excerpt-record-oneshape
description: One-shape micro excerpt for record-normalize. Canonical conditional is ?h only.
---

# ilo micro — record normalize

Prefix. Types: `n t`, `L (M t t)`, `M t t`, `R n t`.

**Maps (text records):** `mmap`, `mset m k v`, `mget-or m k default`, `mhas m k`. Build: `mset (mset (mset mmap "id" "1") "name" " Alice ") "age" "30"`.

**Text/num:** `trm s`, `num s` → `R n t`, `default-on-err (num s) 0`, `str x`, `flr x`, `sum xs`, `avg xs`, `rou x`, `len xs`, `fmt`.

Conditional (only): `?h cond then else`. Example: `m=?h >ov 0 ov 0`. Nested else: `(?h cond then else)`.
Non-canonical here: `?h a b`, `?h{then}{else}`, `?cond{then}{else}`, match `?x{...}`, braceless guard.
Loop: `@name xs{body}` — no `in`, not a decorator.
Keep-or-skip: `out=?h mhas seen id out (+=out r)`.
HOF: `>b` lambda uses a comparison or `default-on-err`, not a braceless guard.

Locals 4+ chars (`cleaned`, `deduped`). Spacing between tokens.
