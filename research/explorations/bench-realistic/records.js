function run(n) {
    let s = 0;
    for (let i = 0; i < n; i++) {
        const v = { x: i, y: i * 2, z: i * 3 };
        const d = v.x + v.y + v.z;
        const v2 = { ...v, x: v.x + 1 };
        s += d + v2.x;
    }
    return s;
}

const N = parseInt(process.argv[2]) || 1000;
// warmup
for (let i = 0; i < 1000; i++) run(N);
const iters = 10000;
const start = process.hrtime.bigint();
let r;
for (let i = 0; i < iters; i++) r = run(N);
const elapsed = Number(process.hrtime.bigint() - start);
console.log(`result:     ${r}`);
console.log(`iterations: ${iters}`);
console.log(`total:      ${(elapsed / 1e6).toFixed(2)}ms`);
console.log(`per call:   ${Math.floor(elapsed / iters)}ns`);
