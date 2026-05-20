#!/bin/bash
set -e

cd "$(dirname "$0")/../.."
ilo_bin="/tmp/ilo-targets/ilo-persona-diag-coverage/release/ilo"

# Collect all diagnostics across examples, output code:has-suggestion per line
for f in examples/*.ilo; do
  "$ilo_bin" check --json "$f" 2>&1 || true
done | jq -r 'select(.code) | "\(.code):\(if (.suggestion != null and .suggestion != "") then 1 else 0 end)"' 2>/dev/null | sort | uniq -c | awk '
  {
    count=$1
    split($2, parts, ":")
    code=parts[1]
    has_sug=parts[2]
    seen[code]++
    with_sug[code] += has_sug
  }
  END {
    thr=80
    fails=0
    for (c in seen) {
      cnt=seen[c]
      ws=with_sug[c]
      pct=int((ws/cnt)*100)
      stat=(pct>=thr) ? "✓" : "✗"
      printf "%s %s: %d/%d (%d%%)\n", stat, c, ws, cnt, pct
      if (pct<thr) fails++
    }
    print ""
    if (fails>0) {
      printf "FAIL: %d code(s) below %d%% threshold\n", fails, thr
      exit 1
    } else {
      printf "PASS: all codes >= %d%%\n", thr
      exit 0
    }
  }
'
