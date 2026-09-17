total=0
for x in 1 2 3 4 5; do
  sq=$(( x * x ))
  [ "$sq" -gt 10 ] && total=$(( total + sq ))
done
echo "$total"
