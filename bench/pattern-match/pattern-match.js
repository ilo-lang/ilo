function cata(x) {
    if (x >= 800) { if (x >= 900) return 9; return 8; }
    if (x >= 600) { if (x >= 700) return 7; return 6; }
    if (x >= 400) { if (x >= 500) return 5; return 4; }
    if (x >= 200) { if (x >= 300) return 3; return 2; }
    return 1;
}

function catb(x) {
    if (x >= 500) return x * 3;
    if (x >= 200) return x * 2;
    return x;
}

function combine(a, b) {
    if (a >= 7) return b + a * 10;
    if (a >= 4) return b + a * 5;
    return b + a;
}

function bench(n) {
    let s = 0;
    for (let i = 0; i < n; i++) {
        const a = cata(i);
        const b = catb(i);
        const c = combine(a, b);
        s += c;
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
