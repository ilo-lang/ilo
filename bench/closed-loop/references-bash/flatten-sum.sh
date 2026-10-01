nested="1 2
3 4
5"
flat=
sum=0
while read -r group; do
  for x in $group; do
    flat="${flat:+$flat }$x"
    sum=$(( sum + x ))
  done
done <<< "$nested"
echo "flat=$flat"
echo "sum=$sum"
