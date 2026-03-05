use std::env;
use std::time::Instant;

#[derive(Clone)]
struct Vec3 { x: i64, y: i64, z: i64 }

fn run(n: i64) -> i64 {
    let mut s: i64 = 0;
    for i in 0..n {
        let v = Vec3 { x: i, y: i * 2, z: i * 3 };
        let d = v.x + v.y + v.z;
        let v2 = Vec3 { x: v.x + 1, ..v };
        s += d + v2.x;
    }
    s
}

fn main() {
    let n: i64 = env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1000);
    for _ in 0..1000 { let _ = run(n); }
    let iters: u128 = 10000;
    let start = Instant::now();
    let mut r = 0i64;
    for _ in 0..iters { r = run(n); }
    let elapsed = start.elapsed().as_nanos();
    println!("result:     {}", r);
    println!("iterations: {}", iters);
    println!("total:      {:.2}ms", elapsed as f64 / 1e6);
    println!("per call:   {}ns", elapsed / iters);
}
