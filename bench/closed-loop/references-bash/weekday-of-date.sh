days=(saturday sunday monday tuesday wednesday thursday friday)
q=18
m=9
year=2026
if [ "$m" -le 2 ]; then
  m=$(( m + 12 ))
  year=$(( year - 1 ))
fi
k=$(( year % 100 ))
j=$(( year / 100 ))
h=$(( (q + (13 * (m + 1)) / 5 + k + k / 4 + j / 4 + 5 * j) % 7 ))
echo "${days[$h]}"
