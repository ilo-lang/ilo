import sys, time
from collections import namedtuple

Vec3 = namedtuple('Vec3', ['x', 'y', 'z'])

def run(n):
    s = 0
    for i in range(n):
        v = Vec3(i, i * 2, i * 3)
        d = v.x + v.y + v.z
        v2 = v._replace(x=v.x + 1)
        s += d + v2.x
    return s

N = int(sys.argv[1]) if len(sys.argv) > 1 else 1000
# warmup
for _ in range(100):
    run(N)
iters = 10000
start = time.monotonic_ns()
for _ in range(iters):
    r = run(N)
elapsed = time.monotonic_ns() - start
print(f"result:     {r}")
print(f"iterations: {iters}")
print(f"total:      {elapsed / 1e6:.2f}ms")
print(f"per call:   {elapsed // iters}ns")
