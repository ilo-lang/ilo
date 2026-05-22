function bench(n) {
    const xs = Array.from({length: n}, (_, i) => i);
    return xs.map(x => x * x).filter(x => x > 0).reduce((a, b) => a + b, 0);
}

const N = parseInt(process.argv[2]) || 500;
for (let i = 0; i < 1000; i++) bench(N);
const iters = 10000;
const start = process.hrtime.bigint();
let r;
for (let i = 0; i < iters; i++) r = bench(N);
const elapsed = Number(process.hrtime.bigint() - start);
console.log(`result:     ${r}`);
console.log(`iterations: ${iters}`);
console.log(`total:      ${(elapsed / 1e6).toFixed(2)}ms`);
console.log(`per call:   ${Math.floor(elapsed / iters)}ns`);
