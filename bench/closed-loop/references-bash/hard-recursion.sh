fib() {
  local n=$1
  if [ "$n" -eq 0 ]; then echo 0; return; fi
  if [ "$n" -eq 1 ]; then echo 1; return; fi
  echo $(( $(fib $((n-1))) + $(fib $((n-2))) ))
}
echo $(( $(fib 15) + $(fib 12) ))
