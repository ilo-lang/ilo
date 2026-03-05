function addn(a, b) { return a + b; }
function muln(a, b) { return a * b; }

function compute(x, y) {
    const a = muln(x, y);
    const b = addn(a, x);
    return addn(b, y);
}

function bench(n) {
    let s = 0;
    for (let i = 0; i < n; i++) {
        s += compute(i, i + 1);
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
