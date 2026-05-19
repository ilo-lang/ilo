---
name: ilo-builtins-core
description: Use this when calling core ilo builtins. Type coercions (len, str, num, trm), list ops, HOFs, and map ops.
---

# ilo builtins - core

Prefix-call: `name arg1 arg2 ...`. Cross-engine unless noted.

## Type / shape

`len str num trm`. `num` returns `R n t`. `str v` coerces any value to text.

## List

`len hd tl at lst take drop slc`; `rev srt rsrt unq uniqby flat grp zip enumerate range`; `chunks window flatmap partition`; `setunion setinter setdiff`. `at xs i` floors floats; negative indexes from end (same for `slc take drop`). Bounds clamp. `lst xs i v` = set index (alias `lset`); last element = `at xs -1`.

## HOFs

`map f xs`, `flt f xs`, `ct f xs`, `fld f xs init`. Inline lambdas: `map (x:n>n;+x 1) xs`.

## Map

`mmap mset mget mhas mdel mkeys mvals mpairs`. `mget` returns nil on miss. `mkeys`/`mvals` sorted. `len m` is entry count. Keys typed text or integer; `mset m 7 v` works. `Int(1)` and `Text("1")` distinct. `mpairs m` -> list of `[k v]` pairs.
