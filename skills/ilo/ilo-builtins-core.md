---
name: ilo-builtins-core
description: Use this when calling core ilo builtins. Type coercions (len, str, num, trm), list ops, HOFs, and map ops.
---

# ilo builtins - core

Prefix-call: `name arg1 arg2 ...`. Cross-engine unless noted.

## Type / shape

`len str num trm`. `num x` is polymorphic — text parses (trims whitespace; Err if unparseable), number is identity-wrapped Ok. Saves the `num (str x)` roundtrip when `x` may already be numeric (e.g. from `jpar!` on a JSON number). Always returns `R n t`. `str v` coerces any value to text.

## Result unwrap

`default-on-err r d` - unwrap `R T E` to `T`, returning `d` if Err. Mirror of `??` for Result (`??` is nil-coalesce for `O T` only). `default-on-err (num s) 0` replaces `?r{~v:v;^_:0}`. Verifier enforces `d` matches Ok type (ILO-T040). Using `??` on a Result triggers ILO-T041 pointing here.

Use when the error payload is discardable and you want a one-liner instead of a `?r{~v:v;^_:d}` match. Works inside any fn (no `R`-return requirement, unlike `!`).

```
port = default-on-err (num "8080") 0                -- text→num with fallback
name = default-on-err (jpth body "user.name") "anon" -- missing/typed path → fallback
```

See `examples/default-on-err.ilo` for the full pattern set.

## List

`len hd tl at lst take drop slc`; `rev srt rsrt unq uniqby flat grp zip enumerate range`; `chunks window flatmap partition`; `setunion setinter setdiff`. `at xs i` floors floats; negative indexes from end (same for `slc take drop`). Bounds clamp. **`lst xs i v` IS the list-set / `lset` / `setat` builtin** — returns a new list with index `i` replaced by `v` (alias `lset`; reach for `lst` whenever you'd write `xs[i] = v` in Python). Last element = `at xs -1`. **`slc xs s -1` with `s>=0` means "to end of list/text"** (Python/JS sugar): `slc xs 2 -1` is everything from index 2 onward; `slc "hello" 0 -1` is `"hello"`. Other negative ends keep relative-offset semantics (`slc xs 0 -2` drops the last two). To "drop the last element" use `take -1 xs`, not `slc xs 0 -1`. `srt` and `srt fn xs` are stable: equal elements (or equal keys) keep their input order, so merging parallel records sorted by a shared timestamp keeps the per-source ordering inside each tie group.

`where cond xs ys > L a` = NumPy `np.where`: element-wise conditional select across three parallel lists. `output[i] = xs[i] if cond[i] else ys[i]`. All three lists must be the same length (mismatch raises `ILO-R009`). Element type of `xs`/`ys` is preserved. Replaces the `map (i:n>_;?h (at cond i) (at xs i) (at ys i)) (range 0 (len xs))` recipe in one call.

`zip xs ys` → `L (L _)` (pairs as 2-element lists, not tuples): `@p (zip xs ys){a=at p 0; b=at p 1; ...}`.

## HOFs

`map f xs`, `flt f xs`, `ct f xs`, `fld f xs init`. Inline lambdas: `map (x:n>n;+x 1) xs`.

**`fld` arg order (gotcha):** outer is `fld fn xs init` (NOT `fn init xs` like Haskell/Python). Lambda is `{acc el> ...}` — accumulator first (NOT element-first like JS `reduce`). Wrong order surfaces as runtime type error, not parse error. Example: `fld {a x> +a x} [1 2 3] 0` → `6`.

## Map

`mmap mset mget mhas mdel mkeys mvals mpairs`. `mget` returns nil on miss. `mkeys`/`mvals` sorted. `len m` is entry count. Keys typed text or integer; `mset m 7 v` works. `Int(1)` and `Text("1")` distinct. `mpairs m` -> list of `[k v]` pairs.
