use std::env;
use std::time::Instant;

fn fib(n: i64) -> i64 {
    if n <= 1 { return n; }
    fib(n - 1) + fib(n - 2)
}

fn main() {
    let n: i64 = env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(25);
    // warmup
    for _ in 0..100 { let _ = fib(n); }
    let iters = 1000;
    let start = Instant::now();
    let mut r = 0i64;
    for _ in 0..iters { r = fib(n); }
    let elapsed = start.elapsed().as_nanos();
    println!("result:     {}", r);
    println!("iterations: {}", iters);
    println!("total:      {:.2}ms", elapsed as f64 / 1e6);
    println!("per call:   {}ns", elapsed / iters);
}
