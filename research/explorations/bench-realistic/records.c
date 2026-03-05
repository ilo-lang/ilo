#include <stdio.h>
#include <stdlib.h>
#include <time.h>

typedef struct { long long x, y, z; } Vec3;

long long run(long long n) {
    long long s = 0;
    for (long long i = 0; i < n; i++) {
        Vec3 v = {i, i * 2, i * 3};
        long long d = v.x + v.y + v.z;
        Vec3 v2 = {v.x + 1, v.y, v.z};
        s += d + v2.x;
    }
    return s;
}

int main(int argc, char **argv) {
    long long n = argc > 1 ? atoll(argv[1]) : 1000;
    for (int i = 0; i < 1000; i++) run(n);
    long long iters = 10000;
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    long long r = 0;
    for (long long i = 0; i < iters; i++) r = run(n);
    clock_gettime(CLOCK_MONOTONIC, &t1);
    long long elapsed = (t1.tv_sec - t0.tv_sec) * 1000000000LL + (t1.tv_nsec - t0.tv_nsec);
    printf("result:     %lld\n", r);
    printf("iterations: %lld\n", iters);
    printf("total:      %.2fms\n", elapsed / 1e6);
    printf("per call:   %lldns\n", elapsed / iters);
    return 0;
}
