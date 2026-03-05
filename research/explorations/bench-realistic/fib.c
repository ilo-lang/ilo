#include <stdio.h>
#include <stdlib.h>
#include <time.h>

long long fib(long long n) {
    if (n <= 1) return n;
    return fib(n - 1) + fib(n - 2);
}

int main(int argc, char **argv) {
    long long n = argc > 1 ? atoll(argv[1]) : 25;
    for (int i = 0; i < 100; i++) fib(n);
    long long iters = 1000;
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    long long r = 0;
    for (long long i = 0; i < iters; i++) r = fib(n);
    clock_gettime(CLOCK_MONOTONIC, &t1);
    long long elapsed = (t1.tv_sec - t0.tv_sec) * 1000000000LL + (t1.tv_nsec - t0.tv_nsec);
    printf("result:     %lld\n", r);
    printf("iterations: %lld\n", iters);
    printf("total:      %.2fms\n", elapsed / 1e6);
    printf("per call:   %lldns\n", elapsed / iters);
    return 0;
}
