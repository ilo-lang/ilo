# Bash control documentation (closed-loop comparator)

Write a complete bash script that prints the required result to stdout and exits 0. Prefer POSIX-friendly bash builtins and `/bin/bash` arithmetic.

## Shape
- Top-level statements and shell functions are fine.
- The harness runs: bash <file>.sh
- Print exactly what the task asks for (no prompts, no debug).

## Patterns
Triangular: tri(){ echo $(( $1 * ($1 + 1) / 2 )); }; tri 10
Helper: double(){ echo $(( 2 * $1 )); }; double 14
List filter/sum: total=0; for x in 1 2 3 4 5; do sq=$((x*x)); [ "$sq" -gt 10 ] && total=$((total+sq)); done; echo "$total"
Env: printf '%s\n' "${ILO_TEST_MSG-}"
Safe div: safe_div(){ if [ "$2" -eq 0 ]; then echo 0; else echo $(( $1 / $2 )); fi; }; safe_div 10 2; safe_div 10 0

Task prompts may name another language; still write bash.
