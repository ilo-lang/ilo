---
name: ilo-builtins
description: Use this when calling ilo's builtin functions. One-line signatures for list, text, IO, HTTP, JSON, map, math, time, and HOF builtins.
---

# ilo builtins

Prefix-call: `name arg1 arg2 ...`. Cross-engine unless noted.

## Math

`abs min max mod flr cel rou rnd rndn clamp`; `sum avg median quantile stdev variance cumsum frq argmax argmin argsort`; `sqrt pow exp log log10 log2`; `sin cos tan asin acos atan atan2`. NaN out-of-domain; NaN propagates, compares false. `rnd`=random; `rou`=round.

## Text

`len str num trm spl cat fmt fmt2 has`; `rgx rgxall rgxall1 rgxsub`; `upr lwr cap padl padr chars ord chr`. `num`->`R n t`. `fmt` no splat.

## List

`len hd tl at lget-or lst take drop slc`; `rev srt rsrt unq uniqby flat grp zip enumerate range`; `chunks window flatmap partition`; `setunion setinter setdiff`. `at xs i` floors floats; negative from end (also `slc/take/drop`). `lst xs i v`=set index (alias `lset`); last=`at xs -1`. `lget-or xs i d`=element or `d` if OOB.

## HOFs

`map f xs`, `flt f xs`, `ct f xs`, `fld f xs init`; `srt fn xs`/`rsrt fn xs` sort-by-key. Lambda: `(x:n>n;+x 1)`.

## Map

```
mmap mset mget mget-or mhas mdel mkeys mvals
```

`mget` nil on miss; `mget-or m k d` returns `d` (typed `v`). `mkeys`/`mvals` sorted. `len m`=entries. Keys text or int; `Int(1)` and `Text("1")` distinct.

## I/O

`rd`, `rdl`, `rdjl`, `rdb`, `wr p s`, `wrl p xs`, `prnt v`. Dirs: `lsd d`, `walk d`, `glob d pat`. Result-wrapped.

## HTTP

`get url`, `post url body`, `get-many urls`; `env name`, `env-all`.

## JSON

`jpar s` parse, `jpth s p` dot-path, `jkeys s p` sorted keys, `jdmp v` serialize.

## Time

`now`(s), `now-ms`, `sleep ms`, `dtfmt ts fmt`, `dtparse s fmt`.

## Examples

`xs >> flt pos >> map sq >> sum`; `r=get! url;name=jpth! r "name"`.
