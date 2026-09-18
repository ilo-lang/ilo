awk 'BEGIN {
  n = split("1 2 3 4 5", xs, " ")
  split("2 4 5 4 5", ys, " ")
  sx = 0; sy = 0
  for (i = 1; i <= n; i++) { sx += xs[i]; sy += ys[i] }
  mx = sx / n; my = sy / n
  num = 0; den = 0
  for (i = 1; i <= n; i++) {
    num += (xs[i] - mx) * (ys[i] - my)
    den += (xs[i] - mx) ^ 2
  }
  printf "slope=%.2f\n", num / den
  printf "intercept=%.2f\n", my - (num / den) * mx
}'
