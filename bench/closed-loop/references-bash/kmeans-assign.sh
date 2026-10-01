points="1 2 3 10 11 12"
c0=2
c1=11
assign=
s0=0; n0=0
s1=0; n1=0
for p in $points; do
  d0=$(( p - c0 )); [ "$d0" -lt 0 ] && d0=$(( -d0 ))
  d1=$(( p - c1 )); [ "$d1" -lt 0 ] && d1=$(( -d1 ))
  if [ "$d0" -le "$d1" ]; then
    a=0; s0=$(( s0 + p )); n0=$(( n0 + 1 ))
  else
    a=1; s1=$(( s1 + p )); n1=$(( n1 + 1 ))
  fi
  assign="${assign:+$assign,}$a"
done
echo "$assign"
printf 'c0=%.2f\n' "$(awk -v s="$s0" -v n="$n0" 'BEGIN { printf "%.2f", s / n }')"
printf 'c1=%.2f\n' "$(awk -v s="$s1" -v n="$n1" 'BEGIN { printf "%.2f", s / n }')"
