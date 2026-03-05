#include <stdio.h>
#include <stdlib.h>
#include <time.h>

__attribute__((noinline))
long long classify(long long x) {
    if (x >= 900) return 30;
    if (x >= 700) return 25;
    if (x >= 500) return 20;
    if (x >= 300) return 15;
    if (x >= 100) return 10;
    return 5;
}

long long bench(long long n) {
    long long s = 0;
    for (long long i = 0; i < n; i++) s += classify(i);
    return s;
}

int main(int argc, char **argv) {
    long long n = argc > 1 ? atoll(argv[1]) : 1000;
    for (int i = 0; i < 1000; i++) bench(n);
    long long iters = 10000;
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    long long r = 0;
    for (long long i = 0; i < iters; i++) r = bench(n);
    clock_gettime(CLOCK_MONOTONIC, &t1);
    long long elapsed = (t1.tv_sec - t0.tv_sec) * 1000000000LL + (t1.tv_nsec - t0.tv_nsec);
    printf("result:     %lld\n", r);
    printf("iterations: %lld\n", iters);
    printf("total:      %.2fms\n", elapsed / 1e6);
    printf("per call:   %lldns\n", elapsed / iters);
    return 0;
}
