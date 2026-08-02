---
name: ilo-edit-loop
description: Use this when an ilo program fails and you need to recover or iterate. Covers the repair loop, JSON diagnostics, `--explain`, and fix patterns for the common ILO-XXXX classes.
---

# ilo edit loop

ilo verifies before it runs, every error carries a stable `ILO-XXXX` code, and diagnostics are one JSON object per line. Route on structure, not prose.

## Loop

1. `ilo check file.@ --json` - verify without running. Exit 0 means valid.
2. Exit 1: read the first diagnostic, route on `code`, edit at `span`. Re-check.
3. Bound retries at 3 per code. Same code three times: stop and dump.
4. Exit 0: `ilo run file.@`. `^e` on stdout is a runtime error; otherwise consume the value.

## Diagnostic shape

```json
{"code":"ILO-T004","message":"...","span":{"file":"x.@","line":3,"col":12,"len":5},"hint":"..."}
```

`code` prefix `L`/`P`/`T`/`R`. `span` 1-based. `hint` is usually the fix verbatim; apply it before guessing. Long form: `ilo explain ILO-XXXX`. Full code list: load `ilo-errors`.

## Common fixes

- **T006 arity** - signature is in the message; no named args.
- **T004 type mismatch** - body's last expression must match declared return.
- **T010 bare `!` rejected** - widen to `>R t e`, use `??`, or match.
- **P009 unparenthesised lambda** - wrap `(p:t>r;body)`.
- **T007 calling a value** - shadowed a builtin, or wrote `fn(x)` for `fn x`.
- **R012 capture not supported** - drop the capture or pass as arg (every public backend supports Phase 2 captures natively).
- **R030 HTTP** - `^e` is a domain outcome, not a panic.

## Escalation

Same `code` three times after applying `hint`: dump source + diagnostics, stop. `ILO-X###` is an internal code: file a bug.

## Tooling

`ilo check --json` verify only; `--strict` exits 1 on warnings. `ilo run --json` wraps stdout as `{"ok"|"error"}`. `ilo explain CODE --json` structured long-form. `ilo serv` long-lived JSON loop without process spawn. `-j` is a universal short alias for `--json` on every subcommand (ILO-442) — `ilo check -j file.ilo`.
