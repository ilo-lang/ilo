---
name: ilo-builtins-text
description: Use this when calling text builtins. Manipulation, regex, formatting (fmt, fmt2), CSV/TSV, date parsing, and crypto (sha256, hmac-sha256, base64, hex, ct-eq).
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

`dur-parse s > R n t` — parse human duration into seconds. Accepts `s/m/h/d/w`, full names (singular + plural), decimals, mixed ("3h 30m", "1.5 hours", "1 week 2 days"). Months unsupported (not fixed length). Leading `-` is sticky: `"-1m 30s"` = -90. Err if empty or no unit found.

`dur-fmt n > t` — seconds to human-readable. Drops zero parts; largest units. Zero = "0s". Negative emits leading minus. Fractional seconds preserved up to 3dp.

```
dur-parse! "3h 30m"  -- 12600
dur-fmt 9720         -- "2h 42m"
dur-fmt -90          -- "-1m 30s"
dur-parse! "-1h 30m" -- -5400 (sticky sign)
```

## Crypto

`sha256 s > t` SHA-256 lowercase hex. `hmac-sha256 key body > t` HMAC-SHA256 hex (webhook signing, API auth). `base64-enc/dec s > t/R t t` standard base64 with `=` padding. `base64url-enc/dec s > t/R t t` url-safe no-pad (JWT). `hex-enc bytes:L n > t` 0-255 list to hex. `hex-dec s > R (L n) t` hex to byte list. `ct-eq a b > b` constant-time equality — use instead of `==` for secrets.

```
sha256 "abc"                  -- ba7816bf...
hmac-sha256 "key" "payload"   -- 64-char hex
base64-dec! (base64-enc "hi") -- "hi"
hex-dec! (hex-enc [255,0,16]) -- [255,0,16]
ct-eq sig expected            -- bool, no timing leak
-- webhook: verify sig:t body:t>b;ct-eq (hmac-sha256 "secret" body) sig
```
