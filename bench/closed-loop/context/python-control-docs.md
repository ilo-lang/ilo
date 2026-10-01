# Python control documentation (closed-loop comparator)

Write a complete Python 3 program that prints the required result to stdout and exits 0. Prefer the standard library only.

## Shape
- Top-level statements and function defs are fine.
- The harness runs: python3 <file>.py
- Print exactly what the task asks for.

## Patterns
Triangular: def tri(n): return n*(n+1)//2; print(tri(10))
Helper: def double(x): return 2*x; print(double(14))
List: xs=[1,2,3,4,5]; print(sum(x*x for x in xs if x%2==1))
Env: import os; print(os.environ.get("ILO_TEST_MSG",""))
Safe div: def safe_div(a,b): return a//b if b!=0 else 0; print(safe_div(10,2)); print(safe_div(10,0))

Task prompts may name another language; still write Python.
