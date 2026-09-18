log="INFO start
ERROR disk full
INFO retry
WARN slow
ERROR timeout
INFO done
WARN mem high
ERROR fatal"
errors=0
warns=0
infos=0
while read -r level rest; do
  case "$level" in
    ERROR) errors=$(( errors + 1 )) ;;
    WARN) warns=$(( warns + 1 )) ;;
    INFO) infos=$(( infos + 1 )) ;;
  esac
done <<< "$log"
echo "ERROR=$errors"
echo "WARN=$warns"
echo "INFO=$infos"
