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

## Linear algebra

`transpose matmul matvec dot solve inv det fft ifft`. Row-major matrices are `L (L n)`, flat vectors are `L n`. `matvec xm ys` is matrix-vector product, returning a flat vector - use it instead of the `flatten matmul xm (map (y:n>L n;[y]) ys)` wrap-as-column ceremony. `solve inv det` use LU decomposition with partial pivoting and error on singular / non-square inputs.
`transpose matmul dot solve inv det fft ifft lstsq`. Matrices are row-major nested lists (`L (L n)`); vectors are flat lists (`L n`).

`lstsq xm ys > L n` is closed-form ordinary least squares via the normal equations: returns the coefficient vector minimising `||xm·b - ys||²`. Collapses the 5-line `solve (matmul (transpose xm) xm) (matmul (transpose xm) ys-col)` recipe into one call. Errors `ILO-R009` on rank-deficient design, underdetermined system (cols > rows), or row/length mismatch. Numerically inferior to QR/SVD for ill-conditioned designs; reach for a dedicated library if condition number is a concern.

```
-- fit y = 2x + 3
xs = [1, 2, 3, 4, 5]
ys = [5, 7, 9, 11, 13]
xm = map (x:n>L n;[1, x]) xs
b = lstsq xm ys                  -- [3, 2]
```
