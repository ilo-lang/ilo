seen=" "
out=
for x in 3 1 3 2 1 4; do
  case "$seen" in
    *" $x "*) ;;
    *)
      seen="$seen$x "
      out="${out:+$out }$x"
      ;;
  esac
done
echo "$out"
