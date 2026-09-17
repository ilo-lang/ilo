---
name: ilo-builtins-sig
description: One-line io/text/math builtin signatures. Pair with ilo-language + ilo-builtins-core in curated contexts.
---

# ilo builtins — signature reference

Prefix-call: `name arg1 arg2 ...`. `!` unwraps `R`; `!!` panics on Err.

## File / path / stdin

`rd path` (text), `rdl path` (lines), `rdjl path` (JSONL), `rdb path` (bytes); `wr path s` (create/replace), `wra path s` (append), `wro path s` (snapshot), `wrl path xs` (lines). `prnt v` prints. `rdin!` all stdin; `rdinl!` lines. Dir: `lsd dir`, `walk dir`, `glob dir pat`. Path: `dirname p`, `basename p`, `pathjoin parts`, `fsize p`, `mtime p`, `isfile p`, `isdir p`. All Result-wrapped.

## HTTP (all take optional trailing `M t t` headers map)

`get url`, `pst url body` (canonical since 0.12.0; `post` invalid), `put url body`, `pat url body`, `del url`, `hed url`, `opt url` — all `R t t` body-only. Timeouts: `get-to url ms`, `pst-to url body ms`. Rich: `getx url` / `pstx url body` → Ok-map `status` (n) / `headers` / `body`; non-2xx is still Ok. Parallel: `get-many urls` → `L (R t t)`; `par-map fn xs n` general fan-out → `L (R b t)`, order-preserving. Stream: `get-stream url` → lazy `L t` lines. Only transport failure is Err; use `getx` when you need status branching.

## JSON

`jpar s` parse → `R _ t`; `jpar-list s` parse, assert array; `jpth s "a.b.0.c"` dot-path lookup (NOT JSONPath; bare indices); `jkeys s path`; `jdmp v` serialize. `jpth!` returns leaf already typed (n/t/b/L/record) — never wrap in `num`/`str`.

## Process / env

`run cmd argv` → map `stdout`/`stderr`/`code` (text); `run2 cmd argv` → typed `r.stdout`/`r.stderr`/`r.exit` (n) — prefer; non-zero exit not Err. `env name`; `env-all`; `exit code`.

## Time / concurrency

`now` (s), `now-ms`, `sleep ms`. `dtfmt ts fmt` / `dtparse s fmt` / `dtparse-rel s now`. `dur-parse s` → seconds; `dur-fmt n`. `spawn fn args` fire-and-forget thread.

## Text core

`len str trm spl cat has`. `spl "a,b" ","` → list. `has s sub` bool. `cat xs sep` JOINS a list (not concat — use `+ a b`). `idxof s sub` → `O n` nil when absent. `upr lwr cap padl padr chars ord chr`. `padr "" n c` = repeat char.

## Regex (pattern FIRST, string LAST)

`rgx pat s` first match → `[whole, cap1, ...]`; `rgxall pat s` all whole matches; `rgxall1 pat s` capture-1 of each; `rgxsub pat repl s`; `rgxsuball pat repl s`; `rgxall-multi pats s`.

## Format

`fmt "x={}" v` template; `{:N}` width, `{:<N}` left-align, `{:Nd}` int width, `{:.Nf}` decimals. Plain `{}` on float = full IEEE round-trip — use `{:.Nf}` or `fmt2 x digits` for readable output. `csv s` / `tsv s` → list of lists.

## Date / duration

`dtparse-rel s now`: `today|yesterday|tomorrow`, `N days ago`, `in N days`, `N weeks/months ago`, `last/next/this <weekday>`, ISO passthrough → `R n t`. Months unsupported in `dur-parse` (s/m/h/d/w; sticky `-` sign). `dur-fmt secs` → human string.

## Encoding / crypto

`urlenc`/`urldec`; `b64`/`b64-dec` vs `b64u`/`b64u-dec` (URL-safe no-pad); `hex s`; `sha256 s`; `hmac-sha256 key msg`; `ct-eq a b` constant-time — always for secrets; `rand-bytes n` CSPRNG.

## Math

`abs min max mod fmod flr cel rou clamp pow sqrt exp log log10 log2`. `mod` is C-style signed (`-1 mod 7 = -1`); use `fmod -1 7 = 6` for ring arithmetic. `sin cos tan asin acos atan atan2`; `pi tau e`. Random: `rnd` float [0,1); `rnd a b` int [a,b] inclusive; `rndn mu sigma` Normal sample (not uniform); `rand-bytes` for secrets.

## Stats

`sum avg median quantile stdev variance cumsum frq argmax argmin argsort prod cprod`. `quantile xs p` linear-interp, p clamped [0,1]. `bisect xs target` = bisect_left; caller sorts.

## Linear algebra

Row-major `L (L n)`, flat `L n`. `transpose matmul matvec dot solve inv det fft ifft lstsq`. `lstsq xm ys` OLS coefficients; `ewm xs a` EMA; errors on singular/shape mismatch.

## List constructors

`linspace a b n` inclusive; `ones n`; `rep n v`.
