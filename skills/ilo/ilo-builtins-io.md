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

`!` auto-unwraps on all HTTP builtins (`get!`, `pst!`, `get-to!`, `pst-to!`): Ok→body, Err propagates out of the enclosing `R`-returning fn. `!!` panics on Err instead (script-style). Prefer `pst!` over `?r{~v:v;^e:^e}` boilerplate when the caller already returns `R`; saves ~30 tokens per call site.

### Parsing Content-Type

No `ct-parse` builtin. Split on `;`, trim, lowercase. For `application/json; charset=utf-8`:

```
raw = "application/json; charset=utf-8"
parts = spl raw ";"
m0 = at parts 0
media = lwr (trm m0)                          -- "application/json"
n = len parts
cs = ?(>=n 2){at parts 1}{""}
kv = spl (trm cs) "="
charset = ?(=(len kv) 2){lwr (at kv 1)}{""}   -- "utf-8" or ""
```

Note: don't bind to `ct`, it shadows a builtin name and the verifier rejects it. Use `raw`, `ctype`, or similar.

Same `spl ";"` + `trm` + `spl "="` recipe handles any `;`-delimited header (`Cache-Control`, `Cookie`, `Set-Cookie` attrs). For `,`-joined values (`Accept: a, b, c`), `spl ","` first then loop.

## JSON

`jpar s` parse (`R _ t`), `jpar-list s` parse and assert array (`R (L _) t` — use when you know the response is an array: `@x (jpar-list! body){...}`), `jpth s path` dot-path (typed), `jkeys s path` sorted object keys, `jdmp v` serialize. Numeric keys stringified in `jdmp`.

`!` auto-unwraps the Result on any of these. Inside an `R`-returning function `r=jpar! body;r.x` is the common shape — saves the `?r{~v:v;^e:^e}` boilerplate per call site. `jpar! body` propagates parse errors out of the enclosing function; `jpar!! body` panics instead. Same for `jpar-list!`, `jpth!`, `jkeys!`.

Non-`R` callers can't use `!`; reach for `default-on-err` (in `ilo-builtins-core`) when the error is discardable: `name=default-on-err (jpth body "user.name") "anon"`.

## Environment / process

`env name` (var), `env-all` (`R (M t t) t`), `exit code`.

`run cmd argv` (`R (M t t) t`) spawns `cmd` with `argv` (`L t`). No shell, no glob, no interpolation. `$` is the sigil shortcut. On `Ok`, the map has three keys, all text: `stdout` (captured stdout, lossy utf-8), `stderr` (captured stderr), `code` (decimal exit code; signalled child reports `signal:<n>`, unknown status reports `unknown`). Non-zero exit is **not** an `Err`, branch on `mget m "code"`. `Err` is reserved for spawn failure or output cap (10 MiB/stream). Stdin is closed; child inherits parent env + cwd. WASM Errs.

```
m=run!! "echo" ["hi"]            -- {"stdout":"hi\n","stderr":"","code":"0"}
out=mget m "stdout"              -- "hi\n"
code=mget m "code"               -- "0"
```

## Time

`now` (s), `now-ms`, `sleep ms`, `clock`. Date parsing/formatting (`dtfmt`, `dtparse`, `dtparse-rel`) lives in `ilo-builtins-text`.
