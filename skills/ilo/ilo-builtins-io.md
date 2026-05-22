---
name: ilo-builtins-io
description: Use this when calling I/O builtins. File read/write, HTTP, JSON, path ops, env, time, and process.
---

# ilo builtins - I/O

Prefix-call: `name arg1 arg2 ...`. Cross-engine unless noted.

## File I/O

`rd path`, `rdl path`, `rdjl path`, `rdb path`, `wr path s`, `wra path s` (append), `wro path s` (truncate-overwrite), `wrl path xs`, `prnt v`. Dir: `lsd dir` (alias `ls`), `walk dir`, `glob dir pat`. Result-wrapped. `walk`/`glob` skip unreadable subdirs; root Errs. Path: `dirname p`, `basename p`, `pathjoin parts`, `fsize path`, `mtime path`, `isfile path`, `isdir path`. Use `wro` to snapshot state on each tick (avoids delete-then-write dance); use `wra` to accumulate/append.

## Stdin

`rdin` (`R t t`) reads all of stdin as text. `rdinl` (`R (L t) t`) reads stdin line-by-line (newlines stripped). Both are 0-arg; call with `!` unwrap in pipelines: `rdin!`, `rdinl!`. Err on WASM.

## HTTP

**Every verb takes an optional trailing `M t t` headers map** for Authorization, Accept, X-API-Key. No wrapper needed - pass the map as the last arg (`get url hs`, `pst url body hs`).

`get url` (`R t t`), `get url headers` (with `M t t` custom headers), `get-to url timeout-ms` (explicit ms timeout).
`pst url body` (`R t t`), `pst url body headers`, `pst-to url body timeout-ms`. (`pst` is the canonical name since 0.12.0; the old name `post` is not accepted.)
`put url body` / `pat url body` mirror `pst` (PUT / PATCH); accept optional headers map.
`del url` / `hed url` / `opt url` mirror `get` (DELETE / HEAD / OPTIONS); accept optional headers map.
`get-many urls` (parallel fan-out, `L (R t t)`).
`par-map fn xs` / `par-map fn xs n` — general parallel fan-out (`L (R b t)`): apply `fn` to every element of `xs` up to `n` at a time (default n = num_cpus; override with `ILO_PAR_MAP_CONCURRENCY`). Order-preserving; per-item errors surface as `Err` in the list without short-circuiting. Works for any fn (CPU, I/O, HTTP); preferred over `get-many` for everything except simple URL fan-out. See `examples/par-map.ilo`.
Verb cluster is limited to the seven safe methods (GET/POST/PUT/PATCH/DELETE/HEAD/OPTIONS); TRACE and CONNECT are deliberately out of scope.
Timeout variants round up to the nearest second. Err on timeout or connection failure.

`getx url` / `pstx url body` (rich response, `R (M t _) t`): Ok-map with `status` (n), `headers` (M t t), `body` (t). Non-2xx is still Ok with status surfaced on the map; only transport failure is Err. Optional trailing request-headers map (M t t), same as `get`/`pst`. Use these when you need conditional requests (304), status-code branching (429), response-header reads (ETag, Link, X-RateLimit-*), or redirect following. Body-only `get`/`pst` stay cheaper for fire-and-forget; `getx`/`pstx` are the heavier variant. Response header names are lowercased on the Ok-map.

`!` auto-unwraps on all HTTP builtins (`get!`, `pst!`, `get-to!`, `pst-to!`, `getx!`, `pstx!`, `put!`, `pat!`, `del!`, `hed!`, `opt!`): Ok→body (or Ok-map for getx/pstx), Err propagates. `!!` panics on Err. Prefer `pst!`/`put!`/`del!` over `?r{~v:v;^e:^e}` boilerplate in `R`-returning callers (~30 tokens/site).

Parsing `;`-delimited headers (Content-Type, Cache-Control, Cookie): no `ct-parse` builtin, use `spl ";"` then `trm`/`lwr`, then `spl "="` per param. `ps=spl raw ";";media=lwr (trm (at ps 0));kv=spl (trm (at ps 1)) "="`. Don't bind to `ct` (shadows builtin).

