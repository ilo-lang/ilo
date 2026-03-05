use std::env;
use std::time::Instant;

#[inline(never)]
fn tier(x: i64) -> i64 {
    if x >= 500 { return 3; }
    if x >= 200 { return 2; }
    1
}

fn bench(n: i64) -> i64 {
    let mut s: i64 = 0;
    for i in 0..n {
        s += i * tier(i);
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
