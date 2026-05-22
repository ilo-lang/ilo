import sys, time

def fib(n):
    if n <= 1:
        return n
    return fib(n - 1) + fib(n - 2)

N = int(sys.argv[1]) if len(sys.argv) > 1 else 25
# warmup
for _ in range(5):
    fib(N)
iters = 100
start = time.monotonic_ns()
for _ in range(iters):
    r = fib(N)
elapsed = time.monotonic_ns() - start
print(f"result:     {r}")
print(f"iterations: {iters}")
print(f"total:      {elapsed / 1e6:.2f}ms")
print(f"per call:   {elapsed // iters}ns")