## JSON

**`jpth` is dot-path, not JSONPath.** `jpth body "user.addrs.0.city"`. Leading `$`, `*`, `[...]` rejected; list indices are bare numbers, not `[0]`.

`jpar s` parse (`R _ t`), `jpar-list s` parse and assert array (`R (L _) t` — use when you know the response is an array: `@x (jpar-list! body){...}`), `jpth s path` dot-path lookup, `jkeys s path` sorted object keys, `jdmp v` serialize. Numeric keys stringified in `jdmp`.

`jpth`/`jpth!` returns the leaf **already typed**: numbers as `n`, text as `t`, bools as `b`, arrays as `L _`, objects as record. Don't wrap in `num`/`str` or re-`jpar`: `age=jpth! body "user.age"` is already `n`. `num (str (jpth! body "x"))` is pure waste (~30 tokens).

`!` auto-unwraps on all of these. Common shape inside `R`-returning fn: `r=jpar! body;r.x`. `jpar!` propagates errors; `jpar!!` panics. Same for `jpar-list!`, `jpth!`, `jkeys!`. Non-`R` callers: use `default-on-err` (in `ilo-builtins-core`): `name=default-on-err (jpth body "user.name") "anon"`.

## Multi-request flows

ilo has no globals. Thread state through function args (in-run) or persist to a file (cross-run). See `examples/paginated-fetch.ilo` and `examples/oauth-token-cache.ilo`.

**Closure-threading** (in-run cache, paginated fetch, rate-limit window). Pass state in, return state out:

```
-- Paginated fetch: cursor + accumulator thread through recursion.
fetch-page url:t cur:t acc:L _>R (L _) t;u=fmt "{}?cursor={}" url cur;b=get! u;page=jpar-list! b;acc=+acc page;nxt=default-on-err (jpth b "next") "";=nxt "" ~acc;fetch-page url nxt acc
```

**File-backed** (cross-run OAuth token, refresh on expiry). Read cached value, do work, write refreshed value back. `wr`/`rd` survive across runs; `now-ms` gives a TTL stamp.

```
load path:t>t;?h (isfile path) (rd!! path) "NO-TOKEN"
save path:t tok:t>t;wr!! path tok;tok
-- Per request: load, attach as bearer, on 401 refresh + save + retry.
-- See examples/oauth-token-cache.ilo for the worked refresh loop.
```

Pick file-backed when state must outlive the program (OAuth refresh tokens, multi-hour TTLs). Pick closure-threading when state lives one run (pagination, rate-limit windows).

## Process spawn

`run cmd argv` (`R (M t t) t`) - argv-list spawn; loose Map with text fields `stdout`, `stderr`, `code` (exit as text). `$cmd argv` is the sigil shortcut.

`run2 cmd argv` (`R RunResult t`) - typed record: `r.stdout` (t), `r.stderr` (t), `r.exit` (n, not text). Prefer `run2` for new code. Non-zero exit is NOT an error; Err only on spawn failure.

```
r=run2!! "git" ["status" "--short"]
r.stdout  -- text
r.exit    -- number (0 = success)
```

## Environment / process

`env name` (var), `env-all` (`R (M t t) t`), `exit code`.

`run` details: no shell, no glob, no interpolation. argv passed to `Command::args`. `code` is decimal (signal -> `signal:<n>`). Non-zero exit is not `Err`; branch on `mget m "code"`. Err = spawn failure or 10 MiB/stream cap. Stdin closed; inherits env + cwd. WASM Errs.

## Time

`now` (s), `now-ms`, `sleep ms`, `clock`. Date parsing/formatting (`dtfmt`, `dtparse`, `dtparse-rel`) lives in `ilo-builtins-text`.

`tz-offset tz:t epoch:n > R n t` - DST-aware UTC offset (s) for IANA tz; east-positive.
