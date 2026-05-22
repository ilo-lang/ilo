use std::env;
use std::time::Instant;

#[inline(never)]
fn cata(x: i64) -> i64 {
    if x >= 800 { if x >= 900 { return 9; } return 8; }
    if x >= 600 { if x >= 700 { return 7; } return 6; }
    if x >= 400 { if x >= 500 { return 5; } return 4; }
    if x >= 200 { if x >= 300 { return 3; } return 2; }
    1
}

#[inline(never)]
fn catb(x: i64) -> i64 {
    if x >= 500 { return x * 3; }
    if x >= 200 { return x * 2; }
    x
}

#[inline(never)]
fn combine(a: i64, b: i64) -> i64 {
    if a >= 7 { return b + a * 10; }
    if a >= 4 { return b + a * 5; }
    b + a
}

fn bench(n: i64) -> i64 {
    let mut s: i64 = 0;
    for i in 0..n {
        let a = cata(i);
        let b = catb(i);
        let c = combine(a, b);
        s += c;
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
