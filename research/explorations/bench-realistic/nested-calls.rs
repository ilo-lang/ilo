use std::env;
use std::time::Instant;

#[inline(never)]
fn addn(a: i64, b: i64) -> i64 { a + b }

#[inline(never)]
fn muln(a: i64, b: i64) -> i64 { a * b }

#[inline(never)]
fn compute(x: i64, y: i64) -> i64 {
    let a = muln(x, y);
    let b = addn(a, x);
    addn(b, y)
}

fn bench(n: i64) -> i64 {
    let mut s: i64 = 0;
    let mut i: i64 = 0;
    while i < n {
        let j = i + 1;
        s += compute(i, j);
        i += 1;
    }
    s
}

fn main() {
    let n: i64 = env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1000);
    for _ in 0..1000 { let _ = bench(n); }
    let iters: u128 = 10000;
    let start = Instant::now();
    let mut r = 0i64;
    for _ in 0..iters { r = bench(n); }
    let elapsed = start.elapsed().as_nanos();
    println!("result:     {}", r);
    println!("iterations: {}", iters);
    println!("total:      {:.2}ms", elapsed as f64 / 1e6);
    println!("per call:   {}ns", elapsed / iters);
}
