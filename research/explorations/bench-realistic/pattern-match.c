#include <stdio.h>
#include <stdlib.h>
#include <time.h>

__attribute__((noinline))
long long cata(long long x) {
    if (x >= 800) { if (x >= 900) return 9; return 8; }
    if (x >= 600) { if (x >= 700) return 7; return 6; }
    if (x >= 400) { if (x >= 500) return 5; return 4; }
    if (x >= 200) { if (x >= 300) return 3; return 2; }
    return 1;
}

__attribute__((noinline))
long long catb(long long x) {
    if (x >= 500) return x * 3;
    if (x >= 200) return x * 2;
    return x;
}

__attribute__((noinline))
long long combine(long long a, long long b) {
    if (a >= 7) return b + a * 10;
    if (a >= 4) return b + a * 5;
    return b + a;
}

long long bench(long long n) {
    long long s = 0;
    for (long long i = 0; i < n; i++) {
        long long a = cata(i);
        long long b = catb(i);
        long long c = combine(a, b);
        s += c;
    }
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
