---
name: ilo-builtins
description: Use this when calling ilo's builtin functions. One-line signatures plus examples for list, text, IO, HTTP, JSON, map, math, time, and HOF builtins.
---

# ilo builtins

Prefix-call syntax: `name arg1 arg2 ...`. All builtins work cross-engine unless noted.

## Math

```
abs min max mod flr cel rou rnd rndn clamp
sum avg median quantile stdev variance cumsum frq
sqrt pow exp log log10 log2
sin cos tan asin acos atan atan2
```

`asin acos sqrt log` return NaN silently out-of-domain. Validate at boundary: `asin (clamp x -1 1)`. NaN propagates; comparisons return false.

## Text

```
len str num trm spl cat fmt fmt2 has
rgx rgxall rgxall1 rgxsub
upr lwr cap padl padr chars ord chr
```

`num` returns `R n t`. `spl "a,b,c" ","` -> `["a","b","c"]`.

## List

```
len hd tl at lst take drop slc
rev srt rsrt unq uniqby flat grp zip enumerate range
chunks window flatmap partition
setunion setinter setdiff
```

`at xs i` floors floats; negative indexes from end. Same for `slc take drop`. Bounds clamp.

## HOFs

```
map f xs               -- apply
flt f xs               -- filter (pred)
ct f xs                -- count where pred
fld f xs init          -- fold/reduce
```

Inline lambdas: `map (x:n>n;+x 1) xs`.

## Map

```
mmap                   -- empty
mset m k v             -- new map with k=v
mget m k               -- value or nil
mhas m k               -- bool
mdel m k               -- new map without k
mkeys m                -- L t sorted keys
mvals m                -- L v sorted by key
len m                  -- entry count
```

Keys are typed text or integer. Numeric keys work directly: `mset m 7 v`. `Int(1)` and `Text("1")` distinct.

## I/O

```
rd path          -- R t t (whole file)
rdl path         -- R L t t (lines)
rdjl path        -- R L _ t (json lines)
rdb path         -- R L n t (bytes)
wr path s        -- R _ t
wrl path xs      -- R _ t
prnt v           -- print + newline
```

All file ops are Result-wrapped; pair with `!` to propagate.

## HTTP

```
get url          -- R t t (alias: $)
post url body    -- R t t
get-many urls    -- R L t t (parallel)
env name         -- t (env var, "" if unset)
```

## JSON

```
jpar s           -- R _ t (parse string)
jpth obj path    -- R _ t (json-pointer style lookup)
jdmp v           -- t (serialize)
```

`jpth r "name"`, `jpth r "items/0/id"`. Numeric map keys stringified in `jdmp`.

## Time

```
now              -- n unix seconds
now-ms           -- n unix millis
sleep ms         -- _ (block)
dtfmt ts fmt     -- R t t
dtparse s fmt    -- R n t
```

## Examples

```
ws=spl line ","
n=num ws.0
xs >> flt pos >> map sq >> sum
r=get! url
name=jpth! r "name"
m=mset (mset mmap "a" 1) "b" 2
ls=rdl! "input.txt"
```
