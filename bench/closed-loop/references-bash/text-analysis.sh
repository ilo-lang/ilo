text="the quick brown fox jumps over the lazy dog the fox runs fast"
chars=${#text}
words=0
total=0
longest=
shortest=
for w in $text; do
  words=$(( words + 1 ))
  len=${#w}
  total=$(( total + len ))
  if [ -z "$longest" ] || [ "$len" -gt "${#longest}" ]; then longest=$w; fi
  if [ -z "$shortest" ] || [ "$len" -lt "${#shortest}" ]; then shortest=$w; fi
done
avg=$(awk -v t="$total" -v n="$words" 'BEGIN { printf "%.1f", t / n }')

echo "words=$words"
echo "chars=$chars"
echo "avg_len=$avg"
echo "longest=$longest"
echo "shortest=$shortest"
