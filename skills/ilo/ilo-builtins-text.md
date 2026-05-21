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

`fmt template args...` (no list splat). `fmt2 x digits > t` is a **decimal formatter**, not a fmt variant: `fmt2 3.14159 2` -> `"3.14"`. Compose: `fmt "x={}" (fmt2 v 2)`.

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

`dur-parse s > R n t` parse human duration to seconds. Accepts `s/m/h/d/w` and full names (singular/plural), decimals, mixed sequences ("3h 30m", "1.5 hours", "1 week 2 days"). Months are **not** supported. Leading `-` is sticky until `+` resets it, so `"-1m 30s"` = `-90`. Err if empty or no unit.

`dur-fmt n > t` format seconds as human-readable. Drops zero parts; largest units ("2h 42m", "1 day", "30s"). Zero -> "0s". Negative emits one leading minus and round-trips. Fractional preserved to 3dp, trailing zeros stripped.

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
