text="go go stop stop stop run run run run"
for w in $text; do echo "$w"; done \
  | awk '{ c[$1]++ } END { for (w in c) print w, c[w] }' \
  | LC_ALL=C sort -k2,2nr -k1,1
