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
