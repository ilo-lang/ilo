---
name: ilo-builtins-math
description: Math builtins for ilo — arithmetic, trig, statistics, and random.
---

# ilo builtins — math

Prefix-call: `name arg1 arg2 ...`. Cross-engine unless noted.

## Arithmetic / rounding

`abs min max mod flr cel rou clamp pow sqrt exp log log10 log2`. `rou`=round (alias `round`).

## Trig

`sin cos tan asin acos atan atan2`. `asin acos sqrt log` NaN out-of-domain. NaN propagates; comparisons false.

## Constants

`pi tau e`.

## Random

`rnd` = random float [0,1) (aliases `rand`, `random`); `rndn` = random integer in range.

## Statistics

`sum avg median quantile stdev variance cumsum frq argmax argmin argsort prod cprod`.
