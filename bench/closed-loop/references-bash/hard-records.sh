sum=0
for qty in 3 5 8; do
  [ "$qty" -gt 4 ] && sum=$(( sum + qty ))
done
echo "$sum"
