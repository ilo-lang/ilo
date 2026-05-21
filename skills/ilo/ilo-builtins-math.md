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

`rnd > n` = uniform float `[0,1)` (aliases `rand`, `random`); `rnd a b > n` = uniform integer in `[a,b]` inclusive (use this for dice / bucket IDs, NOT `rndn 0 n`); `rndn mu sigma > n` = ONE sample from Normal `N(mu, sigma)` (Box-Muller, can be negative / unbounded). `rndn 0 n` is Gaussian noise, not a uniform int. For cryptographic randomness (jti, CSRF tokens, session IDs, nonces) use `rand-bytes n > t` — CSPRNG bytes encoded as base64url-no-pad; URL-safe drop-in for headers / cookies / query strings.
`rnd` = random float [0,1) (aliases `rand`, `random`); `rndn` = sample from Normal `N(mu, sigma)` (Box-Muller). For cryptographic randomness (jti, CSRF tokens, session IDs, nonces) use `rand-bytes n > t` — CSPRNG bytes encoded as base64url-no-pad; URL-safe drop-in for headers / cookies / query strings.
`rnd` = random float [0,1) (aliases `rand`, `random`). `rnd a b` = random integer in [a,b] inclusive. `rndn mu sigma` = ONE sample from Normal `N(mu,sigma)` (Box-Muller), NOT uniform-int. For uniform-int in [0,n): `flr (* (rnd) n)` or use `rnd 0 (- n 1)`.

## Statistics

`sum avg median quantile stdev variance cumsum frq argmax argmin argsort prod cprod bisect`. `bisect xs target` is Python `bisect_left` (O(log N) sorted-list insertion point); caller owns sortedness.

## Linear algebra

`transpose matmul matvec dot solve inv det fft ifft lstsq`. Row-major matrices are `L (L n)`, flat vectors are `L n`. `matvec xm ys` is matrix-vector product, returning a flat vector - use it instead of the `flatten matmul xm (map (y:n>L n;[y]) ys)` wrap-as-column ceremony. `solve inv det` use LU decomposition with partial pivoting and error on singular / non-square inputs.

`lstsq xm ys > L n` is closed-form ordinary least squares via the normal equations: returns the coefficient vector minimising `||xm·b - ys||²`. Collapses the 5-line `solve (matmul (transpose xm) xm) (matmul (transpose xm) ys-col)` recipe into one call. Errors `ILO-R009` on rank-deficient design, underdetermined system (cols > rows), or row/length mismatch. Numerically inferior to QR/SVD for ill-conditioned designs; reach for a dedicated library if condition number is a concern.

```
-- fit y = 2x + 3
xs = [1, 2, 3, 4, 5]
ys = [5, 7, 9, 11, 13]
xm = map (x:n>L n;[1, x]) xs
b = lstsq xm ys                  -- [3, 2]
```
`ewm xs a > L n` = exponential moving average. `ewm[0] = xs[0]`, `ewm[i] = a*xs[i] + (1-a)*ewm[i-1]`. `a` in `[0, 1]` (out-of-range errors `ILO-R009`). Replaces the fold-with-state pattern in one call.

## Numeric prelude (list constructors)

`linspace a b n` = n evenly-spaced floats from a to b inclusive (numpy `endpoint=True`); `linspace 0 10 5` → `[0, 2.5, 5, 7.5, 10]`. `n=0` returns `[]`; `n=1` returns `[a]`. Last element pinned to `b` to avoid float drift.

`ones n` = n copies of `1.0`. Useful for design-matrix columns (`sum (ones n) = n`).

`rep n v` = n copies of any value `v`; element type follows `v`. Useful for accumulator seeds (`rep 8 0`) and constant lookup tables (`rep 7 "x"`).

All three reject negative `n` with ILO-R009 and cap at 1M elements.
