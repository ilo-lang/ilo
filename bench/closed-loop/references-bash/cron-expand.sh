field=1-3,7,9-10
IFS=,
set -- $field
unset IFS
out=
for part in "$@"; do
  case "$part" in
    *-*)
      lo=${part%-*}
      hi=${part#*-}
      for n in $(seq "$lo" "$hi"); do
        out="${out:+$out }$n"
      done
      ;;
    *) out="${out:+$out }$part" ;;
  esac
done
echo "$out"
