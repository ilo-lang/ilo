function lev(x) {
    if (x >= 5000) return 5;
    if (x >= 3000) return 4;
    if (x >= 1000) return 3;
    if (x >= 500) return 2;
    return 1;
}

function bench(n) {
    let s = 0;
    for (let i = 0; i < n; i++) {
        const a = i * 3;
        const b = a + 1;
        const c = b * 2;
        const d = lev(c);
        s += d;
    }
    return s;
}

const N = parseInt(process.argv[2]) || 1000;
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
