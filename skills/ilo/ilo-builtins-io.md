---
name: ilo-builtins-io
description: Use this when calling I/O builtins. File read/write, HTTP, JSON, path ops, env, time, and process.
---

# ilo builtins - I/O

Prefix-call: `name arg1 arg2 ...`. Cross-engine unless noted.

## File I/O

`rd path`, `rdl path`, `rdjl path`, `rdb path`, `wr path s`, `wra path s`, `wrl path xs`, `prnt v`. Dir: `lsd dir` (alias `ls`), `walk dir`, `glob dir pat`. Result-wrapped. `walk`/`glob` skip unreadable subdirs; root Errs. Path: `dirname p`, `basename p`, `pathjoin parts`, `fsize path`, `mtime path`, `isfile path`, `isdir path`.

## HTTP

`get url` (alias `$`, `R t t`), `post url body`, `get-many urls` (parallel).

## JSON

`jpar s` parse, `jpth s path` dot-path (typed), `jkeys s path` sorted object keys, `jdmp v` serialize. Numeric keys stringified in `jdmp`.

## Environment / process

`env name` (var), `env-all` (`R (M t t) t`), `exit code`.

## Time

`now` (s), `now-ms`, `sleep ms`, `clock`.
