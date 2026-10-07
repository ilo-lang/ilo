---
name: ilo-excerpt-csv
description: Micro excerpt for csv-sales-summary (split/filter/group/sum).
---

# ilo micro — csv pipeline

Prefix. Fn / lambda: `(r:L t>b;body)` typed paren form. Types: `n t b`, `L t`, `M t _`.

**Strings:** `spl s sep` → `L t`. `fmt "..." args`. `num s` → `R n t`; unwrap `default-on-err (num s) 0` or helper `to-n`.

**Lists/HOFs:** `map (x:t>L t;spl x ",") rows`, `flt (r:L t>b;>=(to-n (at r 2)) 50) parsed`, `fld (a:n r:L t>n;+a (to-n (at r 2))) g1 0`, `at xs i`, `len`.

**Maps:** `grp key-fn xs`, `mkeys`, `mget`.

**ILO-P023:** HOF predicates return `b` — comparison expr inside lambda, not braceless guard. Wrong: `(r:L t>n;>=…)` / guard form.

Spacing required. Prefer 4+ char locals (`parsed`, `rows`) to avoid reserved 3-char collisions.
