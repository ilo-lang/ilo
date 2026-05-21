---
name: ilo-builtins-text
description: Use this when calling text builtins. Manipulation, regex, formatting (fmt, fmt2), CSV/TSV, and date parsing.
---

# ilo builtins - text

Prefix-call: `name arg1 arg2 ...`. Cross-engine unless noted.

## Core text ops

`len str trm spl cat has`. `spl "a,b,c" ","` -> `["a","b","c"]`. `has s sub` -> bool.

## Case / padding / chars

`upr lwr cap padl padr chars ord chr`. `cap` capitalises first letter.

## Regex

Signatures (pat first, string last):

- `rgx pat s > L t` — first match: list of [whole, cap1, cap2, ...]
- `rgxall pat s > L t` — all whole matches
- `rgxall1 pat s > L t` — capture-group 1 of every match
- `rgxsub pat repl s > t` — substitute first match
- `rgxsuball pat repl s > t` — substitute all matches
- `rgxall-multi pats s > L t` — multi-pattern flat-match (each pattern follows rgxall1 semantics; results concat in pattern order)

## Formatting

`fmt template args...` (no list splat; lists are formatted as a single value).

`fmt2 x:n digits:n > t` — format a number to `digits` decimal places (half-to-even rounding; `digits` clamped to `0..=20`). Not a `fmt` variant: `fmt2` is a **decimal formatter**, not a list-splat form of `fmt`.

```
fmt2 3.14159 2          -- "3.14"
fmt2 1.0 0              -- "1"
fmt "x={}" (fmt2 v 2)   -- compose for template + precision
```

## CSV / TSV

`csv s` -> list of lists (comma-separated); `tsv s` -> list of lists (tab-separated).

## Date parsing

`dtparse s fmt` parse date string to timestamp; `dtfmt ts fmt` format timestamp to string.

`dtparse-rel s now` parse a relative-date phrase to a Unix epoch anchored at `now`. Supported phrases:
- `today`, `yesterday`, `tomorrow`
- `N days ago`, `in N days` (also `N day ago`, `in N day`)
- `N weeks ago`, `in N weeks`; `N months ago`, `in N months`
- `last <weekday>`, `next <weekday>`, `this <weekday>` - weekdays: `monday`-`sunday` or `mon`-`sun`; `last`/`next` never return today
- ISO-8601 `YYYY-MM-DD` passthrough (ignores `now`)

Returns `R n t`. Pass `(now)` as the anchor for live programs.

```
deadline nw:n>n;dtparse-rel!! "in 3 days" nw
last-week-start nw:n>n;dtparse-rel!! "last monday" nw
```

## Duration

`dur-parse s > R n t` — parse human duration string into seconds. Accepts `s/m/h/d/w` abbreviations, full names (week/day/hour/minute/second, singular + plural), decimal quantities, mixed sequences ("3h 30m", "1.5 hours", "1 week 2 days", "90s"). Months are **not** supported ("3mo", "3 months" both error — a month is not a fixed number of seconds; use day counts instead). A leading `-` is sticky: it applies to every following token until an explicit `+` resets it, so `"-1m 30s"` = `-90`. Err if empty or no unit found.

`dur-fmt n > t` — format seconds as human-readable duration. Drops zero parts; uses largest units ("2h 42m", "1 day", "30s"). Zero returns "0s". Negative values emit a single leading minus (`-90` → `"-1m 30s"`) which round-trips back through `dur-parse`. Fractional seconds are preserved with up to 3 decimal places, trailing zeros stripped (`90.5` → `"1m 30.5s"`, `0.5` → `"0.5s"`).

```
secs = dur-parse! "3h 30m"  -- 12600
dur-fmt secs                 -- "3h 30m"
dur-fmt 86400                -- "1 day"
dur-fmt 90                   -- "1m 30s"
dur-fmt 90.5                 -- "1m 30.5s"
dur-fmt -90                  -- "-1m 30s"
dur-parse! "-1h 30m"         -- -5400 (sticky sign)
```

## URL / base64url

`urlenc`/`urldec` (RFC 3986), `b64u`/`b64u-dec` (no-pad). Decoders `>R t t`.
