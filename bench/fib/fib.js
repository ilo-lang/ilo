function fib(n) {
    if (n <= 1) return n;
    return fib(n - 1) + fib(n - 2);
}

const N = parseInt(process.argv[2]) || 25;
// warmup
for (let i = 0; i < 100; i++) fib(N);
const iters = 1000;
const start = process.hrtime.bigint();
let r;
for (let i = 0; i < iters; i++) r = fib(N);
const elapsed = Number(process.hrtime.bigint() - start);
console.log(`result:     ${r}`);
console.log(`iterations: ${iters}`);
console.log(`total:      ${(elapsed / 1e6).toFixed(2)}ms`);
console.log(`per call:   ${Math.floor(elapsed / iters)}ns`);
