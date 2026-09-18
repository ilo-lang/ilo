a=48
b=18
x=$a
y=$b
while [ "$y" -ne 0 ]; do
  t=$(( x % y ))
  x=$y
  y=$t
done
echo "gcd=$x"
echo "lcm=$(( a * b / x ))"
