safe_div() {
  if [ "$2" -eq 0 ]; then echo 0; else echo $(( $1 / $2 )); fi
}
safe_div 10 2
safe_div 5 0
