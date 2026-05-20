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

`rgx s pat` first match, `rgxall s pat` all matches, `rgxall1 s pat` first capture group of each match, `rgxall-multi pats s` multi-pattern flat-match (each pattern follows rgxall1 semantics; results concat in pattern order), `rgxsub s pat repl` substitute.

## Formatting

`fmt template args...` (no list splat); `fmt2 template list` splat form.

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

`dur-parse s > R n t` — parse human duration string into seconds. Accepts `s/m/h/d/w` abbreviations, full names (week/day/hour/minute/second, singular + plural), decimal quantities, mixed sequences ("3h 30m", "1.5 hours", "1 week 2 days", "90s"). Err if empty or no unit found.

`dur-fmt n > t` — format seconds as human-readable duration. Drops zero parts; uses largest units ("2h 42m", "1 day", "30s"). Zero returns "0s".

```
secs = dur-parse! "3h 30m"  -- 12600
dur-fmt secs                 -- "3h 30m"
dur-fmt 86400                -- "1 day"
dur-fmt 90                   -- "1m 30s"
```
