function classify(x) {
    if (x >= 900) return 30;
    if (x >= 700) return 25;
    if (x >= 500) return 20;
    if (x >= 300) return 15;
    if (x >= 100) return 10;
    return 5;
}

function bench(n) {
    let s = 0;
    for (let i = 0; i < n; i++) {
        s += classify(i);
    }
    return s;
}

const N = parseInt(process.argv[2]) || 1000;
// warmup
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
