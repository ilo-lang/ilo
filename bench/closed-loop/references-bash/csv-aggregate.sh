csv="east,10.5
west,4.25
east,6.5
north,3.0
west,2.5"
printf '%s\n' "$csv" \
  | awk -F, '{ sum[$1] += $2 } END { for (r in sum) printf "%s: %.2f\n", r, sum[r] }' \
  | LC_ALL=C sort
