import sys, time

def cata(x):
    if x >= 800:
        if x >= 900: return 9
        return 8
    if x >= 600:
        if x >= 700: return 7
        return 6
    if x >= 400:
        if x >= 500: return 5
        return 4
    if x >= 200:
        if x >= 300: return 3
        return 2
    return 1

def catb(x):
    if x >= 500: return x * 3
    if x >= 200: return x * 2
    return x

def combine(a, b):
    if a >= 7: return b + a * 10
    if a >= 4: return b + a * 5
    return b + a

def bench(n):
    s = 0
    for i in range(n):
        a = cata(i)
        b = catb(i)
        c = combine(a, b)
        s += c
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
