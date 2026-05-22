import sys, time
from functools import reduce

def sq(x):   return x * x
def pos(x):  return x > 0
def add(a, b): return a + b

def bench(n):
    xs = list(range(n))
    return reduce(add, filter(pos, map(sq, xs)), 0)

N = int(sys.argv[1]) if len(sys.argv) > 1 else 500
for _ in range(100):
    bench(N)
iters = 10000
start = time.monotonic_ns()
for _ in range(iters):
    r = bench(N)
elapsed = time.monotonic_ns() - start
print(f"result:     {r}")
print(f"iterations: {iters}")
print(f"total:      {elapsed / 1e6:.2f}ms")
print(f"per call:   {elapsed // iters}ns")
