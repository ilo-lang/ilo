out=
for n in $(seq 1 15); do
  if [ $(( n % 15 )) -eq 0 ]; then v=FizzBuzz
  elif [ $(( n % 3 )) -eq 0 ]; then v=Fizz
  elif [ $(( n % 5 )) -eq 0 ]; then v=Buzz
  else v=$n
  fi
  out="${out:+$out,}$v"
done
echo "$out"
