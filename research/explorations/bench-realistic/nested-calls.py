import sys, time

def addn(a, b):
    return a + b

def muln(a, b):
    return a * b

def compute(x, y):
    a = muln(x, y)
    b = addn(a, x)
    return addn(b, y)

def bench(n):
    s = 0
    i = 0
    while i < n:
        j = i + 1
        s += compute(i, j)
        i += 1
    return s

N = int(sys.argv[1]) if len(sys.argv) > 1 else 1000
# warmup
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
