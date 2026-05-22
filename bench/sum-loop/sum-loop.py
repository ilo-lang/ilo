import sys, time

def tier(x):
    if x >= 500: return 3
    if x >= 200: return 2
    return 1

def bench(n):
    s = 0
    for i in range(n):
        s += i * tier(i)
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
