---
name: ilo-builtins-io
description: Use this when calling I/O builtins. File read/write, HTTP, JSON, path ops, env, time, and process.
---

# ilo builtins - I/O

Prefix-call: `name arg1 arg2 ...`. Cross-engine unless noted.

## File I/O

`rd path`, `rdl path`, `rdjl path`, `rdb path`, `wr path s`, `wra path s`, `wrl path xs`, `prnt v`. Dir: `lsd dir` (alias `ls`), `walk dir`, `glob dir pat`. Result-wrapped. `walk`/`glob` skip unreadable subdirs; root Errs. Path: `dirname p`, `basename p`, `pathjoin parts`, `fsize path`, `mtime path`, `isfile path`, `isdir path`.

## Stdin

`rdin` (`R t t`) reads all of stdin as text. `rdinl` (`R (L t) t`) reads stdin line-by-line (newlines stripped). Both are 0-arg; call with `!` unwrap in pipelines: `rdin!`, `rdinl!`. Err on WASM.

## HTTP

`get url` (`R t t`), `get url headers` (with `M t t` custom headers), `get-to url timeout-ms` (explicit ms timeout).
`pst url body` (`R t t`), `pst url body headers`, `pst-to url body timeout-ms`.
`get-many urls` (parallel fan-out, `L (R t t)`).
Timeout variants round up to the nearest second. Err on timeout or connection failure.

`!` auto-unwraps on all HTTP builtins (`get!`, `pst!`, `get-to!`, `pst-to!`): Ok→body, Err propagates. `!!` panics on Err. Prefer `pst!` over `?r{~v:v;^e:^e}` boilerplate in `R`-returning callers (~30 tokens/site).

Parsing `;`-delimited headers (Content-Type, Cache-Control, Cookie): no `ct-parse` builtin, use `spl ";"` then `trm`/`lwr`, then `spl "="` per param. `ps=spl raw ";";media=lwr (trm (at ps 0));kv=spl (trm (at ps 1)) "="`. Don't bind to `ct` (shadows builtin).

## JSON

`jpar s` parse (`R _ t`), `jpar-list s` parse and assert array (`R (L _) t` — use when you know the response is an array: `@x (jpar-list! body){...}`), `jpth s path` dot-path (typed), `jkeys s path` sorted object keys, `jdmp v` serialize. Numeric keys stringified in `jdmp`.

`!` auto-unwraps on all of these. Common shape inside `R`-returning fn: `r=jpar! body;r.x`. `jpar!` propagates errors; `jpar!!` panics. Same for `jpar-list!`, `jpth!`, `jkeys!`. Non-`R` callers: use `default-on-err` (in `ilo-builtins-core`): `name=default-on-err (jpth body "user.name") "anon"`.

## Environment / process

`env name` (var), `env-all` (`R (M t t) t`), `exit code`.

`run cmd argv` (`R (M t t) t`) spawns `cmd` with `argv` (`L t`). No shell, no glob, no interpolation. `$` is the sigil shortcut. `Ok` map keys (all text): `stdout`, `stderr`, `code` (decimal; signal -> `signal:<n>`, else `unknown`). Non-zero exit is **not** `Err`; branch on `mget m "code"`. `Err` = spawn failure or 10 MiB/stream cap. Stdin closed; inherits parent env + cwd. WASM Errs.

```
m=run!! "echo" ["hi"]            -- {"stdout":"hi\n","stderr":"","code":"0"}
out=mget m "stdout"              -- "hi\n"
code=mget m "code"               -- "0"
```

## Time

`now` (s), `now-ms`, `sleep ms`, `clock`. Date parsing/formatting (`dtfmt`, `dtparse`, `dtparse-rel`) lives in `ilo-builtins-text`.
