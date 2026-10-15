text=abababab
count=0
pos=0
while [ "$pos" -le $(( ${#text} - 2 )) ]; do
  if [ "${text:$pos:2}" = "ab" ]; then count=$(( count + 1 )); pos=$(( pos + 2 )); else pos=$(( pos + 1 )); fi
done
echo "$count"
