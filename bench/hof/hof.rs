use std::env;
use std::time::Instant;

fn bench(n: i64) -> i64 {
    (0..n).map(|x| x * x).filter(|&x| x > 0).sum()
}

fn main() {
    let n: i64 = env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(500);
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
