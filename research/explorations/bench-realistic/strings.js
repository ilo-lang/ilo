function bench(n) {
    let s = 0;
    for (let i = 0; i < n; i++) {
        const t = "item-" + String(i);
        const parts = t.split("-");
        const j = parts.join("_");
        s += j.length;
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
