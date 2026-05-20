---
name: ilo-builtins-math
description: Use this when calling math builtins. Arithmetic, trig, constants (pi, tau, e), random, and statistics.
---

# ilo builtins - math

Prefix-call: `name arg1 arg2 ...`. Cross-engine unless noted.

## Arithmetic / rounding

`abs min max mod fmod flr cel rou clamp pow sqrt exp log log10 log2`. `rou`=round (alias `round`).

**`mod` is C-style signed remainder** - result sign matches the dividend. `-1 mod 7 = -1`. For weekday/timezone/ring arithmetic where you need a non-negative result, use `fmod`: `fmod -1 7 = 6`. No more `(raw + 7) % 7` workaround.

## Trig

`sin cos tan asin acos atan atan2`. `asin acos sqrt log` NaN out-of-domain. NaN propagates; comparisons false.

## Constants

`pi tau e`.

## Random

`rnd` = random float [0,1) (aliases `rand`, `random`); `rndn` = random integer in range.

## Statistics

`sum avg median quantile stdev variance cumsum frq argmax argmin argsort prod cprod`.
