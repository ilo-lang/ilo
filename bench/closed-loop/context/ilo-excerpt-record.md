---
name: ilo-excerpt-record
description: Micro excerpt for record-normalize (maps/trim/coerce/dedup).
---

# ilo micro — record normalize

Prefix. Types: `n t`, `L (M t t)`, `M t t`, `R n t`.

**Maps (text records):** `mmap`, `mset m k v`, `mget-or m k default`, `mhas m k`. Build: `mset (mset (mset mmap "id" "1") "name" " Alice ") "age" "30"`.

**Text/num:** `trm s`, `num s` → `R n t`, `default-on-err (num s) 0`, `str x`, `flr x`, `sum xs`, `avg xs`, `rou x`, `len xs`, `fmt`.

**Loops:** `@r rs{body}`; `cnt` continue; `+=` append to list.

**ILO-P023:** no braceless guards in paren-lambdas. Use `default-on-err` for bad ages.

Locals 4+ chars (`cleaned`, `deduped`). Spacing between tokens.
