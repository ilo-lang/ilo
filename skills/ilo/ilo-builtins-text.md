---
name: ilo-builtins-text
description: Use this when calling text builtins. Manipulation, regex, formatting (fmt, fmt2), CSV/TSV, and date parsing.
---

# ilo builtins - text

Prefix-call: `name arg1 arg2 ...`. Cross-engine unless noted.

## Core text ops

`len str trm spl cat has`. `spl "a,b,c" ","` -> `["a","b","c"]`. `has s sub` -> bool. `cat xs sep` joins a list - NOT two-string concat; use `+ a b` for that. `fmt` for templates.

## Text search

`idxof s sub > O n` — first occurrence index of `sub` in `s`, in Unicode code-point units (same scale as `at`). Returns nil when not found; use `??` for a default: `i = ?? (idxof s ",") (len s)`. Tree-bridge eligible (0.13.0).

## Case / padding / chars

`upr lwr cap padl padr chars ord chr`. `cap` capitalises first letter. `padr "" n c` is the repeat-character idiom (n copies of 1-char `c`): `padr "" 10 "#"` -> `"##########"`. Use it for histogram bars, divider lines.

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

**Float precision gotcha.** Plain `{}` on a float emits the **full IEEE 754 round-trip** representation, not a human-friendly truncation. `fmt "{}" /1 6` -> `"0.16666666666666666"`, `fmt "{}" +0.1 0.2` -> `"0.30000000000000004"`. Integer-valued floats render without a decimal (`fmt "{}" 2.0` -> `"2"`). For readable output use a precision spec or `fmt2`:

```
fmt "GC={:.2f}%" pct           -- "GC=54.17%"          (ratio-as-percent, canonical)
fmt "pi={:.4f}" 3.14159265     -- "pi=3.1416"
fmt "x={}" (fmt2 v 6)          -- compose fmt2 for 6dp without the spec
fmt "n={:5d}" 42               -- "n=   42"            (integer width, space-pad)
fmt "{:<8}|" "ok"              -- "ok      |"          (left-align width)
```

Supported specs (parser-checked; everything else errors with `ILO-T013` / `ILO-R009`):

- `{}` — Display (full precision for floats).
- `{.Nf}` / `{:.Nf}` — N decimal places, half-to-even rounding. Number arg required.
- `{:N}` — right-align Display to width N (space-pad).
- `{:<N}` — left-align Display to width N.
- `{:Nd}` — integer right-align to width N (truncates toward zero).

Out of scope: `{:e}` / `{:g}` (scientific / sig-figs), `{:06d}` (zero-pad), `{:+}` (forced sign), `{:x}` (hex). Compose `fmt2` / `padl` / `padr` instead.

## CSV / TSV

`csv s` -> list of lists (comma-separated); `tsv s` -> list of lists (tab-separated).

## Date parsing

`dtparse s fmt` parse date string to timestamp; `dtfmt ts fmt` format timestamp to string. dtfmt takes epoch SECONDS (from `now`). If using `now-ms`, convert first: `/now-ms 1000`.

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

## Crypto

`sha256 s > t` SHA-256 lowercase hex (64 chars). `hmac-sha256 key msg > t` HMAC-SHA256 lowercase hex. `b64 s > t` / `b64-dec s > R t t` standard base64 (`=` padding; distinct from `b64u`/`b64u-dec` URL-safe no-pad). `hex s > t` lowercase hex of UTF-8 bytes. `hex-rev s > t` reverse byte order of hex-encoded string (byte-pair-wise; even length required, odd errors ILO-T013; case preserved; use for little↔big endian, e.g. Bitcoin txid). `ct-eq a b > b` constant-time text equality - use this to verify HMAC signatures or compare any secret; never `=`, which short-circuits and leaks timing.

## Token counting

`tokcount s > n` approximate cl100k_base token count of `s`. On native targets uses tiktoken-rs cl100k_base BPE (exact OpenAI tokenisation); on WASM falls back to bytes/3.4 stub (~5% for English prose). Use for skill-file budget checks and prompt-sizing estimates. *Experimental* (ILO-413).
