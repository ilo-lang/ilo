import sys, time
from collections import namedtuple

Entry = namedtuple('Entry', ['k', 'v', 'sum'])

def bench(n):
    s = 0
    for i in range(n):
        e = Entry(i, i * 7, 0)
        e2 = e._replace(sum=e.k + e.v)
        s += e2.sum
    return s

N = int(sys.argv[1]) if len(sys.argv) > 1 else 1000
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
