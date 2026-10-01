count=0
largest=0
for n in $(seq 2 29); do
  prime=1
  d=2
  while [ $(( d * d )) -le "$n" ]; do
    if [ $(( n % d )) -eq 0 ]; then prime=0; break; fi
    d=$(( d + 1 ))
  done
  if [ "$prime" -eq 1 ]; then
    count=$(( count + 1 ))
    largest=$n
  fi
done
echo "count=$count"
echo "largest=$largest"
