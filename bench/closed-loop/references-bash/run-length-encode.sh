text=aaabbccccd
out=
i=0
n=${#text}
while [ "$i" -lt "$n" ]; do
  ch=${text:$i:1}
  run=1
  while [ "$i" -lt $(( n - 1 )) ] && [ "${text:$(( i + 1 )):1}" = "$ch" ]; do
    i=$(( i + 1 ))
    run=$(( run + 1 ))
  done
  out="$out$ch$run"
  i=$(( i + 1 ))
done
echo "$out"
