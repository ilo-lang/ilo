---
name: ilo-builtins
description: Use this when calling ilo's builtin functions. One-line signatures plus examples for list, text, IO, HTTP, JSON, map, math, time, and HOF builtins.
---

# ilo builtins

Prefix-call syntax: `name arg1 arg2 ...`. All builtins work cross-engine unless noted.

## Math

`abs min max mod flr cel rou rnd rndn clamp`; `sum avg median quantile stdev variance cumsum frq`; `sqrt pow exp log log10 log2`; `sin cos tan asin acos atan atan2`. `asin acos sqrt log` return NaN out-of-domain; clamp at the boundary. NaN propagates; comparisons return false. `rnd` is random (aliases: `rand`, `random`), NOT round; `rou` is round (alias: `round`).

## Text

`len str num trm spl cat fmt fmt2 has`; `rgx rgxall rgxall1 rgxsub`; `upr lwr cap padl padr chars ord chr`. `num` returns `R n t`. `spl "a,b,c" ","` -> `["a","b","c"]`.

## List

`len hd tl at lst take drop slc`; `rev srt rsrt unq uniqby flat grp zip enumerate range`; `chunks window flatmap partition`; `setunion setinter setdiff`. `at xs i` floors floats; negative indexes from end (same for `slc take drop`). Bounds clamp.

## HOFs

`map f xs`, `flt f xs` (filter), `ct f xs` (count), `fld f xs init` (reduce). Inline lambdas: `map (x:n>n;+x 1) xs`.

## Map

```
mmap mset mget mhas mdel mkeys mvals
```

`mget` returns nil on miss. `mkeys`/`mvals` sorted. `len m` is entry count. Keys typed text or integer; `mset m 7 v` works. `Int(1)` and `Text("1")` distinct.

## I/O

`rd path`, `rdl path`, `rdjl path`, `rdb path`, `wr path s`, `wrl path xs`, `prnt v`. Dir: `lsd dir` (non-recursive), `walk dir` (recursive), `glob dir pat`. All Result-wrapped. `walk`/`glob` silently skip unreadable subdirs (permission denied etc.) so a single locked sibling doesn't abort the whole traversal; an unreadable root still returns Err.

## HTTP

`get url` (alias `$`, `R t t`), `post url body`, `get-many urls` (parallel). `env name` (var), `env-all` (`R (M t t) t`).

## JSON

`jpar s` parse, `jpth s path` dot-path (typed), `jkeys s path` sorted object keys, `jdmp v` serialize. Numeric keys stringified in `jdmp`.

## Time

`now` (s), `now-ms`, `sleep ms`, `dtfmt ts fmt`, `dtparse s fmt`.

## Examples

```
xs >> flt pos >> map sq >> sum
r=get! url; name=jpth! r "name"
ls=rdl! "input.txt"
```
