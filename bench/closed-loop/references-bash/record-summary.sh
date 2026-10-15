records="10 5.00 widget
3 15.00 gadget
7 8.00 gizmo"
total_cents=0
over=
while read -r qty price name; do
  cents=$(( qty * ${price//./} ))
  total_cents=$(( total_cents + cents ))
  if [ "$cents" -gt 4000 ]; then
    over="${over:+$over,}$name"
  fi
done <<< "$records"
printf 'total=%d.%02d\n' $(( total_cents / 100 )) $(( total_cents % 100 ))
echo "over=$over"
