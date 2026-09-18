LC_ALL=C
data=widget:10:5.00,gadget:3:15.00,gizmo:7:8.00,tool:2:20.00,book:15:2.00
records=
IFS=,
for rec in $data; do
  name=${rec%%:*}
  rest=${rec#*:}
  qty=${rest%%:*}
  price=${rest#*:}
  records="$records$qty $price $name
"
done
unset IFS

printf '%s' "$records" \
  | awk '$1 * $2 > 20 { printf "%.2f %s\n", $1 * $2, $3 }' \
  | sort -rn \
  | awk '{ printf "%s: %s\n", $2, $1 }'
